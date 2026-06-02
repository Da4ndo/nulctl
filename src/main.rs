use clap::Parser;
use colored::Colorize;
use nulctl::cli::{Cli, Command};
use nulctl::identity::{self, Identity, IdentityOutcome};
use nulctl::{app, auth, commands};

#[tokio::main]
async fn main() {
	let cli = Cli::parse();

	match cli.command {
		Some(Command::Servers { command }) => {
			if let Err(e) = commands::servers::run(command) {
				eprintln!("{} {}", "✗".red().bold(), e);
				std::process::exit(1);
			}
		}
		Some(cmd) => {
			let target = cli.target.as_deref().unwrap_or("localhost");
			let (identity, outcome) = match Identity::load_or_generate() {
				Ok(pair) => pair,
				Err(e) => {
					eprintln!("{} {}", "✗".red().bold(), e);
					std::process::exit(1);
				}
			};
			if outcome == IdentityOutcome::Created {
				identity.eprint_setup_instructions();
				return;
			}
			let mut conn = match auth::connect_and_auth(target, &identity).await {
				Ok(c) => c,
				Err(e) => {
					eprintln!("{} {}", "✗".red().bold(), identity::auth_error_hint(&e, &identity));
					std::process::exit(1);
				}
			};

			if let Err(e) = commands::dispatch(cmd, &mut conn).await {
				eprintln!("{} {}", "✗".red().bold(), e);
				std::process::exit(1);
			}
		}
		None => {
			if let Err(e) = app::run(cli.target).await {
				eprintln!("{} {}", "✗".red().bold(), e);
				std::process::exit(1);
			}
		}
	}
}
