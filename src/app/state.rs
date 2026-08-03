use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::process::Command;
use std::path::PathBuf;

use dockman_lib::animations::IconAnimation;

use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::{LayerShell, LayerSurface},
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_device::WlDataDevice,
    wl_data_device_manager::WlDataDeviceManager,
    wl_output::WlOutput,
    wl_pointer::WlPointer,
    wl_seat::WlSeat,
    wl_subcompositor::WlSubcompositor,
};
use wayland_client::Connection;

use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use crate::graphics::hide::AutoHideState;
use crate::models::{WindowDiagnostics, BadgeUpdate};
use crate::render::font::FontManager;
use crate::resolvers::search_icon_list_file;

use super::icon_load::IconLoader;
use super::types::{DndState, DockInstance, HoverState, MenuState};

pub struct IconLoadResult {
    pub app_id: String,
    pub rgba: Vec<u8>,
    pub size: u32,
    pub animation: Option<IconAnimation>,
}

pub struct AppState {
    pub connection: Connection,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub pool: SlotPool,
    pub seat_state: SeatState,
    pub layer_surface: Option<LayerSurface>,
    pub current_buffer: Option<Buffer>,
    pub width: i32,
    pub height: i32,
    pub toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub open_windows: HashMap<ObjectId, WindowDiagnostics>,
    pub pinned_apps: Vec<String>,
    pub menu_state: MenuState,
    pub hover_state: HoverState,
    pub last_interact_time: std::time::Instant,
    pub needs_redraw: bool,
    pub last_mouse_pos: Option<(i32, i32)>,
    pub is_dragging: bool,
    pub drag_start_x: i32,
    pub drag_start_y: i32,
    pub dragged_app_id: Option<String>,
    pub last_drag_draw: std::time::Instant,
    pub sys_scanner: sysinfo::System,
    pub current_output: Option<WlOutput>,
    pub docks: Vec<DockInstance>,
    // Scale Tracking
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub fractional_scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,
    // Animation controller for missing icons
    pub fallback_anim: dockman_lib::animations::IconAnimation,
    // Drag and drop
    pub dnd_state: DndState,
    pub data_device_manager: Option<WlDataDeviceManager>,
    pub data_device: Option<WlDataDevice>,
    // DBus badges
    pub badges: HashMap<String, BadgeUpdate>,
    // Async Icon Loading & Caching
    pub icon_cache: HashMap<String, (Vec<u8>, u32)>,
    pub pending_icon_searches: HashSet<String>,
    pub icon_rx: Receiver<IconLoadResult>,
    pub icon_tx: Sender<IconLoadResult>,
    pub subcompositor: WlSubcompositor,
    // Icon load
    pub icon_load: IconLoader,
    pub animations: HashMap<String, IconAnimation>,
    // Autohide
    pub hide_state: AutoHideState,
}

/// Generates a blank/generic 48x48 RGBA fallback icon when an icon cannot be found anywhere
pub(crate) fn load_generic_fallback_bytes() -> Option<(Vec<u8>, u32)> {
    let size = 48;
    // Semi-transparent gray box (RGBA)
    let rgba = vec![128, 128, 128, 180].repeat((size * size) as usize);
    Some((rgba, size))
}

impl AppState {
    pub fn get_app_id_at_location(&self, x: f64, y: f64) -> Option<String> {
        let box_size = 48.0;
        let spacing = 12.0;
        let dock_height = 60.0;
        let dock_top_bound = (self.height as f64) - dock_height;

        if y < dock_top_bound {
            return None;
        }

        let mut apps_in_dock: Vec<String> = self.pinned_apps.clone();
        for window in self.open_windows.values() {
            let id = if !window.app_id.is_empty() {
                window.app_id.clone()
            } else if !window.title.is_empty() {
                window.title.clone()
            } else {
                "Unknown".to_string()
            };
            if !apps_in_dock.contains(&id) {
                apps_in_dock.push(id);
            }
        }

        let total_items = apps_in_dock.len();
        let content_width = if total_items > 0 { 
            total_items as f64 * box_size + (total_items + 1) as f64 * spacing
        } else { 
            0.0 
        };
        
        let start_offset_x = if (self.width as f64) > content_width { 
            ((self.width as f64) - content_width) / 2.0 
        } else { 
            0.0 
        };

        for (index, app_id) in apps_in_dock.iter().enumerate() {
            let start_x = start_offset_x + spacing + index as f64 * (box_size + spacing);
            let hit_start_x = start_x - (spacing / 2.0);
            let hit_end_x = start_x + box_size + (spacing / 2.0);

            if x >= hit_start_x && x <= hit_end_x {
                return Some(app_id.clone());
            }
        }
        None
    }

    pub fn update_dnd_hover_target(&mut self, x: f64, y: f64) {
        let new_app = self.get_app_id_at_location(x, y);
        let new_index = new_app.as_ref().map(|_| 0);
        if self.dnd_state.hovered_dock_index != new_index {
            self.dnd_state.hovered_dock_index = new_index;
            self.needs_redraw = true;
        }
    }

    pub fn handle_file_drop_on_icon(&mut self, x: f64, y: f64, file_paths: Vec<PathBuf>) {
        if file_paths.is_empty() { return; }

        if let Some(app_id) = self.get_app_id_at_location(x, y) {
            let launcher_path = crate::handlers::get_launcher_path();
            let mut normalized_app_id = app_id.clone();
            if !normalized_app_id.starts_with("steam_icon_") {
                if let Some(idx) = normalized_app_id.rfind('_') {
                    if normalized_app_id[idx + 1..].chars().all(|c| c.is_numeric()) {
                        normalized_app_id = normalized_app_id[..idx].to_string();
                    }
                }
            }

            let mut cmd = Command::new("sh");
            cmd.arg(&launcher_path).arg(&normalized_app_id);
            for path in &file_paths {
                cmd.arg(path);
            }
            let _ = cmd.spawn();
        }
    }

    pub fn get_context_menu_bounds(&self, _phys_width: i32, _phys_height: i32, scale_factor: f32) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = crate::render::context_menu::get_context_menu_bounds(
            self.menu_state.x as i32,
            self.menu_state.y as i32,
            self.width as i32,
            self.height as i32,
            self.menu_state.items.len() as i32,
            scale_factor as f32,
        );
        ((x as f32).round() as i32, (y as f32).round() as i32, (w as f32).round() as i32, (h as f32).round() as i32)
    }

    /// Returns true if at least one window, pinned app, or background icon search is active
    pub fn is_animating(&self) -> bool {
        let windows_loading = self.open_windows
            .values()
            .any(|win| !win.icon_resolved && win.icon_rgba.is_none());

        let pinned_loading = self.pinned_apps
            .iter()
            .any(|app_id| !self.icon_cache.contains_key(app_id));

        let searches_pending = !self.pending_icon_searches.is_empty();

        windows_loading || pinned_loading || searches_pending
    }

    pub fn close_application_completely(&mut self, app_id: &str) {
        let mut pids_to_kill = Vec::new();
        let target_app_lower = app_id.to_lowercase();

        for win in self.open_windows.values_mut() {
            let win_app_id = if !win.app_id.is_empty() {
                win.app_id.to_lowercase()
            } else {
                win.title.to_lowercase()
            };

            if win_app_id == target_app_lower
                || win_app_id.starts_with(&target_app_lower)
                || target_app_lower.contains(&win_app_id)
            {
                win.handle.close();
                if let Some(pid) = win.matched_pid {
                    pids_to_kill.push(pid);
                }
            }
        }

        if target_app_lower.contains("steam") || app_id.starts_with("steam_icon_") {
            let _ = std::process::Command::new("steam")
                .arg("-shutdown")
                .spawn();
        }

        for pid in &pids_to_kill {
            unsafe { libc::kill(*pid as i32, libc::SIGTERM); }
        }

        self.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let mut clean_name = target_app_lower.as_str();
        if !clean_name.starts_with("steam_icon_") {
            if let Some(idx) = clean_name.rfind('_') {
                if clean_name[idx+1..].chars().all(|c| c.is_numeric()) {
                    clean_name = &clean_name[..idx];
                }
            }
        }

        for (pid, proc_) in self.sys_scanner.processes() {
            let proc_name = proc_.name().to_string_lossy().to_lowercase();
            if proc_name == clean_name || proc_name.contains(clean_name) || clean_name.contains(&proc_name) {
                unsafe { libc::kill(pid.as_u32() as i32, libc::SIGTERM); }
            }
        }

        self.needs_redraw = true;
    }

    pub fn cycle_window_for_app(&mut self, target_app_id: &str, reverse: bool) {
        let mut matching_windows: Vec<(&wayland_client::backend::ObjectId, &crate::models::WindowDiagnostics)> = self
            .open_windows
            .iter()
            .filter(|(_, win)| {
                let id = if !win.app_id.is_empty() {
                    win.app_id.as_str()
                } else if !win.title.is_empty() {
                    win.title.as_str()
                } else {
                    "Unknown"
                };
                id == target_app_id
            })
            .collect();

        if matching_windows.len() <= 1 { return; }

        matching_windows.sort_by(|a, b| a.1.title.cmp(&b.1.title));

        let active_idx = matching_windows.iter().position(|(_, win)| win.is_activated);

        let count = matching_windows.len();
        let next_idx = match active_idx {
            Some(idx) => {
                if reverse { (idx + count - 1) % count } else { (idx + 1) % count }
            }
            None => 0,
        };

        let (_, target_win) = matching_windows[next_idx];
        if let Some(ref seat) = self.wl_seat {
            target_win.handle.activate(seat);
            self.needs_redraw = true;
        }
    }

    /// Sends an icon load request to the background worker pool asynchronously
    pub fn request_icon_load(&mut self, app_id: String) {
        if self.icon_cache.contains_key(&app_id) || !self.pending_icon_searches.insert(app_id.clone()) {
            return;
        }

        // Force an immediate redraw so the fallback animation starts playing on frame 1
        self.needs_redraw = true;

        // Push into the channel queue instantly with zero thread-spawn overhead
        self.icon_load.request(app_id);
    }

    /// Polls background load queue and returns true if any new icons were inserted
    pub fn process_loaded_icons(&mut self) -> bool {
        let mut updated = false;

        while let Ok(result) = self.icon_rx.try_recv() {
            self.pending_icon_searches.remove(&result.app_id);
            self.icon_cache.insert(result.app_id.clone(), (result.rgba, result.size));
            if let Some(anim) = result.animation {
                self.animations.insert(result.app_id, anim); // <--- Register active animation
            }
            updated = true;
        }

        updated
    }

    pub fn update_window_icon(&mut self, window_id: ObjectId) {
        if let Some(window) = self.open_windows.get_mut(&window_id) {
            if window.icon_resolved { return; }
            
            let mut search_id = if !window.app_id.trim().is_empty() {
                window.app_id.trim().to_string()
            } else {
                window.title.trim().to_string()
            };

            let lower_id = search_id.to_lowercase();
            if lower_id.contains("task manager") || lower_id == "taskman" {
                search_id = "taskman".to_string();
            }
            
            if !search_id.starts_with("steam_icon_") {
                if let Some(idx) = search_id.rfind('_') {
                    if search_id[idx+1..].chars().all(|c| c.is_numeric()) {
                        search_id = search_id[..idx].to_string();
                    }
                }
            }

            self.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            if window.matched_pid.is_none() {
                let target_app = search_id.to_lowercase();
                if let Some((pid, _)) = self.sys_scanner.processes().iter().find(|(_, p)| {
                    let proc_name = p.name().to_string_lossy().to_lowercase();
                    proc_name.contains(&target_app) || target_app.contains(&proc_name)
                }) {
                    window.matched_pid = Some(pid.as_u32());
                }
            }

            let pid_opt = window.matched_pid.map(sysinfo::Pid::from_u32);

            if let Some((appid, steam_name, steam_icon_path)) = dockman_lib::resolve_steam_game_details(&search_id, &window.title, &self.sys_scanner, pid_opt) {
                let target_size = 48;
                if let Some((_, _, rgba_data)) = crate::terminal_graphics::load_image_raw_rgba(&steam_icon_path, target_size) {
                    let icon_key = format!("steam_icon_{}", appid);
                    window.app_name = steam_name;
                    window.app_id = icon_key.clone();
                    window.icon_name = icon_key.clone();
                    window.icon_rgba = Some(rgba_data.clone());
                    window.icon_size = target_size;
                    window.icon_resolved = true;
                    self.icon_cache.insert(icon_key.clone(), (rgba_data.clone(), target_size));
                    if self.pinned_apps.contains(&icon_key) {
                        crate::cache::save_cached_icon(&icon_key, target_size, target_size, &rgba_data);
                    }
                    return;
                }
            }

            for pinned_id in &self.pinned_apps {
                let p_lower = pinned_id.to_lowercase();
                let s_lower = search_id.to_lowercase();
                if p_lower == s_lower && !p_lower.starts_with("steam_icon_") && !s_lower.starts_with("steam_icon_") {
                    search_id = pinned_id.clone();
                    window.app_id = search_id.clone();
                    break;
                }
            }

            if !search_id.is_empty() && !crate::icon_utils::get_icon_from_desktop(&search_id).is_some() {
                if let Some(resolved_id) = crate::icon_utils::find_desktop_file_by_exec(&search_id) {
                    search_id = resolved_id;
                    window.app_id = search_id.clone();
                }
            }

            if !search_id.is_empty() {
                let icon_name = crate::icon_utils::extract_icon_name(&search_id);
                let icon_path = crate::get_icon_path(&search_id);
                
                let mut raw_pixels = None;
                let target_size = 48;
                if let Some(path) = icon_path {
                    if let Some((_, _, rgba_data)) = crate::terminal_graphics::load_image_raw_rgba(&path, target_size) {
                        raw_pixels = Some(rgba_data.clone());
                        self.icon_cache.insert(search_id.clone(), (rgba_data.clone(), target_size));
                        if self.pinned_apps.contains(&search_id) {
                            crate::cache::save_cached_icon(&search_id, target_size, target_size, &rgba_data);
                        }
                    }
                }
                window.icon_name = icon_name;
                window.icon_rgba = raw_pixels.clone();
                window.icon_size = target_size;
                if raw_pixels.is_some() {
                    window.icon_resolved = true;
                }
            }
        }
    }
}