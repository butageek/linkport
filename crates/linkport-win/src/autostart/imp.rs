//! Login auto-start via the native per-user mechanism:
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` (no Task Scheduler,
//! no admin rights needed).

use std::path::Path;
use winreg::enums::*;
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE_NAME: &str = "Linkport";

/// Register a command to run at user login.
pub fn enable(command: &str) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE)
        .map_err(|e| format!("open Run key: {e}"))?;
    key.set_value(VALUE_NAME, &command)
        .map_err(|e| format!("write Run value: {e}"))
}

/// Remove the auto-start entry (succeeds when absent).
pub fn disable() -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(RUN_KEY, KEY_SET_VALUE) {
        Ok(key) => {
            let _ = key.delete_value(VALUE_NAME);
            Ok(())
        }
        Err(e) => Err(format!("open Run key: {e}")),
    }
}

/// The registered command, if any.
pub fn current_command() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(RUN_KEY).ok()?;
    key.get_value(VALUE_NAME).ok()
}

/// Enabled only when the Run value exists AND points at an executable that
/// still exists on disk (a stale entry from a moved install reads as off,
/// and re-enabling overwrites it).
pub fn is_enabled() -> bool {
    match current_command() {
        Some(cmd) => exe_from_command(&cmd)
            .map(|p| Path::new(&p).exists())
            .unwrap_or(false),
        None => false,
    }
}

/// Extract the executable path from a Run-style command (quoted or bare).
fn exe_from_command(command: &str) -> Option<String> {
    let cmd = command.trim();
    if let Some(stripped) = cmd.strip_prefix('"') {
        stripped.split_once('"').map(|(exe, _)| exe.to_string())
    } else {
        cmd.split_once(' ').map(|(exe, _)| exe.to_string())
    }
}
