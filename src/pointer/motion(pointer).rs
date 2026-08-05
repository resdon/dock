use std::collections::HashMap;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_surface::WlSurface;

use super::coordinates::map_coordinates;
use crate::ContextMenuGeometry;
use crate::AppState;

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

                let mut mapped_coords = None;

                for dock in &state.docks {
                    let scale = dock.scale_factor as f64;

                    // 1. Check hover popup surface coordinates
                    if let Some(ref popup) = dock.hover_popup {
                        if event.surface == popup.surface {
                            if let Some(ref app_id) = state.hover_state.app_id {
                                let apps_in_dock = state.get_apps_in_dock();
                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock
                                    .iter()
                                    .position(|id| id == app_id)
                                    .unwrap_or(0);

                                let running_by_app = state.get_running_by_app();
                                let count = running_by_app.get(app_id).map_or(1, |v| v.len().max(1));

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

                    // 2. Check context menu surface coordinates
                    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                        let phys_width = (dock.width as f64 * scale).round() as i32;
                        let phys_height = (dock.height as f64 * scale).round() as i32;
                        let menu_item_count = state.menu_state.items.len();

                        let mut anchor_x = phys_width / 2;

                        if let Some(ref target_app) = state.menu_state.target_app_id {
                            for d in &state.docks {
                                if let Some(ref dock_state) = d.dock_state {
                                    if let Some(pin) = dock_state.pins.iter().find(|p| &p.app_id == target_app) {
                                        let pin_center_logical = pin.x as f32 + (pin.size as f32 / 2.0);
                                        anchor_x = (pin_center_logical * scale as f32).round() as i32;
                                        break;
                                    }
                                }
                            }
                        }

                        let mut geom = ContextMenuGeometry::default().compute_bounds(
                            anchor_x,
                            phys_height,
                            phys_width,
                            phys_height,
                            menu_item_count,
                            scale,
                        );

                        let screen_height_phys = (state.height as f64 * scale).round() as i32;
                        let dock_top_phys = screen_height_phys - phys_height;
                        geom.y = dock_top_phys - geom.phys_height;

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
            PointerEventKind::Leave { .. } => {
                // Leave events are handled coordinate-wise in update_hover_and_proximity
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

    let (dock_width, dock_height_val, dock_scale) = state
        .docks
        .first()
        .map(|d| (d.width, d.height, d.scale_factor as f64))
        .unwrap_or((100, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_height = (dock_height_val as f64 * dock_scale).round() as i32;

    // 1. Context Menu Handling
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let menu_item_count = state.menu_state.items.len();

        let mut anchor_x = phys_width / 2;
        let mut icon_hit_start_x = 0;
        let mut icon_hit_end_x = 0;
        let mut found_pin = false;

        if let Some(ref target_app) = state.menu_state.target_app_id {
            for d in &state.docks {
                if let Some(ref dock_state) = d.dock_state {
                    for pin in &dock_state.pins {
                        if &pin.app_id == target_app {
                            let pin_center_logical = pin.x as f32 + (pin.size as f32 / 2.0);
                            anchor_x = (pin_center_logical * dock_scale as f32).round() as i32;

                            let hit_start = pin.x.saturating_sub(spacing / 2) as f32;
                            let hit_end = (pin.x + pin.size as i32 + (spacing / 2)) as f32;

                            icon_hit_start_x = (hit_start * scale_factor).round() as i32;
                            icon_hit_end_x = (hit_end * scale_factor).round() as i32;
                            found_pin = true;
                            break;
                        }
                    }
                }
                if found_pin {
                    break;
                }
            }
        }

        if !found_pin {
            let half_box = ((spacing * 2) as f32 * scale_factor).round() as i32;
            icon_hit_start_x = anchor_x - half_box;
            icon_hit_end_x = anchor_x + half_box;
        }

        let mut geom = ContextMenuGeometry::default().compute_bounds(
            anchor_x,
            phys_height,
            phys_width,
            phys_height,
            menu_item_count,
            dock_scale,
        );

        let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;
        let dock_top_phys = screen_height_phys - phys_height;
        geom.y = dock_top_phys - geom.phys_height;

        if ptr_x_scaled >= geom.x
            && ptr_x_scaled <= (geom.x + geom.phys_width)
            && ptr_y_scaled >= geom.y
            && ptr_y_scaled <= (geom.y + geom.phys_height)
        {
            pointer_on_context_menu = true;
        }

        let icon_top = (dock_top_bound as f32 * scale_factor).round() as i32;
        let in_gap_y = ptr_y_scaled >= (geom.y - 10) && ptr_y_scaled <= (icon_top + (dock_height as f32 * scale_factor) as i32);

        let leeway_min_x = icon_hit_start_x.min(geom.x) - 10;
        let leeway_max_x = icon_hit_end_x.max(geom.x + geom.phys_width) + 10;

        if in_gap_y && ptr_x_scaled >= leeway_min_x && ptr_x_scaled <= leeway_max_x {
            pointer_in_leeway = true;
        }

        let menu_in_safe_zone = pointer_on_context_menu || pointer_in_leeway;

        if menu_in_safe_zone {
            for dock in &mut state.docks {
                dock.menu_fade.show();
            }
        } else {
            for dock in &mut state.docks {
                dock.menu_fade.hide();
            }
            state.menu_state.is_open = false;
            state.menu_state.target_app_id = None;
            layer_changed = true;
        }
    } else {
        // 2. Window Hover & Icon Proximity
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
                if should_be_visible {
                    break;
                }
            }
        }

        let active_app_id = new_app_id.clone().or_else(|| {
            if state.hover_state.is_visible {
                state.hover_state.app_id.clone()
            } else {
                None
            }
        });

        if let Some(app_id) = active_app_id {
            let win_count = running_by_app.get(&app_id).map_or(1, |v| v.len().max(1));

            let hovered_app_index = apps_in_dock
                .iter()
                .position(|id| id == &app_id)
                .unwrap_or(0);

            let (menu_x, menu_y, menu_width, menu_height, _, _) = window_list_geometry.compute_bounds(
                phys_width,
                phys_height,
                total_items,
                hovered_app_index,
                win_count,
                dock_scale,
            );

            let is_over_window_list = ptr_x_scaled >= menu_x
                && ptr_x_scaled <= (menu_x + menu_width)
                && ptr_y_scaled >= menu_y
                && ptr_y_scaled <= (menu_y + menu_height);

            if is_over_window_list {
                should_be_visible = true;
                new_app_id = Some(app_id.clone());
            }

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
                if found_pin {
                    break;
                }
            }

            if found_pin {
                let icon_top = (dock_top_bound as f32 * scale_factor).round() as i32;
                let in_gap_y = ptr_y_scaled >= (menu_y - 10) && ptr_y_scaled <= (icon_top + (dock_height as f32 * scale_factor) as i32);

                let leeway_min_x = icon_hit_start_x.min(menu_x) - 10;
                let leeway_max_x = icon_hit_end_x.max(menu_x + menu_width) + 10;

                if in_gap_y && ptr_x_scaled >= leeway_min_x && ptr_x_scaled <= leeway_max_x {
                    pointer_in_leeway = true;
                    new_app_id = Some(app_id);
                }
            }
        }
    }

    let effective_should_be_visible = should_be_visible || pointer_in_leeway || pointer_on_context_menu;

    let visibility_changed = effective_should_be_visible != state.hover_state.is_visible;
    let app_changed = effective_should_be_visible && (new_app_id != state.hover_state.app_id);

    if visibility_changed || app_changed {
        state.hover_state.is_visible = effective_should_be_visible;
        state.hover_state.app_id = if effective_should_be_visible {
            new_app_id.or_else(|| state.hover_state.app_id.clone())
        } else {
            None
        };
        layer_changed = true;

        for dock in &mut state.docks {
            if effective_should_be_visible {
                dock.hover_fade.show();
            } else {
                dock.hover_fade.hide();
            }
        }
    }

    layer_changed
}
