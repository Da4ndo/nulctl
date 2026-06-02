use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerAction {
	Up,
	Down,
	Select,
	Add,
	Delete,
	Quit,
	Char(char),
	Backspace,
	ConfirmAdd,
	CancelAdd,
}

pub fn map_picker_key(key: &KeyEvent, adding: bool) -> Option<PickerAction> {
	if is_quit(key) {
		return Some(PickerAction::Quit);
	}
	if adding {
		return map_add_input_key(key);
	}
	match key.code {
		KeyCode::Up | KeyCode::Char('k') => Some(PickerAction::Up),
		KeyCode::Down | KeyCode::Char('j') => Some(PickerAction::Down),
		KeyCode::Enter => Some(PickerAction::Select),
		KeyCode::Char('a') => Some(PickerAction::Add),
		KeyCode::Char('d') | KeyCode::Delete => Some(PickerAction::Delete),
		_ => None,
	}
}

fn map_add_input_key(key: &KeyEvent) -> Option<PickerAction> {
	match key.code {
		KeyCode::Enter => Some(PickerAction::ConfirmAdd),
		KeyCode::Esc => Some(PickerAction::CancelAdd),
		KeyCode::Backspace => Some(PickerAction::Backspace),
		KeyCode::Char(c) => Some(PickerAction::Char(c)),
		_ => None,
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DashboardAction {
	Quit,
	Switch,
	Update,
	ChartPanLeft,
	ChartPanRight,
	DockerScrollUp,
	DockerScrollDown,
}

pub fn map_dashboard_key(key: &KeyEvent) -> Option<DashboardAction> {
	if is_quit(key) {
		return Some(DashboardAction::Quit);
	}
	match key.code {
		KeyCode::Up | KeyCode::Char('k') => Some(DashboardAction::DockerScrollUp),
		KeyCode::Down | KeyCode::Char('j') => Some(DashboardAction::DockerScrollDown),
		KeyCode::Left | KeyCode::Char('h') => Some(DashboardAction::ChartPanLeft),
		KeyCode::Right | KeyCode::Char('l') => Some(DashboardAction::ChartPanRight),
		KeyCode::Char('u') => Some(DashboardAction::Update),
		KeyCode::Char('s') => Some(DashboardAction::Switch),
		_ => None,
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogAction {
	Quit,
	ScrollUp,
	ScrollDown,
}

pub fn map_log_key(key: &KeyEvent, finished: bool) -> Option<LogAction> {
	if is_quit(key) && finished {
		return Some(LogAction::Quit);
	}
	match key.code {
		KeyCode::Up | KeyCode::Char('k') => Some(LogAction::ScrollUp),
		KeyCode::Down | KeyCode::Char('j') => Some(LogAction::ScrollDown),
		_ => None,
	}
}

fn is_quit(key: &KeyEvent) -> bool {
	key.code == KeyCode::Char('q')
		|| key.code == KeyCode::Esc
		|| (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

	#[test]
	fn dashboard_maps_docker_scroll_keys() {
		assert_eq!(
			map_dashboard_key(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
			Some(DashboardAction::DockerScrollUp)
		);
		assert_eq!(
			map_dashboard_key(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE)),
			Some(DashboardAction::DockerScrollDown)
		);
	}

	#[test]
	fn dashboard_maps_chart_pan_keys() {
		assert_eq!(
			map_dashboard_key(&KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
			Some(DashboardAction::ChartPanLeft)
		);
		assert_eq!(
			map_dashboard_key(&KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
			Some(DashboardAction::ChartPanRight)
		);
	}

	#[test]
	fn dashboard_maps_action_keys() {
		assert_eq!(
			map_dashboard_key(&KeyEvent::new(KeyCode::Char('u'), KeyModifiers::NONE)),
			Some(DashboardAction::Update)
		);
	}

	#[test]
	fn picker_maps_navigation_keys() {
		assert_eq!(
			map_picker_key(&KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE), false),
			Some(PickerAction::Down)
		);
		assert_eq!(
			map_picker_key(&KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE), false),
			Some(PickerAction::Add)
		);
	}

	#[test]
	fn add_input_maps_confirm_and_cancel() {
		assert_eq!(
			map_add_input_key(&KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
			Some(PickerAction::ConfirmAdd)
		);
		assert_eq!(
			map_add_input_key(&KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
			Some(PickerAction::CancelAdd)
		);
	}

	#[test]
	fn log_key_scrolls_when_finished() {
		assert_eq!(
			map_log_key(&KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), true),
			Some(LogAction::ScrollUp)
		);
		assert!(map_log_key(&KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), false).is_none());
	}
}
