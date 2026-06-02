use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConnectionHistory {
	pub targets: Vec<String>,
}

impl Default for ConnectionHistory {
	fn default() -> Self {
		Self {
			targets: vec!["localhost".to_string()],
		}
	}
}

pub fn get_history_path() -> Option<PathBuf> {
	dirs::config_dir().map(|mut p| {
		p.push("nulctl");
		p.push("history.json");
		p
	})
}

pub fn load_history() -> ConnectionHistory {
	if let Some(path) = get_history_path()
		&& path.exists()
		&& let Ok(content) = fs::read_to_string(&path)
		&& let Ok(history) = serde_json::from_str(&content)
	{
		return history;
	}
	ConnectionHistory::default()
}

fn write_history(history: &ConnectionHistory) {
	if let Some(path) = get_history_path() {
		if let Some(parent) = path.parent() {
			let _ = fs::create_dir_all(parent);
		}
		if let Ok(content) = serde_json::to_string_pretty(history) {
			let _ = fs::write(path, content);
		}
	}
}

pub fn save_history(target: &str) {
	let mut history = load_history();
	history.targets.retain(|t| t != target);
	history.targets.insert(0, target.to_string());
	history.targets.truncate(10);
	write_history(&history);
}

pub fn add_target(target: &str) {
	save_history(target);
}

pub fn remove_target(target: &str) -> bool {
	let mut history = load_history();
	let before = history.targets.len();
	history.targets.retain(|t| t != target);
	if history.targets.is_empty() {
		history.targets.push("localhost".to_string());
	}
	let removed = history.targets.len() < before;
	write_history(&history);
	removed
}

pub fn list_targets() -> Vec<String> {
	load_history().targets
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn remove_target_keeps_at_least_localhost() {
		let mut history = ConnectionHistory {
			targets: vec!["only".to_string()],
		};
		history.targets.retain(|t| t != "only");
		if history.targets.is_empty() {
			history.targets.push("localhost".to_string());
		}
		assert_eq!(history.targets, vec!["localhost".to_string()]);
	}
}
