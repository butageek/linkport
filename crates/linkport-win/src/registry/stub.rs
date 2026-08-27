//! Non-Windows stubs so the workspace compiles during development on Linux.

use std::path::Path;

pub fn register(_exe: &Path) -> Result<(), String> {
    Err("browser registration is only supported on Windows".to_string())
}

pub fn unregister() -> Result<(), String> {
    Err("browser registration is only supported on Windows".to_string())
}

pub fn is_registered() -> bool {
    false
}
