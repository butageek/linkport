//! Non-Windows stub.

pub fn ensure_start_menu_shortcut(_target: &std::path::Path) -> Result<(), String> {
    Err("Start Menu shortcuts are only supported on Windows".to_string())
}
