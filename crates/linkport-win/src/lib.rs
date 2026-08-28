//! Windows integration for Linkport: default-browser registration and
//! browser discovery.
//!
//! On non-Windows platforms these operations become no-op stubs so the
//! workspace still compiles for development.

pub mod autostart;
mod discover;
mod registry;
pub mod shortcut;

pub use discover::{discover_browsers, system_default_exe, DiscoveredBrowser};
pub use registry::{is_registered, register, unregister};
