use super::terminal::Terminal;
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use std::future::Future;
use std::time::Duration;
use tokio::time;

const SPINNER: [&str; 4] = ["⠋", "⠙", "⠹", "⠸"];

pub async fn while_loading<F, T>(
	terminal: &mut Terminal,
	target: &str,
	message: &str,
	fut: F,
) -> Result<T, String>
where
	F: Future<Output = Result<T, String>>,
{
	let mut fut = Box::pin(fut);
	let mut frame = 0usize;
	let mut tick = time::interval(Duration::from_millis(120));
	tick.tick().await;

	loop {
		terminal
			.draw(|f| render_loading(f, target, message, frame))
			.map_err(|e| e.to_string())?;

		tokio::select! {
			result = fut.as_mut() => return result,
			_ = tick.tick() => frame = frame.wrapping_add(1),
		}
	}
}

fn render_loading(frame: &mut ratatui::Frame, target: &str, message: &str, tick: usize) {
	let area = frame.area();
	let spinner = SPINNER[tick % SPINNER.len()];
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(" nulctl · connecting ");

	let inner = Layout::default()
		.direction(Direction::Vertical)
		.constraints([
			Constraint::Length(1),
			Constraint::Length(1),
			Constraint::Length(1),
			Constraint::Min(1),
		])
		.margin(2)
		.split(block.inner(area));

	frame.render_widget(block, area);
	frame.render_widget(
		Paragraph::new(Line::from(vec![
			Span::styled(format!("{spinner} "), Style::default().fg(Color::Cyan)),
			Span::styled(message, Style::default().add_modifier(Modifier::BOLD)),
		]))
		.alignment(Alignment::Center),
		inner[1],
	);
	frame.render_widget(
		Paragraph::new(target).alignment(Alignment::Center),
		inner[2],
	);
}
