use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::geometry::context_menu::ContextMenuGeometry;
use crate::AppState;

/// Checks whether the current logical pointer coordinates are within a specified distance (`margin`)
/// of the hover menu popup for an active app.
pub fn is_pointer_near_hover_menu(
    state: &AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    ptr_log_x: i32,
    ptr_log_y: i32,
    margin: i32,
) -> bool {
    let Some(app_id) = &state.hover_state.app_id else {
        return false;
    };

    let Some(windows) = running_by_app.get(app_id) else {
        return false;
    };

    let win_count = windows.len().max(1);
    let total_apps = apps_in_dock.len();
    let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

    let phys_width = (state.width as f32 * scale_factor).round() as i32;
    let phys_height = (state.height as f32 * scale_factor).round() as i32;

    let anchor_x = if total_apps > 0 {
        let app_w = phys_width / total_apps as i32;
        (hovered_app_index as i32 * app_w) + (app_w / 2)
    } else {
        phys_width / 2
    };

    let geom = ContextMenuGeometry::default().compute_bounds(
        anchor_x,
        phys_height,
        phys_width,
        phys_height,
        win_count,
        scale_factor as f64,
    );

    // Convert physical menu bounds back to logical coordinates for proximity evaluation
    let menu_log_x = (geom.x as f32 / scale_factor).round() as i32;
    let menu_log_y = (geom.y as f32 / scale_factor).round() as i32;
    let menu_log_w = (geom.logical_width as f32 / scale_factor).round() as i32;
    let menu_log_h = (geom.logical_height as f32 / scale_factor).round() as i32;

    ptr_log_x >= (menu_log_x - margin)
        && ptr_log_x <= (menu_log_x + menu_log_w + margin)
        && ptr_log_y >= (menu_log_y - margin)
        && ptr_log_y <= (menu_log_y + menu_log_h + margin)
}
