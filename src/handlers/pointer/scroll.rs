use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use crate::app::AppState;
use crate::render::window_list::get_hover_menu_bounds;

pub fn handle_scroll_events(
    state: &mut AppState,
    events: &[PointerEvent],
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32), // (dock_height, box_size, spacing, start_offset_x)
) -> bool {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    let phys_surface_height = (state.height as f32 * scale_factor).round() as i32;
    let dock_top_bound = phys_surface_height.saturating_sub(dock_height);
    let is_over_icons = state.pointer_y >= dock_top_bound;

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

                        if state.pointer_x >= hit_start_x && state.pointer_x <= hit_end_x {
                            target_app = Some(app_id.clone());
                            break;
                        }
                    }
                }

                if target_app.is_none() && state.hover_state.is_visible {
                    if let Some(ref app_id) = state.hover_state.app_id {
                        if running_by_app.contains_key(app_id) {
                            let win_count = state.open_windows.values().filter(|w| w.app_id == *app_id).count().max(1);
                            // Correct implementation:
                            let total_apps = apps_in_dock.len();
                            let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);
                            let menu_w = (180.0_f64 * state.scale_factor).round() as i32;
                            let menu_h = (win_count as f64 * 30.0_f64 * state.scale_factor).round() as i32;
                            let scale_i32 = state.scale_factor.round() as i32;

                            let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                state.width as i32,
                                state.height as i32,
                                total_apps,
                                hovered_app_index,
                                menu_w,
                                menu_h,
                                scale_i32,
                            );
                            let ptr_x = (state.pointer_x as f32 * scale_factor) as i32;
                            let ptr_y = (state.pointer_y as f32 * scale_factor) as i32;

                            if ptr_x >= menu_x && ptr_x <= menu_x + menu_width && ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
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