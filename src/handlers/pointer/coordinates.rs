use crate::app::AppState;
use crate::render::window_list::get_hover_menu_bounds;
use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

pub fn map_coordinates(
    event: &PointerEvent,
    state: &AppState,
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> (f32, f32) {
    let (px, py) = event.position;
    
    if let Some(dock_surf) = dock_surface_ptr {
        let is_main_surface = event.surface == *dock_surf;
        if !is_main_surface {
            // Context menu check
            if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                let phys_width = (state.width as f32 * scale_factor).round() as i32;
                let phys_height = (state.height as f32 * scale_factor).round() as i32;
                let (menu_x, menu_y, _, _) = state.get_context_menu_bounds(phys_width, phys_height, scale_factor);
                return (px as f32 + (menu_x as f32 / scale_factor), py as f32 + (menu_y as f32 / scale_factor));
            }

            // Hover state check
            if state.hover_state.is_visible {
                if let Some(ref app_id) = state.hover_state.app_id {
                    // ✅ Handle fallback ID matching
                    let win_count = state.open_windows
                        .values()
                        .filter(|w| {
                            let win_id = if !w.app_id.is_empty() { &w.app_id } else { "Unknown" };
                            win_id == app_id
                        })
                        .count()
                        .max(1);

                    // ✅ Use full dock app list (pinned + unpinned active)
                    let apps_in_dock = state.get_apps_in_dock();
                    let total_apps = apps_in_dock.len();
                    let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                    let menu_w = (180.0_f64 * state.scale_factor as f64).round() as i32;
                    let menu_h = (win_count as f64 * 30.0_f64 * state.scale_factor as f64).round() as i32;
                    let scale_i32 = state.scale_factor.round() as i32;

                    let (menu_x, menu_y, _, _) = get_hover_menu_bounds(
                        state.width as i32,
                        state.height as i32,
                        total_apps,
                        hovered_app_index,
                        menu_w,
                        menu_h,
                        scale_i32,
                    );
                    return (px as f32 + (menu_x as f32 / scale_factor), py as f32 + (menu_y as f32 / scale_factor));
                }
            }
        }
    }

    (px as f32, py as f32)
}