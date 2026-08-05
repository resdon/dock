use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::AppState;
use crate::geometry::context_menu::ContextMenuGeometry;
use crate::geometry::WindowListGeometry;

pub fn map_coordinates(
    event: &PointerEvent,
    state: &AppState,
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> (f32, f32) {
    let (px, py) = (event.position.0 as f32, event.position.1 as f32);

    let Some(dock_surf) = dock_surface_ptr else {
        return (px, py);
    };

    if event.surface == *dock_surf {
        return (px, py);
    }

    let phys_width = (state.width as f32 * scale_factor).round() as i32;
    let phys_height = (state.height as f32 * scale_factor).round() as i32;
    let phys_dock_height = (60.0 * scale_factor).round() as i32;

    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let total_items = state.menu_state.items.len();
        let geom = ContextMenuGeometry::default().compute_bounds(
            phys_width / 2,
            phys_dock_height,
            phys_width,
            phys_dock_height,
            total_items,
            scale_factor as f64,
        );
        let menu_y = (phys_height - phys_dock_height) - geom.phys_height;
        return (
            px + (geom.x as f32 / scale_factor),
            py + (menu_y as f32 / scale_factor),
        );
    }

    if state.hover_state.is_visible {
        if let Some(app_id) = &state.hover_state.app_id {
            let running_by_app = state.get_running_by_app();
            let win_count = running_by_app.get(app_id).map_or(0, |v| v.len()).max(1);

            let apps_in_dock = state.get_apps_in_dock();
            let total_apps = apps_in_dock.len();
            let hovered_app_index =
                apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

            let geometry = WindowListGeometry::default();
            let (menu_x, _, _, menu_height, _, _) = geometry.compute_bounds(
                phys_width,
                phys_dock_height,
                total_apps,
                hovered_app_index,
                win_count,
                scale_factor as f64,
            );

            let menu_y = (phys_height - phys_dock_height) - menu_height;

            return (
                px + (menu_x as f32 / scale_factor),
                py + (menu_y as f32 / scale_factor),
            );
        }
    }

    (px, py)
}
