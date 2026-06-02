use crate::telemetry::DockerContainer;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

pub fn container_name_color(status: &str) -> Color {
	let lower = status.to_ascii_lowercase();
	if lower.starts_with("restarting") {
		Color::Yellow
	} else if lower.starts_with("exited") || lower.starts_with("dead") {
		Color::Red
	} else {
		Color::Blue
	}
}

pub fn docker_max_scroll(total_lines: usize, viewport_height: usize) -> usize {
	total_lines.saturating_sub(viewport_height)
}

pub fn clamp_docker_scroll(scroll: usize, max_scroll: usize) -> usize {
	scroll.min(max_scroll)
}

pub fn docker_scroll_up(scroll: usize) -> usize {
	scroll.saturating_sub(1)
}

pub fn docker_scroll_down(scroll: usize, max_scroll: usize) -> usize {
	(scroll + 1).min(max_scroll)
}

pub fn docker_lines(containers: &[DockerContainer]) -> Vec<Line<'static>> {
	let mut lines = Vec::new();
	for (idx, container) in containers.iter().enumerate() {
		if idx > 0 {
			lines.push(Line::from(""));
		}
		lines.push(Line::from(vec![Span::styled(
			container.name.clone(),
			Style::default()
				.fg(container_name_color(&container.status))
				.add_modifier(Modifier::BOLD),
		)]));
		lines.push(field_line("status", &container.status));
		lines.push(field_line("image", &container.image));
		lines.push(field_line("created", &container.created));
		lines.push(field_line("ports", &container.ports));
	}
	lines
}

fn field_line(label: &str, value: &str) -> Line<'static> {
	Line::from(vec![
		Span::styled(
			format!("  {label:<7}"),
			Style::default().fg(Color::DarkGray),
		),
		Span::raw(value.to_string()),
	])
}

pub fn visible_docker_lines(
	all_lines: &[Line<'static>],
	scroll: usize,
	viewport_height: usize,
) -> Vec<Line<'static>> {
	if viewport_height == 0 {
		return vec![];
	}
	let max_scroll = docker_max_scroll(all_lines.len(), viewport_height);
	let scroll = clamp_docker_scroll(scroll, max_scroll);
	all_lines
		.iter()
		.skip(scroll)
		.take(viewport_height)
		.cloned()
		.collect()
}

pub fn docker_title(containers: &[DockerContainer], scroll: usize, max_scroll: usize) -> String {
	let count = containers.len();
	if count == 0 {
		return " Docker · none ".to_string();
	}
	if max_scroll > 0 {
		format!(" Docker · {count} · ↑/↓ scroll ({scroll}/{max_scroll}) ")
	} else {
		format!(" Docker · {count} ")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::telemetry::DockerContainer;

	fn sample(name: &str, status: &str) -> DockerContainer {
		DockerContainer {
			name: name.to_string(),
			status: status.to_string(),
			created: "2024-01-01".to_string(),
			image: "nginx:latest".to_string(),
			ports: "80/tcp".to_string(),
		}
	}

	#[test]
	fn container_name_color_by_status() {
		assert_eq!(container_name_color("Up 2 hours"), Color::Blue);
		assert_eq!(
			container_name_color("Restarting (1) 5 seconds ago"),
			Color::Yellow,
		);
		assert_eq!(container_name_color("Exited (0) 1 day ago"), Color::Red);
		assert_eq!(container_name_color("Dead"), Color::Red);
	}

	#[test]
	fn docker_scroll_clamps_to_content() {
		assert_eq!(docker_max_scroll(10, 4), 6);
		assert_eq!(clamp_docker_scroll(9, 6), 6);
		assert_eq!(docker_scroll_up(0), 0);
		assert_eq!(docker_scroll_down(5, 6), 6);
	}

	#[test]
	fn docker_lines_include_all_fields() {
		let lines = docker_lines(&[sample("web", "Up 1 hour")]);
		assert_eq!(lines.len(), 5);
		let text: String = lines[0].spans[0].content.clone().into();
		assert_eq!(text, "web");
	}

	#[test]
	fn visible_docker_lines_respects_viewport() {
		let lines = docker_lines(&[
			sample("a", "Up"),
			sample("b", "Up"),
		]);
		let visible = visible_docker_lines(&lines, 1, 3);
		assert_eq!(visible.len(), 3);
	}
}
