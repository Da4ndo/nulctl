use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::terminal::{
	disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::ExecutableCommand;
use std::io::{self, stdout, Write};
use std::time::Duration;

pub type Terminal = ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>;

pub fn setup_terminal() -> Result<Terminal, String> {
	let mut out = stdout();
	enable_raw_mode().map_err(|e| e.to_string())?;
	out.execute(EnterAlternateScreen)
		.map_err(|e| e.to_string())?;
	let backend = ratatui::backend::CrosstermBackend::new(out);
	ratatui::Terminal::new(backend).map_err(|e| e.to_string())
}

pub fn teardown_terminal() -> Result<(), String> {
	disable_raw_mode().map_err(|e| e.to_string())?;
	if let Ok(out) = io::stdout().execute(LeaveAlternateScreen) {
		let _ = out.flush();
	}
	Ok(())
}

pub fn poll_blocking_event(timeout: Duration) -> Result<Option<Event>, io::Error> {
	if !crossterm::event::poll(timeout)? {
		return Ok(None);
	}
	crossterm::event::read().map(Some)
}

pub async fn read_key(timeout: Duration) -> Result<Option<KeyEvent>, String> {
	let event = tokio::task::spawn_blocking(move || poll_blocking_event(timeout))
		.await
		.map_err(|e| e.to_string())?
		.map_err(|e| e.to_string())?;

	Ok(match event {
		Some(Event::Key(key)) => Some(key),
		_ => None,
	})
}

pub fn is_quit(key: &KeyEvent) -> bool {
	key.code == KeyCode::Char('q')
		|| key.code == KeyCode::Esc
		|| (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn is_quit_detects_q_and_esc() {
		assert!(is_quit(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE)));
		assert!(is_quit(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
		assert!(!is_quit(&KeyEvent::new(KeyCode::Char('m'), KeyModifiers::NONE)));
	}
}
