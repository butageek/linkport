#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{disable, enable, is_enabled, repair_stale};
#[cfg(not(windows))]
pub use stub::{disable, enable, is_enabled, repair_stale};
