use crate::connection::Connection;
use crate::markup;
use crate::streaming::{run_stream, StreamOutcome};
use std::io::Write;

pub async fn run(conn: &mut Connection) -> Result<(), String> {
	let mut stdout = std::io::stdout();
	match run_stream(conn, "agent.update", serde_json::json!({}), |chunk| {
		let _ = writeln!(stdout, "{}", markup::render(&chunk));
		let _ = stdout.flush();
	})
	.await
	{
		StreamOutcome::Completed => Ok(()),
		StreamOutcome::AgentRestarted => {
			eprintln!("{}", markup::render("[dim]Agent restarted[/dim]"));
			Ok(())
		}
		StreamOutcome::Failed(e) => Err(e),
	}
}
