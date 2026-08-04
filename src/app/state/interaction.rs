use std::path::PathBuf;
use std::process::Command;
use wayland_client::QueueHandle;
use super::AppState;

impl AppState {
    pub fn update_dnd_hover_target(&mut self, x: f64, y: f64) {
        let new_app = self.get_app_id_at_location(x, y);
        let apps_in_dock = self.get_apps_in_dock();
        let new_index = new_app.and_then(|app_id| {
            apps_in_dock.iter().position(|id| id == &app_id)
        });

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

    pub fn get_context_menu_bounds(&self, phys_width: i32, phys_height: i32, scale_factor: f32) -> (i32, i32, i32, i32) {
        let (x, y, w, h) = crate::render::context_menu::get_context_menu_bounds(
            self.menu_state.x as i32,
            self.menu_state.y as i32,
            phys_width,  
            phys_height, 
            self.menu_state.items.len() as i32,
            scale_factor as f32,
        );
        ((x as f32).round() as i32, (y as f32).round() as i32, (w as f32).round() as i32, (h as f32).round() as i32)
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
                self.animations.insert(result.app_id, anim); 
            }
            updated = true;
        }

        updated
    }
    
    pub fn update_hover_and_hide_timers(&mut self) -> bool {
        let mut state_changed = false;

        // 1. Immediately dismiss hover popup when pointer leaves surface
        if !self.interaction.pointer_inside {
            if self.hover_state.is_visible {
                self.hover_state.is_visible = false;
                self.hover_state.app_id = None;
                self.hover_state.last_leave_time = None;
                self.needs_redraw = true;
                state_changed = true;
            }
        }

        // 2. Evaluate proximity (sole authority for proximity updates)
        let is_near = self.check_dock_proximity();
        self.hide_state.update_proximity(is_near);

        // 3. Advance fade animation
        if self.hide_state.tick() {
            self.needs_redraw = true;
            state_changed = true;
        }

        state_changed
    }

    /// Advances animation timers and updates compositor input regions on visibility changes.
    pub fn update_animations(&mut self, qh: &QueueHandle<AppState>) {
        let state_changed = self.hide_state.tick(); 

        // Full surface horizontal bounds
        let container_start_x = 0;
        let content_width = self.width as i32;

        // Synchronize Wayland input regions when transitioning between hidden and visible
        if self.hide_state.just_became_hidden {
            self.set_dock_input_region(true, container_start_x, content_width, qh);
            self.hide_state.just_became_hidden = false;
        } else if self.hide_state.just_became_visible {
            self.set_dock_input_region(false, 0, 0, qh);
            self.hide_state.just_became_visible = false;
        }
        
        if state_changed {
            self.needs_redraw = true;
        }
    }
}
