use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use crate::app::AppState;
use crate::render::window_list::get_hover_menu_bounds;
use super::coordinates::map_coordinates;

pub fn handle_motion_events(
    state: &mut AppState,
    events: &[PointerEvent],
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> bool {
    let mut layer_changed = false;

    for event in events {
        match event.kind {
            PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                if !state.is_pointer_inside {
                    state.is_pointer_inside = true;
                    layer_changed = true;
                }

                let (mapped_x, mapped_y) = map_coordinates(event, state, dock_surface_ptr, scale_factor);
                let new_x = mapped_x as i32;
                let new_y = mapped_y as i32;

                if state.pointer_x != new_x || state.pointer_y != new_y {
                    state.pointer_x = new_x;
                    state.pointer_y = new_y;
                    layer_changed = true;
                }

                let is_dock_visible = !state.hide_state.is_fully_hidden();
                if !is_dock_visible && state.hover_state.is_visible {
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
            PointerEventKind::Leave { .. } => {
                // 1. Mark pointer as outside by default.
                // If the mouse moved into a popup subsurface in this frame, a subsequent
                // Enter event in the same event batch will immediately set this back to true.
                state.is_pointer_inside = false;

                if let Some(dock_surf) = dock_surface_ptr {
                    if event.surface == *dock_surf {
                        // 2. Pointer left the main dock surface -> start the hover leave grace timer
                        if state.hover_state.is_visible && state.hover_state.last_leave_time.is_none() {
                            state.hover_state.last_leave_time = Some(std::time::Instant::now());
                        }
                    } else {
                        // 3. Pointer left a subsurface (e.g. hover list or context menu)
                        // Do NOT force menu_state.is_open = false here! Let click/proximity logic manage menus.
                        if !state.menu_state.is_open && state.hover_state.is_visible && state.hover_state.last_leave_time.is_none() {
                            state.hover_state.last_leave_time = Some(std::time::Instant::now());
                        }
                    }
                } else {
                    // Fallback if dock surface pointer is not available
                    if !state.menu_state.is_open && state.hover_state.is_visible && state.hover_state.last_leave_time.is_none() {
                        state.hover_state.last_leave_time = Some(std::time::Instant::now());
                    }
                }

                layer_changed = true;
            }
            _ => {}
        }
    }

    layer_changed
}

pub fn update_hover_and_proximity(
    state: &mut AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32), // (dock_height, box_size, spacing, start_offset_x)
) -> bool {
    let (dock_height, box_size, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    // --- Compute Exact Rendered Dock Bounds ---
    let phys_surface_height = (state.height as f32 * scale_factor).round() as i32;
    let total_items = apps_in_dock.len();
    let calculated_dock_width = if total_items > 0 {
        total_items as i32 * box_size + (total_items as i32 + 1) * spacing
    } else {
        100
    };

    let dock_top_bound = phys_surface_height.saturating_sub(dock_height);
    let dock_bottom_bound = phys_surface_height;
    let dock_left_bound = start_offset_x;
    let dock_right_bound = start_offset_x + calculated_dock_width;

    // --- Hover Tracking ---
    let mut should_be_visible = false;
    let mut new_app_id = None;
    let mut new_x = state.hover_state.x;
    let is_over_icons = state.pointer_y >= dock_top_bound && state.pointer_y <= dock_bottom_bound;

    if is_over_icons {
        for (index, app_id) in apps_in_dock.iter().enumerate() {
            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
            let hit_start_x = start_x.saturating_sub(spacing / 2);
            let hit_end_x = start_x + box_size + (spacing / 2);

            if state.pointer_x >= hit_start_x && state.pointer_x <= hit_end_x {
                should_be_visible = true;
                new_app_id = Some(app_id.clone());
                break;
            }
        }
    }

    // --- Hover Window List Bounds Check ---
    if !should_be_visible && state.hover_state.is_visible {
        if let Some(ref app_id) = state.hover_state.app_id {
            if running_by_app.contains_key(app_id) {
                let win_count = state.open_windows.values().filter(|w| w.app_id == *app_id).count().max(1);
                let total_apps = apps_in_dock.len();
                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);
                let menu_w = (180.0_f64 * state.scale_factor as f64).round() as i32;
                let menu_h = (win_count as f64 * 30.0_f64 * state.scale_factor as f64).round() as i32;
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

                if ptr_x >= menu_x && ptr_x <= (menu_x + menu_width) && ptr_y >= menu_y && ptr_y <= (menu_y + menu_height) {
                    should_be_visible = true;
                    new_app_id = Some(app_id.clone());
                    new_x = state.hover_state.x;
                }
            }
        }
    }

    // Context Menu Leeway
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let phys_width = (state.width as f32 * scale_factor).round() as i32;
        let phys_height = (state.height as f32 * scale_factor).round() as i32;
        let (menu_x, menu_y, menu_width, total_menu_h) = state.get_context_menu_bounds(phys_width, phys_height, scale_factor);

        let ptr_x = (state.pointer_x as f32 * scale_factor) as i32;
        let ptr_y = (state.pointer_y as f32 * scale_factor) as i32;
        let leeway = (20.0 * scale_factor).round() as i32;

        let inside_extended = ptr_x >= (menu_x - leeway)
            && ptr_x <= (menu_x + menu_width + leeway)
            && ptr_y >= (menu_y - leeway)
            && ptr_y <= (menu_y + total_menu_h + leeway);

        if !inside_extended {
            state.menu_state.is_open = false;
            layer_changed = true;
        }
    }

    // --- Leave Detection against DOCK BOUNDS (+ 10px margin) ---
    let margin = (10.0 * scale_factor).round() as i32;
    let pointer_on_dock = state.pointer_x >= (dock_left_bound - margin)
        && state.pointer_x <= (dock_right_bound + margin)
        && state.pointer_y >= (dock_top_bound - margin)
        && state.pointer_y <= (dock_bottom_bound + margin);

    let mut effective_should_be_visible = should_be_visible;

    if !effective_should_be_visible && state.hover_state.is_visible {
        if !pointer_on_dock {
            // Pointer is outside the rendered dock rectangle -> start/check leave timer
            let leave_time = *state.hover_state.last_leave_time.get_or_insert_with(std::time::Instant::now);
            if leave_time.elapsed() < std::time::Duration::from_millis(300) {
                effective_should_be_visible = true;
                layer_changed = true;
            } else {
                state.hover_state.last_leave_time = None;
            }
        } else {
            // Pointer is still inside dock icon area/padding -> reset leave timer
            state.hover_state.last_leave_time = None;
        }
    } else if effective_should_be_visible {
        state.hover_state.last_leave_time = None;
    }

    if effective_should_be_visible != state.hover_state.is_visible || new_app_id != state.hover_state.app_id {
        state.hover_state.is_visible = effective_should_be_visible;
        state.hover_state.app_id = new_app_id;
        state.hover_state.x = new_x;
        layer_changed = true;
    }

    layer_changed
}