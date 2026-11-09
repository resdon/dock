//! Interaction state helpers.
//!
//! Utility methods for querying and updating interaction state.

use super::AppState;
use std::path::PathBuf;
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
            crate::handlers::dock::launch_app_with_files(&app_id, &file_paths);
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

        let target = self
            .window_list_state
            .target_app_id
            .as_deref()
            .unwrap_or_default();
        let apps = self.get_apps_in_dock();
        let anchor_x = crate::pointer::coordinates::calculate_popup_anchor(
            self,
            target,
            &apps,
            dock_width,
            scale as f64,
            (60, 48, 8, 0),
        );
        let item_count = self.window_count_for_app(target) as i32;
        let anchor_y = 0;
        let geom = PopupGeometry::compute_bounds(
            PopupType::WindowList,
            anchor_x,
            anchor_y,
            dock_width,
            dock_height,
            item_count,
            scale,
        );
        (geom.x, geom.y, geom.phys_width, geom.phys_height)
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

        let (anchor_x, anchor_y) = if let Some((x, y)) = self.menu_state.anchor_override {
            (
                (x as f64 * scale as f64).round() as i32,
                (y as f64 * scale as f64).round() as i32,
            )
        } else {
            let target = self.menu_state.target_app_id.as_deref().unwrap_or_default();
            let apps = self.get_apps_in_dock();
            let anchor_x = crate::pointer::coordinates::calculate_popup_anchor(
                self,
                target,
                &apps,
                dock_width,
                scale as f64,
                (60, 48, 8, 0),
            );
            let anchor_y = 0;
            (anchor_x, anchor_y)
        };
        let item_count = self.menu_state.items.len() as i32;
        let geom = PopupGeometry::compute_bounds(
            PopupType::ContextMenu,
            anchor_x,
            anchor_y,
            dock_width,
            dock_height,
            item_count,
            scale,
        );
        (geom.x, geom.y, geom.phys_width, geom.phys_height)
    }

    /// Sends an icon load request to the background worker pool asynchronously
    pub fn request_icon_load(&mut self, app_id: String) {
        if self.icon_cache.contains_key(&app_id)
            || !self.pending_icon_searches.insert(app_id.clone())
        {
            return;
        }

        // Present a loading/fallback result as soon as the background request
        // completes, without creating a periodic animation on the main thread.
        self.needs_redraw = true;

        // Push into the channel queue instantly with zero thread-spawn overhead
        self.icon_load.request(app_id);
    }

    /// Polls background load queue and returns true if any new icons were inserted
    pub fn process_loaded_icons(&mut self) -> bool {
        let mut updated = false;

        while let Ok(result) = self.icon_rx.try_recv() {
            self.pending_icon_searches.remove(&result.app_id);
            let app_id = result.app_id.clone();
            self.icon_cache
                .insert(app_id.clone(), (result.rgba, result.size));
            if let Some(anim) = result.animation {
                self.animations.insert(app_id, anim);
            } else {
                // A previously animated icon can become static after a reload.
                // Remove the old clock so it cannot keep the main loop awake.
                self.animations.remove(&app_id);
            }
            updated = true;
        }

        updated
    }

    pub fn update_hover_and_hide_timers(&mut self) -> bool {
        let state_changed = false;
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

        // Hide animation timing is owned by interaction::update(). This legacy
        // helper only evaluates proximity; it must never become a second clock.
        state_changed
    }

    /// Advances animation timers and updates compositor input regions on visibility changes.
    pub fn update_animations(&mut self, qh: &QueueHandle<AppState>) {
        // Do not tick hide_state here.  interaction::update() is the sole owner
        // of the hide animation clock. This method only applies edge transitions
        // to the Wayland input region.
        // Full surface horizontal bounds
        let container_start_x = 0;

        // Synchronize Wayland input regions when transitioning between hidden and visible.
        // A zero width means each DockInstance's negotiated width is used.
        if self.hide_state.just_became_hidden {
            self.set_dock_input_region(true, container_start_x, 0, qh);
            self.hide_state.just_became_hidden = false;
        } else if self.hide_state.just_became_visible {
            self.set_dock_input_region(false, 0, 0, qh);
            self.hide_state.just_became_visible = false;
        }

        // No redraw is requested here: the caller already observes the animation
        // transition from interaction::update().
    }
}
