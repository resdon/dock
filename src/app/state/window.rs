use wayland_client::backend::ObjectId;
use super::AppState;

impl AppState {
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
            .filter(|(_, win)| win.resolved_app_id() == target_app_id)
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

    pub fn update_window_list(&mut self, active_app_ids: &[String]) {
        let mut state_changed = false;

        // Clear hover popup if the hovered app is no longer running
        if let Some(ref hovered_id) = self.hover_state.app_id {
            if !active_app_ids.iter().any(|id| id == hovered_id) {
                self.hover_state.is_visible = false;
                self.hover_state.app_id = None;
                state_changed = true;
            }
        }

        // Only set needs_redraw if a state change occurred AND dock is visible/interactive
        if state_changed && (self.interaction.pointer_inside || !self.hide_state.is_fully_hidden()) {
            self.needs_redraw = true;
        }
    }
}
