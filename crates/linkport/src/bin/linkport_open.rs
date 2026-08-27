//! GUI-subsystem URL handler entry point.
//!
//! This is the binary Windows launches for every clicked link (registered as
//! the ProgId shell open command). Building it with the `windows` subsystem
//! means no console window ever flashes. It never prints: failures are
//! recorded in the event log and shown in the portal instead.

#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let Some(url) = std::env::args().nth(1) else {
        return;
    };
    let _ = linkport::open::open_url(&url);
}
