use ed25519_dalek::{Signature, Signer, SigningKey};
use std::fs;
use std::path::PathBuf;

pub struct Identity {
	keypair: SigningKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityOutcome {
	Loaded,
	Created,
}

impl Identity {
	pub fn load_or_generate() -> Result<(Self, IdentityOutcome), String> {
		let path = identity_path().ok_or("Could not determine config directory")?;

		if path.exists() {
			let bytes = fs::read(&path)
				.map_err(|e| format!("Failed to read identity file: {}", e))?;
			match bytes.try_into() as Result<[u8; 32], _> {
				Ok(key_bytes) => {
					return Ok((
						Self {
							keypair: SigningKey::from_bytes(&key_bytes),
						},
						IdentityOutcome::Loaded,
					));
				}
				Err(_) => {
					// Corrupted or wrong-length key file — regenerate below.
				}
			}
		}

		let mut secret = [0u8; 32];
		getrandom::fill(&mut secret).map_err(|e| format!("Failed to generate key: {}", e))?;
		let keypair = SigningKey::from_bytes(&secret);

		if let Some(parent) = path.parent() {
			let _ = fs::create_dir_all(parent);
		}

		fs::write(&path, keypair.to_bytes())
			.map_err(|e| format!("Failed to save identity file: {}", e))?;

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let perms = fs::Permissions::from_mode(0o600);
			fs::set_permissions(&path, perms)
				.map_err(|e| format!("Failed to set key file permissions: {}", e))?;
		}

		Ok((
			Self { keypair },
			IdentityOutcome::Created,
		))
	}

	pub fn eprint_setup_instructions(&self) {
		let path = identity_path().unwrap_or_else(|| PathBuf::from("identity.key"));
		eprintln!("Generated new identity at {}", path.display());
		eprintln!("Your public key is: {}", self.public_key_hex());
		eprintln!("Add this to the agent's allowed_keys config, then restart nulnet.");
	}

	pub fn public_key_hex(&self) -> String {
		hex::encode(self.keypair.verifying_key().as_bytes())
	}

	pub fn sign(&self, message: &[u8]) -> String {
		let signature: Signature = self.keypair.sign(message);
		hex::encode(signature.to_bytes())
	}
}

pub fn identity_path() -> Option<PathBuf> {
	dirs::config_dir().map(|mut p| {
		p.push("nulctl");
		p.push("identity.key");
		p
	})
}

pub fn auth_error_hint(error: &str, identity: &Identity) -> String {
	let lower = error.to_lowercase();
	if lower.contains("authentication rejected")
		|| lower.contains("public key not allowed")
		|| lower.contains("authentication failed")
	{
		format!(
			"{error}\n\nYour public key:\n{}\n\nAdd it to allowed_keys on the agent, then restart nulnet.",
			identity.public_key_hex()
		)
	} else {
		error.to_string()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn auth_error_hint_includes_pubkey_on_rejection() {
		let secret = [1u8; 32];
		let identity = Identity {
			keypair: SigningKey::from_bytes(&secret),
		};
		let hint = auth_error_hint("Authentication rejected: Public key not allowed", &identity);
		assert!(hint.contains(&identity.public_key_hex()));
		assert!(!auth_error_hint("connection refused", &identity).contains("Your public key:"));
	}
}
