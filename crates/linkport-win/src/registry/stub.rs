//! Non-Windows stubs so the workspace compiles during development on Linux.

use std::path::Path;

pub fn register(_exe: &Path, _open_command: &str) -> Result<(), String> {
    Err("browser registration is only supported on Windows".to_string())
}

pub fn unregister() -> Result<(), String> {
    Err("browser registration is only supported on Windows".to_string())
}

pub fn is_registered() -> bool {
    false
}

pub fn registered_handler() -> Option<String> {
    None
}

pub fn handler_ok() -> bool {
    true
}

pub fn repair_handler(_open_command: &str) -> Result<(), String> {
    Err("browser registration is only supported on Windows".to_string())
}
