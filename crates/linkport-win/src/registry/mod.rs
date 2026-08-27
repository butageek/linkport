#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{is_registered, register, unregister};
#[cfg(not(windows))]
pub use stub::{is_registered, register, unregister};
