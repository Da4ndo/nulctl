# **NULCTL**

[![CI](https://img.shields.io/github/actions/workflow/status/Da4ndo/nulctl/ci.yml?branch=main&label=CI)](https://github.com/Da4ndo/nulctl/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/actions/workflow/status/Da4ndo/nulctl/release-nulctl.yml?label=Release)](https://github.com/Da4ndo/nulctl/actions/workflows/release-nulctl.yml)
[![Latest Release](https://img.shields.io/github/v/release/Da4ndo/nulctl)](https://github.com/Da4ndo/nulctl/releases/latest)
[![License](https://img.shields.io/github/license/Da4ndo/nulctl)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.98+-orange.svg?logo=rust)](https://www.rust-lang.org)

Operator CLI for the [nulnet](https://github.com/nulnet/nulnet) Linux host agent. TUI-first live telemetry dashboard, plus Clap subcommands for scripting and automation.

Built with Rust + Ratatui.

> [!NOTE]
> **nulnet** runs on the server; **nulctl** runs on your machine. Install the agent first, add your client public key to `allowed_keys`, then connect with `nulctl`. See the [nulnet README](https://github.com/nulnet/nulnet#configuration) for agent setup.

---

## Install

### GitHub Releases (recommended)

Pick the asset for your platform from [latest release](https://github.com/Da4ndo/nulctl/releases/latest):

| Platform | Asset |
| -------- | ----- |
| Linux x86_64 | `nulctl-linux-x86_64` |
| macOS Apple Silicon | `nulctl-darwin-aarch64` |
| macOS Intel | `nulctl-darwin-x86_64` |
| Windows x86_64 | `nulctl-windows-x86_64.exe` |

Each binary ships with a matching `.sha256` file for verification.

**Linux example:**

```bash
curl -fsSL https://github.com/Da4ndo/nulctl/releases/latest/download/nulctl-linux-x86_64 -o nulctl
curl -fsSL https://github.com/Da4ndo/nulctl/releases/latest/download/nulctl-linux-x86_64.sha256 -o nulctl.sha256
sha256sum -c nulctl.sha256
chmod +x nulctl
sudo install -m 755 nulctl /usr/local/bin/nulctl
```

### Build from source

Requires Rust 1.80+ (edition 2024).

```bash
git clone https://github.com/Da4ndo/nulctl.git
cd nulctl
cargo build --release
# binary: target/release/nulctl
```

> [!TIP]
> Debug builds connect to `./nulnet.sock` by default. Override with `NULNET_SOCK=/path/to/nulnet.sock` when developing against a local agent.

---

## First-time setup

1. Run `nulctl` once (or any subcommand). It creates `~/.config/nulctl/identity.key` and prints your **Ed25519 public key** to stderr.
2. Add that hex key to `allowed_keys` in the agent config ([config.example.toml](https://github.com/nulnet/nulnet/blob/main/config.example.toml)).
3. Restart the agent: `sudo systemctl restart nulnet`.
4. Run `nulctl` again.

Headless mode exits after printing the key on first run. The TUI shows your public key on the connection-error screen if auth fails (no stdout spam during the dashboard).

---

## Usage

### Interactive (default)

```bash
nulctl                        # server picker → live dashboard
nulctl -t localhost           # skip picker, connect directly
nulctl -t user@host           # remote via SSH
```

#### Dashboard keybindings

| Key | Action |
| --- | ------ |
| `q` / `Esc` | Quit |
| `u` | Run agent update (streaming log panel) |
| `s` | Switch server (back to picker) |
| `Ctrl+C` | Quit |

#### Server picker keybindings

| Key | Action |
| --- | ------ |
| `↑` / `↓` or `j` / `k` | Select server |
| `Enter` | Connect |
| `a` | Add new target |
| `d` | Delete selected |
| `q` | Quit |

### Headless subcommands

```bash
nulctl telemetry -t localhost                    # latest snapshot JSON
nulctl telemetry history --hours 24 --limit 500
nulctl telemetry range --since 1747560000 --until 1747563600

nulctl update -t user@host                       # stream agent self-update
nulctl version -t localhost                      # agent version

nulctl servers list
nulctl servers add user@host
nulctl servers remove user@host
```

Global `-t` / `--target` skips the picker in both TUI and headless modes. Headless commands default to `localhost` when `-t` is omitted.

---

## Options

### CLI flags

| Flag | Description |
| ---- | ----------- |
| `-t`, `--target <host>` | Agent target (`localhost` or `user@host`). Skips server picker when set. |

### Local files

| Path | Description |
| ---- | ----------- |
| `~/.config/nulctl/identity.key` | Ed25519 secret key (`060` on Unix; created on first run) |
| `~/.config/nulctl/history.json` | Recent connection targets (up to 10) |

---

## Connection modes

**Local** — connects to `/opt/nulnet/nulnet.sock` (release default), matching the agent’s `socket_path`.

**Remote via SSH** — proxies the Unix socket over SSH using `socat` on the remote host. No extra ports required.

> [!IMPORTANT]
> Remote hosts require **socat** for socket forwarding. Install it via the nulnet installer or manually with `apt-get install socat`.

---

## Agent API mapping

`nulctl` speaks the same JSON-RPC-over-Unix-socket protocol as documented in the [nulnet API](https://github.com/nulnet/nulnet#api).

| nulctl command | Agent RPC | Auth |
| -------------- | --------- | ---- |
| *(connect)* | `agent.auth.request`, `agent.auth.verify` | Challenge–response |
| `nulctl version` | `agent.version` | Yes |
| `nulctl telemetry` | `telemetry.get_latest` | Yes |
| `nulctl telemetry history` | `telemetry.get_bulk` | Yes |
| `nulctl telemetry range` | `telemetry.get_range` | Yes |
| `nulctl update` | `agent.update` | Yes (streamed) |
| Dashboard (live) | `telemetry.get_latest` (polled) | Yes |

Telemetry JSON matches the agent snapshot schema (CPU, GPU, memory, disks, containers, load, uptime, etc.). See the [nulnet telemetry example](https://github.com/nulnet/nulnet#telemetry).

---

## Development

```bash
cargo test --locked
cargo clippy --locked -- -D warnings
```

Optional CRAP score gate (requires `cargo-llvm-cov` and `cargo-crap`):

```bash
cargo llvm-cov --lcov --output-path lcov.info
cargo crap --lcov lcov.info --fail-above
```

---

## Contributing

Issues and pull requests are welcome on [github.com/Da4ndo/nulctl](https://github.com/Da4ndo/nulctl).

1. Fork and clone the repo
2. Create a branch for your change
3. Run `cargo clippy` and `cargo test` before opening a PR
4. Tag releases follow semver (`v0.5.4`); CI builds platform binaries and attaches them to the GitHub release

---

## License

[BSD-3-Clause](LICENSE) © 2026 Da4ndo
