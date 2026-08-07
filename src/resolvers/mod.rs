// src/resolvers/mod.rs

pub mod desktop;
pub mod icon;
pub mod icon_list;
pub mod steam;

pub use desktop::{
    clean_exec_field, find_desktop_file_by_exec, find_desktop_file_by_name, get_desktop_actions,
    parse_desktop_actions, DesktopAction,
};
pub use icon::{
    extract_icon_name, find_icon_by_name, find_icon_path, get_icon_from_desktop, get_icon_path,
};
pub use icon_list::{search_icon_list_file, spawn_startup_indexer};
pub use steam::resolve_steam_game_details;
