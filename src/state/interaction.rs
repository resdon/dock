use super::AppState;
use std::path::PathBuf;
use std::process::Command;
use wayland_client::QueueHandle;

use crate::geometry::popup::{PopupGeometry, PopupType};

impl AppState {
    pub fn update_focus_timer(&mut self) {
        if self.focus_action_performed {
            if let Some(time) = self.focus_action_time {
                if time.elapsed() >= std::time::Duration::from_secs(1) {
                    self.focus_action_performed = false;
                    self.focus_action_time = None;
                }
            }
        }
    }

    pub fn update_dnd_hover_target(&mut self, x: f64, y: f64) {
        let new_app = self.get_app_id_at_location(x, y);
        let apps_in_dock = self.get_apps_in_dock();
        let new_index = new_app.and_then(|app_id| apps_in_dock.iter().position(|id| id == &app_id));

        if self.dnd_state.hovered_dock_index != new_index {
            self.dnd_state.hovered_dock_index = new_index;
            self.needs_redraw = true;
        }
    }

    pub fn handle_file_drop_on_icon(&mut self, x: f64, y: f64, file_paths: Vec<PathBuf>) {
        if file_paths.is_empty() {
            return;
        }

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

    /// Computes window-list bounds in the dock surface's physical coordinate space.
    /// Popup subsurfaces are children of the dock, so y=0 is the top of the dock.
    pub fn get_window_list_bounds(
        &self,
        phys_width: i32,
        phys_height: i32,
        scale_factor: f32,
    ) -> (i32, i32, i32, i32) {
        let dock = self.docks.first();
        let dock_width = dock
            .map(|d| (d.width as f64 * d.scale_factor).round() as i32)
            .unwrap_or(phys_width.max(1));
        let dock_height = dock
            .map(|d| (d.height as f64 * d.scale_factor).round() as i32)
            .unwrap_or(phys_height.max(1));
        let scale = dock.map(|d| d.scale_factor as f32).unwrap_or(scale_factor);

        let target = self.window_list_state.target_app_id.as_deref().unwrap_or_default();
        let apps = self.get_apps_in_dock();
        let anchor_x = crate::pointer::coordinates::calculate_popup_anchor(
            self, target, &apps, dock_width, scale as f64, (60, 48, 8, 0),
        );
        let item_count = self
            .open_windows
            .values()
            .filter(|w| w.app_id == target)
            .count() as i32;
        let geom = PopupGeometry::compute_bounds(
            PopupType::WindowList, anchor_x, 0, dock_width, dock_height, item_count, scale,
        );
        (geom.x, geom.y, geom.logical_width, geom.logical_height)
    }

    /// Computes context-menu bounds in the dock surface's physical coordinate space.
    pub fn get_context_menu_bounds(
        &self,
        phys_width: i32,
        phys_height: i32,
        scale_factor: f32,
    ) -> (i32, i32, i32, i32) {
        let dock = self.docks.first();
        let dock_width = dock
            .map(|d| (d.width as f64 * d.scale_factor).round() as i32)
            .unwrap_or(phys_width.max(1));
        let dock_height = dock
            .map(|d| (d.height as f64 * d.scale_factor).round() as i32)
            .unwrap_or(phys_height.max(1));
        let scale = dock.map(|d| d.scale_factor as f32).unwrap_or(scale_factor);

        let target = self.menu_state.target_app_id.as_deref().unwrap_or_default();
        let apps = self.get_apps_in_dock();
        let anchor_x = crate::pointer::coordinates::calculate_popup_anchor(
            self, target, &apps, dock_width, scale as f64, (60, 48, 8, 0),
        );
        let item_count = self.menu_state.items.len() as i32;
        let geom = PopupGeometry::compute_bounds(
            PopupType::ContextMenu, anchor_x, 0, dock_width, dock_height, item_count, scale,
        );
        (geom.x, geom.y, geom.logical_width, geom.logical_height)
    }

    /// Sends an icon load request to the background worker pool asynchronously
    pub fn request_icon_load(&mut self, app_id: String) {
        if self.icon_cache.contains_key(&app_id)
            || !self.pending_icon_searches.insert(app_id.clone())
        {
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
            self.icon_cache
                .insert(result.app_id.clone(), (result.rgba, result.size));
            if let Some(anim) = result.animation {
                self.animations.insert(result.app_id, anim);
            }
            updated = true;
        }

        updated
    }

    pub fn update_hover_and_hide_timers(&mut self) -> bool {
        let mut state_changed = false;
		/*
        // 1. Immediately dismiss hover popup when pointer leaves surface
        if !self.interaction.pointer_inside && self.hover_state.is_visible {
            self.hover_state.is_visible = false;
            self.hover_state.app_id = None;
            self.hover_state.last_leave_time = None;
            self.needs_redraw = true;
            state_changed = true;
        }
        */

        // 2. Evaluate proximity (sole authority for proximity updates)
        let is_near = self.check_dock_proximity();
        self.interaction.is_pointer_near = is_near;

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
        let content_width = self.width;

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
