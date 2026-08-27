//! Windows integration for Linkport: default-browser registration and
//! browser discovery.
//!
//! On non-Windows platforms these operations become no-op stubs so the
//! workspace still compiles for development.

mod discover;
mod registry;

pub use discover::{discover_browsers, DiscoveredBrowser};
pub use registry::{is_registered, register, unregister};

/// Result type for Windows integration operations.
pub type Result<T> = std::result::Result<T, String>;
