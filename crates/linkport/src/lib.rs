//! Linkport library: shared logic for the windowless main binary and the
//! `linkport-cli` console tool.

pub mod api;
pub mod open;
pub mod paths;
pub mod portal;
#[cfg(windows)]
pub mod tray;

/// The windowless main binary (`linkport.exe`): this exe itself when we are
/// it, else the sibling copy (the normal side-by-side deployment), else
/// `None` in dev when only the console binary exists.
pub fn main_binary_path() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "linkport.exe"
    } else {
        "linkport"
    };
    if exe
        .file_name()
        .is_some_and(|n| n.eq_ignore_ascii_case(name))
    {
        return Some(exe);
    }
    let sibling = exe.parent()?.join(name);
    sibling.exists().then_some(sibling)
}

/// Build the shell open command registered with Windows: the windowless
/// `linkport.exe` handler when available (no console flash on link clicks),
/// else this exe's `open` subcommand as a dev fallback.
pub fn default_handler_command() -> String {
    if let Some(main) = main_binary_path() {
        return format!("\"{}\" \"%1\"", main.display());
    }
    match std::env::current_exe().ok() {
        Some(e) => format!("\"{}\" open \"%1\"", e.display()),
        None => String::new(),
    }
}

/// Login auto-start command: the windowless `linkport.exe` so no window
/// appears at sign-in.
pub fn autostart_command() -> String {
    match main_binary_path().or_else(|| std::env::current_exe().ok()) {
        Some(exe) => format!("\"{}\" serve", exe.display()),
        None => String::new(),
    }
}
