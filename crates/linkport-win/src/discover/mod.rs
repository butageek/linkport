#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{discover_browsers, DiscoveredBrowser};
#[cfg(not(windows))]
pub use stub::{discover_browsers, DiscoveredBrowser};
