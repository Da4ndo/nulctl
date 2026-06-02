use crate::telemetry::TelemetrySnapshot;
use ratatui::style::Color;

pub struct MetricDef {
	pub label: &'static str,
	pub color: Color,
}

pub const CHART_METRICS: [MetricDef; 4] = [
	MetricDef { label: "CPU", color: Color::Green },
	MetricDef { label: "RAM", color: Color::Cyan },
	MetricDef { label: "GPU", color: Color::Magenta },
	MetricDef { label: "DISK", color: Color::Yellow },
];

pub struct ActiveMetric {
	pub def: &'static MetricDef,
	pub usage_fn: fn(&TelemetrySnapshot) -> f64,
}

pub fn has_gpu(snapshots: &[TelemetrySnapshot]) -> bool {
	snapshots.iter().any(|s| !s.gpu.is_empty())
}

pub fn active_chart_metrics(snapshots: &[TelemetrySnapshot]) -> Vec<ActiveMetric> {
	let mut metrics = vec![
		ActiveMetric {
			def: &CHART_METRICS[0],
			usage_fn: cpu_usage,
		},
		ActiveMetric {
			def: &CHART_METRICS[1],
			usage_fn: memory_usage,
		},
	];
	if has_gpu(snapshots) {
		metrics.push(ActiveMetric {
			def: &CHART_METRICS[2],
			usage_fn: gpu_usage,
		});
	}
	metrics.push(ActiveMetric {
		def: &CHART_METRICS[3],
		usage_fn: disk_usage,
	});
	metrics
}

pub const CHART_WINDOW_SECS: u64 = 3600;
pub const CHART_PAN_STEP_SECS: u64 = 900;

pub enum PanLeftResult {
	Moved,
	NeedOlderHistory,
	AtLimit,
}

pub fn visible_range(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
) -> (usize, usize) {
	let len = snapshots.len();
	if len == 0 {
		return (0, 0);
	}
	let end = len.saturating_sub(pan_from_end).max(1).min(len);
	let end_ts = snapshots[end - 1].timestamp;
	let min_ts = end_ts.saturating_sub(CHART_WINDOW_SECS);
	let mut start = end.saturating_sub(1);
	while start > 0 && snapshots[start - 1].timestamp >= min_ts {
		start -= 1;
	}
	(start, end)
}

pub fn visible_snapshots(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
) -> &[TelemetrySnapshot] {
	let (start, end) = visible_range(snapshots, pan_from_end);
	&snapshots[start..end]
}

pub fn is_live(pan_from_end: usize) -> bool {
	pan_from_end == 0
}

pub fn max_pan_offset(len: usize) -> usize {
	len.saturating_sub(1)
}

pub fn pan_left(len: usize, pan_from_end: usize) -> PanLeftResult {
	if len <= 1 {
		return PanLeftResult::AtLimit;
	}
	let max_pan = max_pan_offset(len);
	if pan_from_end >= max_pan {
		PanLeftResult::NeedOlderHistory
	} else {
		PanLeftResult::Moved
	}
}

pub fn pan_left_new_offset(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
) -> usize {
	let len = snapshots.len();
	if len == 0 {
		return 0;
	}
	let end_idx = len
		.saturating_sub(pan_from_end)
		.max(1)
		.min(len)
		- 1;
	let target_ts = snapshots[end_idx]
		.timestamp
		.saturating_sub(CHART_PAN_STEP_SECS);
	pan_offset_for_timestamp(snapshots, target_ts)
}

pub fn pan_offset_for_timestamp(
	snapshots: &[TelemetrySnapshot],
	timestamp: u64,
) -> usize {
	let len = snapshots.len();
	if len == 0 {
		return 0;
	}
	let idx = snapshots
		.iter()
		.position(|s| s.timestamp >= timestamp)
		.unwrap_or(len.saturating_sub(1));
	len.saturating_sub(idx + 1)
}

pub fn apply_pan_right(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
) -> usize {
	if pan_from_end == 0 {
		return 0;
	}
	let len = snapshots.len();
	let end_idx = len
		.saturating_sub(pan_from_end)
		.max(1)
		.min(len)
		- 1;
	let target_ts = snapshots[end_idx].timestamp + CHART_PAN_STEP_SECS;
	let offset = pan_offset_for_timestamp(snapshots, target_ts);
	if offset == 0 {
		0
	} else {
		offset
	}
}

pub fn should_prefetch_older(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
	history_exhausted: bool,
	server_oldest: Option<u64>,
	inflight: bool,
) -> bool {
	if inflight || history_exhausted || snapshots.len() < 2 {
		return false;
	}
	let (start, _) = visible_range(snapshots, pan_from_end);
	if start != 0 {
		return false;
	}
	let Some(loaded_oldest) = snapshots.first().map(|s| s.timestamp) else {
		return false;
	};
	server_oldest.is_some_and(|oldest| loaded_oldest > oldest)
}

pub enum PanLeftAction {
	MoveTo(usize),
	LoadOlder,
	WaitForLoad,
	None,
}

pub fn plan_pan_left(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
	inflight: bool,
) -> PanLeftAction {
	let len = snapshots.len();
	match pan_left(len, pan_from_end) {
		PanLeftResult::Moved => {
			PanLeftAction::MoveTo(pan_left_new_offset(snapshots, pan_from_end))
		}
		PanLeftResult::NeedOlderHistory if inflight => PanLeftAction::WaitForLoad,
		PanLeftResult::NeedOlderHistory => PanLeftAction::LoadOlder,
		PanLeftResult::AtLimit => PanLeftAction::None,
	}
}

pub enum PanLeftApply {
	SetPan(usize),
	SetStatus(String),
}

pub fn resolve_pan_left_fetch(
	snapshots: &[TelemetrySnapshot],
	pan_from_end: usize,
	fetch: Result<bool, String>,
) -> PanLeftApply {
	match fetch {
		Ok(true) => {
			let offset = match plan_pan_left(snapshots, pan_from_end, false) {
				PanLeftAction::MoveTo(o) => o,
				_ => pan_left_new_offset(snapshots, pan_from_end),
			};
			PanLeftApply::SetPan(offset)
		}
		Ok(false) => PanLeftApply::SetStatus(
			"No more telemetry history available".to_string(),
		),
		Err(e) => PanLeftApply::SetStatus(format!("History load error: {e}")),
	}
}

pub fn build_usage_points(
	snapshots: &[TelemetrySnapshot],
	usage_fn: fn(&TelemetrySnapshot) -> f64,
) -> Vec<(f64, f64)> {
	if snapshots.is_empty() {
		return vec![];
	}
	let last = snapshots.len().saturating_sub(1) as f64;
	snapshots
		.iter()
		.enumerate()
		.map(|(i, snapshot)| {
			let x = if last == 0.0 { 0.0 } else { i as f64 / last * 100.0 };
			(x, usage_fn(snapshot).clamp(0.0, 100.0))
		})
		.collect()
}

pub fn cpu_usage(snapshot: &TelemetrySnapshot) -> f64 {
	snapshot.cpu.usage
}

pub fn memory_usage(snapshot: &TelemetrySnapshot) -> f64 {
	snapshot.memory.usage
}

pub fn gpu_usage(snapshot: &TelemetrySnapshot) -> f64 {
	snapshot.gpu.first().map(|g| g.usage).unwrap_or(0.0)
}

pub fn disk_usage(snapshot: &TelemetrySnapshot) -> f64 {
	snapshot
		.disk
		.iter()
		.map(|d| d.usage)
		.max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
		.unwrap_or(0.0)
}

pub fn time_span_label(snapshots: &[TelemetrySnapshot]) -> String {
	match (snapshots.first(), snapshots.last()) {
		(Some(first), Some(last)) if first.timestamp != last.timestamp => {
			format!(
				"{} → {}",
				format_timestamp(first.timestamp),
				format_timestamp(last.timestamp)
			)
		}
		(Some(only), _) => format_timestamp(only.timestamp),
		_ => "no data".to_string(),
	}
}

pub fn format_timestamp(ts: u64) -> String {
	let (year, month, day, hour, minute, second) = unix_to_civil(ts);
	format!("{month:02}/{day:02}/{year} {hour:02}:{minute:02}:{second:02}")
}

fn unix_to_civil(ts: u64) -> (u64, u64, u64, u64, u64, u64) {
	let days = ts / 86_400;
	let tod = ts % 86_400;
	let hour = tod / 3_600;
	let minute = (tod % 3_600) / 60;
	let second = tod % 60;

	let mut y = 1970u64;
	let mut remaining = days;

	loop {
		let year_days = if is_leap(y) { 366 } else { 365 };
		if remaining < year_days {
			break;
		}
		remaining -= year_days;
		y += 1;
	}

	let leap = is_leap(y);
	let month_days = if leap {
		[31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	} else {
		[31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
	};

	let mut m = 1u64;
	for &md in &month_days {
		if remaining < md {
			break;
		}
		remaining -= md;
		m += 1;
	}

	(y, m, remaining + 1, hour, minute, second)
}

fn is_leap(year: u64) -> bool {
	year.is_multiple_of(4) && !year.is_multiple_of(100) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::telemetry::{Cpu, DiskStats, GpuInfo, LoadAverage, MemoryStats, TelemetrySnapshot};

	fn sample_snapshot(ts: u64, cpu: f64, mem: f64) -> TelemetrySnapshot {
		TelemetrySnapshot {
			timestamp: ts,
			os: Some("Test OS".to_string()),
			cpu: Cpu { usage: cpu, info: None },
			gpu: vec![GpuInfo {
				name: "GPU".to_string(),
				usage: cpu + 5.0,
				vram_used: "1 GB".to_string(),
				vram_total: "8 GB".to_string(),
				vram_usage: 12.5,
			}],
			memory: MemoryStats {
				usage: mem,
				used_gb: 4.0,
				total_gb: 8.0,
			},
			disk: vec![DiskStats {
				name: "/dev/sda1".to_string(),
				usage: mem + 10.0,
				used_size: "50 GB".to_string(),
				total_size: "100 GB".to_string(),
			}],
			containers: vec![],
			uptime_seconds: 3600,
			load_average: LoadAverage {
				one: 1.0,
				five: 1.0,
				fifteen: 1.0,
			},
			cached_telemetry_size: "0 B".to_string(),
		}
	}

	#[test]
	fn build_usage_points_spans_zero_to_hundred_on_x() {
		let snapshots = vec![
			sample_snapshot(100, 10.0, 20.0),
			sample_snapshot(200, 30.0, 40.0),
			sample_snapshot(300, 50.0, 60.0),
		];
		let points = build_usage_points(&snapshots, cpu_usage);
		assert_eq!(points.len(), 3);
		assert!((points[0].0 - 0.0).abs() < f64::EPSILON);
		assert!((points[2].0 - 100.0).abs() < f64::EPSILON);
	}

	#[test]
	fn disk_usage_uses_max_across_disks() {
		let mut snap = sample_snapshot(1, 0.0, 0.0);
		snap.disk.push(DiskStats {
			name: "/dev/sdb1".to_string(),
			usage: 88.0,
			used_size: "1 GB".to_string(),
			total_size: "2 GB".to_string(),
		});
		assert!((disk_usage(&snap) - 88.0).abs() < f64::EPSILON);
	}

	#[test]
	fn time_span_label_formats_range() {
		let snapshots = vec![
			sample_snapshot(1_700_000_000, 1.0, 2.0),
			sample_snapshot(1_700_003_600, 3.0, 4.0),
		];
		let label = time_span_label(&snapshots);
		assert!(label.contains('→'));
	}

	#[test]
	fn format_timestamp_includes_date() {
		assert_eq!(format_timestamp(3_661), "01/01/1970 01:01:01");
	}

	#[test]
	fn has_gpu_false_when_empty() {
		let mut snap = sample_snapshot(1, 1.0, 2.0);
		snap.gpu.clear();
		assert!(!has_gpu(&[snap]));
	}

	#[test]
	fn active_chart_metrics_omits_gpu_when_none() {
		let mut snap = sample_snapshot(1, 1.0, 2.0);
		snap.gpu.clear();
		let metrics = active_chart_metrics(&[snap]);
		assert_eq!(metrics.len(), 3);
		assert!(metrics.iter().all(|m| m.def.label != "GPU"));
	}

	#[test]
	fn visible_range_live_shows_one_hour_window() {
		let snapshots = hour_of_samples(120, 30);
		let (start, end) = visible_range(&snapshots, 0);
		assert_eq!(end, 120);
		assert_eq!(start, 0);
		assert_eq!(
			snapshots[end - 1].timestamp - snapshots[start].timestamp,
			3570,
		);
	}

	#[test]
	fn visible_range_panned_shifts_back() {
		let snapshots = hour_of_samples(120, 30);
		let (start, end) = visible_range(&snapshots, 30);
		assert!(end < 120);
		assert!(start < end);
	}

	fn hour_of_samples(count: usize, step_secs: u64) -> Vec<TelemetrySnapshot> {
		(0..count)
			.map(|i| sample_snapshot(1_700_000_000 + i as u64 * step_secs, 1.0, 2.0))
			.collect()
	}

	#[test]
	fn pan_left_moves_offset() {
		let snapshots = hour_of_samples(120, 30);
		assert!(matches!(pan_left(snapshots.len(), 0), PanLeftResult::Moved));
		assert!(pan_left_new_offset(&snapshots, 0) > 0);
	}

	#[test]
	fn should_prefetch_when_window_hits_oldest_loaded() {
		let snapshots = hour_of_samples(120, 30);
		let server_oldest = snapshots[0].timestamp.saturating_sub(3600);
		assert!(should_prefetch_older(
			&snapshots, 0, false, Some(server_oldest), false,
		));
		assert!(!should_prefetch_older(
			&snapshots, 0, true, Some(server_oldest), false,
		));
		assert!(!should_prefetch_older(
			&snapshots, 0, false, Some(server_oldest), true,
		));
		assert!(!should_prefetch_older(
			&snapshots, 0, false, Some(snapshots[0].timestamp), false,
		));
	}

	#[test]
	fn apply_pan_right_steps_toward_live() {
		let snapshots = hour_of_samples(120, 30);
		let panned = pan_left_new_offset(&snapshots, 0);
		assert!(apply_pan_right(&snapshots, panned) < panned);
	}

	#[test]
	fn pan_left_at_max_requests_history() {
		assert!(matches!(pan_left(50, 49), PanLeftResult::NeedOlderHistory));
	}

	#[test]
	fn pan_offset_for_timestamp_finds_index() {
		let snapshots = hour_of_samples(10, 30);
		let offset = pan_offset_for_timestamp(&snapshots, snapshots[5].timestamp);
		assert_eq!(offset, 4);
	}

	#[test]
	fn plan_pan_left_moves_or_loads() {
		let snapshots = hour_of_samples(120, 30);
		assert!(matches!(
			plan_pan_left(&snapshots, 0, false),
			PanLeftAction::MoveTo(_)
		));
		assert!(matches!(
			plan_pan_left(&snapshots, 0, true),
			PanLeftAction::MoveTo(_)
		));
		assert!(matches!(
			plan_pan_left(&snapshots, 119, true),
			PanLeftAction::WaitForLoad
		));
	}

	#[test]
	fn resolve_pan_left_fetch_after_success() {
		let snapshots = hour_of_samples(120, 30);
		let apply = resolve_pan_left_fetch(&snapshots, 119, Ok(true));
		assert!(matches!(apply, PanLeftApply::SetPan(_)));
	}
}
