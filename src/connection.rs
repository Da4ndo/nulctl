use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::UnixStream;
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::{timeout, Duration};

/// Release default socket path; must match agent `socket_path` in production config.
const REMOTE_SOCKET_PATH: &str = "/opt/nulnet/nulnet.sock";

/// Timeout for a single request-response round trip.
const RPC_TIMEOUT: Duration = Duration::from_secs(30);

/// Per-line timeout for streaming commands (e.g. agent update).
const STREAM_LINE_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteProxyMethod {
	Socat,
	NcOpenbsd,
	Python3,
}

impl RemoteProxyMethod {
	pub fn label(self) -> &'static str {
		match self {
			Self::Socat => "socat",
			Self::NcOpenbsd => "nc.openbsd",
			Self::Python3 => "python3",
		}
	}
}

#[derive(Debug, Clone)]
pub enum TransportMethod {
	LocalUnix { path: String },
	RemoteSsh {
		target: String,
		proxy: RemoteProxyMethod,
	},
}

impl TransportMethod {
	pub fn description(&self) -> String {
		match self {
			Self::LocalUnix { path } => format!("local Unix socket ({path})"),
			Self::RemoteSsh { target, proxy } => {
				format!("SSH → {target}, proxy: {} → {REMOTE_SOCKET_PATH}", proxy.label())
			}
		}
	}

	/// Short label used in the REPL prompt right-side indicator.
	pub fn short_label(&self) -> &'static str {
		match self {
			Self::LocalUnix { .. } => "local",
			Self::RemoteSsh { .. } => "ssh",
		}
	}
}

/// Shell snippet executed on the server over `ssh`.
/// Debian's default `nc` lacks `-U`. Prefer socat, then nc.openbsd, then python3.
fn remote_socket_proxy_shell(socket_path: &str) -> String {
	format!(
		"if command -v socat >/dev/null 2>&1; then \
exec socat - UNIX-CONNECT:{socket_path}; \
elif command -v nc.openbsd >/dev/null 2>&1; then \
exec nc.openbsd -U {socket_path}; \
elif command -v python3 >/dev/null 2>&1; then \
exec python3 -c \"import select,socket,sys\\n\
s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)\\n\
s.connect(sys.argv[1])\\n\
while True:\\n\
 r,_,_=select.select([s,sys.stdin.buffer],[],[])\\n\
 for x in r:\\n\
  if x is s:\\n\
   d=s.recv(4096)\\n\
   if not d: raise SystemExit(0)\\n\
   sys.stdout.buffer.write(d);sys.stdout.buffer.flush()\\n\
  else:\\n\
   d=sys.stdin.buffer.read(4096)\\n\
   if not d: raise SystemExit(0)\\n\
   s.sendall(d)\" {socket_path}; \
else \
  echo nulctl: server needs socat or netcat-openbsd >&2; exit 1; fi",
		socket_path = socket_path
	)
}

fn shell_single_quote(s: &str) -> String {
	format!("'{}'", s.replace('\'', "'\"'\"'"))
}

async fn detect_remote_proxy(target: &str) -> Result<RemoteProxyMethod, String> {
	let script = "if command -v socat >/dev/null 2>&1; then echo socat; \
elif command -v nc.openbsd >/dev/null 2>&1; then echo nc.openbsd; \
elif command -v python3 >/dev/null 2>&1; then echo python3; \
else echo none; fi";
	let remote_cmd = format!("sh -c {}", shell_single_quote(script));

	let output = Command::new("ssh")
		.arg(target)
		.arg(remote_cmd)
		.output()
		.await
		.map_err(|e| format!("Failed to probe remote proxy tools: {}", e))?;

	if !output.status.success() {
		return Err(format!(
			"SSH probe failed: {}",
			String::from_utf8_lossy(&output.stderr)
		));
	}

	match String::from_utf8_lossy(&output.stdout).trim() {
		"socat" => Ok(RemoteProxyMethod::Socat),
		"nc.openbsd" => Ok(RemoteProxyMethod::NcOpenbsd),
		"python3" => Ok(RemoteProxyMethod::Python3),
		other => Err(format!(
			"No socket proxy on server (got {:?}). Install socat: apt install socat",
			other
		)),
	}
}

#[derive(Debug, Serialize)]
pub struct AgentRequest {
	pub id: String,
	pub command: String,
	pub params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct AgentResponse {
	#[allow(dead_code)]
	pub id: String,
	pub status: String,
	pub data: Option<serde_json::Value>,
	pub error: Option<String>,
}

enum ConnectionInner {
	Local {
		writer: WriteHalf<UnixStream>,
		reader: BufReader<ReadHalf<UnixStream>>,
	},
	Remote {
		#[allow(dead_code)]
		child: Child,
		stdin: ChildStdin,
		reader: BufReader<ChildStdout>,
	},
}

/// Holds transport metadata and a persistent reader/writer pair for the agent socket.
pub struct Connection {
	transport: TransportMethod,
	inner: ConnectionInner,
}

impl Connection {
	pub fn transport(&self) -> &TransportMethod {
		&self.transport
	}

	pub async fn connect(target: &str) -> Result<Self, String> {
		if target == "localhost" {
			// In debug builds, allow overriding the socket path via env var so the
			// developer doesn't need to run nulctl from a specific directory.
			let socket_path = if cfg!(debug_assertions) {
				std::env::var("NULNET_SOCK")
					.map(std::path::PathBuf::from)
					.unwrap_or_else(|_| std::path::PathBuf::from("./nulnet.sock"))
			} else {
				std::path::PathBuf::from(REMOTE_SOCKET_PATH)
			};

			let path_str = socket_path.display().to_string();
			match UnixStream::connect(&socket_path).await {
				Ok(stream) => {
					let (read_half, write_half) = tokio::io::split(stream);
					Ok(Connection {
						transport: TransportMethod::LocalUnix { path: path_str },
						inner: ConnectionInner::Local {
							writer: write_half,
							reader: BufReader::new(read_half),
						},
					})
				}
				Err(e) => Err(format!(
					"Failed to connect to local socket at {}: {}",
					socket_path.display(),
					e
				)),
			}
		} else {
			let proxy = detect_remote_proxy(target).await?;
			let script = remote_socket_proxy_shell(REMOTE_SOCKET_PATH);
			let remote_cmd = format!("sh -c {}", shell_single_quote(&script));

			let mut cmd = Command::new("ssh");
			cmd.arg(target);
			cmd.arg(remote_cmd);
			cmd.stdin(Stdio::piped());
			cmd.stdout(Stdio::piped());
			cmd.stderr(Stdio::inherit());

			let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn ssh: {}", e))?;
			let stdin = child.stdin.take().ok_or("Failed to open stdin")?;
			let stdout = child.stdout.take().ok_or("Failed to open stdout")?;

			Ok(Connection {
				transport: TransportMethod::RemoteSsh {
					target: target.to_string(),
					proxy,
				},
				inner: ConnectionInner::Remote {
					child,
					stdin,
					reader: BufReader::new(stdout),
				},
			})
		}
	}

	pub async fn send_command(
		&mut self,
		command: &str,
		params: serde_json::Value,
	) -> Result<AgentResponse, String> {
		let req_line = build_request_line(command, params)?;
		self.write_line(&req_line).await?;

		let line = self.read_line_with_timeout(RPC_TIMEOUT).await?;
		serde_json::from_str(&line).map_err(|e| e.to_string())
	}

	pub async fn send_command_streaming<F>(
		&mut self,
		command: &str,
		params: serde_json::Value,
		mut on_chunk: F,
	) -> Result<AgentResponse, String>
	where
		F: FnMut(String),
	{
		let req_line = build_request_line(command, params)?;
		self.write_line(&req_line).await?;

		loop {
			let line = self.read_line_with_timeout(STREAM_LINE_TIMEOUT).await?;
			let res: AgentResponse = serde_json::from_str(&line).map_err(|e| e.to_string())?;
			if res.status == "streaming" {
				if let Some(data) = res.data
					&& let Some(output) = data.get("output").and_then(|v| v.as_str())
				{
					on_chunk(output.to_string());
				}
			} else {
				return Ok(res);
			}
		}
	}

	async fn write_line(&mut self, line: &str) -> Result<(), String> {
		match &mut self.inner {
			ConnectionInner::Local { writer, .. } => {
				writer.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
				writer.flush().await.map_err(|e| e.to_string())
			}
			ConnectionInner::Remote { stdin, .. } => {
				stdin.write_all(line.as_bytes()).await.map_err(|e| e.to_string())?;
				stdin.flush().await.map_err(|e| e.to_string())
			}
		}
	}

	async fn read_line_with_timeout(&mut self, dur: Duration) -> Result<String, String> {
		let mut line = String::new();
		let read_fut = async {
			match &mut self.inner {
				ConnectionInner::Local { reader, .. } => reader.read_line(&mut line).await,
				ConnectionInner::Remote { reader, .. } => reader.read_line(&mut line).await,
			}
		};
		match timeout(dur, read_fut).await {
			Err(_) => Err(format!("Agent response timed out after {}s", dur.as_secs())),
			Ok(Err(e)) => Err(e.to_string()),
			Ok(Ok(0)) => Err("Connection closed by agent".to_string()),
			Ok(Ok(_)) => Ok(line),
		}
	}
}

fn build_request_line(command: &str, params: serde_json::Value) -> Result<String, String> {
	let req = AgentRequest {
		id: uuid::Uuid::new_v4().to_string(),
		command: command.to_string(),
		params,
	};
	let json = serde_json::to_string(&req).map_err(|e| e.to_string())?;
	Ok(format!("{}\n", json))
}
