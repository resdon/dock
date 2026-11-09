//! Pointer proximity detection.
//!
//! Determines whether the pointer is near the dock edge for auto-show.

use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::handlers::dock::get_windows_for_app;
use crate::AppState;

use super::coordinates;

use std::collections::HashMap;
use wayland_client::backend::ObjectId;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HoverResult {
    pub visible: bool,
    pub app_id: Option<String>,
    pub pointer_on_window_list: bool,
}

pub fn update_hover(
    state: &AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> HoverResult {
    let (dock_height, _, spacing, _) = layout;

    // 1. Check if pointer is directly over an app icon in the dock
    if let Some(app_id) = pointer_on_icon(state, apps_in_dock, layout) {
        return HoverResult {
            visible: true,
            app_id: Some(app_id),
            pointer_on_window_list: false,
        };
    }

    // 2. Check if pointer is over an active window list popup
    if let Some(ref target_app) = state.window_list_state.target_app_id {
        let windows = get_windows_for_app(target_app, running_by_app);

        if state.window_list_state.is_open
            && pointer_on_popup(
                state,
                PopupType::WindowList,
                target_app,
                windows.len(),
                apps_in_dock,
                scale_factor,
                layout,
            )
        {
            return HoverResult {
                visible: true,
                app_id: Some(target_app.clone()),
                pointer_on_window_list: true,
            };
        }

        // 3. Check if pointer is within the leeway corridor between icon and window list
        if pointer_between_icon_and_window(
            state,
            target_app,
            apps_in_dock,
            running_by_app,
            scale_factor,
            dock_height,
            spacing,
        ) {
            return HoverResult {
                visible: true,
                app_id: Some(target_app.clone()),
                pointer_on_window_list: false,
            };
        }
    }

    HoverResult::default()
}

/// Generic hit-testing routine for all popup surface bounds.
pub fn pointer_on_popup(
    state: &AppState,
    popup_type: PopupType,
    target_app: &str,
    _item_count: usize,
    _apps_in_dock: &[String],
    _scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
    let px = state.interaction.pointer_position.x;
    let py = state.interaction.pointer_position.y;

    for dock in &state.docks {
        let popup = match popup_type {
            PopupType::ContextMenu => dock.context_menu_popup.as_ref(),
            PopupType::WindowList => dock.window_list_popup.as_ref(),
            PopupType::Custom { .. } => None,
        };

        let Some(popup) = popup else {
            continue;
        };

        if !target_app.is_empty()
            && popup
                .current_app_id
                .as_deref()
                .is_some_and(|id| id != target_app)
        {
            continue;
        }

        let x = popup.position.0 as f64;
        let y = popup.position.1 as f64;
        let w = popup.width as f64;
        let h = popup.height as f64;

        if px >= x && px < x + w && py >= y && py < y + h {
            return true;
        }
    }

    false
}

pub fn pointer_between_icon_and_window(
    state: &AppState,
    app_id: &str,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    dock_height: i32,
    _spacing: i32,
) -> bool {
    let windows = get_windows_for_app(app_id, running_by_app);
    if windows.is_empty() || !state.window_list_state.is_open {
        return false;
    }

    let dock = match state.docks.first() {
        Some(dock) => dock,
        None => return false,
    };
    let dock_scale = dock.scale_factor as f32;
    let phys_width = (dock.width as f64 * dock.scale_factor).round() as i32;
    let phys_height = (dock.height as f64 * dock.scale_factor).round() as i32;

    let anchor_x = coordinates::calculate_popup_anchor(
        state,
        app_id,
        apps_in_dock,
        phys_width,
        dock_scale as f64,
        (dock_height, 48, 8, 0),
    );
    let geom = PopupGeometry::compute_bounds(
        PopupType::WindowList,
        anchor_x,
        0,
        phys_width,
        phys_height,
        windows.len() as i32,
        dock_scale,
    );

    let px = (state.interaction.pointer_position.x * dock_scale as f64).round() as i32;
    let py = (state.interaction.pointer_position.y * dock_scale as f64).round() as i32;

    // Small dock-local corridor keeps the window list stable while moving
    // vertically from the icon to the popup.
    let gap_top = geom.y + geom.phys_height - (6.0 * dock_scale as f64).round() as i32;
    let gap_bottom = (6.0 * dock_scale as f64).round() as i32;
    let in_gap_y = py >= gap_top && py <= gap_bottom;
    let padding = (12.0 * dock_scale as f64).round() as i32;
    let leeway_min_x = geom.x.min(anchor_x - padding);
    let leeway_max_x = (geom.x + geom.phys_width).max(anchor_x + padding);

    let _ = scale_factor;
    in_gap_y && px >= leeway_min_x && px <= leeway_max_x
}

pub fn pointer_on_icon(
    state: &AppState,
    _apps_in_dock: &[String],
    _layout: (i32, i32, i32, i32),
) -> Option<String> {
    // Keep icon hit-testing in one coordinate space and one implementation.
    // get_app_id_at_location() is also used by DnD, so clicks/hover/scroll/drop
    // cannot drift apart when the monitor uses fractional scaling.
    let pointer = state.interaction.pointer_position;
    state.get_app_id_at_location(pointer.x, pointer.y)
}
