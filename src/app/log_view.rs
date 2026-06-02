use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::keys::{map_log_key, LogAction};
use super::terminal::{self, Terminal};
use crate::connection::Connection;
use crate::streaming::{push_log_line, run_stream, StreamOutcome};
use crossterm::event::KeyEventKind;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tokio::time;

pub struct LogViewState {
	pub title: String,
	pub lines: VecDeque<String>,
	pub scroll: usize,
	pub finished: bool,
	pub status: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogViewExit {
	Done,
	AgentRestarted,
}

pub async fn run(
	terminal: &mut Terminal,
	conn: &mut Connection,
	title: &str,
	command: &str,
) -> Result<LogViewExit, String> {
	let shared = Arc::new(Mutex::new(LogViewState {
		title: title.to_string(),
		lines: VecDeque::new(),
		scroll: 0,
		finished: false,
		status: "Running…".to_string(),
	}));

	let stream_state = shared.clone();
	let mut stream_fut = Box::pin(run_stream(
		conn,
		command,
		serde_json::json!({}),
		move |chunk| {
			if let Ok(mut state) = stream_state.lock() {
				push_log_line(&mut state.lines, chunk);
			}
		},
	));

	let mut redraw = time::interval(Duration::from_millis(100));
	redraw.tick().await;

	let mut stream_done = false;

	loop {
		{
			let state = shared.lock().map_err(|e| e.to_string())?;
			terminal
				.draw(|frame| render_log(frame, &state))
				.map_err(|e| e.to_string())?;
		}

		let finished = shared.lock().map_err(|e| e.to_string())?.finished;

		if finished {
			let key = terminal::read_key(Duration::from_millis(250)).await?;
			let Some(key) = key.filter(|k| k.kind == KeyEventKind::Press) else {
				continue;
			};

			if terminal::is_quit(&key) {
				return Ok(LogViewExit::Done);
			}

			let mut state = shared.lock().map_err(|e| e.to_string())?;
			match map_log_key(&key, true) {
				Some(LogAction::Quit) => return Ok(LogViewExit::Done),
				Some(LogAction::ScrollUp) => {
					state.scroll = state.scroll.saturating_sub(1);
				}
				Some(LogAction::ScrollDown) => {
					let max = state.lines.len().saturating_sub(1);
					state.scroll = (state.scroll + 1).min(max);
				}
				None => {}
			}
			continue;
		}

		tokio::select! {
			o = &mut stream_fut, if !stream_done => {
				stream_done = true;
				let restarted = matches!(&o, StreamOutcome::AgentRestarted);
				{
					let mut state = shared.lock().map_err(|e| e.to_string())?;
					match o {
						StreamOutcome::Completed => state.status = "Completed".to_string(),
						StreamOutcome::AgentRestarted => {
							state.status = "Agent restarting…".to_string();
							state.lines.push_back(
								"Waiting for agent to restart…".to_string(),
							);
						}
						StreamOutcome::Failed(e) => state.status = format!("Error: {e}"),
					}
				}
				if restarted {
					time::sleep(Duration::from_secs(2)).await;
					return Ok(LogViewExit::AgentRestarted);
				}
				shared.lock().map_err(|e| e.to_string())?.finished = true;
			}
			_ = redraw.tick() => {}
			key = terminal::read_key(Duration::from_millis(50)) => {
				if let Ok(Some(key)) = key
					&& key.kind == KeyEventKind::Press
					&& terminal::is_quit(&key)
				{
					return Ok(LogViewExit::Done);
				}
			}
		}
	}
}

fn render_log(frame: &mut ratatui::Frame, state: &LogViewState) {
	let area = frame.area();
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints([Constraint::Length(3), Constraint::Min(1), Constraint::Length(1)])
		.split(area);

	let header = Paragraph::new(vec![
		Line::from(vec![
			Span::styled("Command: ", Style::default().fg(Color::DarkGray)),
			Span::raw(state.title.clone()),
		]),
		Line::from(vec![
			Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
			Span::raw(state.status.clone()),
		]),
	])
	.block(
		Block::default()
			.borders(Borders::ALL)
			.title(format!(" {} ", state.title)),
	);
	frame.render_widget(header, chunks[0]);

	let visible: Vec<Line> = state
		.lines
		.iter()
		.skip(state.scroll)
		.take(chunks[1].height.saturating_sub(2) as usize)
		.map(|l| Line::from(l.as_str()))
		.collect();
	frame.render_widget(Paragraph::new(visible).wrap(Wrap { trim: false }), chunks[1]);

	let footer = if state.finished {
		"↑/↓ scroll · q close"
	} else {
		"Streaming… · q cancel"
	};
	frame.render_widget(Paragraph::new(footer), chunks[2]);
}
