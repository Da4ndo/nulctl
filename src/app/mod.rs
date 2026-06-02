mod chart;
mod dashboard;
mod docker_panel;
mod keys;
mod loading;
mod log_view;
mod picker;
mod reconnect;
mod render;
mod terminal;

pub use dashboard::{DashboardOptions, SessionExit};

use crate::auth::fetch_agent_version;
use crate::identity::{self, Identity, IdentityOutcome};
use dashboard::SessionExit as DashExit;
use crate::version::{fetch_latest_agent_version, update_available};
use picker::PickerExit;
use terminal::{setup_terminal, teardown_terminal, Terminal};

pub async fn run(cli_target: Option<String>) -> Result<(), String> {
	let (identity, outcome) = Identity::load_or_generate()?;
	if outcome == IdentityOutcome::Created {
		identity.eprint_setup_instructions();
		return Ok(());
	}

	let mut terminal = setup_terminal()?;
	let result = run_session(&mut terminal, cli_target, &identity).await;
	teardown_terminal()?;
	result
}

async fn run_session(
	terminal: &mut Terminal,
	cli_target: Option<String>,
	identity: &Identity,
) -> Result<(), String> {
	let mut pinned = cli_target;

	loop {
		let target = if let Some(t) = pinned.take() {
			t
		} else {
			match picker::run_tui(terminal).await? {
				PickerExit::Quit => return Ok(()),
				PickerExit::Selected(t) => t,
			}
		};

		match connect_dashboard(terminal, &target, identity).await? {
			DashExit::Quit => return Ok(()),
			DashExit::SwitchServer => continue,
		}
	}
}

async fn connect_dashboard(
	terminal: &mut Terminal,
	target: &str,
	identity: &Identity,
) -> Result<DashExit, String> {
	let mut conn = match loading::while_loading(
		terminal,
		target,
		"Connecting and authenticating…",
		crate::auth::connect_and_auth(target, identity),
	)
	.await
	{
		Ok(c) => c,
		Err(e) => {
			let message = identity::auth_error_hint(&e, identity);
			show_connect_error(terminal, target, &message).await?;
			return Ok(DashExit::SwitchServer);
		}
	};

	crate::history::save_history(target);

	let (agent_version, latest) = loading::while_loading(
		terminal,
		target,
		"Fetching agent version…",
		async {
			let agent = fetch_agent_version(&mut conn).await;
			let latest = fetch_latest_agent_version().await;
			Ok((agent, latest))
		},
	)
	.await?;

	let agent_update_available =
		update_available(&agent_version, latest.as_deref());

	dashboard::run(
		terminal,
		&mut conn,
		target,
		identity,
		agent_version,
		agent_update_available,
		DashboardOptions::default(),
	)
	.await
}

async fn show_connect_error(
	terminal: &mut Terminal,
	target: &str,
	error: &str,
) -> Result<(), String> {
	use crossterm::event::KeyEventKind;
	use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
	use std::time::Duration;

	loop {
		terminal
			.draw(|frame| {
				let msg = format!(
					"Failed to connect to {target}\n\n{error}\n\nPress any key to return…"
				);
				frame.render_widget(
					Paragraph::new(msg)
						.wrap(Wrap { trim: true })
						.block(
							Block::default()
								.borders(Borders::ALL)
								.title(" Connection error "),
						),
					frame.area(),
				);
			})
			.map_err(|e| e.to_string())?;

		let key = terminal::read_key(Duration::from_millis(250)).await?;
		if key.filter(|k| k.kind == KeyEventKind::Press).is_some() {
			return Ok(());
		}
	}
}
