//! The app itself: GUI-subsystem (windowless) entry point.
//!
//! Three invocations:
//! - `linkport.exe "<url>"` — the OS-facing URL handler registered in the
//!   registry's shell open command. No console window ever flashes;
//!   failures are recorded in the event log and shown in the portal.
//! - `linkport.exe serve`  — start the portal daemon (used by the login
//!   auto-start entry, so nothing flashes at sign-in either).
//! - `linkport.exe` (no arguments) — double-click / Start Menu launch:
//!   start the daemon, or open the portal if one is already running. This
//!   is how the daemon comes back after a tray Quit.
//!
//! Terminal/debug workflows live in the sibling `linkport-cli.exe`.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let arg = std::env::args().nth(1).unwrap_or_default();
    let arg = arg.trim();
    if arg.is_empty() || arg == "serve" {
        start_daemon();
        return;
    }
    let _ = linkport::open::open_url(arg);
}

/// Launched bare: become the daemon unless one is already listening on the
/// configured port — in that case surface the portal (the daemon may only
/// be missing its tray icon, e.g. after an Explorer restart).
fn start_daemon() {
    // Repair a registration left pointing at a moved/deleted install
    // before anything else — even when another daemon already owns the
    // port and this process would only surface the portal.
    #[cfg(windows)]
    linkport::portal::heal_windows_registration();

    let port = linkport::paths::load_or_default().portal.port;
    if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
        if let Ok(token) = linkport::paths::ensure_token() {
            linkport::portal::open_portal_url(&linkport::portal::AppState::new(token, port));
        }
        return;
    }
    let _ = linkport::portal::serve(None, false);
}
