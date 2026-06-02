use crate::connection::Connection;
use crate::markup;

pub enum StreamOutcome {
	Completed,
	AgentRestarted,
	Failed(String),
}

pub const LOG_BUFFER_CAP: usize = 500;

pub fn push_log_line(lines: &mut std::collections::VecDeque<String>, raw: String) {
	let plain = markup::to_plain(&markup::render(&raw));
	for line in plain.lines() {
		lines.push_back(line.to_string());
		while lines.len() > LOG_BUFFER_CAP {
			lines.pop_front();
		}
	}
}

pub fn classify_stream_response(
	command: &str,
	res: &Result<crate::connection::AgentResponse, String>,
) -> StreamOutcome {
	let is_update = command == "agent.update";
	match res {
		Ok(res) if res.status == "ok" => {
			if is_update
				&& res
					.data
					.as_ref()
					.and_then(|d| d.get("restarting"))
					.and_then(|v| v.as_bool())
					== Some(true)
			{
				StreamOutcome::AgentRestarted
			} else {
				StreamOutcome::Completed
			}
		}
		Ok(res) => StreamOutcome::Failed(
			res.error.clone().unwrap_or_else(|| "Unknown error".to_string()),
		),
		Err(_) if is_update => StreamOutcome::AgentRestarted,
		Err(e) => StreamOutcome::Failed(e.clone()),
	}
}

pub async fn run_stream<F>(
	conn: &mut Connection,
	command: &str,
	params: serde_json::Value,
	on_line: F,
) -> StreamOutcome
where
	F: FnMut(String),
{
	let res = conn
		.send_command_streaming(command, params, on_line)
		.await;
	classify_stream_response(command, &res)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn push_log_line_respects_cap() {
		let mut lines = std::collections::VecDeque::new();
		for i in 0..600 {
			push_log_line(&mut lines, format!("line {i}"));
		}
		assert_eq!(lines.len(), LOG_BUFFER_CAP);
		assert_eq!(lines.back().map(String::as_str), Some("line 599"));
	}

	#[test]
	fn classify_update_restart() {
		use crate::connection::AgentResponse;
		let res = Ok(AgentResponse {
			id: "1".to_string(),
			status: "ok".to_string(),
			data: Some(serde_json::json!({ "restarting": true })),
			error: None,
		});
		assert!(matches!(
			classify_stream_response("agent.update", &res),
			StreamOutcome::AgentRestarted
		));
	}
}
