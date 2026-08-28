//! Non-Windows stub.

pub fn enable(_command: &str) -> Result<(), String> {
    Err("auto-start is only supported on Windows".to_string())
}

pub fn disable() -> Result<(), String> {
    Err("auto-start is only supported on Windows".to_string())
}

pub fn is_enabled() -> bool {
    false
}
