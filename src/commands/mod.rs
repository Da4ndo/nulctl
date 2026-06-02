pub mod servers;
mod telemetry;
mod update;
mod version;

use crate::cli::Command;
use crate::connection::Connection;

pub enum Handler {
	Telemetry(crate::cli::TelemetryArgs),
	Update,
	Version,
}

pub fn classify(command: Command) -> Handler {
	match command {
		Command::Telemetry(args) => Handler::Telemetry(args),
		Command::Update => Handler::Update,
		Command::Version => Handler::Version,
		Command::Servers { .. } => unreachable!("servers handled in main"),
	}
}

pub async fn dispatch(command: Command, conn: &mut Connection) -> Result<(), String> {
	match classify(command) {
		Handler::Telemetry(args) => telemetry::run(args, conn).await,
		Handler::Update => update::run(conn).await,
		Handler::Version => version::run(conn).await,
	}
}

#[cfg(test)]
mod tests {
	use super::{classify, Handler};
	use crate::cli::Command;

	#[test]
	fn classify_routes_telemetry() {
		assert!(matches!(
			classify(Command::Telemetry(crate::cli::TelemetryArgs { sub: None })),
			Handler::Telemetry(_)
		));
	}
}
