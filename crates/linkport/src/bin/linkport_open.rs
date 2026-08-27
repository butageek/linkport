//! GUI-subsystem entry point for windowless operation.
//!
//! Two invocations:
//! - `linkport-open.exe "<url>"` — the OS-facing URL handler registered in
//!   the registry's shell open command. No console window ever flashes;
//!   failures are recorded in the event log and shown in the portal.
//! - `linkport-open.exe serve`  — start the portal daemon (used by the
//!   login auto-start entry, so nothing flashes at sign-in either).

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let Some(arg) = std::env::args().nth(1) else {
        return;
    };
    if arg == "serve" {
        let _ = linkport::portal::serve(None, false);
        return;
    }
    let _ = linkport::open::open_url(&arg);
}
