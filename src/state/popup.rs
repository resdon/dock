//! Popup open/close logic.
//!
//! Manages popup lifecycle: opening, closing, and state transitions.

use super::AppState;
use std::fs;
use wayland_client::backend::ObjectId;

/// Read the process group ID from /proc/<pid>/stat.
/// Returns 0 if the file cannot be read or parsed.
fn read_proc_pgid(pid: u32) -> u32 {
    let path = format!("/proc/{}/stat", pid);
    if let Ok(contents) = fs::read_to_string(&path) {
        // Format: pid (comm) state ppid pgrp ...
        // Find the closing paren to skip the comm field (which may contain spaces)
        if let Some(close_paren) = contents.rfind(')') {
            let after_comm = &contents[close_paren + 2..]; // skip ") "
            let fields: Vec<&str> = after_comm.split_whitespace().collect();
            // fields[0] = state, fields[1] = ppid, fields[2] = pgrp
            if fields.len() >= 3 {
                if let Ok(pgid) = fields[2].parse::<u32>() {
                    return pgid;
                }
            }
        }
    }
    0
}

fn normalize_app_id(app_id: &str) -> &str {
    if app_id.starts_with("steam_icon_") {
        return app_id;
    }
    if let Some(idx) = app_id.rfind('_') {
        if app_id[idx + 1..].chars().all(|c| c.is_numeric()) {
            return &app_id[..idx];
        }
    }
    app_id
}

fn app_matches_target(candidate: &str, target: &str) -> bool {
    let candidate_norm = normalize_app_id(candidate).to_ascii_lowercase();
    let target_norm = normalize_app_id(target).to_ascii_lowercase();
    candidate_norm == target_norm
}

impl AppState {
    pub fn set_popup_input_region(
        &self,
        surface: &wayland_client::protocol::wl_surface::WlSurface,
        width: i32,
        height: i32,
        scale_factor: f32,
    ) {
        // Call create_region on the underlying wl_compositor() using self.qh
        let region = self
            .compositor_state
            .wl_compositor()
            .create_region(&self.qh, ());

        let logical_w = (width as f32 / scale_factor).round().max(1.0) as i32;
        let logical_h = (height as f32 / scale_factor).round().max(1.0) as i32;

        region.add(0, 0, logical_w, logical_h);
        surface.set_input_region(Some(&region));
        region.destroy();
    }

    pub fn close_application_completely(&mut self, app_id: &str) {
        let my_pid = unsafe { libc::getpid() as u32 };
        let my_ppid = unsafe { libc::getppid() as u32 };
        let my_pgid = unsafe { libc::getpgrp() as u32 };
        let target_norm = normalize_app_id(app_id).to_ascii_lowercase();
        let mut pids_to_kill = Vec::new();

        let targets: Vec<(ObjectId, Option<u32>)> = self
            .windows
            .values()
            .filter_map(|win| {
                let win_app_id = win
                    .app_id
                    .as_ref()
                    .map(|id| id.as_str())
                    .filter(|id| !id.is_empty())
                    .unwrap_or("");
                let win_title = win.title.as_str();
                let window_identifier = if !win_app_id.is_empty() {
                    win_app_id
                } else {
                    win_title
                };
                if app_matches_target(window_identifier, &target_norm) {
                    Some((win.id.0.clone(), win.process_id.map(|pid| pid.0)))
                } else {
                    None
                }
            })
            .collect();

        for (object_id, pid) in targets {
            if let Some(handle) = self.window_handle(&object_id) {
                handle.close();
            }
            if let Some(pid) = pid {
                if pid == my_pid || pid == my_ppid {
                    continue;
                }

                // Only terminate the exact process we have a direct PID for. This avoids
                // searching the full system and prevents killing unrelated apps by name.
                let pid_pgid = read_proc_pgid(pid);
                if pid_pgid != my_pgid && pid > 1 {
                    pids_to_kill.push(pid);
                }
            }
        }

        for pid in &pids_to_kill {
            if *pid > 1 && *pid != my_pid && *pid != my_ppid {
                unsafe {
                    libc::kill(*pid as i32, libc::SIGTERM);
                }
            }
        }

        self.needs_redraw = true;
    }

    pub fn cycle_window_for_app(&mut self, target_app_id: &str, reverse: bool) {
        let mut matching_windows: Vec<&crate::state::WindowState> = self
            .windows
            .values()
            .filter(|win| {
                win.app_id.as_ref().map(|id| id.as_str()) == Some(target_app_id)
                    || win.title == target_app_id
            })
            .collect();

        if matching_windows.len() <= 1 {
            return;
        }

        matching_windows.sort_by_key(|win| win.order_index);

        let active_idx = matching_windows.iter().position(|win| win.focused);
        let count = matching_windows.len();
        let next_idx = match active_idx {
            Some(idx) if reverse => (idx + count - 1) % count,
            Some(idx) => (idx + 1) % count,
            None => 0,
        };

        let target_id = matching_windows[next_idx].id.0.clone();
        if let Some(seat) = &self.wl_seat {
            let handle = self.window_handles.get(&target_id).cloned();
            if let Some(handle) = handle {
                handle.activate(seat);
            }
            self.needs_redraw = true;
        }
    }

    pub fn update_window_icon(&mut self, window_id: ObjectId) {
        let Some(window_id) = self.window_id_for_object(&window_id) else {
            return;
        };

        let Some(app_id) = self.windows.get(&window_id).and_then(|w| {
            w.app_id
                .as_ref()
                .map(|id| id.to_string())
                .filter(|id| !id.is_empty())
                .or_else(|| (!w.title.is_empty()).then(|| w.title.clone()))
        }) else {
            return;
        };

        if let Some((rgba, size)) = self.icon_cache.get(&app_id).cloned() {
            if let Some(window) = self.windows.get_mut(&window_id) {
                window.icon_rgba = Some(rgba);
                window.icon_size = size;
                window.icon_name = app_id;
                window.icon_resolved = true;
            }
        } else {
            self.request_icon_load(app_id);
        }
    }

    pub fn update_window_list(&mut self, active_app_ids: &[String]) {
        let mut state_changed = false;

        // Clear hover popup if the hovered app is no longer running
        if let Some(ref hovered_id) = self.hover_state.app_id {
            if !active_app_ids.contains(hovered_id) {
                self.hover_state.is_visible = false;
                self.hover_state.app_id = None;
                state_changed = true;
            }
        }

        // Only set needs_redraw if a state change occurred AND dock is visible/interactive
        if state_changed && (self.interaction.pointer_inside) {
            self.needs_redraw = true;
        }
    }
    /// Starts the context-menu fade-out without destroying its state. The
    /// renderer keeps the subsurface alive until the fade reaches zero.
    pub fn request_context_menu_close(&mut self) -> bool {
        let was_open = self.menu_state.is_open
            || self.menu_state.target_app_id.is_some()
            || self.menu_state.target_tray_item.is_some()
            || self
                .docks
                .iter()
                .any(|dock| !dock.menu_fade.is_fully_hidden());
        if !was_open {
            return false;
        }

        self.menu_state.just_opened = false;
        self.menu_state.waiting_for_initial_release = false;
        self.menu_state.pointer_inside_popup = false;
        for dock in &mut self.docks {
            dock.menu_fade.hide();
        }
        self.needs_redraw = true;
        true
    }

    /// Closes the context menu immediately, used when switching to another
    /// popup type so two mutually-exclusive menus cannot overlap.
    pub fn close_context_menu_now(&mut self) -> bool {
        let had_state = self.menu_state.is_open
            || self.menu_state.target_app_id.is_some()
            || self.menu_state.target_tray_item.is_some()
            || !self.menu_state.items.is_empty();
        self.menu_state.is_open = false;
        self.menu_state.target_app_id = None;
        self.menu_state.target_window = None;
        self.menu_state.target_tray_item = None;
        self.menu_state.anchor_override = None;
        self.menu_state.items.clear();
        self.menu_state.just_opened = false;
        self.menu_state.waiting_for_initial_release = false;
        self.menu_state.opened_by_button = None;
        self.menu_state.pointer_inside_popup = false;
        for dock in &mut self.docks {
            dock.menu_fade.current_alpha = 0.0;
            dock.menu_fade.target_alpha = 0.0;
        }
        if had_state {
            self.needs_redraw = true;
        }
        had_state
    }

    /// Starts the window-list fade-out without clearing its target until the
    /// animation is fully hidden. Keeping the target stable prevents the popup
    /// anchor from jumping during the fade.
    pub fn request_window_list_close(&mut self) -> bool {
        let was_open = self.window_list_state.is_open
            || self.window_list_state.target_app_id.is_some()
            || self
                .docks
                .iter()
                .any(|dock| !dock.window_list_fade.is_fully_hidden());
        if !was_open {
            return false;
        }

        self.window_list_state.is_open = false;
        self.window_list_state.target_app_id = None;
        self.window_list_state.target_window = None;
        self.window_list_state.just_opened = false;
        self.window_list_state.waiting_for_initial_release = false;
        self.window_list_state.pointer_inside_popup = false;
        for dock in &mut self.docks {
            dock.window_list_fade.hide();
        }
        self.needs_redraw = true;
        true
    }

    /// Closes the window list immediately when switching to context-menu mode.
    pub fn close_window_list_now(&mut self) -> bool {
        let had_state =
            self.window_list_state.is_open || self.window_list_state.target_app_id.is_some();
        self.window_list_state.is_open = false;
        self.window_list_state.target_app_id = None;
        self.window_list_state.target_window = None;
        self.window_list_state.just_opened = false;
        self.window_list_state.waiting_for_initial_release = false;
        self.window_list_state.pointer_inside_popup = false;
        for dock in &mut self.docks {
            dock.window_list_fade.current_alpha = 0.0;
            dock.window_list_fade.target_alpha = 0.0;
        }
        if had_state {
            self.needs_redraw = true;
        }
        had_state
    }

    /// Finishes popup state cleanup after a fade reaches alpha 0.
    pub fn finalize_popup_closures(&mut self) -> bool {
        let mut changed = false;
        let menu_hidden = !self.docks.is_empty()
            && self
                .docks
                .iter()
                .all(|dock| dock.menu_fade.is_fully_hidden());
        if self.menu_state.is_open && menu_hidden {
            self.menu_state.is_open = false;
            self.menu_state.target_app_id = None;
            self.menu_state.target_window = None;
            self.menu_state.target_tray_item = None;
            self.menu_state.anchor_override = None;
            self.menu_state.items.clear();
            self.menu_state.just_opened = false;
            self.menu_state.waiting_for_initial_release = false;
            self.menu_state.opened_by_button = None;
            self.menu_state.pointer_inside_popup = false;
            changed = true;
        }

        let list_hidden = !self.docks.is_empty()
            && self
                .docks
                .iter()
                .all(|dock| dock.window_list_fade.is_fully_hidden());
        if self.window_list_state.is_open && list_hidden {
            self.window_list_state.is_open = false;
            self.window_list_state.target_app_id = None;
            self.window_list_state.target_window = None;
            self.window_list_state.just_opened = false;
            self.window_list_state.waiting_for_initial_release = false;
            self.window_list_state.pointer_inside_popup = false;
            changed = true;
        }

        if changed {
            self.needs_redraw = true;
        }
        changed
    }
}
