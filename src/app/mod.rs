//! Application UI components.
//!
//! Contains all dock UI elements: draw coordinator, window list, context menu,
//! widget bars, audio player, Bluetooth panel, system tray, and more.

pub mod audio;
pub mod audio_player_bar;
pub mod audio_popup;
pub mod bluetooth;
pub mod bluetooth_popup;
pub mod context_menu;
pub mod dock;
pub mod draw;
pub mod icon_load;
pub mod network;
pub mod widget_bar;
pub mod window_list;

pub use icon_load::IconLoader;

pub mod process_monitor;
pub mod process_popup;
pub mod rclone;
pub mod rclone_popup;
pub mod system_bar;

pub mod dashed_block;
pub mod date_time_block;
pub mod side_bars;
