use tokio::process::Command;

const DEFAULT_GITHUB_REPO: &str = "nulnet/nulnet";
const USER_AGENT: &str = concat!("nulctl/", env!("CARGO_PKG_VERSION"));

pub async fn fetch_latest_agent_version() -> Option<String> {
	if let Ok(cdn) = std::env::var("NULNET_CDN_BASE")
		&& !cdn.trim().is_empty()
	{
		return fetch_latest_agent_version_cdn(&cdn).await;
	}
	latest_from_github(DEFAULT_GITHUB_REPO).await
}

pub async fn latest_from_github(repo: &str) -> Option<String> {
	let url = format!(
		"https://api.github.com/repos/{}/releases/latest",
		repo
	);
	let body = curl_get(&url).await?;
	let version = parse_github_tag(&body)?;
	if version.is_empty() {
		None
	} else {
		Some(version)
	}
}

fn parse_github_tag(body: &str) -> Option<String> {
	let value: serde_json::Value = serde_json::from_str(body).ok()?;
	let tag = value.get("tag_name")?.as_str()?;
	let version = tag.trim().trim_start_matches('v').to_string();
	if version.is_empty() {
		None
	} else {
		Some(version)
	}
}

async fn fetch_latest_agent_version_cdn(base: &str) -> Option<String> {
	let base = base.trim_end_matches('/');
	let url = format!("{}/version.txt", base);
	let version = curl_get(&url).await?;
	if version.is_empty() {
		None
	} else {
		Some(version)
	}
}

async fn curl_get(url: &str) -> Option<String> {
	let out = Command::new("curl")
		.args([
			"-fsSL",
			"--connect-timeout",
			"10",
			"--max-time",
			"30",
			"-A",
			USER_AGENT,
			url,
		])
		.output()
		.await
		.ok()?;

	if !out.status.success() {
		return None;
	}

	let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
	if text.is_empty() {
		None
	} else {
		Some(text)
	}
}

/// Returns `true` when `candidate` is strictly newer than `current` by semver.
pub fn semver_is_newer(candidate: &str, current: &str) -> bool {
	parse_semver(candidate) > parse_semver(current)
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
	let v = v.trim().trim_start_matches('v');
	let mut parts = v.splitn(3, '.');
	let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
	let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
	let patch = parts
		.next()
		.and_then(|p| p.split('-').next())
		.and_then(|p| p.parse().ok())
		.unwrap_or(0);
	(major, minor, patch)
}

pub fn update_available(current: &str, latest: Option<&str>) -> Option<String> {
	let latest = latest?;
	if current == "?" || semver_is_newer(latest, current) {
		Some(latest.to_string())
	} else {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn semver_is_newer_compares_patch() {
		assert!(semver_is_newer("1.3.0", "1.2.0"));
		assert!(!semver_is_newer("1.2.0", "1.3.0"));
	}

	#[test]
	fn semver_is_newer_strips_v_prefix() {
		assert!(semver_is_newer("v2.0.0", "1.9.9"));
	}

	#[test]
	fn semver_is_newer_equal_is_false() {
		assert!(!semver_is_newer("1.2.0", "1.2.0"));
	}

	#[test]
	fn update_available_when_latest_newer() {
		assert_eq!(
			update_available("1.2.0", Some("1.3.0")),
			Some("1.3.0".to_string()),
		);
	}

	#[test]
	fn update_available_none_when_up_to_date() {
		assert!(update_available("1.3.0", Some("1.3.0")).is_none());
	}

	#[test]
	fn update_available_when_current_unknown() {
		assert_eq!(
			update_available("?", Some("1.3.0")),
			Some("1.3.0".to_string()),
		);
	}

	#[test]
	fn parse_github_tag_strips_v_prefix() {
		let body = r#"{"tag_name":"v2.1.0"}"#;
		assert_eq!(parse_github_tag(body), Some("2.1.0".to_string()));
	}
}
