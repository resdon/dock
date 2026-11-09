//! Background listener services.
//!
//! D-Bus tray listener, Unity badge listener, and Wayland event listeners.

pub mod dbus_tray;
pub mod dbus_unity;
pub mod wayland;

pub use dbus_tray::start_status_notifier_listener;
pub use dbus_unity::start_unity_dbus_listener;
pub use wayland::*;
