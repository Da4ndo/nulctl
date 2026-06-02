use crate::connection::Connection;
use crate::identity::Identity;

pub async fn connect_and_auth(target: &str, identity: &Identity) -> Result<Connection, String> {
	let mut conn = Connection::connect(target).await?;
	authenticate(&mut conn, identity).await?;
	Ok(conn)
}

pub async fn authenticate(conn: &mut Connection, identity: &Identity) -> Result<(), String> {
	let auth_res = conn
		.send_command(
			"agent.auth.request",
			serde_json::json!({ "pubkey": identity.public_key_hex() }),
		)
		.await
		.map_err(|e| format!("Failed to send auth request: {e}"))?;

	if auth_res.status != "ok" {
		return Err(format!(
			"Authentication rejected: {}",
			auth_res.error.unwrap_or_default()
		));
	}

	let data = auth_res
		.data
		.ok_or("Authentication failed: agent returned no challenge data")?;

	let nonce_hex = data
		.get("nonce")
		.and_then(|v| v.as_str())
		.ok_or("Authentication failed: agent did not return a nonce")?;

	let nonce_bytes =
		hex::decode(nonce_hex).map_err(|_| "Authentication failed: invalid nonce encoding")?;

	let signature = identity.sign(&nonce_bytes);

	let verify_res = conn
		.send_command(
			"agent.auth.verify",
			serde_json::json!({ "signature": signature }),
		)
		.await
		.map_err(|e| format!("Authentication verification failed: {e}"))?;

	if verify_res.status != "ok" {
		return Err(format!(
			"Authentication failed: {}",
			verify_res.error.unwrap_or_default()
		));
	}

	Ok(())
}

pub async fn fetch_agent_version(conn: &mut Connection) -> String {
	match conn
		.send_command("agent.version", serde_json::json!({}))
		.await
	{
		Ok(res) if res.status == "ok" => res
			.data
			.and_then(|d| d.get("version").and_then(|v| v.as_str().map(str::to_string)))
			.unwrap_or_else(|| "?".to_string()),
		_ => "?".to_string(),
	}
}
