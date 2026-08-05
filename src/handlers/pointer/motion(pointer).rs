use std::collections::HashMap;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;

use super::coordinates::map_coordinates;
use crate::ContextMenuGeometry;
use crate::app::AppState;

/// Handles incoming pointer motion and entry/leave events, updating interaction state,
/// pointer positions, and drag thresholds.
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
                if !state.interaction.pointer_inside {
                    state.interaction.pointer_inside = true;
                    layer_changed = true;
                }

                // Check if the event occurs on a hover popup surface or context menu surface and map coordinates accordingly
                let mut mapped_coords = None;
                for dock in &state.docks {
                    let scale = dock.scale_factor as f64;
                    if let Some(ref popup) = dock.hover_popup {
                        if event.surface == popup.surface {
                            if let Some(ref app_id) = state.hover_state.app_id {
                                let apps_in_dock = state.get_apps_in_dock();
                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock
                                    .iter()
                                    .position(|id| id == app_id)
                                    .unwrap_or(0);

                                // Count matching open windows directly without cloning into a HashMap
                                let count = state
                                    .open_windows
                                    .values()
                                    .filter(|win| win.outputs.contains(&dock.output))
                                    .filter(|w| {
                                        let id = if !w.app_id.is_empty() {
                                            w.app_id.as_str()
                                        } else {
                                            "Unknown"
                                        };
                                        id == app_id.as_str()
                                    })
                                    .count();

                                let phys_width = (dock.width as f64 * scale).round() as i32;
                                let phys_height = (dock.height as f64 * scale).round() as i32;

                                let geometry = crate::geometry::WindowListGeometry::default();
                                let (menu_x, menu_y, _, _, _, _) = geometry.compute_bounds(
                                    phys_width,
                                    phys_height,
                                    total_apps,
                                    hovered_app_index,
                                    count,
                                    scale,
                                );

                                let dock_logical_x = (menu_x as f64 / scale) + event.position.0;
                                let dock_logical_y = (menu_y as f64 / scale) + event.position.1;
                                mapped_coords = Some((dock_logical_x as f32, dock_logical_y as f32));
                                break;
                            }
                        }
                    }

                    // Map coordinates if event occurs on the open context menu surface
                    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                        let phys_width = (dock.width as f64 * scale).round() as i32;
                        let phys_height = (dock.height as f64 * scale).round() as i32;
                        let menu_item_count = state.menu_state.items.len();
                        let geom = ContextMenuGeometry::default().compute_bounds(
                            phys_width / 2,
                            phys_height,
                            phys_width,
                            phys_height,
                            menu_item_count,
                            scale,
                        );

                        let is_menu_surface = dock.context_menu_popup.as_ref().map_or(false, |p| p.surface == event.surface);

                        if is_menu_surface {
                            let menu_logical_x = (geom.x as f64 / scale) + event.position.0;
                            let menu_logical_y = (geom.y as f64 / scale) + event.position.1;
                            mapped_coords = Some((menu_logical_x as f32, menu_logical_y as f32));
                            break;
                        }
                    }
                }

                let (mapped_x, mapped_y) = match mapped_coords {
                    Some(coords) => coords,
                    None => map_coordinates(event, state, dock_surface_ptr, scale_factor),
                };

                let new_x = mapped_x as f64;
                let new_y = mapped_y as f64;

                if state.interaction.pointer_position.x != new_x || state.interaction.pointer_position.y != new_y {
                    state.interaction.pointer_position.x = new_x;
                    state.interaction.pointer_position.y = new_y;
                    layer_changed = true;
                }

                // Hide hover state if the dock is fully hidden
                if state.hide_state.is_fully_hidden() && state.hover_state.is_visible {
                    state.hover_state.is_visible = false;
                    state.hover_state.app_id = None;
                    layer_changed = true;
                }

                // Evaluate drag threshold if an application is selected for dragging
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
                state.interaction.pointer_inside = false;
                layer_changed = true;
            }
            _ => {}
        }
    }

    layer_changed
}

/// Evaluates hover states, proximity bounds for dock icons, and context menu intersection limits.
pub fn update_hover_and_proximity(
    state: &mut AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let (dock_height, _box_size, spacing, _start_offset_x) = layout;
    let mut layer_changed = false;

    let total_items = apps_in_dock.len();

    let dock_top_bound = (state.height as i32).saturating_sub(dock_height);
    let dock_bottom_bound = state.height as i32;

    let pointer_y = state.interaction.pointer_position.y as f32;
    let pointer_x = state.interaction.pointer_position.x as f32;

    let mut should_be_visible = false;
    let mut new_app_id = None;
    let mut pointer_on_context_menu = false;
    let mut pointer_in_leeway = false;

    let ptr_x_scaled = (pointer_x * scale_factor).round() as i32;
    let ptr_y_scaled = (pointer_y * scale_factor).round() as i32;

    let window_list_geometry = crate::geometry::WindowListGeometry::default();

    let (dock_width, dock_height_val, dock_scale) = state.docks.first()
        .map(|d| (d.width, d.height, d.scale_factor as f64))
        .unwrap_or((100, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_height = (dock_height_val as f64 * dock_scale).round() as i32;

    // 1. If context menu is open, calculate its bounds using ContextMenuGeometry and suppress hover popups
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let menu_item_count = state.menu_state.items.len();
        let geom = ContextMenuGeometry::default().compute_bounds(
            phys_width / 2,
            phys_height,
            phys_width,
            phys_height,
            menu_item_count,
            dock_scale,
        );

        if ptr_x_scaled >= geom.x
            && ptr_x_scaled <= (geom.x + geom.phys_width)
            && ptr_y_scaled >= geom.y
            && ptr_y_scaled <= (geom.y + geom.phys_height)
        {
            pointer_on_context_menu = true;
        }
    } else {
        // 2. Evaluate hover icons & window lists when context menu is closed
        let is_over_icons = pointer_y >= dock_top_bound as f32 && pointer_y <= dock_bottom_bound as f32;

        if is_over_icons {
            for dock in &state.docks {
                if let Some(ref dock_state) = dock.dock_state {
                    for pin in &dock_state.pins {
                        let start_x = pin.x;
                        let box_size = pin.size as i32;
                        let hit_start_x = start_x.saturating_sub(spacing / 2);
                        let hit_end_x = start_x + box_size + (spacing / 2);

                        if pointer_x >= hit_start_x as f32 && pointer_x <= hit_end_x as f32 {
                            should_be_visible = true;
                            new_app_id = Some(pin.app_id.clone());
                            break;
                        }
                    }
                }
                if should_be_visible { break; }
            }
        }

        // Evaluate the window list and calculate leeway for the active item
        let active_app_id = new_app_id.clone().or_else(|| {
            if state.hover_state.is_visible {
                state.hover_state.app_id.clone()
            } else {
                None
            }
        });

        if let Some(app_id) = active_app_id {
            if let Some(wins) = running_by_app.get(&app_id) {
                let hovered_app_index = apps_in_dock
                    .iter()
                    .position(|id| id == &app_id)
                    .unwrap_or(0);

                let (menu_x, menu_y, menu_width, menu_height, _, _) = window_list_geometry.compute_bounds(
                    phys_width,
                    phys_height,
                    total_items,
                    hovered_app_index,
                    wins.len(),
                    dock_scale,
                );

                // Check if pointer is actively hovering the window list menu
                if !should_be_visible && state.hover_state.is_visible {
                    if ptr_x_scaled >= menu_x
                        && ptr_x_scaled <= (menu_x + menu_width)
                        && ptr_y_scaled >= menu_y
                        && ptr_y_scaled <= (menu_y + menu_height)
                    {
                        should_be_visible = true;
                        new_app_id = Some(app_id.clone());
                    }
                }

                // 3. Compute dynamic leeway corridor between the icon and the menu
                let mut icon_hit_start_x = 0;
                let mut icon_hit_end_x = 0;
                let mut found_pin = false;

                for dock in &state.docks {
                    if let Some(ref dock_state) = dock.dock_state {
                        for pin in &dock_state.pins {
                            if pin.app_id == app_id {
                                let hit_start = pin.x.saturating_sub(spacing / 2) as f32;
                                let hit_end = (pin.x + pin.size as i32 + (spacing / 2)) as f32;
                                
                                icon_hit_start_x = (hit_start * scale_factor).round() as i32;
                                icon_hit_end_x = (hit_end * scale_factor).round() as i32;
                                found_pin = true;
                                break;
                            }
                        }
                    }
                    if found_pin { break; }
                }

                if found_pin {
                    let icon_top = (dock_top_bound as f32 * scale_factor).round() as i32;
                    let icon_bottom = (dock_bottom_bound as f32 * scale_factor).round() as i32;

                    // The bounding box covering both the rendered menu and the dock icon
                    let leeway_min_x = icon_hit_start_x.min(menu_x);
                    let leeway_max_x = icon_hit_end_x.max(menu_x + menu_width);
                    let leeway_min_y = icon_top.min(menu_y);
                    let leeway_max_y = icon_bottom.max(menu_y + menu_height);

                    if ptr_x_scaled >= leeway_min_x
                        && ptr_x_scaled <= leeway_max_x
                        && ptr_y_scaled >= leeway_min_y
                        && ptr_y_scaled <= leeway_max_y
                    {
                        pointer_in_leeway = true;
                    }
                }
            }
        }
    }

    // 4. State updates and grace period timers
    let mut effective_should_be_visible = should_be_visible;

    if effective_should_be_visible {
        state.hover_state.last_leave_time = None;
    } else if state.hover_state.is_visible {
        let in_safe_zone = pointer_in_leeway || pointer_on_context_menu;
        
        if in_safe_zone {
            effective_should_be_visible = true;
            state.hover_state.last_leave_time = None;
        } else {
            let leave_time = *state
                .hover_state
                .last_leave_time
                .get_or_insert_with(std::time::Instant::now);
                
            if leave_time.elapsed() < std::time::Duration::from_secs(1) {
                effective_should_be_visible = true;
            } else {
                state.hover_state.last_leave_time = None;
            }
        }
    }

    if effective_should_be_visible != state.hover_state.is_visible
        || new_app_id != state.hover_state.app_id
    {
        state.hover_state.is_visible = effective_should_be_visible;
        state.hover_state.app_id = new_app_id;
        layer_changed = true;
    }

    layer_changed
}
