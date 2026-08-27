//! Well-known paths (config, token, event log) and the portal token.

use anyhow::Result;
use linkport_core::config::{self, Config};
use std::io::Write;
use std::path::PathBuf;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
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

/// Pause-routing flag: a plain file whose presence puts the hot path into
/// "send everything to the default browser" mode. Checked by every open,
/// so toggling takes effect instantly — even mid-portal-restart.
pub fn pause_flag_path() -> PathBuf {
    config_dir().join("paused.flag")
}

pub fn is_paused() -> bool {
    pause_flag_path().exists()
}

pub fn set_paused(paused: bool) -> std::io::Result<()> {
    if paused {
        if let Some(parent) = pause_flag_path().parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::File::create(pause_flag_path())?;
    } else {
        match std::fs::remove_file(pause_flag_path()) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
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
