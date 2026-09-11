#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::{
    handler_ok, is_registered, register, registered_handler, repair_handler, unregister,
};
#[cfg(not(windows))]
pub use stub::{
    handler_ok, is_registered, register, registered_handler, repair_handler, unregister,
};
