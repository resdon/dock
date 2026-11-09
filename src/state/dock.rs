//! Dock surface state.
//!
//! Calculates dock width and manages surface dimensions.

use super::AppState;
use smithay_client_toolkit::shell::WaylandSurface;
use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use wayland_client::QueueHandle;

pub const BOX_SIZE: f64 = 48.0;
pub const SPACING: f64 = 8.0;
pub const DOCK_HEIGHT: f64 = 60.0;

impl AppState {
    pub(crate) fn canonical_running_app_id(&self, raw_id: &str) -> String {
        let normalized = crate::handlers::dock::normalize_app_id(raw_id);

        if let Some(pinned) = self
            .pinned_apps
            .iter()
            .find(|id| id.eq_ignore_ascii_case(&normalized) || id.eq_ignore_ascii_case(raw_id))
        {
            return pinned.clone();
        }

        // Desktop IDs differ from the process names reported by these apps.
        // Keep the default app pins attached to their live windows.
        let lowered = normalized.to_ascii_lowercase();
        let requested_pin = if lowered.contains("amnezia") {
            Some("AmneziaVPN")
        } else if lowered.contains("qbittorrent") {
            Some("org.qbittorrent.qBittorrent")
        } else {
            None
        };
        if let Some(pin) = requested_pin.and_then(|wanted| {
            self.pinned_apps
                .iter()
                .find(|id| id.eq_ignore_ascii_case(wanted))
        }) {
            return pin.clone();
        }

        // WezTerm can report GUI/client aliases such as `wezterm-gui` while
        // the pinned desktop entry is `org.wezfurlong.wezterm`. Canonicalize
        // those variants before building the dock item list.
        if normalized.contains("wezterm")
            && self
                .pinned_apps
                .iter()
                .any(|id| id.eq_ignore_ascii_case("org.wezfurlong.wezterm"))
        {
            return self
                .pinned_apps
                .iter()
                .find(|id| id.eq_ignore_ascii_case("org.wezfurlong.wezterm"))
                .cloned()
                .unwrap_or_else(|| normalized.clone());
        }

        // Steam creates a helper window process whose app-id is commonly
        // `steamwebhelper`, while the desktop application is `steam`. Keep the
        // running window attached to the pinned Steam icon instead of creating
        // a second icon.
        if (normalized.eq_ignore_ascii_case("steamwebhelper")
            || normalized.eq_ignore_ascii_case("steamwebhelper-bin"))
            && self
                .pinned_apps
                .iter()
                .any(|id| id.eq_ignore_ascii_case("steam"))
        {
            return self
                .pinned_apps
                .iter()
                .find(|id| id.eq_ignore_ascii_case("steam"))
                .cloned()
                .unwrap_or_else(|| normalized.clone());
        }

        normalized
    }

    /// Returns the current ordered list of app IDs in the dock (pinned + unpinned running).
    pub fn get_apps_in_dock(&self) -> Vec<String> {
        // Pinned order is authoritative. Running/unpinned applications are added
        // in stable window order so popup anchors cannot jump between frames.
        let mut apps_in_dock = self.pinned_apps.clone();

        let mut windows: Vec<&crate::state::WindowState> = self.windows.values().collect();
        windows.sort_by_key(|window| (window.order_index, window.id.0.protocol_id()));

        for window in windows {
            let raw_id = window
                .app_id
                .as_ref()
                .map(|id| id.to_string())
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| {
                    if !window.title.is_empty() {
                        window.title.clone()
                    } else {
                        "Unknown".to_string()
                    }
                });
            let id = self.canonical_running_app_id(&raw_id);
            if !apps_in_dock.contains(&id) {
                apps_in_dock.push(id);
            }
        }

        apps_in_dock
    }

    pub fn get_running_by_app(&self) -> HashMap<String, Vec<ObjectId>> {
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new();

        for (id, window) in &self.windows {
            let raw_id = window
                .app_id
                .as_ref()
                .map(|id| id.to_string())
                .filter(|id| !id.is_empty())
                .unwrap_or_else(|| {
                    if !window.title.is_empty() {
                        window.title.clone()
                    } else {
                        "Unknown".to_string()
                    }
                });
            let app_id = self.canonical_running_app_id(&raw_id);
            running_by_app.entry(app_id).or_default().push(id.0.clone());
        }

        for windows in running_by_app.values_mut() {
            windows.sort_by_key(|id| {
                self.window_id_for_object(id)
                    .map(|wid| {
                        self.windows
                            .get(&wid)
                            .map(|w| w.order_index)
                            .unwrap_or(u64::MAX)
                    })
                    .unwrap_or(u64::MAX)
            });
        }

        running_by_app
    }

    pub fn window_count_for_app(&self, app_id: &str) -> usize {
        self.get_running_by_app().get(app_id).map_or(0, Vec::len)
    }

    pub fn get_app_id_at_location(&self, x: f64, y: f64) -> Option<String> {
        // Pointer coordinates are local to the dock layer surface. Use the
        // actual surface dimensions, not AppState::width (which is the output
        // width), otherwise hit-testing is shifted on small dock surfaces.
        let dock = self.docks.first();
        let dock_width = dock.map(|d| d.width as f64).unwrap_or(self.width as f64);
        let app_row_height = DOCK_HEIGHT;

        if x < 0.0 || y < 0.0 || x >= dock_width || y >= app_row_height {
            return None;
        }

        let apps_in_dock = self.get_apps_in_dock();
        let total_items = apps_in_dock.len();
        if total_items == 0 {
            return None;
        }

        let total_icons_width =
            total_items as f64 * BOX_SIZE + total_items.saturating_sub(1) as f64 * SPACING;
        let start_x = ((dock_width - total_icons_width) / 2.0).max(0.0);

        for (index, app_id) in apps_in_dock.iter().enumerate() {
            let icon_x = start_x + index as f64 * (BOX_SIZE + SPACING);
            // Keep the hit area consistent with the visual spacing: half of the
            // inter-icon gap belongs to each neighboring icon.
            let hit_start = icon_x - SPACING / 2.0;
            let hit_end = icon_x + BOX_SIZE + SPACING / 2.0;
            if x >= hit_start && x < hit_end {
                return Some(app_id.clone());
            }
        }

        None
    }

    /// Single authoritative check for mouse proximity over dock surface or active popups
    pub fn check_dock_proximity(&self) -> bool {
        // A popup remains part of the interaction domain while it is visible
        // or fading. This prevents auto-hide from tearing down its parent
        // surface in the middle of a transition.
        if self.audio_panel_open
            || self.bluetooth_panel_open
            || self.rclone_panel_open
            || self.process_panel_open
            || self.menu_state.is_open
            || self.window_list_state.is_open
            || self.docks.iter().any(|dock| {
                !dock.menu_fade.is_fully_hidden() || !dock.window_list_fade.is_fully_hidden()
            })
        {
            return true;
        }

        // Pointer events are delivered in layer-surface-local coordinates.
        // There is no screen-height offset to reconstruct here. `pointer_inside`
        // already tells us whether the pointer is inside the dock's active input
        // region (full dock while visible, 3px reveal strip while hidden).
        self.interaction.pointer_inside
    }

    /// Recalculate the layer-shell width as running, unpinned applications are
    /// added to or removed from the dock. A resize is requested only when the
    /// desired width changes; the surface is not redrawn until the compositor
    /// sends its matching configure event.
    pub fn sync_dock_surface_width(&mut self, app_count: usize) -> bool {
        let tray_item_count = self.tray_items.len();
        let desired_width =
            crate::render::tray::preferred_surface_width_with_tray(app_count, tray_item_count);
        let hidden = self.hide_state.is_fully_hidden();
        let qh = self.qh.clone();
        let compositor_state = &self.compositor_state;
        let mut changed = false;

        for dock in &mut self.docks {
            if dock.requested_width == desired_width {
                continue;
            }

            let previous_width = dock.width;
            dock.requested_width = desired_width;
            if previous_width == desired_width {
                continue;
            }

            dock.width = desired_width;
            dock.configured = false;
            dock.status_strip_rendered = false;
            if let Some(widget_bar) = dock.status_widget_bar.as_mut() {
                let (bar_x, bar_y) = crate::render::widget_bar::widget_bar_position(
                    desired_width as i32,
                    tray_item_count,
                    dock.height as i32,
                );
                widget_bar.position = (bar_x, bar_y);
                widget_bar.subsurface.set_position(bar_x, bar_y);
            }
            if let Some(system_bar) = dock.system_status_bar.as_mut() {
                let (bar_x, bar_y) = crate::render::tray::system_bar_position(desired_width as i32);
                system_bar.position = (bar_x, bar_y);
                system_bar.subsurface.set_position(bar_x, bar_y);
                system_bar.width =
                    crate::render::tray::system_bar_logical_width(desired_width as i32) as u32;
            }
            // Only change the width; use config dock height to keep the dock positioned correctly
            let dock_height = self.config.dock_height;
            dock.surface.set_size(desired_width, dock_height);
            if hidden {
                // Resizing a hidden layer surface must also resize its 3px
                // bottom reveal region; Wayland preserves the old region.
                dock.update_input_region(compositor_state, true, 0, desired_width as i32, &qh);
            } else {
                dock.surface.wl_surface().commit();
            }
            changed = true;
        }

        if changed {
            self.needs_redraw = true;
        }
        changed
    }

    pub fn set_dock_input_region(
        &self,
        is_hidden: bool,
        container_start_x: i32,
        container_width: i32,
        qh: &QueueHandle<AppState>,
    ) {
        // Use each layer surface's negotiated width. AppState::width is not the
        // dock width (and may still be its startup placeholder value).
        for dock in &self.docks {
            let effective_width = if container_width > 0 {
                container_width
            } else {
                dock.width as i32
            };
            dock.update_input_region(
                &self.compositor_state,
                is_hidden,
                container_start_x,
                effective_width,
                qh,
            );
        }
    }
}
