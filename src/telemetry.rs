use crate::connection::Connection;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// How far back to load on connect (hours). Older data is fetched on demand.
pub const DEFAULT_INITIAL_HISTORY_HOURS: u64 = 1;
/// Each background/history fetch requests this many seconds before the oldest loaded point.
pub const HISTORY_CHUNK_SECS: u64 = 24 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TelemetryInfo {
	pub oldest_timestamp: Option<u64>,
	pub newest_timestamp: Option<u64>,
	pub snapshot_count: u64,
	pub retention_days: u64,
	pub interval_seconds: u64,
}

impl TelemetryInfo {
	pub fn server_oldest(&self) -> u64 {
		self.oldest_timestamp.unwrap_or(0)
	}

	pub fn loaded_to_oldest(&self, loaded_oldest: u64) -> bool {
		self.oldest_timestamp
			.is_some_and(|oldest| loaded_oldest <= oldest)
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
	pub timestamp: u64,
	pub os: Option<String>,
	pub cpu: Cpu,
	pub gpu: Vec<GpuInfo>,
	pub memory: MemoryStats,
	pub disk: Vec<DiskStats>,
	#[serde(default)]
	pub containers: Vec<DockerContainer>,
	pub uptime_seconds: u64,
	pub load_average: LoadAverage,
	#[serde(default = "default_cached_telemetry_size")]
	pub cached_telemetry_size: String,
}

fn default_cached_telemetry_size() -> String {
	"0 B".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cpu {
	pub usage: f64,
	pub info: Option<CpuInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
	pub model: String,
	pub cores: u32,
	pub threads: u32,
	pub frequency_mhz: u64,
	pub display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
	pub name: String,
	pub usage: f64,
	pub vram_used: String,
	pub vram_total: String,
	pub vram_usage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
	pub usage: f64,
	pub used_gb: f64,
	pub total_gb: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStats {
	pub name: String,
	pub usage: f64,
	pub used_size: String,
	pub total_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadAverage {
	pub one: f64,
	pub five: f64,
	pub fifteen: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
	pub name: String,
	pub status: String,
	pub created: String,
	pub image: String,
	pub ports: String,
}

impl TelemetrySnapshot {
	pub fn format_uptime(&self) -> String {
		let secs = self.uptime_seconds;
		let days = secs / 86400;
		let hours = (secs % 86400) / 3600;
		if days > 0 {
			format!("{}d {}h", days, hours)
		} else if hours > 0 {
			format!("{}h {}m", hours, (secs % 3600) / 60)
		} else {
			format!("{}m", secs / 60)
		}
	}
}

pub async fn fetch_info(conn: &mut Connection) -> Result<TelemetryInfo, String> {
	let res = conn
		.send_command("telemetry.get_info", serde_json::json!({}))
		.await?;
	if res.status != "ok" {
		return Err(agent_error(res));
	}
	let data = res.data.ok_or_else(|| "missing telemetry info".to_string())?;
	serde_json::from_value(data).map_err(|e| e.to_string())
}

pub async fn fetch_latest(
	conn: &mut Connection,
) -> Result<Option<TelemetrySnapshot>, String> {
	let res = conn
		.send_command("telemetry.get_latest", serde_json::json!({}))
		.await?;
	parse_optional_snapshot(res)
}

pub async fn fetch_range(
	conn: &mut Connection,
	since: u64,
	until: u64,
	limit: Option<u64>,
) -> Result<Vec<TelemetrySnapshot>, String> {
	let mut params = serde_json::json!({ "since": since, "until": until });
	if let Some(n) = limit {
		params["limit"] = serde_json::json!(n);
	}
	let res = conn.send_command("telemetry.get_range", params).await?;
	parse_snapshot_list(res)
}

async fn handle_bulk_response(
	conn: &mut Connection,
	hours: u64,
	limit: Option<u64>,
	response: crate::connection::AgentResponse,
) -> Result<Vec<TelemetrySnapshot>, String> {
	if response.status == "ok" {
		return parse_snapshot_list(response);
	}
	if is_unknown_bulk_command(&response) {
		return fetch_bulk_fallback(conn, hours, limit).await;
	}
	Err(agent_error(response))
}

pub async fn fetch_bulk(
	conn: &mut Connection,
	hours: u64,
	limit: Option<u64>,
) -> Result<Vec<TelemetrySnapshot>, String> {
	let params = bulk_params(hours, limit);
	match conn.send_command("telemetry.get_bulk", params).await {
		Ok(response) => handle_bulk_response(conn, hours, limit, response).await,
		Err(e) => Err(e),
	}
}

fn bulk_params(hours: u64, limit: Option<u64>) -> serde_json::Value {
	let mut params = serde_json::json!({ "hours": hours });
	if let Some(n) = limit {
		params["limit"] = serde_json::json!(n);
	}
	params
}

fn is_unknown_bulk_command(res: &crate::connection::AgentResponse) -> bool {
	res.error.as_deref() == Some("Unknown command: telemetry.get_bulk")
}

async fn fetch_bulk_fallback(
	conn: &mut Connection,
	hours: u64,
	limit: Option<u64>,
) -> Result<Vec<TelemetrySnapshot>, String> {
	let now = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap_or_default()
		.as_secs();
	let since = now.saturating_sub(hours * 3600);
	fetch_range(conn, since, now, limit).await
}

fn agent_error(res: crate::connection::AgentResponse) -> String {
	res.error.unwrap_or_else(|| "unknown error".to_string())
}

fn parse_optional_snapshot(
	res: crate::connection::AgentResponse,
) -> Result<Option<TelemetrySnapshot>, String> {
	if res.status != "ok" {
		return Err(agent_error(res));
	}
	match res.data {
		Some(data) => {
			let snapshot: TelemetrySnapshot =
				serde_json::from_value(data).map_err(|e| e.to_string())?;
			Ok(Some(snapshot))
		}
		None => Ok(None),
	}
}

fn parse_snapshot_list(
	res: crate::connection::AgentResponse,
) -> Result<Vec<TelemetrySnapshot>, String> {
	if res.status != "ok" {
		return Err(agent_error(res));
	}
	match res.data {
		Some(data) => {
			let snapshots: Vec<TelemetrySnapshot> =
				serde_json::from_value(data).map_err(|e| e.to_string())?;
			Ok(snapshots)
		}
		None => Ok(vec![]),
	}
}

pub struct OlderFetchSession {
	pub since: u64,
	pub until: u64,
	pub anchor_ts: Option<u64>,
}

pub enum OlderFetchStart {
	Ready(OlderFetchSession),
	Stop(OlderFetchStop),
}

pub enum OlderFetchStop {
	Exhausted,
	NoData,
}

pub fn start_older_fetch(
	history_exhausted: bool,
	oldest_loaded: Option<u64>,
	server_oldest: u64,
	chunk_secs: u64,
	anchor_ts: Option<u64>,
) -> OlderFetchStart {
	match plan_older_fetch(history_exhausted, oldest_loaded, server_oldest, chunk_secs) {
		OlderFetchPlan::Fetch { since, until } => {
			OlderFetchStart::Ready(OlderFetchSession { since, until, anchor_ts })
		}
		OlderFetchPlan::Exhausted => OlderFetchStart::Stop(OlderFetchStop::Exhausted),
		OlderFetchPlan::NoData => OlderFetchStart::Stop(OlderFetchStop::NoData),
	}
}

pub struct OlderFetchFinalize {
	pub extended: bool,
	pub history_exhausted: bool,
	pub pan_offset: Option<usize>,
	pub error: Option<String>,
}

pub fn finalize_older_fetch(
	snapshots: &mut Vec<TelemetrySnapshot>,
	info: Option<&TelemetryInfo>,
	older: Vec<TelemetrySnapshot>,
	anchor_ts: Option<u64>,
) -> OlderFetchFinalize {
	let apply = apply_older_fetch(snapshots, older, info, anchor_ts);
	OlderFetchFinalize {
		extended: apply.extended,
		history_exhausted: apply.history_exhausted,
		pan_offset: apply.pan_offset,
		error: None,
	}
}

pub fn older_fetch_error(message: String) -> OlderFetchFinalize {
	OlderFetchFinalize {
		extended: false,
		history_exhausted: false,
		pan_offset: None,
		error: Some(message),
	}
}

/// Outcome of planning an older-history fetch.
pub enum OlderFetchPlan {
	Fetch { since: u64, until: u64 },
	Exhausted,
	NoData,
}

pub fn plan_older_fetch(
	history_exhausted: bool,
	oldest_loaded: Option<u64>,
	server_oldest: u64,
	chunk_secs: u64,
) -> OlderFetchPlan {
	if history_exhausted {
		return OlderFetchPlan::Exhausted;
	}
	let Some(oldest_loaded) = oldest_loaded else {
		return OlderFetchPlan::NoData;
	};
	match plan_older_range(oldest_loaded, server_oldest, chunk_secs) {
		Some((since, until)) => OlderFetchPlan::Fetch { since, until },
		None => OlderFetchPlan::Exhausted,
	}
}

pub struct OlderFetchApply {
	pub extended: bool,
	pub history_exhausted: bool,
	pub pan_offset: Option<usize>,
}

pub fn apply_older_fetch(
	snapshots: &mut Vec<TelemetrySnapshot>,
	older: Vec<TelemetrySnapshot>,
	info: Option<&TelemetryInfo>,
	anchor_ts: Option<u64>,
) -> OlderFetchApply {
	let extended = merge_older_snapshots(snapshots, older);
	let history_exhausted =
		history_exhausted_after_extend(info, snapshots, extended);
	let pan_offset = anchor_ts
		.and_then(|ts| {
			snapshots
				.iter()
				.position(|s| s.timestamp >= ts)
				.map(|idx| snapshots.len().saturating_sub(idx + 1))
		});
	OlderFetchApply {
		extended,
		history_exhausted,
		pan_offset,
	}
}

/// Range for the next older-history fetch, or `None` when nothing remains.
pub fn plan_older_range(
	oldest_loaded: u64,
	server_oldest: u64,
	chunk_secs: u64,
) -> Option<(u64, u64)> {
	if oldest_loaded <= server_oldest {
		return None;
	}
	let until = oldest_loaded.saturating_sub(1);
	let since = oldest_loaded.saturating_sub(chunk_secs).max(server_oldest);
	Some((since, until))
}

pub fn merge_snapshot(
	snapshots: &mut Vec<TelemetrySnapshot>,
	snapshot: TelemetrySnapshot,
) {
	if snapshots.last().is_some_and(|s| s.timestamp >= snapshot.timestamp) {
		return;
	}
	snapshots.push(snapshot);
}

/// Prepend older snapshots; returns whether any new points were added.
pub fn merge_older_snapshots(
	snapshots: &mut Vec<TelemetrySnapshot>,
	older: Vec<TelemetrySnapshot>,
) -> bool {
	if older.is_empty() {
		return false;
	}
	let cutoff = snapshots.first().map(|s| s.timestamp);
	let mut merged: Vec<TelemetrySnapshot> = older
		.into_iter()
		.filter(|s| cutoff.is_none_or(|c| s.timestamp < c))
		.collect();
	if merged.is_empty() {
		return false;
	}
	merged.append(snapshots);
	merged.sort_by_key(|s| s.timestamp);
	merged.dedup_by_key(|s| s.timestamp);
	*snapshots = merged;
	true
}

pub fn history_exhausted_after_extend(
	info: Option<&TelemetryInfo>,
	snapshots: &[TelemetrySnapshot],
	extended: bool,
) -> bool {
	if !extended {
		return true;
	}
	let Some(oldest_loaded) = snapshots.first().map(|s| s.timestamp) else {
		return true;
	};
	info.is_some_and(|i| i.loaded_to_oldest(oldest_loaded))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn start_and_finalize_older_fetch() {
		let start = start_older_fetch(false, Some(5000), 0, 3600, Some(4800));
		let OlderFetchStart::Ready(session) = start else {
			panic!("expected ready");
		};
		let mut snapshots = vec![sample(4800), sample(5000)];
		let info = TelemetryInfo {
			oldest_timestamp: Some(100),
			newest_timestamp: Some(5000),
			snapshot_count: 2,
			retention_days: 5,
			interval_seconds: 30,
		};
		let done = finalize_older_fetch(
			&mut snapshots,
			Some(&info),
			vec![sample(1400)],
			session.anchor_ts,
		);
		assert!(done.extended);
		assert!(done.error.is_none());
	}

	#[test]
	fn plan_older_fetch_respects_exhausted_and_bounds() {
		assert!(matches!(
			plan_older_fetch(true, Some(5000), 0, 3600),
			OlderFetchPlan::Exhausted
		));
		assert!(matches!(
			plan_older_fetch(false, None, 0, 3600),
			OlderFetchPlan::NoData
		));
		assert!(matches!(
			plan_older_fetch(false, Some(5000), 0, 3600),
			OlderFetchPlan::Fetch { .. }
		));
	}

	#[test]
	fn apply_older_fetch_updates_pan_and_exhausted() {
		let info = TelemetryInfo {
			oldest_timestamp: Some(100),
			newest_timestamp: Some(200),
			snapshot_count: 2,
			retention_days: 5,
			interval_seconds: 30,
		};
		let mut snapshots = vec![sample(150), sample(200)];
		let apply = apply_older_fetch(&mut snapshots, vec![sample(100)], Some(&info), Some(150));
		assert!(apply.extended);
		assert!(apply.history_exhausted);
		assert_eq!(apply.pan_offset, Some(1));
	}

	#[test]
	fn plan_older_range_steps_back_in_chunks() {
		let chunk = 3600;
		assert_eq!(plan_older_range(5000, 0, chunk), Some((1400, 4999)));
		assert_eq!(plan_older_range(1000, 0, chunk), Some((0, 999)));
		assert_eq!(plan_older_range(500, 500, chunk), None);
	}

	#[test]
	fn merge_older_snapshots_prepends_and_dedupes() {
		let mut snapshots = vec![sample(10), sample(20)];
		assert!(merge_older_snapshots(&mut snapshots, vec![sample(5), sample(10)]));
		assert_eq!(snapshots.len(), 3);
		assert_eq!(snapshots[0].timestamp, 5);
	}

	#[test]
	fn merge_older_snapshots_empty_returns_false() {
		let mut snapshots = vec![sample(10)];
		assert!(!merge_older_snapshots(&mut snapshots, vec![]));
		assert!(!merge_older_snapshots(&mut snapshots, vec![sample(10)]));
	}

	#[test]
	fn merge_snapshot_appends_without_trim() {
		let mut snapshots = vec![];
		for ts in 1..=5 {
			merge_snapshot(&mut snapshots, sample(ts));
		}
		assert_eq!(snapshots.len(), 5);
	}

	#[test]
	fn merge_snapshot_skips_duplicate_timestamp() {
		let mut snapshots = vec![sample(1)];
		merge_snapshot(&mut snapshots, sample(1));
		assert_eq!(snapshots.len(), 1);
	}

	#[test]
	fn history_exhausted_when_at_server_oldest() {
		let info = TelemetryInfo {
			oldest_timestamp: Some(100),
			newest_timestamp: Some(200),
			snapshot_count: 2,
			retention_days: 5,
			interval_seconds: 30,
		};
		let snaps = vec![sample(100), sample(200)];
		assert!(history_exhausted_after_extend(Some(&info), &snaps, true));
	}

	fn sample(ts: u64) -> TelemetrySnapshot {
		TelemetrySnapshot {
			timestamp: ts,
			os: None,
			cpu: Cpu { usage: 0.0, info: None },
			gpu: vec![],
			memory: MemoryStats {
				usage: 0.0,
				used_gb: 0.0,
				total_gb: 0.0,
			},
			disk: vec![],
			containers: vec![],
			uptime_seconds: 0,
			load_average: LoadAverage {
				one: 0.0,
				five: 0.0,
				fifteen: 0.0,
			},
			cached_telemetry_size: "0 B".to_string(),
		}
	}
}
