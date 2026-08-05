use std::collections::HashMap;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use wayland_client::backend::ObjectId;

use crate::AppState;

pub fn handle_scroll_events(
    state: &mut AppState,
    events: &[PointerEvent],
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    let dock_top_bound = (state.height as i32).saturating_sub(dock_height);
    let is_over_icons = state.interaction.pointer_position.y >= dock_top_bound.into();

    let phys_width = (state.width as f32 * scale_factor).round() as i32;
    let phys_height = (state.height as f32 * scale_factor).round() as i32;

    let geometry = crate::geometry::WindowListGeometry::default();

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
                        let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
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

                if target_app.is_none() && state.hover_state.is_visible {
                    if let Some(ref app_id) = state.hover_state.app_id {
                        if let Some(wins) = running_by_app.get(app_id) {
                            let total_apps = apps_in_dock.len();
                            let hovered_app_index = apps_in_dock
                                .iter()
                                .position(|id| id == app_id)
                                .unwrap_or(0);

                            let (menu_x, menu_y, menu_width, menu_height, _, _) = geometry.compute_bounds(
                                phys_width,
                                phys_height,
                                total_apps,
                                hovered_app_index,
                                wins.len(),
                                scale_factor as f64,
                            );

                            let ptr_x = (state.interaction.pointer_position.x as f32 * scale_factor).round() as i32;
                            let ptr_y = (state.interaction.pointer_position.y as f32 * scale_factor).round() as i32;

                            if ptr_x >= menu_x
                                && ptr_x <= menu_x + menu_width
                                && ptr_y >= menu_y
                                && ptr_y <= menu_y + menu_height
                            {
                                target_app = Some(app_id.clone());
                            }
                        }
                    }
                }

                if let Some(app_id) = target_app {
                    state.cycle_window_for_app(&app_id, scroll_val < 0.0);
                    layer_changed = true;
                }
            }
        }
    }

    layer_changed
}
