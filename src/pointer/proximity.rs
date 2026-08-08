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
        let windows = get_windows_for_app(target_app, running_by_app, &state.open_windows);

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
    item_count: usize,
    apps_in_dock: &[String],
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    if item_count == 0 {
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
        target_app,
        apps_in_dock,
        phys_width,
        dock_scale as f64,
        layout,
    );
    let geom = PopupGeometry::compute_bounds(
        popup_type,
        anchor_x,
        0,
        phys_width,
        phys_height,
        item_count as i32,
        dock_scale,
    );

    let px = (state.interaction.pointer_position.x * dock_scale as f64).round() as i32;
    let py = (state.interaction.pointer_position.y * dock_scale as f64).round() as i32;

    let _ = scale_factor;
    px >= geom.x
        && px <= geom.x + geom.phys_width
        && py >= geom.y
        && py <= geom.y + geom.phys_height
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
    let windows = get_windows_for_app(app_id, running_by_app, &state.open_windows);
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
    apps_in_dock: &[String],
    layout: (i32, i32, i32, i32),
) -> Option<String> {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let pointer_x = state.interaction.pointer_position.x as f32;
    let pointer_y = state.interaction.pointer_position.y as f32;

    let is_over_y = pointer_y >= 0.0 && pointer_y <= dock_height as f32;
    if !is_over_y && !state.interaction.pointer_inside {
        return None;
    }

    for dock in &state.docks {
        if let Some(ref dock_state) = dock.dock_state {
            for pin in &dock_state.pins {
                let hit_start_x = pin.x.saturating_sub(spacing / 2) as f32;
                let hit_end_x = (pin.x + pin.size as i32 + (spacing / 2)) as f32;

                if pointer_x >= hit_start_x && pointer_x <= hit_end_x {
                    return Some(pin.app_id.clone());
                }
            }
        }
    }

    for (idx, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_offset_x + spacing + idx as i32 * (box_size + spacing);
        let hit_start_x = start_x.saturating_sub(spacing / 2) as f32;
        let hit_end_x = (start_x + box_size + spacing / 2) as f32;

        if pointer_x >= hit_start_x && pointer_x <= hit_end_x {
            return Some(app_id.clone());
        }
    }

    None
}