use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
	author,
	version,
	about = "NULNET agent CLI",
	long_about = "TUI-first operator CLI for nulnet agents. \
	              Run without subcommands for the live dashboard, \
	              or use subcommands for scripting."
)]
pub struct Cli {
	/// Target agent (e.g. localhost, user@host). Skips server picker when set.
	#[arg(short, long, global = true)]
	pub target: Option<String>,

	#[command(subcommand)]
	pub command: Option<Command>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
	/// Telemetry snapshots (JSON to stdout). Default subcommand: latest.
	Telemetry(TelemetryArgs),
	/// Stream agent self-update from CDN.
	Update,
	/// Print agent version.
	Version,
	/// Manage saved server targets.
	Servers {
		#[command(subcommand)]
		command: ServersCommand,
	},
}

#[derive(clap::Args, Debug)]
pub struct TelemetryArgs {
	#[command(subcommand)]
	pub sub: Option<TelemetrySubcommand>,
}

#[derive(Subcommand, Debug)]
pub enum TelemetrySubcommand {
	/// Bulk history for the last N hours.
	History {
		#[arg(long, default_value = "1")]
		hours: u64,
		#[arg(long)]
		limit: Option<u64>,
	},
	/// Snapshots between Unix timestamps.
	Range {
		#[arg(long)]
		since: u64,
		#[arg(long)]
		until: u64,
		#[arg(long)]
		limit: Option<u64>,
	},
}

#[derive(Subcommand, Debug)]
pub enum ServersCommand {
	/// List saved targets.
	List,
	/// Add a target to history.
	Add {
		target: String,
	},
	/// Remove a target from history.
	Remove {
		target: String,
	},
}
