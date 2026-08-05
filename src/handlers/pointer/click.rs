use std::collections::HashMap;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use wayland_client::backend::ObjectId;

use crate::app::AppState;
use crate::geometry::WindowListGeometry;
use crate::geometry::context_menu::{ContextMenuGeometry, BASE_ITEM_HEIGHT};
use super::context_menu::{execute_menu_action, open_context_menu};
use super::dock::{get_windows_for_app, handle_dock_press, handle_dock_release};

const BTN_LEFT: u32 = 272;
const BTN_RIGHT: u32 = 273;
const BTN_MIDDLE: u32 = 274;

pub fn handle_click_events(
    state: &mut AppState,
    events: &[PointerEvent],
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let (dock_height, _, _, _) = layout;
    let mut layer_changed = false;

    let ptr_log_x = state.interaction.pointer_position.x as i32;
    let ptr_log_y = state.interaction.pointer_position.y as i32;

    let dock_top_bound = (state.height as i32).saturating_sub(dock_height);
    let is_over_icons = ptr_log_y >= dock_top_bound;

    let (dock_width, dock_height_val, dock_scale) = state.docks.first()
        .map(|d| (d.width, d.height, d.scale_factor as f64))
        .unwrap_or((state.width as u32, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_height = (dock_height_val as f64 * dock_scale).round() as i32;
    let item_h = (BASE_ITEM_HEIGHT as f64 * dock_scale).round() as i32;

    for event in events {
        match event.kind {
            PointerEventKind::Leave { .. } => {
                if state.is_dragging {
                    state.is_dragging = false;
                    state.dragged_app_id = None;
                    layer_changed = true;
                }
            }
            PointerEventKind::Press { button, .. } => {
                // Check clicks inside the context menu popup using ContextMenuGeometry and surface mapping (matching window list)
                if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                    let total_apps = apps_in_dock.len();
                    let target_index = state
                        .menu_state
                        .target_app_id
                        .as_ref()
                        .and_then(|app_id| apps_in_dock.iter().position(|id| id == app_id))
                        .unwrap_or(0);

                    let box_size = 48;
                    let spacing = 8;
                    let icon_base_size = (box_size as f64 * dock_scale).round() as i32;
                    let icon_spacing = (spacing as f64 * dock_scale).round() as i32;
                    let total_icons_width = total_apps as i32 * icon_base_size + (total_apps as i32 - 1).max(0) * icon_spacing;
                    let start_x_offset = (phys_width - total_icons_width) / 2;

                    let anchor_x = if total_apps > 0 {
                        let pin_x = start_x_offset + target_index as i32 * (icon_base_size + icon_spacing);
                        pin_x + icon_base_size / 2
                    } else {
                        phys_width / 2
                    };
                    let anchor_y = 0; // Anchored at the top of the dock

                    let geom = ContextMenuGeometry::default().compute_bounds(
                        anchor_x,
                        anchor_y,
                        phys_width,
                        phys_height,
                        state.menu_state.items.len(),
                        dock_scale,
                    );

                    let menu_x = geom.x;
                    let menu_y = geom.y;
					let menu_width = geom.logical_width;
					let menu_height = geom.logical_height;

                    let mut ptr_phys_x = (state.interaction.pointer_position.x as f32 * dock_scale as f32).round() as i32;
                    let mut ptr_phys_y = (state.interaction.pointer_position.y as f32 * dock_scale as f32).round() as i32;

                    let mut on_popup = false;
                    for dock in &state.docks {
                        if let Some(ref popup) = dock.context_menu_popup {
                            if event.surface == popup.surface {
                                ptr_phys_x = menu_x + (event.position.0 * dock_scale).round() as i32;
                                ptr_phys_y = menu_y + (event.position.1 * dock_scale).round() as i32;
                                on_popup = true;
                                break;
                            }
                        }
                    }

					if on_popup || (ptr_phys_x >= menu_x && ptr_phys_x <= menu_x + menu_width && ptr_phys_y >= menu_y && ptr_phys_y <= menu_y + menu_height) {
                        let clicked_idx = ((ptr_phys_y - menu_y) / item_h) as usize;
                        
                        // Clone item_type to drop the immutable borrow on state.menu_state before mutating state
                        let item_type_opt = state.menu_state.items.get(clicked_idx).map(|item| item.item_type.clone());

                        if let Some(item_type) = item_type_opt {
                            execute_menu_action(state, &item_type);
                        }

                        state.menu_state.is_open = false;
                        state.is_dragging = false;
                        state.dragged_app_id = None;
                        layer_changed = true;
                        continue;
                    }
                }

                // Check clicks inside the hover window list popup using WindowListGeometry and surface mapping[cite: 7]
                if state.hover_state.is_visible {
                    if let Some(ref app_id) = state.hover_state.app_id {
                        let windows = get_windows_for_app(app_id, running_by_app, &state.open_windows);
                        let win_count = windows.len();
                        let total_apps = apps_in_dock.len();
                        let hovered_app_index = apps_in_dock
                            .iter()
                            .position(|id| id == app_id)
                            .unwrap_or(0);

                        let geometry = WindowListGeometry::default();
                        let (menu_x, menu_y, menu_width, menu_height, _, _) = geometry.compute_bounds(
                            phys_width,
                            phys_height,
                            total_apps,
                            hovered_app_index,
                            win_count,
                            dock_scale,
                        );

                        let mut ptr_phys_x = (state.interaction.pointer_position.x as f32 * dock_scale as f32).round() as i32;
                        let mut ptr_phys_y = (state.interaction.pointer_position.y as f32 * dock_scale as f32).round() as i32;

                        let mut on_popup = false;
                        for dock in &state.docks {
                            if let Some(ref popup) = dock.hover_popup {
                                if event.surface == popup.surface {
                                    ptr_phys_x = menu_x + (event.position.0 * dock_scale).round() as i32;
                                    ptr_phys_y = menu_y + (event.position.1 * dock_scale).round() as i32;
                                    on_popup = true;
                                    break;
                                }
                            }
                        }

                        if on_popup || (ptr_phys_x >= menu_x && ptr_phys_x <= menu_x + menu_width && ptr_phys_y >= menu_y && ptr_phys_y <= menu_y + menu_height) {
                            let clicked_idx = ((ptr_phys_y - menu_y) / item_h) as usize;
                            if let Some(handle_id) = windows.get(clicked_idx) {
                                let close_box_x_start = menu_x + menu_width - item_h;
                                let is_on_close_box = ptr_phys_x >= close_box_x_start;

                                match button {
                                    BTN_LEFT => {
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            if is_on_close_box {
                                                win.handle.close();
                                            } else if let Some(seat) = &state.wl_seat {
                                                win.handle.activate(seat);
                                            }
                                        }
                                    }
                                    BTN_MIDDLE => {
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            win.handle.close();
                                        }
                                    }
                                    _ => {}
                                }
                            }
                            state.hover_state.is_visible = false;
                            state.hover_state.app_id = None;
                            layer_changed = true;
                            continue;
                        }
                    }
                }

                match button {
                    BTN_LEFT => {
                        if is_over_icons {
                            layer_changed |= handle_dock_press(
                                state,
                                button,
                                ptr_log_x,
                                ptr_log_y,
                                apps_in_dock,
                                layout,
                                scale_factor,
                            );
                        }
                    }
                    BTN_RIGHT => {
                        if is_over_icons {
                            layer_changed |= open_context_menu(
                                state,
                                ptr_log_x,
                                apps_in_dock,
                                running_by_app,
                                scale_factor,
                                layout,
                            );
                        }
                    }
                    _ => {}
                }
            }
            PointerEventKind::Release { button, .. } => {
                if state.menu_state.is_open
                    && state.menu_state.waiting_for_initial_release
                    && state.menu_state.opened_by_button == Some(button)
                {
                    state.menu_state.waiting_for_initial_release = false;
                    layer_changed = true;
                    continue;
                }

                if button == BTN_LEFT || button == BTN_MIDDLE {
                    layer_changed |= handle_dock_release(
                        state,
                        button,
                        ptr_log_x,
                        is_over_icons,
                        apps_in_dock,
                        running_by_app,
                        layout,
                        scale_factor,
                    );
                }
            }
            _ => {}
        }
    }

    if !state.menu_state.waiting_for_initial_release {
        state.menu_state.just_opened = false;
    }

    layer_changed
}
