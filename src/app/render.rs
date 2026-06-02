use super::chart::{
	active_chart_metrics, build_usage_points, is_live, time_span_label, ActiveMetric,
};
use super::dashboard::DashboardState;
use super::docker_panel::{
	clamp_docker_scroll, docker_lines, docker_max_scroll, docker_title, visible_docker_lines,
};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::symbols;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Axis, Block, Borders, Chart, Dataset, GraphType, Paragraph};
use ratatui::Frame;

pub fn render(frame: &mut Frame, state: &mut DashboardState) {
	let area = frame.area();
	let mut constraints = vec![];
	if state.agent_update_available.is_some() {
		constraints.push(Constraint::Length(1));
	}
	constraints.extend([
		Constraint::Length(server_panel_height()),
		Constraint::Min(14),
		Constraint::Length(3),
		Constraint::Length(1),
	]);
	let chunks = Layout::default()
		.direction(Direction::Vertical)
		.constraints(constraints)
		.split(area);

	let mut idx = 0;
	if state.agent_update_available.is_some() {
		render_update_banner(frame, chunks[idx], state);
		idx += 1;
	}
	render_server_panel(frame, chunks[idx], state);
	render_usage_chart(frame, chunks[idx + 1], state);
	render_current_values(frame, chunks[idx + 2], state);
	render_footer(frame, chunks[idx + 3], state);
}

fn render_update_banner(frame: &mut Frame, area: Rect, state: &DashboardState) {
	let Some(latest) = &state.agent_update_available else {
		return;
	};
	let line = Line::from(vec![
		Span::styled(
			" ⚠ ",
			Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
		),
		Span::styled(
			format!(
				"Agent update available: v{} → v{}  (press u to update)",
				state.agent_version, latest
			),
			Style::default().fg(Color::Yellow),
		),
	]);
	frame.render_widget(Paragraph::new(line), area);
}

fn server_panel_height() -> u16 {
	8
}

fn render_server_panel(frame: &mut Frame, area: Rect, state: &mut DashboardState) {
	let has_docker = state
		.latest()
		.is_some_and(|s| !s.containers.is_empty());
	if has_docker {
		let cols = Layout::default()
			.direction(Direction::Horizontal)
			.constraints([Constraint::Percentage(58), Constraint::Fill(1)])
			.split(area);
		render_server_stats(frame, cols[0], state);
		render_docker_panel(frame, cols[1], state);
	} else {
		render_server_stats(frame, area, state);
	}
}

fn render_server_stats(frame: &mut Frame, area: Rect, state: &DashboardState) {
	let lines = server_lines(state);
	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Cyan))
		.title(format!(
			" Server · {} · agent v{} ",
			state.target, state.agent_version
		));
	frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_docker_panel(frame: &mut Frame, area: Rect, state: &mut DashboardState) {
	let containers = state
		.latest()
		.map(|s| s.containers.clone())
		.unwrap_or_default();
	if containers.is_empty() {
		return;
	}

	let inner_height = area.height.saturating_sub(2) as usize;
	state.docker_viewport_lines = inner_height;

	let all_lines = docker_lines(&containers);
	let max_scroll = docker_max_scroll(all_lines.len(), inner_height);
	state.docker_scroll = clamp_docker_scroll(state.docker_scroll, max_scroll);
	let visible = visible_docker_lines(&all_lines, state.docker_scroll, inner_height);
	let title = docker_title(&containers, state.docker_scroll, max_scroll);

	let block = Block::default()
		.borders(Borders::ALL)
		.border_style(Style::default().fg(Color::Blue))
		.title(title);
	frame.render_widget(Paragraph::new(visible).block(block), area);
}

const LABEL_WIDTH: usize = 9;

fn stat_label(text: &str, color: Color) -> Span<'static> {
	Span::styled(
		format!("{text:<LABEL_WIDTH$}"),
		Style::default().fg(color).bold(),
	)
}

fn server_lines(state: &DashboardState) -> Vec<Line<'static>> {
	let Some(s) = state.latest() else {
		return vec![Line::from("Waiting for telemetry data…")];
	};

	let os = s.os.clone().unwrap_or_else(|| "unknown".to_string());
	let cpu = s
		.cpu
		.info
		.as_ref()
		.map(|i| i.display.clone())
		.unwrap_or_else(|| "—".to_string());
	let disks = if s.disk.is_empty() {
		"—".to_string()
	} else {
		s.disk
			.iter()
			.map(|d| format!("{} {:.1}% ({}/{})", d.name, d.usage, d.used_size, d.total_size))
			.collect::<Vec<_>>()
			.join("  ·  ")
	};

	let gpu_text = s.gpu.first().map(|g| {
		format!(
			"{} {:.1}% · VRAM {}/{} ({:.1}%)",
			truncate(&g.name, 28),
			g.usage,
			g.vram_used,
			g.vram_total,
			g.vram_usage
		)
	}).unwrap_or_else(|| "not detected".to_string());

	vec![
		Line::from(vec![stat_label("OS", Color::Blue), Span::raw(os)]),
		Line::from(vec![
			stat_label("Uptime", Color::Red),
			Span::raw(s.format_uptime()),
		]),
		Line::from(vec![
			stat_label("CPU", Color::Green),
			Span::raw(cpu),
			Span::raw("  "),
			stat_label("Load", Color::Green),
			Span::raw(format!(
				"{:.2} / {:.2} / {:.2}",
				s.load_average.one, s.load_average.five, s.load_average.fifteen
			)),
		]),
		Line::from(vec![
			stat_label("RAM", Color::Cyan),
			Span::raw(format!(
				"{:.1}% · {:.1} / {:.1} GB",
				s.memory.usage, s.memory.used_gb, s.memory.total_gb
			)),
		]),
		Line::from(vec![
			stat_label("GPU", Color::Magenta),
			Span::raw(gpu_text),
		]),
		Line::from(vec![
			stat_label("DISK", Color::Yellow),
			Span::raw(disks),
		]),
		Line::from(vec![
			stat_label("Cache", Color::LightCyan),
			Span::raw(s.cached_telemetry_size.clone()),
		]),
	]
}

fn render_usage_chart(frame: &mut Frame, area: Rect, state: &DashboardState) {
	let visible = state.chart_view();
	let span = time_span_label(visible);
	let live_tag = if is_live(state.chart_pan_from_end) {
		"live"
	} else {
		"historical"
	};
	let block = Block::default()
		.borders(Borders::ALL)
		.title(format!(" Usage % over time · {span} · {live_tag} "))
		.title_bottom(format!(
			" {} samples · 1h window · poll every {}s · ←/→ pan ",
			visible.len(),
			state.poll_interval_secs
		));

	if visible.is_empty() {
		frame.render_widget(Paragraph::new("No history loaded yet").block(block), area);
		return;
	}

	let metrics = active_chart_metrics(visible);
	let point_sets: Vec<Vec<(f64, f64)>> = metrics
		.iter()
		.map(|m| build_usage_points(visible, m.usage_fn))
		.collect();
	let datasets = chart_datasets(&metrics, &point_sets);

	let x_end = if is_live(state.chart_pan_from_end) {
		"now".bold()
	} else {
		Span::styled("end", Style::default().fg(Color::DarkGray))
	};

	let chart = Chart::new(datasets)
		.block(block)
		.x_axis(
			Axis::default()
				.title("time →")
				.style(Style::default().fg(Color::DarkGray))
				.bounds([0.0, 100.0])
				.labels(["oldest".bold(), x_end]),
		)
		.y_axis(
			Axis::default()
				.title("%")
				.style(Style::default().fg(Color::DarkGray))
				.bounds([0.0, 100.0])
				.labels([Span::raw("0"), Span::raw("50"), Span::raw("100")]),
		);

	frame.render_widget(chart, area);
}

fn chart_datasets<'a>(
	metrics: &'a [ActiveMetric],
	point_sets: &'a [Vec<(f64, f64)>],
) -> Vec<Dataset<'a>> {
	metrics
		.iter()
		.zip(point_sets.iter())
		.map(|(metric, points)| {
			Dataset::default()
				.name(metric.def.label)
				.marker(symbols::Marker::Braille)
				.style(Style::default().fg(metric.def.color))
				.graph_type(GraphType::Line)
				.data(points)
		})
		.collect()
}

fn render_current_values(frame: &mut Frame, area: Rect, state: &DashboardState) {
	let visible = state.chart_view();
	let at_view_end = visible.last();
	let metrics = active_chart_metrics(visible);
	let count = metrics.len().max(1);
	let pct = (100 / count) as u16;

	let cols = Layout::default()
		.direction(Direction::Horizontal)
		.constraints(vec![Constraint::Percentage(pct); count])
		.split(area);

	for (col, metric) in cols.iter().zip(metrics.iter()) {
		let value = at_view_end.map(|s| (metric.usage_fn)(s)).unwrap_or(0.0);
		let bar = usage_bar(value);
		let text = Line::from(vec![
			Span::styled(
				format!("{} ", metric.def.label),
				Style::default().fg(metric.def.color).bold(),
			),
			Span::raw(format!("{:.1}%  ", value)),
			Span::styled(bar, Style::default().fg(metric.def.color)),
		]);
		frame.render_widget(Paragraph::new(text), *col);
	}
}

fn usage_bar(usage: f64) -> String {
	let width = 12usize;
	let filled = ((usage / 100.0).clamp(0.0, 1.0) * width as f64).round() as usize;
	format!(
		"[{}{}]",
		"█".repeat(filled),
		"░".repeat(width.saturating_sub(filled))
	)
}

fn render_footer(frame: &mut Frame, area: Rect, state: &DashboardState) {
	let default_status = if state.history_fetch_inflight {
		"Loading older telemetry…"
	} else if is_live(state.chart_pan_from_end) {
		"Live"
	} else {
		"Historical view"
	};
	let status = state
		.status_message
		.as_deref()
		.unwrap_or(default_status);

	let has_docker = state
		.latest()
		.is_some_and(|s| !s.containers.is_empty());

	let mut spans = vec![
		Span::styled(status, Style::default().fg(if state.status_message.is_some() {
			Color::Red
		} else if is_live(state.chart_pan_from_end) {
			Color::DarkGray
		} else {
			Color::Yellow
		})),
		Span::raw(" · "),
		Span::styled("←/→", Style::default().add_modifier(Modifier::BOLD)),
		Span::styled(" pan ", Style::default().fg(Color::DarkGray)),
	];
	if has_docker {
		spans.extend([
			Span::styled("↑/↓", Style::default().add_modifier(Modifier::BOLD)),
			Span::styled(" docker ", Style::default().fg(Color::DarkGray)),
		]);
	}
	spans.extend([
		Span::styled("u", Style::default().add_modifier(Modifier::BOLD)),
		Span::styled(" update ", Style::default().fg(Color::DarkGray)),
		Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
		Span::styled(" switch ", Style::default().fg(Color::DarkGray)),
		Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
		Span::styled(" quit", Style::default().fg(Color::DarkGray)),
	]);

	frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn truncate(s: &str, max: usize) -> String {
	if s.chars().count() <= max {
		s.to_string()
	} else {
		format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>())
	}
}
