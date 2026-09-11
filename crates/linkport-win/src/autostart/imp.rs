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

/// Enabled only when the Run value exists AND points at an executable that
/// still exists on disk (a stale entry from a moved install reads as off,
/// and re-enabling overwrites it).
pub fn is_enabled() -> bool {
    match run_value() {
        Some(cmd) => {
            let (exe, _) = linkport_core::launcher::parse_command_template(&cmd);
            Path::new(&exe).exists()
        }
        None => false,
    }
}

/// The registered Run command, if any.
fn run_value() -> Option<String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(RUN_KEY).ok()?;
    key.get_value(VALUE_NAME).ok()
}

/// Repair a stale auto-start entry after the install moved: when the Run
/// value exists but points at an executable that is gone (old version
/// folder), overwrite it with `current_command`. A deliberately disabled
/// auto-start has no value at all and is left alone. Returns `true` when a
/// repair happened.
pub fn repair_stale(current_command: &str) -> Result<bool, String> {
    let Some(existing) = run_value() else {
        return Ok(false);
    };
    let (exe, _) = linkport_core::launcher::parse_command_template(&existing);
    if Path::new(&exe).exists() {
        return Ok(false);
    }
    enable(current_command).map(|()| true)
}
