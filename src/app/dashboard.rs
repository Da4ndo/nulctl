use super::keys::{map_dashboard_key, DashboardAction};
use super::log_view::{LogViewExit, run as run_log_view};
use super::reconnect::{
	decode_poll_result, reconnect_with_retry, PollTick, ReconnectReason,
};
use super::render;
use super::terminal::{self, Terminal};
use crate::auth::fetch_agent_version;
use crate::connection::Connection;
use crate::identity::Identity;
use crate::telemetry::{
	fetch_bulk, fetch_info, fetch_latest, fetch_range, finalize_older_fetch,
	older_fetch_error, start_older_fetch, TelemetryInfo, TelemetrySnapshot,
	DEFAULT_INITIAL_HISTORY_HOURS, HISTORY_CHUNK_SECS, OlderFetchStart, OlderFetchStop,
};
use crate::version::{fetch_latest_agent_version, update_available};
use super::chart::{
	apply_pan_right, is_live, resolve_pan_left_fetch, should_prefetch_older, PanLeftAction,
	PanLeftApply,
};
use crossterm::event::KeyEventKind;
use std::time::Duration;
use tokio::time;

pub struct DashboardOptions {
	pub initial_history_hours: u64,
	pub poll_interval_secs: u64,
}

impl Default for DashboardOptions {
	fn default() -> Self {
		Self {
			initial_history_hours: DEFAULT_INITIAL_HISTORY_HOURS,
			poll_interval_secs: 5,
		}
	}
}

pub struct DashboardState {
	pub target: String,
	pub agent_version: String,
	pub agent_update_available: Option<String>,
	pub telemetry_info: Option<TelemetryInfo>,
	pub snapshots: Vec<TelemetrySnapshot>,
	pub history_exhausted: bool,
	pub chart_pan_from_end: usize,
	pub poll_interval_secs: u64,
	pub status_message: Option<String>,
	pub history_fetch_inflight: bool,
	pub docker_scroll: usize,
	pub docker_viewport_lines: usize,
}

impl DashboardState {
	pub fn latest(&self) -> Option<&TelemetrySnapshot> {
		self.snapshots.last()
	}

	pub fn chart_view(&self) -> &[TelemetrySnapshot] {
		super::chart::visible_snapshots(&self.snapshots, self.chart_pan_from_end)
	}

	fn server_oldest(&self) -> Option<u64> {
		self.telemetry_info.as_ref().and_then(|i| i.oldest_timestamp)
	}
}

pub enum SessionExit {
	Quit,
	SwitchServer,
}

struct InitialLoad {
	snapshots: Vec<TelemetrySnapshot>,
	info: Option<TelemetryInfo>,
}

async fn load_initial(conn: &mut Connection, hours: u64) -> Result<InitialLoad, String> {
	let info = fetch_info(conn).await.ok();
	let snapshots = fetch_bulk(conn, hours, None).await?;
	Ok(InitialLoad { snapshots, info })
}

fn initial_history_exhausted(info: Option<&TelemetryInfo>, snapshots: &[TelemetrySnapshot]) -> bool {
	let Some(loaded_oldest) = snapshots.first().map(|s| s.timestamp) else {
		return true;
	};
	info.is_some_and(|i| i.loaded_to_oldest(loaded_oldest))
}

pub async fn run(
	terminal: &mut Terminal,
	conn: &mut Connection,
	target: &str,
	identity: &Identity,
	agent_version: String,
	agent_update_available: Option<String>,
	opts: DashboardOptions,
) -> Result<SessionExit, String> {
	let InitialLoad { snapshots, info } = super::loading::while_loading(
		terminal,
		target,
		"Loading telemetry history…",
		load_initial(conn, opts.initial_history_hours),
	)
	.await?;

	let history_exhausted = initial_history_exhausted(info.as_ref(), &snapshots);

	let mut state = DashboardState {
		target: target.to_string(),
		agent_version,
		agent_update_available,
		telemetry_info: info,
		snapshots,
		history_exhausted,
		chart_pan_from_end: 0,
		poll_interval_secs: opts.poll_interval_secs,
		status_message: None,
		history_fetch_inflight: false,
		docker_scroll: 0,
		docker_viewport_lines: 0,
	};

	let mut poll_interval = time::interval(Duration::from_secs(opts.poll_interval_secs));
	poll_interval.tick().await;

	loop {
		terminal
			.draw(|frame| render::render(frame, &mut state))
			.map_err(|e| e.to_string())?;

		tokio::select! {
			_ = poll_interval.tick() => {
				poll_telemetry(
					terminal, conn, target, identity, &mut state, &opts,
				)
				.await?;
				maybe_prefetch_older(conn, &mut state).await;
			}
			key = terminal::read_key(Duration::from_millis(250)) => {
				let Some(key) = key?.filter(|k| k.kind == KeyEventKind::Press) else {
					continue;
				};
				if terminal::is_quit(&key) {
					return Ok(SessionExit::Quit);
				}
				match map_dashboard_key(&key) {
					Some(DashboardAction::Quit) => return Ok(SessionExit::Quit),
					Some(DashboardAction::Switch) => return Ok(SessionExit::SwitchServer),
					Some(DashboardAction::ChartPanLeft) => {
						handle_pan_left(conn, &mut state).await;
					}
					Some(DashboardAction::ChartPanRight) => {
						handle_pan_right(&mut state);
					}
					Some(DashboardAction::DockerScrollUp) => {
						handle_docker_scroll_up(&mut state);
					}
					Some(DashboardAction::DockerScrollDown) => {
						handle_docker_scroll_down(&mut state);
					}
					Some(DashboardAction::Update) => {
						match run_log_view(
							terminal, conn, "update", "agent.update",
						)
						.await?
						{
							LogViewExit::AgentRestarted => {
								reconnect_and_reload(
									terminal,
									conn,
									target,
									identity,
									&mut state,
									&opts,
									ReconnectReason::AfterUpdate,
								)
								.await?;
							}
							LogViewExit::Done => {
								refresh_version_warning(conn, &mut state).await;
							}
						}
					}
					None => {}
				}
			}
		}
	}
}

async fn handle_pan_left(conn: &mut Connection, state: &mut DashboardState) {
	match super::chart::plan_pan_left(
		&state.snapshots,
		state.chart_pan_from_end,
		state.history_fetch_inflight,
	) {
		PanLeftAction::MoveTo(offset) => {
			set_pan(state, offset);
			maybe_prefetch_older(conn, state).await;
		}
		PanLeftAction::LoadOlder => {
			let pan = state.chart_pan_from_end;
			let fetch = extend_history_quiet(conn, state).await;
			let apply = resolve_pan_left_fetch(&state.snapshots, pan, fetch);
			apply_pan_left_result(state, apply);
		}
		PanLeftAction::WaitForLoad => {
			state.status_message =
				Some("Loading older telemetry…".to_string());
		}
		PanLeftAction::None => {}
	}
}

fn apply_pan_left_result(state: &mut DashboardState, apply: PanLeftApply) {
	match apply {
		PanLeftApply::SetPan(offset) => set_pan(state, offset),
		PanLeftApply::SetStatus(msg) => state.status_message = Some(msg),
	}
}

fn apply_poll_snapshot(state: &mut DashboardState, snapshot: TelemetrySnapshot) {
	let was_live = is_live(state.chart_pan_from_end);
	crate::telemetry::merge_snapshot(&mut state.snapshots, snapshot);
	if was_live {
		state.chart_pan_from_end = 0;
	}
	if !state.history_fetch_inflight {
		state.status_message = None;
	}
}

fn handle_docker_scroll_up(state: &mut DashboardState) {
	if state.latest().is_none_or(|s| s.containers.is_empty()) {
		return;
	}
	state.docker_scroll = super::docker_panel::docker_scroll_up(state.docker_scroll);
}

fn handle_docker_scroll_down(state: &mut DashboardState) {
	let Some(snapshot) = state.latest() else {
		return;
	};
	if snapshot.containers.is_empty() {
		return;
	}
	let line_count = super::docker_panel::docker_lines(&snapshot.containers).len();
	let viewport = state.docker_viewport_lines.max(1);
	let max_scroll = super::docker_panel::docker_max_scroll(line_count, viewport);
	state.docker_scroll = super::docker_panel::docker_scroll_down(
		state.docker_scroll,
		max_scroll,
	);
}

fn handle_pan_right(state: &mut DashboardState) {
	set_pan(
		state,
		apply_pan_right(&state.snapshots, state.chart_pan_from_end),
	);
}

fn set_pan(state: &mut DashboardState, offset: usize) {
	state.chart_pan_from_end = offset;
	if !state.history_fetch_inflight {
		state.status_message = None;
	}
}

async fn maybe_prefetch_older(conn: &mut Connection, state: &mut DashboardState) {
	if !should_prefetch_older(
		&state.snapshots,
		state.chart_pan_from_end,
		state.history_exhausted,
		state.server_oldest(),
		state.history_fetch_inflight,
	) {
		return;
	}
	let _ = extend_history_quiet(conn, state).await;
}

fn prepare_older_fetch(state: &mut DashboardState) -> Option<crate::telemetry::OlderFetchSession> {
	match start_older_fetch(
		state.history_exhausted,
		state.snapshots.first().map(|s| s.timestamp),
		state
			.telemetry_info
			.as_ref()
			.map(|i| i.server_oldest())
			.unwrap_or(0),
		HISTORY_CHUNK_SECS,
		pan_anchor_timestamp(state),
	) {
		OlderFetchStart::Ready(session) => Some(session),
		OlderFetchStart::Stop(OlderFetchStop::Exhausted) => {
			state.history_exhausted = true;
			None
		}
		OlderFetchStart::Stop(OlderFetchStop::NoData) => None,
	}
}

fn set_history_loading(state: &mut DashboardState) {
	state.history_fetch_inflight = true;
	if state.status_message.is_none() {
		state.status_message = Some("Loading older telemetry…".to_string());
	}
}

fn apply_older_fetch_done(
	state: &mut DashboardState,
	done: crate::telemetry::OlderFetchFinalize,
) -> Result<bool, String> {
	if let Some(err) = done.error {
		state.status_message = Some(format!("History load error: {err}"));
		return Err(err);
	}
	state.history_exhausted = done.history_exhausted;
	if let Some(offset) = done.pan_offset {
		state.chart_pan_from_end = offset;
	}
	state.status_message = None;
	Ok(done.extended)
}

async fn fetch_older_chunk(
	conn: &mut Connection,
	session: &crate::telemetry::OlderFetchSession,
	snapshots: &mut Vec<TelemetrySnapshot>,
	info: Option<&TelemetryInfo>,
) -> crate::telemetry::OlderFetchFinalize {
	match fetch_range(conn, session.since, session.until, None).await {
		Ok(older) => finalize_older_fetch(
			snapshots,
			info,
			older,
			session.anchor_ts,
		),
		Err(e) => older_fetch_error(e),
	}
}

async fn extend_history_quiet(
	conn: &mut Connection,
	state: &mut DashboardState,
) -> Result<bool, String> {
	let Some(session) = prepare_older_fetch(state) else {
		return Ok(false);
	};
	set_history_loading(state);
	let info = state.telemetry_info.as_ref();
	let done = fetch_older_chunk(
		conn,
		&session,
		&mut state.snapshots,
		info,
	)
	.await;
	state.history_fetch_inflight = false;
	apply_older_fetch_done(state, done)
}

fn pan_anchor_timestamp(state: &DashboardState) -> Option<u64> {
	if is_live(state.chart_pan_from_end) {
		return None;
	}
	state.chart_view().last().map(|s| s.timestamp)
}

async fn reload_session(
	terminal: &mut Terminal,
	conn: &mut Connection,
	target: &str,
	state: &mut DashboardState,
	opts: &DashboardOptions,
) -> Result<(), String> {
	let (agent_version, latest) = super::loading::while_loading(
		terminal,
		target,
		"Fetching agent version…",
		async {
			let agent = fetch_agent_version(conn).await;
			let latest = fetch_latest_agent_version().await;
			Ok((agent, latest))
		},
	)
	.await?;

	state.agent_version = agent_version;
	state.agent_update_available =
		update_available(&state.agent_version, latest.as_deref());
	state.chart_pan_from_end = 0;
	state.status_message = None;
	state.history_fetch_inflight = false;
	state.history_exhausted = false;

	let InitialLoad { snapshots, info } = super::loading::while_loading(
		terminal,
		target,
		"Loading telemetry history…",
		load_initial(conn, opts.initial_history_hours),
	)
	.await?;

	state.telemetry_info = info;
	state.snapshots = snapshots;
	state.history_exhausted = initial_history_exhausted(
		state.telemetry_info.as_ref(),
		&state.snapshots,
	);
	Ok(())
}

async fn reconnect_and_reload(
	terminal: &mut Terminal,
	conn: &mut Connection,
	target: &str,
	identity: &Identity,
	state: &mut DashboardState,
	opts: &DashboardOptions,
	reason: ReconnectReason,
) -> Result<(), String> {
	*conn = reconnect_with_retry(terminal, target, identity, reason).await?;
	reload_session(terminal, conn, target, state, opts).await
}

async fn poll_telemetry(
	terminal: &mut Terminal,
	conn: &mut Connection,
	target: &str,
	identity: &Identity,
	state: &mut DashboardState,
	opts: &DashboardOptions,
) -> Result<(), String> {
	match decode_poll_result(fetch_latest(conn).await) {
		PollTick::Snapshot(snapshot) => {
			apply_poll_snapshot(state, snapshot);
			Ok(())
		}
		PollTick::Waiting => {
			state.status_message =
				Some("No telemetry yet — waiting for agent snapshot".to_string());
			Ok(())
		}
		PollTick::ConnectionLost => {
			reconnect_and_reload(
				terminal,
				conn,
				target,
				identity,
				state,
				opts,
				ReconnectReason::ConnectionLost,
			)
			.await
		}
		PollTick::Failed(e) => {
			state.status_message = Some(format!("Poll error: {e}"));
			Ok(())
		}
	}
}

async fn refresh_version_warning(conn: &mut Connection, state: &mut DashboardState) {
	state.agent_version = fetch_agent_version(conn).await;
	let latest = fetch_latest_agent_version().await;
	state.agent_update_available =
		update_available(&state.agent_version, latest.as_deref());
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::telemetry::{older_fetch_error, OlderFetchFinalize, TelemetryInfo};

	fn sample_state(snapshots: Vec<TelemetrySnapshot>) -> DashboardState {
		DashboardState {
			target: "test".to_string(),
			agent_version: "1.0.0".to_string(),
			agent_update_available: None,
			telemetry_info: Some(TelemetryInfo {
				oldest_timestamp: Some(100),
				newest_timestamp: Some(5000),
				snapshot_count: 2,
				retention_days: 5,
				interval_seconds: 30,
			}),
			snapshots,
			history_exhausted: false,
			chart_pan_from_end: 0,
			poll_interval_secs: 5,
			status_message: None,
			history_fetch_inflight: false,
			docker_scroll: 0,
			docker_viewport_lines: 0,
		}
	}

	#[test]
	fn prepare_older_fetch_returns_session_when_more_exists() {
		let mut state = sample_state(vec![
			TelemetrySnapshot {
				timestamp: 5000,
				os: None,
				cpu: crate::telemetry::Cpu { usage: 0.0, info: None },
				gpu: vec![],
				memory: crate::telemetry::MemoryStats {
					usage: 0.0,
					used_gb: 0.0,
					total_gb: 0.0,
				},
				disk: vec![],
				containers: vec![],
				uptime_seconds: 0,
				load_average: crate::telemetry::LoadAverage {
					one: 0.0,
					five: 0.0,
					fifteen: 0.0,
				},
				cached_telemetry_size: "0 B".to_string(),
			},
		]);
		assert!(prepare_older_fetch(&mut state).is_some());
	}

	#[test]
	fn apply_older_fetch_done_surfaces_errors() {
		let mut state = sample_state(vec![]);
		let err = apply_older_fetch_done(&mut state, older_fetch_error("boom".into()));
		assert!(err.is_err());
		assert!(state.status_message.as_deref().unwrap().contains("boom"));
	}

	#[test]
	fn apply_older_fetch_done_clears_status_on_success() {
		let mut state = sample_state(vec![]);
		state.status_message = Some("Loading older telemetry…".to_string());
		let ok = apply_older_fetch_done(
			&mut state,
			OlderFetchFinalize {
				extended: true,
				history_exhausted: false,
				pan_offset: None,
				error: None,
			},
		);
		assert!(ok.unwrap());
		assert!(state.status_message.is_none());
	}
}
