use super::loading;
use super::terminal::Terminal;
use crate::connection::Connection;
use crate::identity::Identity;
use std::time::Duration;
use tokio::time;

pub const MAX_ATTEMPTS: u32 = 30;
const RETRY_DELAY: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconnectReason {
	AfterUpdate,
	ConnectionLost,
}

pub fn is_connection_error(err: &str) -> bool {
	let lower = err.to_lowercase();
	[
		"connection closed",
		"broken pipe",
		"connection reset",
		"early eof",
		"unexpected eof",
		"connection refused",
		"failed to write",
		"failed to read",
	]
	.iter()
	.any(|needle| lower.contains(needle))
}

pub enum PollTick<T> {
	Snapshot(T),
	Waiting,
	ConnectionLost,
	Failed(String),
}

pub fn decode_poll_result<T>(result: Result<Option<T>, String>) -> PollTick<T> {
	match result {
		Ok(Some(value)) => PollTick::Snapshot(value),
		Ok(None) => PollTick::Waiting,
		Err(e) if is_connection_error(&e) => PollTick::ConnectionLost,
		Err(e) => PollTick::Failed(e),
	}
}

pub fn reconnect_message(attempt: u32, reason: ReconnectReason) -> String {
	match (attempt, reason) {
		(1, ReconnectReason::AfterUpdate) => {
			"Reconnecting after agent update…".to_string()
		}
		(1, ReconnectReason::ConnectionLost) => {
			"Connection lost — reconnecting…".to_string()
		}
		_ => format!("Reconnecting… (attempt {attempt}/{MAX_ATTEMPTS})"),
	}
}

pub async fn reconnect_with_retry(
	terminal: &mut Terminal,
	target: &str,
	identity: &Identity,
	reason: ReconnectReason,
) -> Result<Connection, String> {
	let mut last_err = String::new();
	for attempt in 1..=MAX_ATTEMPTS {
		let message = reconnect_message(attempt, reason);
		match loading::while_loading(
			terminal,
			target,
			&message,
			crate::auth::connect_and_auth(target, identity),
		)
		.await
		{
			Ok(conn) => return Ok(conn),
			Err(e) => {
				last_err = e;
				if attempt < MAX_ATTEMPTS {
					time::sleep(RETRY_DELAY).await;
				}
			}
		}
	}
	Err(format!("Could not reconnect: {last_err}"))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reconnect_message_first_attempt_update() {
		assert_eq!(
			reconnect_message(1, ReconnectReason::AfterUpdate),
			"Reconnecting after agent update…",
		);
	}

	#[test]
	fn reconnect_message_first_attempt_lost() {
		assert_eq!(
			reconnect_message(1, ReconnectReason::ConnectionLost),
			"Connection lost — reconnecting…",
		);
	}

	#[test]
	fn is_connection_error_detects_broken_pipe() {
		assert!(is_connection_error("Broken pipe (os error 32)"));
		assert!(is_connection_error("Connection closed by agent"));
		assert!(!is_connection_error("Unknown command: foo"));
	}

	#[test]
	fn decode_poll_result_variants() {
		assert!(matches!(decode_poll_result(Ok(Some(1))), PollTick::Snapshot(1)));
		assert!(matches!(
			decode_poll_result::<i32>(Ok(None)),
			PollTick::Waiting
		));
		assert!(matches!(
			decode_poll_result::<i32>(Err("Broken pipe".into())),
			PollTick::ConnectionLost
		));
	}
}
