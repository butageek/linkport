#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{current_command, disable, enable, is_enabled};
#[cfg(not(windows))]
pub use stub::{current_command, disable, enable, is_enabled};
