use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use crate::geometry::WindowListGeometry;
use crate::geometry::context_menu::ContextMenuGeometry;
use crate::AppState;
use crate::handlers::dock::get_windows_for_app;

/// Calculates the physical horizontal anchor coordinate for a target app icon.
pub fn calculate_context_menu_anchor(
    state: &AppState,
    apps_in_dock: &[String],
    phys_width: i32,
    dock_scale: f64,
    layout: (i32, i32, i32, i32),
) -> i32 {
    let (_dock_height, box_size, spacing, start_offset_x) = layout;

    if let Some(ref target_app) = state.menu_state.target_app_id {
        // 1. Try finding in pinned dock state
        for d in &state.docks {
            if let Some(ref dock_state) = d.dock_state {
                if let Some(pin) = dock_state.pins.iter().find(|p| &p.app_id == target_app) {
                    let pin_center_logical = pin.x as f32 + (pin.size as f32 / 2.0);
                    return (pin_center_logical * dock_scale as f32).round() as i32;
                }
            }
        }

        // 2. Fallback for unpinned apps: account for pinned item count offset
        let pinned_count = state
            .docks
            .first()
            .and_then(|d| d.dock_state.as_ref())
            .map(|ds| ds.pins.len())
            .unwrap_or(0);

        if let Some(idx) = apps_in_dock.iter().position(|id| id == target_app) {
            // Check if apps_in_dock includes pinned items or only unpinned items
            let is_full_list = state
                .docks
                .first()
                .and_then(|d| d.dock_state.as_ref())
                .and_then(|ds| ds.pins.first())
                .map_or(false, |first_pin| apps_in_dock.first() == Some(&first_pin.app_id));

            let slot_idx = if is_full_list { idx } else { pinned_count + idx };

            let start_x = start_offset_x + spacing + slot_idx as i32 * (box_size + spacing);
            let pin_center_logical = start_x as f32 + (box_size as f32 / 2.0);
            return (pin_center_logical * dock_scale as f32).round() as i32;
        }
    }
    phys_width / 2
}

pub fn pointer_on_icon(
    state: &AppState,
    apps_in_dock: &[String],
    layout: (i32, i32, i32, i32),
) -> Option<String> {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let pointer_x = state.interaction.pointer_position.x as f32;
    let pointer_y = state.interaction.pointer_position.y as f32;

    let dock_top_bound = (state.height as i32).saturating_sub(dock_height);
    let dock_bottom_bound = state.height as i32;

    let is_over_y = pointer_y >= dock_top_bound as f32 && pointer_y <= dock_bottom_bound as f32;
    if !is_over_y {
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

pub fn pointer_on_window_list(
    state: &AppState,
    app_id: &str,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    dock_height: i32,
) -> bool {
    let windows = get_windows_for_app(app_id, running_by_app, &state.open_windows);
    if windows.is_empty() {
        return false;
    }

    let ptr_x_scaled = (state.interaction.pointer_position.x * scale_factor as f64).round() as i32;
    let ptr_y_scaled = (state.interaction.pointer_position.y * scale_factor as f64).round() as i32;

    let dock_scale = state
        .docks
        .first()
        .map(|d| d.scale_factor as f64)
        .unwrap_or(scale_factor as f64);

    let phys_width = (state.width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;
    let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;

    let total_items = apps_in_dock.len();
    let win_count = windows.len();
    let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

    let (menu_x, _, menu_width, menu_height, _, _) = WindowListGeometry::default().compute_bounds(
        phys_width,
        phys_dock_height,
        total_items,
        hovered_app_index,
        win_count,
        dock_scale,
    );

    let menu_y = (screen_height_phys - phys_dock_height) - menu_height;

    let in_list = ptr_x_scaled >= menu_x
        && ptr_x_scaled <= (menu_x + menu_width)
        && ptr_y_scaled >= menu_y
        && ptr_y_scaled <= (menu_y + menu_height);


    in_list
}

pub fn pointer_between_icon_and_window(
    _state: &AppState,
    _app_id: &str,
    _apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    _scale_factor: f32,
    _dock_height: i32,
    _spacing: i32,
) -> bool {

	// Disabled for now
	false
	/*
    let windows = get_windows_for_app(app_id, running_by_app, &state.open_windows);
    if windows.is_empty() {
        return false;
    }

    let dock_scale = state
        .docks
        .first()
        .map(|d| d.scale_factor as f64)
        .unwrap_or(scale_factor as f64);

    let ptr_x_scaled = (state.interaction.pointer_position.x * dock_scale).round() as i32;
    let ptr_y_scaled = (state.interaction.pointer_position.y * dock_scale).round() as i32;

    let phys_width = (state.width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;
    let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;

    let total_items = apps_in_dock.len();
    let win_count = windows.len();
    let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

    let (menu_x, _, menu_width, menu_height, _, _) = WindowListGeometry::default().compute_bounds(
        phys_width,
        phys_dock_height,
        total_items,
        hovered_app_index,
        win_count,
        dock_scale,
    );

    let menu_y = (screen_height_phys - phys_dock_height) - menu_height;

    let mut icon_hit_start_x = 0;
    let mut icon_hit_end_x = 0;
    let mut found_pin = false;

    for dock in &state.docks {
        if let Some(ref dock_state) = dock.dock_state {
            if let Some(pin) = dock_state.pins.iter().find(|p| p.app_id == app_id) {
                let hit_start = pin.x as f64 * dock_scale;
                let hit_end = (pin.x + pin.size as i32) as f64 * dock_scale;

                icon_hit_start_x = hit_start.round() as i32;
                icon_hit_end_x = hit_end.round() as i32;
                found_pin = true;
                break;
            }
        }
    }

    if !found_pin {
        if let Some(idx) = apps_in_dock.iter().position(|id| id == app_id) {
            let box_size = dock_height - spacing;
            let total_items = apps_in_dock.len() as i32;
            let content_width = if total_items > 0 {
                total_items * box_size + (total_items + 1) * spacing
            } else {
                0
            };
            let start_offset_x = if (state.width as i32) > content_width {
                (state.width as i32 - content_width) / 2
            } else {
                0
            };
            let start_x = start_offset_x + spacing + idx as i32 * (box_size + spacing);
            
            icon_hit_start_x = (start_x as f64 * dock_scale).round() as i32;
            icon_hit_end_x = ((start_x + box_size) as f64 * dock_scale).round() as i32;
            found_pin = true;
        }
    }

    if !found_pin {
        return false;
    }

    // 1. Strictly bound vertical gap between bottom of menu popup and top of dock icon
    let gap_top = menu_y + menu_height - 6; // overlap 6px into bottom of popup
    let dock_top = screen_height_phys - phys_dock_height;
    let gap_bottom = dock_top + 6;          // overlap 6px into top of dock

    let in_gap_y = ptr_y_scaled >= gap_top && ptr_y_scaled <= gap_bottom;

    // 2. Restrict horizontal corridor to icon span + small buffer, or center towards menu anchor
    let padding = (12.0 * dock_scale).round() as i32;
    let leeway_min_x = icon_hit_start_x.min(menu_x) - padding;
    let leeway_max_x = icon_hit_end_x.max(menu_x + menu_width) + padding;

    // Constrain corridor horizontally to icon X range (+ buffer) to prevent swallowing adjacent icons
    let tight_icon_min_x = icon_hit_start_x - padding;
    let tight_icon_max_x = icon_hit_end_x + padding;

    let final_min_x = leeway_min_x.max(tight_icon_min_x);
    let final_max_x = leeway_max_x.min(tight_icon_max_x);

    in_gap_y && ptr_x_scaled >= final_min_x && ptr_x_scaled <= final_max_x
	*/
}

pub fn pointer_on_context_menu(
    state: &AppState,
    apps_in_dock: &[String],
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    if !state.menu_state.is_open || state.menu_state.items.is_empty() {
        return false;
    }

    let (dock_height, _, _, _) = layout;

    let dock_scale = state
        .docks
        .first()
        .map(|d| d.scale_factor as f64)
        .unwrap_or(scale_factor as f64);

    let ptr_x_scaled = (state.interaction.pointer_position.x * dock_scale).round() as i32;
    let ptr_y_scaled = (state.interaction.pointer_position.y * dock_scale).round() as i32;

    // Use full screen width to prevent clamping anchor_x to a truncated dock width
    let phys_screen_width = (state.width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;
    let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;

    let anchor_x = calculate_context_menu_anchor(state, apps_in_dock, phys_screen_width, dock_scale, layout);

    let mut geom = ContextMenuGeometry::default().compute_bounds(
        anchor_x,
        phys_dock_height,
        phys_screen_width,
        phys_dock_height,
        state.menu_state.items.len(),
        dock_scale,
    );

    geom.y = (screen_height_phys - phys_dock_height) - geom.phys_height;
    let menu_width = (geom.logical_width as f64 * dock_scale).round() as i32;
    let menu_height = geom.phys_height;

    let menu_min_x = geom.x.max(0);
    let menu_max_x = (geom.x + menu_width).min(phys_screen_width);

    let in_menu = ptr_x_scaled >= menu_min_x
        && ptr_x_scaled <= menu_max_x
        && ptr_y_scaled >= geom.y
        && ptr_y_scaled <= (geom.y + menu_height);

    in_menu
}

pub fn pointer_in_context_menu_leeway(
    _state: &AppState,
    _apps_in_dock: &[String],
    _scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
	false
	/*
    if !state.menu_state.is_open || state.menu_state.items.is_empty() {
        return false;
    }

    let (dock_height, _, _, _) = layout;

    let dock_scale = state
        .docks
        .first()
        .map(|d| d.scale_factor as f64)
        .unwrap_or(scale_factor as f64);

    let ptr_x_scaled = (state.interaction.pointer_position.x * dock_scale).round() as i32;
    let ptr_y_scaled = (state.interaction.pointer_position.y * dock_scale).round() as i32;

    let dock_width = state.docks.first().map(|d| d.width).unwrap_or(state.width as u32);
    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;
    let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;

    let anchor_x = calculate_context_menu_anchor(state, apps_in_dock, phys_width, dock_scale, layout);

    // 1. Current Context Menu Area Bounds
    let mut geom = ContextMenuGeometry::default().compute_bounds(
        anchor_x,
        phys_dock_height,
        phys_width,
        phys_dock_height,
        state.menu_state.items.len(),
        dock_scale,
    );

    geom.y = (screen_height_phys - phys_dock_height) - geom.phys_height;
    let menu_width = (geom.logical_width as f64 * dock_scale).round() as i32;
    let menu_height = geom.phys_height;

    let menu_min_x = geom.x.max(0);
    let menu_max_x = (geom.x + menu_width).min(phys_width);

    let in_menu = ptr_x_scaled >= menu_min_x
        && ptr_x_scaled <= menu_max_x
        && ptr_y_scaled >= geom.y
        && ptr_y_scaled <= (geom.y + menu_height);

    // 2. Main Rendered Dock Area Bounds
    let dock_min_x = 0;
    let dock_max_x = phys_width;
    let dock_min_y = screen_height_phys - phys_dock_height;
    let dock_max_y = screen_height_phys;

    let in_dock = ptr_x_scaled >= dock_min_x
        && ptr_x_scaled <= dock_max_x
        && ptr_y_scaled >= dock_min_y
        && ptr_y_scaled <= dock_max_y;

    // 3. Leeway Corridor
    let leeway_min_x = menu_min_x.min(anchor_x - 30);
    let leeway_max_x = menu_max_x.max(anchor_x + 30);
    let leeway_min_y = geom.y + menu_height;
    let leeway_max_y = dock_min_y;

    let in_leeway = ptr_x_scaled >= leeway_min_x
        && ptr_x_scaled <= leeway_max_x
        && ptr_y_scaled >= leeway_min_y
        && ptr_y_scaled <= leeway_max_y;

    // Force true if any cursor motion event is active in this frame
    let is_inside = in_menu || in_dock || in_leeway || state.menu_state.cursor_moved;

    is_inside
    */
}
