use crate::cli::{TelemetryArgs, TelemetrySubcommand};
use crate::connection::Connection;
use crate::telemetry::{fetch_bulk, fetch_latest, fetch_range};

pub async fn run(args: TelemetryArgs, conn: &mut Connection) -> Result<(), String> {
	match args.sub {
		None => print_latest(conn).await,
		Some(TelemetrySubcommand::History { hours, limit }) => {
			print_history(conn, hours, limit).await
		}
		Some(TelemetrySubcommand::Range {
			since,
			until,
			limit,
		}) => print_range(conn, since, until, limit).await,
	}
}

async fn print_latest(conn: &mut Connection) -> Result<(), String> {
	match fetch_latest(conn).await? {
		Some(snapshot) => {
			println!("{}", serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?);
			Ok(())
		}
		None => Err("No telemetry data available yet".to_string()),
	}
}

async fn print_history(
	conn: &mut Connection,
	hours: u64,
	limit: Option<u64>,
) -> Result<(), String> {
	let snapshots = fetch_bulk(conn, hours, limit).await?;
	if snapshots.is_empty() {
		return Err("No telemetry history available for the requested period".to_string());
	}
	println!("{}", serde_json::to_string_pretty(&snapshots).map_err(|e| e.to_string())?);
	Ok(())
}

async fn print_range(
	conn: &mut Connection,
	since: u64,
	until: u64,
	limit: Option<u64>,
) -> Result<(), String> {
	let snapshots = fetch_range(conn, since, until, limit).await?;
	if snapshots.is_empty() {
		return Err("No telemetry in range".to_string());
	}
	println!("{}", serde_json::to_string_pretty(&snapshots).map_err(|e| e.to_string())?);
	Ok(())
}

#[cfg(test)]
mod tests {
	#[test]
	fn telemetry_subcommand_defaults_to_latest_via_none() {
		let args = crate::cli::TelemetryArgs { sub: None };
		assert!(args.sub.is_none());
	}
}
