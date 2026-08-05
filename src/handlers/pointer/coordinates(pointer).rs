use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::app::AppState;
use crate::geometry::context_menu::ContextMenuGeometry;

pub fn map_coordinates(
    event: &PointerEvent,
    state: &AppState,
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> (f32, f32) {
    let (px, py) = (event.position.0 as f32, event.position.1 as f32);

    // If event is on the main dock surface or dock surface is unbound, return local relative position directly
    let Some(dock_surf) = dock_surface_ptr else {
        return (px, py);
    };

    if event.surface == *dock_surf {
        return (px, py);
    }

    let phys_width = (state.width as f32 * scale_factor).round() as i32;
    let phys_height = (state.height as f32 * scale_factor).round() as i32;

    // Subsurface offset mapping: Context Menu
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let (menu_x, menu_y, _, _) =
            state.get_context_menu_bounds(phys_width, phys_height, scale_factor);
        return (
            px + (menu_x as f32 / scale_factor),
            py + (menu_y as f32 / scale_factor),
        );
    }

    // Subsurface offset mapping: Hover Window List Popup
    if state.hover_state.is_visible {
        if let Some(app_id) = &state.hover_state.app_id {
            let win_count = state
                .open_windows
                .values()
                .filter(|w| {
                    let win_id = if w.app_id.is_empty() {
                        "Unknown"
                    } else {
                        &w.app_id
                    };
                    win_id == app_id
                })
                .count()
                .max(1);

            let apps_in_dock = state.get_apps_in_dock();
            let total_apps = apps_in_dock.len();
            let hovered_app_index =
                apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

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

            return (
                px + (geom.x as f32 / scale_factor),
                py + (geom.y as f32 / scale_factor),
            );
        }
    }

    (px, py)
}
