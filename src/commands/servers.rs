use crate::cli::ServersCommand;
use crate::history;

pub fn run(command: ServersCommand) -> Result<(), String> {
	match command {
		ServersCommand::List => {
			for target in history::list_targets() {
				println!("{target}");
			}
			Ok(())
		}
		ServersCommand::Add { target } => {
			if target.trim().is_empty() {
				return Err("Target cannot be empty".to_string());
			}
			history::add_target(target.trim());
			Ok(())
		}
		ServersCommand::Remove { target } => {
			if !history::remove_target(&target) {
				return Err(format!("Target not found: {target}"));
			}
			Ok(())
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn list_command_is_ok() {
		assert!(run(ServersCommand::List).is_ok());
	}

	#[test]
	fn add_rejects_empty() {
		assert!(run(ServersCommand::Add {
			target: "  ".to_string()
		})
		.is_err());
	}
}
