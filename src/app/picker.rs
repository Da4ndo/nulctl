use super::keys::{map_picker_key, PickerAction};
use super::terminal::{self, Terminal};
use crate::history;
use crossterm::event::KeyEventKind;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
use std::time::Duration;

pub enum PickerExit {
	Selected(String),
	Quit,
}

pub struct PickerState {
	pub targets: Vec<String>,
	pub selected: usize,
	pub adding: bool,
	pub input: String,
	pub status: Option<String>,
}

pub enum ApplyResult {
	Continue,
	Exit(PickerExit),
}

pub fn new_picker_state(targets: Vec<String>) -> PickerState {
	let mut state = PickerState {
		targets,
		selected: 0,
		adding: false,
		input: String::new(),
		status: None,
	};
	if state.targets.is_empty() {
		state.targets.push("localhost".to_string());
	}
	state
}

pub fn apply_action(state: &mut PickerState, action: PickerAction) -> ApplyResult {
	match action {
		PickerAction::Quit => ApplyResult::Exit(PickerExit::Quit),
		PickerAction::Up => {
			move_selection(state, -1);
			ApplyResult::Continue
		}
		PickerAction::Down => {
			move_selection(state, 1);
			ApplyResult::Continue
		}
		PickerAction::Select => select_target(state),
		PickerAction::Add => {
			state.adding = true;
			state.input.clear();
			state.status = None;
			ApplyResult::Continue
		}
		PickerAction::Delete => {
			delete_selected(state);
			ApplyResult::Continue
		}
		PickerAction::CancelAdd => {
			state.adding = false;
			state.input.clear();
			ApplyResult::Continue
		}
		PickerAction::Backspace => {
			state.input.pop();
			ApplyResult::Continue
		}
		PickerAction::Char(c) => {
			state.input.push(c);
			ApplyResult::Continue
		}
		PickerAction::ConfirmAdd => confirm_add(state),
	}
}

fn move_selection(state: &mut PickerState, delta: i32) {
	if state.targets.is_empty() {
		return;
	}
	if delta < 0 {
		state.selected = state.selected.saturating_sub(1);
	} else {
		state.selected = (state.selected + 1).min(state.targets.len() - 1);
	}
}

fn select_target(state: &PickerState) -> ApplyResult {
	if state.adding || state.targets.is_empty() {
		return ApplyResult::Continue;
	}
	ApplyResult::Exit(PickerExit::Selected(
		state.targets[state.selected].clone(),
	))
}

fn delete_selected(state: &mut PickerState) {
	if state.adding || state.targets.is_empty() {
		return;
	}
	let target = state.targets[state.selected].clone();
	let _ = history::remove_target(&target);
	state.targets = history::list_targets();
	if state.selected >= state.targets.len() {
		state.selected = state.targets.len().saturating_sub(1);
	}
}

fn confirm_add(state: &mut PickerState) -> ApplyResult {
	let target = state.input.trim().to_string();
	if target.is_empty() {
		state.status = Some("Target cannot be empty".to_string());
		return ApplyResult::Continue;
	}
	history::add_target(&target);
	state.targets = history::list_targets();
	state.selected = 0;
	state.adding = false;
	state.input.clear();
	ApplyResult::Continue
}

pub async fn run_tui(terminal: &mut Terminal) -> Result<PickerExit, String> {
	let mut state = new_picker_state(history::list_targets());

	loop {
		terminal
			.draw(|frame| render_picker(frame, &state))
			.map_err(|e| e.to_string())?;

		let key = terminal::read_key(Duration::from_millis(250)).await?;
		let Some(key) = key.filter(|k| k.kind == KeyEventKind::Press) else {
			continue;
		};

		let Some(action) = map_picker_key(&key, state.adding) else {
			continue;
		};

		match apply_action(&mut state, action) {
			ApplyResult::Continue => {}
			ApplyResult::Exit(PickerExit::Quit) => return Ok(PickerExit::Quit),
			ApplyResult::Exit(PickerExit::Selected(target)) => {
				history::save_history(&target);
				return Ok(PickerExit::Selected(target));
			}
		}
	}
}

fn render_picker(frame: &mut ratatui::Frame, state: &PickerState) {
	let area = frame.area();
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(" nulctl · select server ");

	if state.adding {
		let chunks = Layout::default()
			.direction(Direction::Vertical)
			.constraints([Constraint::Length(3), Constraint::Min(1)])
			.split(area);
		let input = Paragraph::new(format!("Target: {}", state.input))
			.block(Block::default().borders(Borders::ALL).title(" Add server "));
		frame.render_widget(input, chunks[0]);
		let help = Paragraph::new("Enter confirm · Esc cancel");
		frame.render_widget(help, chunks[1]);
		return;
	}

	let items: Vec<ListItem> = state
		.targets
		.iter()
		.enumerate()
		.map(|(i, t)| {
			let style = if i == state.selected {
				Style::default()
					.fg(Color::Cyan)
					.add_modifier(Modifier::BOLD)
			} else {
				Style::default()
			};
			ListItem::new(Line::from(Span::styled(t.clone(), style)))
		})
		.collect();

	let list = List::new(items).block(block);
	frame.render_widget(list, area);

	if let Some(status) = &state.status {
		let footer = Paragraph::new(status.as_str()).style(Style::default().fg(Color::Red));
		frame.render_widget(footer, area);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn move_selection_respects_bounds() {
		let mut state = new_picker_state(vec!["a".into(), "b".into(), "c".into()]);
		apply_action(&mut state, PickerAction::Up);
		assert_eq!(state.selected, 0);
		apply_action(&mut state, PickerAction::Down);
		apply_action(&mut state, PickerAction::Down);
		assert_eq!(state.selected, 2);
		apply_action(&mut state, PickerAction::Down);
		assert_eq!(state.selected, 2);
	}

	#[test]
	fn select_returns_target_when_not_adding() {
		let mut state = new_picker_state(vec!["host".into()]);
		let result = apply_action(&mut state, PickerAction::Select);
		assert!(matches!(
			result,
			ApplyResult::Exit(PickerExit::Selected(ref t)) if t == "host"
		));
	}

	#[test]
	fn confirm_add_rejects_empty_input() {
		let mut state = new_picker_state(vec!["localhost".into()]);
		state.adding = true;
		apply_action(&mut state, PickerAction::ConfirmAdd);
		assert_eq!(state.status.as_deref(), Some("Target cannot be empty"));
	}

	#[test]
	fn add_mode_accepts_char_input() {
		let mut state = new_picker_state(vec!["localhost".into()]);
		state.adding = true;
		apply_action(&mut state, PickerAction::Char('x'));
		assert_eq!(state.input, "x");
		apply_action(&mut state, PickerAction::Backspace);
		assert!(state.input.is_empty());
	}

	#[test]
	fn add_action_enters_add_mode() {
		let mut state = new_picker_state(vec!["localhost".into()]);
		apply_action(&mut state, PickerAction::Add);
		assert!(state.adding);
	}

	#[test]
	fn cancel_add_clears_mode() {
		let mut state = new_picker_state(vec!["localhost".into()]);
		state.adding = true;
		state.input = "partial".into();
		apply_action(&mut state, PickerAction::CancelAdd);
		assert!(!state.adding);
		assert!(state.input.is_empty());
	}
}
