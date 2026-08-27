//! Well-known paths (config, token, event log) and the portal token.

use anyhow::Result;
use linkport_core::config::{self, Config};
use std::io::Write;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::temp_dir())
        .join("linkport")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

pub fn events_path() -> PathBuf {
    config_dir().join("events.jsonl")
}

pub fn token_path() -> PathBuf {
    config_dir().join("token")
}

pub fn load_or_default() -> Config {
    config::load(&config_path()).unwrap_or_default()
}

/// Load the portal token, creating one on first use (24 random bytes, hex).
pub fn ensure_token() -> Result<String> {
    if let Ok(existing) = std::fs::read_to_string(token_path()) {
        let token = existing.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }
    let mut bytes = [0u8; 24];
    getrandom::getrandom(&mut bytes).map_err(|e| anyhow::anyhow!("rng failure: {e}"))?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();

    let path = token_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::File::create(&path)?;
    f.write_all(token.as_bytes())?;
    Ok(token)
}
