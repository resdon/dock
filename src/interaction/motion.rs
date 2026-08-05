use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use wayland_client::protocol::wl_surface::WlSurface;
use crate::AppState;
use crate::pointer::coordinates::map_coordinates;

/// Handles pointer motion and entry events, updating pointer coordinates and drag threshold.
pub fn handle_motion_events(
    state: &mut AppState,
    events: &[PointerEvent],
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> bool {
    let mut layer_changed = false;

    for event in events {
        if matches!(event.kind, PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. }) {
            // Set cursor_moved to true because a Wayland motion/enter event occurred
            state.menu_state.cursor_moved = true;

            if !state.interaction.pointer_inside {
                state.interaction.pointer_inside = true;
                layer_changed = true;
            }

            let (mapped_x, mapped_y) = map_coordinates(event, state, dock_surface_ptr, scale_factor);
            let (new_x, new_y) = (mapped_x as f64, mapped_y as f64);

            if state.interaction.pointer_position.x != new_x || state.interaction.pointer_position.y != new_y {
                state.interaction.pointer_position.x = new_x;
                state.interaction.pointer_position.y = new_y;
                layer_changed = true;
            }

            if state.hide_state.is_fully_hidden() && state.hover_state.is_visible {
                state.hover_state.is_visible = false;
                state.hover_state.app_id = None;
                layer_changed = true;
            }

            if state.dragged_app_id.is_some() && !state.is_dragging {
                let dx = mapped_x - state.drag_start_x as f32;
                let dy = mapped_y - state.drag_start_y as f32;
                if (dx * dx + dy * dy) > 25.0 {
                    state.is_dragging = true;
                    layer_changed = true;
                }
            }
        }
    }

    layer_changed
}
