//! Start Menu shortcut so a daemon quit from the tray can be restarted
//! from the Start menu (search "Linkport").

#[cfg(windows)]
mod imp;
#[cfg(not(windows))]
mod stub;

#[cfg(windows)]
pub use imp::ensure_start_menu_shortcut;
#[cfg(not(windows))]
pub use stub::ensure_start_menu_shortcut;
