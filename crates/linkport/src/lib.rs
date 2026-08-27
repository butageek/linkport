//! Linkport library: shared logic for the CLI and the GUI-subsystem URL
//! handler binary.

pub mod api;
pub mod open;
pub mod paths;
pub mod portal;
#[cfg(windows)]
pub mod tray;

/// Build the shell open command registered with Windows, preferring the
/// console-less `linkport-open` handler when it sits next to this binary.
pub fn default_handler_command() -> String {
    let exe = std::env::current_exe().ok();
    if let Some(e) = &exe {
        if let Some(dir) = e.parent() {
            let name = if cfg!(windows) {
                "linkport-open.exe"
            } else {
                "linkport-open"
            };
            let sibling = dir.join(name);
            if sibling.exists() {
                return format!("\"{}\" \"%1\"", sibling.display());
            }
        }
    }
    match exe {
        Some(e) => format!("\"{}\" open \"%1\"", e.display()),
        None => String::new(),
    }
}

/// Login auto-start command: prefer the console-less `linkport-open.exe`
/// so no window appears at sign-in.
pub fn autostart_command() -> String {
    let exe = std::env::current_exe().ok();
    if let Some(e) = &exe {
        if let Some(dir) = e.parent() {
            let name = if cfg!(windows) {
                "linkport-open.exe"
            } else {
                "linkport-open"
            };
            let sibling = dir.join(name);
            if sibling.exists() {
                return format!("\"{}\" serve", sibling.display());
            }
        }
    }
    match exe {
        Some(e) => format!("\"{}\" serve", e.display()),
        None => String::new(),
    }
}
