use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use wayland_client::QueueHandle;
use super::AppState;

const BOX_SIZE: f64 = 48.0;
const SPACING: f64 = 12.0;
const MAX_DOCK_WIDTH: f64 = 800.0;
pub const DOCK_HEIGHT: f64 = 60.0;

impl AppState {
    /// Returns the current ordered list of app IDs in the dock (pinned + unpinned running).
    pub fn get_apps_in_dock(&self) -> Vec<String> {
        let mut apps_in_dock = self.pinned_apps.clone();
        for window in self.open_windows.values() {
            let id = window.resolved_app_id().to_string();
            if !apps_in_dock.contains(&id) {
                apps_in_dock.push(id);
            }
        }
        apps_in_dock
    }

    /// Builds a map of App IDs to their sorted open window ObjectIds.
    pub fn get_running_by_app(&self) -> HashMap<String, Vec<ObjectId>> {
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new();

        for (id, window) in &self.open_windows {
            running_by_app
                .entry(window.resolved_app_id().to_string())
                .or_default()
                .push(id.clone());
        }

        for windows in running_by_app.values_mut() {
            windows.sort_by(|a, b| {
                let title_a = self.open_windows.get(a).map(|w| w.title.as_str()).unwrap_or("");
                let title_b = self.open_windows.get(b).map(|w| w.title.as_str()).unwrap_or("");
                title_a.cmp(title_b)
            });
        }

        running_by_app
    }

    pub fn get_app_id_at_location(&self, x: f64, y: f64) -> Option<String> {
        let screen_height = self.height as f64;
        let dock_top_bound = screen_height - DOCK_HEIGHT;

        // Check vertical boundaries (top and bottom)
        if y < dock_top_bound || y > screen_height {
            return None;
        }

        let apps_in_dock = self.get_apps_in_dock();
        let total_items = apps_in_dock.len();
        if total_items == 0 {
            return None;
        }

        let slot_width = BOX_SIZE + SPACING;
        let calculated_width = (total_items as f64 * BOX_SIZE + (total_items + 1) as f64 * SPACING)
            .min(MAX_DOCK_WIDTH);

        let start_offset_x = if (self.width as f64) > calculated_width {
            ((self.width as f64) - calculated_width) / 2.0
        } else {
            0.0
        };

        let hit_start_min = start_offset_x + (SPACING / 2.0);
        let hit_end_max = start_offset_x + calculated_width - (SPACING / 2.0);

        if x < hit_start_min || x > hit_end_max {
            return None;
        }

        // Direct O(1) slot calculation relative to hit region
        let index = ((x - hit_start_min) / slot_width) as usize;
        let index = index.min(total_items - 1);

        apps_in_dock.get(index).cloned()
    }

    /// Single authoritative check for mouse proximity over dock surface or active popups
    pub fn check_dock_proximity(&self) -> bool {
        // 1. Context menu explicitly keeps the dock open while open
        if self.menu_state.is_open {
            return true;
        }

        // 2. If the pointer is not inside the Wayland surface, proximity is false
        if !self.interaction.pointer_inside {
            return false;
        }

        // 3. Compute actual rendered dock bounding box (in logical coordinates)
        let apps_in_dock = self.get_apps_in_dock();
        let total_items = apps_in_dock.len();
        if total_items == 0 {
            return false;
        }

        let calculated_width = (total_items as f64 * BOX_SIZE + (total_items + 1) as f64 * SPACING)
            .min(MAX_DOCK_WIDTH);

        // Centered horizontal bounds
        let start_x = if (self.width as f64) > calculated_width {
            ((self.width as f64) - calculated_width) / 2.0
        } else {
            0.0
        };
        let end_x = start_x + calculated_width;

        // Bottom-anchored vertical bounds
        let top_y = ((self.height as f64) - DOCK_HEIGHT).max(0.0);
        let bottom_y = self.height as f64;

        // 4. Test pointer coordinates with a 15px margin tolerance
        let margin = 15.0;
        let px = self.interaction.pointer_position.x;
        let py = self.interaction.pointer_position.y;

        let within_x = px >= (start_x - margin) && px <= (end_x + margin);
        let within_y = py >= (top_y - margin) && py <= (bottom_y + margin);

        within_x && within_y
    }

    pub fn set_dock_input_region(
        &self,
        is_hidden: bool,
        container_start_x: i32,
        container_width: i32,
        qh: &QueueHandle<AppState>,
    ) {
        // Iterate through active output dock instances
        for dock in &self.docks {
            dock.update_input_region(&self.compositor_state, is_hidden, container_start_x, container_width, qh);
        }
    }
}
