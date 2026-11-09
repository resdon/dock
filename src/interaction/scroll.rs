//! Scroll wheel handling.
//!
//! Processes scroll events over the dock area.

use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::AppState;
use smithay_client_toolkit::shell::WaylandSurface;

/// Processes scroll wheel events over the dock area.
///
/// Scrolling over a dock icon cycles through that app's windows.
/// Scrolling over a tray icon sends scroll actions to the tray item.
///
/// # Returns
/// `true` if any state changed and a redraw is needed.
pub fn handle_scroll_events(
    state: &mut AppState,
    events: &[PointerEvent],
    _apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    _scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
    let mut layer_changed = false;

    for event in events {
        if let PointerEventKind::Axis {
            vertical,
            horizontal,
            ..
        } = &event.kind
        {
            let tray_hit =
                state.tray_item_at_surface(&event.surface, event.position.0, event.position.1);

            if let Some(index) = tray_hit {
                let (delta, orientation) = if horizontal.value120 != 0 {
                    (horizontal.value120, "horizontal")
                } else if horizontal.discrete != 0 {
                    (horizontal.discrete, "horizontal")
                } else if horizontal.absolute.abs() > f64::EPSILON {
                    (horizontal.absolute.round() as i32, "horizontal")
                } else if vertical.value120 != 0 {
                    (vertical.value120, "vertical")
                } else if vertical.discrete != 0 {
                    (vertical.discrete, "vertical")
                } else {
                    (vertical.absolute.round() as i32, "vertical")
                };

                if delta != 0 {
                    let (mapped_x, mapped_y) = crate::pointer::coordinates::map_coordinates(
                        event,
                        state,
                        None,
                        _scale_factor,
                    );
                    let _ = state.send_tray_action(
                        index,
                        crate::models::TrayActionKind::Scroll {
                            delta,
                            orientation: orientation.to_string(),
                        },
                        mapped_x.round() as i32,
                        mapped_y.round() as i32,
                    );
                }
                continue;
            }

            let event_on_dock_surface = state
                .docks
                .iter()
                .any(|dock| dock.surface.wl_surface() == &event.surface);
            if !event_on_dock_surface {
                continue;
            }

            let scroll_val = if vertical.discrete != 0 {
                vertical.discrete as f64
            } else {
                vertical.absolute
            };

            if scroll_val != 0.0 {
                let target_app = state.get_app_id_at_location(
                    state.interaction.pointer_position.x,
                    state.interaction.pointer_position.y,
                );

                if let Some(app_id) = target_app {
                    state.cycle_window_for_app(&app_id, scroll_val < 0.0);
                    layer_changed = true;
                }
            }
        }
    }

    layer_changed
}
