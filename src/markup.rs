use colored::Colorize;

/// Render simple `[tag]...[/tag]` markup using the `colored` crate.
pub fn render(input: &str) -> String {
	let s = apply_tag(input, "bold", |t| t.bold().to_string());
	let s = apply_tag(&s, "dim", |t| t.dimmed().to_string());
	let s = apply_tag(&s, "green", |t| t.green().to_string());
	let s = apply_tag(&s, "red", |t| t.red().to_string());
	let s = apply_tag(&s, "yellow", |t| t.yellow().to_string());
	let s = apply_tag(&s, "cyan", |t| t.cyan().to_string());
	apply_tag(&s, "blue", |t| t.blue().to_string())
}

/// Strip markup tags and ANSI escapes for TUI log display.
pub fn to_plain(input: &str) -> String {
	strip_ansi(&render(input))
}

fn apply_tag(input: &str, tag: &str, f: impl Fn(&str) -> String) -> String {
	let open = format!("[{tag}]");
	let close = format!("[/{tag}]");
	let mut out = String::with_capacity(input.len() * 2);
	let mut rest = input;
	while let Some(start) = rest.find(&open) {
		out.push_str(&rest[..start]);
		let after = &rest[start + open.len()..];
		if let Some(end) = after.find(&close) {
			out.push_str(&f(&after[..end]));
			rest = &after[end + close.len()..];
		} else {
			out.push_str(&open);
			rest = after;
		}
	}
	out.push_str(rest);
	out
}

fn strip_ansi(input: &str) -> String {
	let mut out = String::with_capacity(input.len());
	let mut chars = input.chars().peekable();
	while let Some(c) = chars.next() {
		if c == '\x1b' {
			while chars.next().is_some_and(|n| n != 'm') {}
			continue;
		}
		out.push(c);
	}
	out
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn to_plain_strips_tags() {
		assert_eq!(to_plain("[green]ok[/green]"), "ok");
	}
}
