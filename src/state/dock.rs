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

fn raw_window_app_id(window: &crate::state::WindowState) -> String {
    window
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
        })
}

impl AppState {
    pub(crate) fn compute_apps_in_dock_uncached(&self) -> Vec<String> {
        let mut apps_in_dock = self.pinned_apps.clone();
        let mut windows: Vec<&crate::state::WindowState> = self.windows.values().collect();
        windows.sort_by_key(|window| (window.order_index, window.id.0.protocol_id()));
        for window in windows {
            let raw_id = raw_window_app_id(window);
            let id = self.canonical_running_app_id(&raw_id);
            if !apps_in_dock.contains(&id) {
                apps_in_dock.push(id);
            }
        }
        apps_in_dock
    }

    pub(crate) fn compute_running_by_app_uncached(&self) -> HashMap<String, Vec<ObjectId>> {
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new();
        for (id, window) in &self.windows {
            let raw_id = raw_window_app_id(window);
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

    pub(crate) fn canonical_running_app_id(&self, raw_id: &str) -> String {
        let normalized = crate::handlers::dock::normalize_app_id(raw_id);

        if let Some(pinned) = self
            .pinned_apps
            .iter()
            .find(|id| id.eq_ignore_ascii_case(&normalized) || id.eq_ignore_ascii_case(raw_id))
        {
            return pinned.clone();
        }

        // Configurable exact/normalized aliases (best-effort, case-insensitive).
        for (alias, canonical) in &self.config.app_id_aliases {
            if normalized.eq_ignore_ascii_case(alias) || raw_id.eq_ignore_ascii_case(alias) {
                if let Some(pinned) = self
                    .pinned_apps
                    .iter()
                    .find(|id| id.eq_ignore_ascii_case(canonical))
                {
                    return pinned.clone();
                }
                return canonical.clone();
            }
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

        // The bluetooth-widget-complete app reports its window title as "Bluetooth"
        // and its binary name as "widget-test". Canonicalize both to "bluetooth"
        // so the dock shows the system Bluetooth icon and groups the window correctly.
        if lowered == "bluetooth" || lowered == "widget-test" {
            return "bluetooth".to_string();
        }

        normalized
    }

    /// Returns the current ordered list of app IDs in the dock (pinned + unpinned running).
    pub fn get_apps_in_dock(&self) -> Vec<String> {
        if let Some(cached) = &self.cached_apps_in_dock {
            return cached.clone();
        }
        self.compute_apps_in_dock_uncached()
    }

    pub fn get_running_by_app(&self) -> HashMap<String, Vec<ObjectId>> {
        if let Some(cached) = &self.cached_running_by_app {
            return cached.clone();
        }
        self.compute_running_by_app_uncached()
    }

    pub fn window_count_for_app(&self, app_id: &str) -> usize {
        self.windows
            .values()
            .filter(|window| {
                let raw_id = raw_window_app_id(window);
                self.canonical_running_app_id(&raw_id) == app_id
            })
            .count()
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

        // Reposition both blocks to follow the dock's new width.
        self.sync_dashed_block_position();
        self.sync_date_time_block_position();

        if changed {
            self.needs_redraw = true;
        }
        changed
    }

    /// Reposition the dashed block layer surface when the dock's width changes.
    ///
    /// The dashed block is anchored to BOTTOM|LEFT (for bottom dock), so we
    /// recalculate the left margin to keep it immediately beside the dock's
    /// left edge.
    pub fn sync_dashed_block_position(&mut self) {
        let config = &self.config;
        let gap = config.dashed_block_gap_from_dock as i32;
        let margin = config.margin;

        for dock in &mut self.docks {
            let Some(ref mut db) = dock.dashed_block_surface else {
                continue;
            };
            let output_size = self
                .output_state
                .info(&dock.output)
                .and_then(|info| info.logical_size)
                .unwrap_or((1920, 1080));
            let output_width = output_size.0 as i32;
            let dock_width = dock.width as i32;
            let surface_height = dock.height as i32;
            let db_width = config.dashed_block_width as i32;

            match config.anchor {
                crate::config::DockAnchor::Bottom => {
                    let dock_left_edge = (output_width / 2) - (dock_width / 2);
                    let left_margin = dock_left_edge - gap - db_width;
                    db.surface.set_margin(0, 0, margin, left_margin.max(0));
                }
                crate::config::DockAnchor::Top => {
                    let dock_left_edge = (output_width / 2) - (dock_width / 2);
                    let left_margin = dock_left_edge - gap - db_width;
                    db.surface.set_margin(margin, 0, 0, left_margin.max(0));
                }
                crate::config::DockAnchor::Left => {
                    let dock_bottom_edge = (output_size.1 as i32 / 2) + (surface_height / 2);
                    let bottom_margin = output_size.1 as i32 - dock_bottom_edge - gap;
                    db.surface.set_margin(0, 0, bottom_margin.max(0), margin);
                }
                crate::config::DockAnchor::Right => {
                    let dock_bottom_edge = (output_size.1 as i32 / 2) + (surface_height / 2);
                    let bottom_margin = output_size.1 as i32 - dock_bottom_edge - gap;
                    db.surface.set_margin(0, margin, bottom_margin.max(0), 0);
                }
            }
            db.surface.wl_surface().commit();
        }
    }

    /// Reposition the date/time block layer surface when the dock's width changes.
    ///
    /// The date/time block is anchored to BOTTOM|RIGHT (for bottom dock), so we
    /// recalculate the right margin to keep it immediately beside the dock's
    /// right edge.
    pub fn sync_date_time_block_position(&mut self) {
        let config = &self.config;
        let gap = config.date_time_block_gap_from_dock as i32;
        let margin = config.margin;

        for dock in &mut self.docks {
            let Some(ref mut dt) = dock.date_time_block_surface else {
                continue;
            };
            let output_size = self
                .output_state
                .info(&dock.output)
                .and_then(|info| info.logical_size)
                .unwrap_or((1920, 1080));
            let output_width = output_size.0 as i32;
            let dock_width = dock.width as i32;
            let surface_height = dock.height as i32;
            let dt_width = config.date_time_block_width as i32;

            match config.anchor {
                crate::config::DockAnchor::Bottom => {
                    let dock_right_edge = (output_width / 2) + (dock_width / 2);
                    let right_margin = output_width - dock_right_edge - gap - dt_width;
                    dt.surface.set_margin(0, right_margin.max(0), margin, 0);
                }
                crate::config::DockAnchor::Top => {
                    let dock_right_edge = (output_width / 2) + (dock_width / 2);
                    let right_margin = output_width - dock_right_edge - gap - dt_width;
                    dt.surface.set_margin(margin, right_margin.max(0), 0, 0);
                }
                crate::config::DockAnchor::Left => {
                    let dock_bottom_edge = (output_size.1 as i32 / 2) + (surface_height / 2);
                    let bottom_margin = output_size.1 as i32 - dock_bottom_edge - gap;
                    dt.surface.set_margin(0, 0, bottom_margin.max(0), margin);
                }
                crate::config::DockAnchor::Right => {
                    let dock_bottom_edge = (output_size.1 as i32 / 2) + (surface_height / 2);
                    let bottom_margin = output_size.1 as i32 - dock_bottom_edge - gap;
                    dt.surface.set_margin(0, margin, bottom_margin.max(0), 0);
                }
            }
            dt.surface.wl_surface().commit();
        }
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
