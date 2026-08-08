use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::AppState;

pub fn handle_scroll_events(
    state: &mut AppState,
    events: &[PointerEvent],
    apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    // Pointer coordinates are local to the dock layer surface.
    let is_over_icons = state.interaction.pointer_position.y >= 0.0
        && state.interaction.pointer_position.y <= dock_height as f64;

    let _phys_width = (state.width as f32 * scale_factor).round() as i32;
    let _phys_height = (state.height as f32 * scale_factor).round() as i32;

    for event in events {
        if let PointerEventKind::Axis { vertical, .. } = &event.kind {
            let scroll_val = if vertical.discrete != 0 {
                vertical.discrete as f64
            } else {
                vertical.absolute
            };

            if scroll_val != 0.0 {
                let mut target_app: Option<String> = None;

                if is_over_icons {
                    for (index, app_id) in apps_in_dock.iter().enumerate() {
                        let start_x =
                            start_offset_x + spacing + index as i32 * (box_size + spacing);
                        let hit_start_x = start_x.saturating_sub(spacing / 2);
                        let hit_end_x = start_x + box_size + (spacing / 2);

                        if state.interaction.pointer_position.x >= hit_start_x.into()
                            && state.interaction.pointer_position.x <= hit_end_x.into()
                        {
                            target_app = Some(app_id.clone());
                            break;
                        }
                    }
                }

/*                if target_app.is_none() && state.hover_state.is_visible {
                    if let Some(ref app_id) = state.hover_state.app_id {
                        if let Some(wins) = running_by_app.get(app_id) {
                            let total_apps = apps_in_dock.len();
                            let hovered_app_index =
                                apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

							let click_x = (state.interaction.pointer_position.x * scale_factor as f64).round() as i32;
							let dock_height = state.docks.first().map_or(60.0, |d| d.height as f64);
							let dock_top_logical = (state.height as f64 - dock_height).max(0.0);
							let click_y = ((dock_top_logical + state.interaction.pointer_position.y) * scale_factor as f64).round() as i32;

							let (menu_x, menu_y, menu_width, menu_height, _, _) = gPopupGeometry::compute_bounds(
                                PopupType::WindowList, x, y, w, h, count as i32, scale as f32
                            )

                            let ptr_x = (state.interaction.pointer_position.x as f32 * scale_factor)
                                .round() as i32;
                            let ptr_y = (state.interaction.pointer_position.y as f32 * scale_factor)
                                .round() as i32;

                            if ptr_x >= menu_x
                                && ptr_x <= menu_x + menu_width
                                && ptr_y >= menu_y
                                && ptr_y <= menu_y + menu_height
                            {
                                target_app = Some(app_id.clone());
                            }
                        }
                    }
                }*/

                if let Some(app_id) = target_app {
                    state.cycle_window_for_app(&app_id, scroll_val < 0.0);
                    layer_changed = true;
                }
            }
        }
    }

    layer_changed
}
