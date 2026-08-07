use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::geometry::context_menu::{ContextMenuGeometry, BASE_ITEM_HEIGHT};
use crate::geometry::WindowListGeometry;
use crate::handlers::context_menu::{execute_menu_action, open_context_menu};
use crate::handlers::dock::{get_windows_for_app, handle_dock_press, handle_dock_release};
use crate::interaction::proximity::calculate_context_menu_anchor;
use crate::AppState;

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
    let (dock_height, _, _spacing, _) = layout;
    let mut layer_changed = false;

    let ptr_log_x = state.interaction.pointer_position.x as i32;
    let ptr_log_y = state.interaction.pointer_position.y as i32;

    let dock_top_bound = state.height.saturating_sub(dock_height);
    let is_over_icons = ptr_log_y >= dock_top_bound;

    let (dock_width, dock_height_val, dock_scale) = state
        .docks
        .first()
        .map(|d| (d.width, d.height, d.scale_factor))
        .unwrap_or((state.width as u32, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height_val as f64 * dock_scale).round() as i32;
    let screen_height_phys = (state.height as f64 * dock_scale).round() as i32;
    let item_h = (BASE_ITEM_HEIGHT as f64 * dock_scale).round() as i32;

    for event in events {
        match event.kind {
            PointerEventKind::Leave { .. } => {}
            PointerEventKind::Press { button, .. } => {
                if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                    let anchor_x = calculate_context_menu_anchor(
                        state,
                        apps_in_dock,
                        phys_width,
                        dock_scale,
                        layout,
                    );

                    let geom = ContextMenuGeometry::default().compute_bounds(
                        anchor_x,
                        phys_dock_height,
                        phys_width,
                        phys_dock_height,
                        state.menu_state.items.len(),
                        dock_scale,
                    );

                    let menu_x = geom.x;
                    let menu_y = (screen_height_phys - phys_dock_height) - geom.phys_height;
                    // Convert logical width to physical and use physical height for consistency
                    let menu_width = (geom.logical_width as f64 * dock_scale).round() as i32;
                    let menu_height = geom.phys_height;

                    let mut ptr_phys_x = (state.interaction.pointer_position.x as f32
                        * dock_scale as f32)
                        .round() as i32;
                    let mut ptr_phys_y = (state.interaction.pointer_position.y as f32
                        * dock_scale as f32)
                        .round() as i32;

                    let mut on_popup = false;
                    for dock in &state.docks {
                        if let Some(ref popup) = dock.context_menu_popup {
                            if event.surface == popup.surface {
                                ptr_phys_x =
                                    menu_x + (event.position.0 * dock_scale).round() as i32;
                                ptr_phys_y =
                                    menu_y + (event.position.1 * dock_scale).round() as i32;
                                on_popup = true;
                                break;
                            }
                        }
                    }

                    if on_popup
                        || (ptr_phys_x >= menu_x
                            && ptr_phys_x <= menu_x + menu_width
                            && ptr_phys_y >= menu_y
                            && ptr_phys_y <= menu_y + menu_height)
                    {
                        let clicked_idx = ((ptr_phys_y - menu_y) / item_h) as usize;

                        let item_type_opt = state
                            .menu_state
                            .items
                            .get(clicked_idx)
                            .map(|item| item.item_type.clone());

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

                if state.hover_state.is_visible {
                    if let Some(ref app_id) = state.hover_state.app_id {
                        let windows =
                            get_windows_for_app(app_id, running_by_app, &state.open_windows);
                        let win_count = windows.len();
                        let total_apps = apps_in_dock.len();
                        let hovered_app_index =
                            apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                        let geometry = WindowListGeometry::default();
                        let (menu_x, _, menu_width, menu_height, _, _) = geometry.compute_bounds(
                            phys_width,
                            phys_dock_height,
                            total_apps,
                            hovered_app_index,
                            win_count,
                            dock_scale,
                        );

                        let menu_y = (screen_height_phys - phys_dock_height) - menu_height;

                        let mut ptr_phys_x = (state.interaction.pointer_position.x as f32
                            * dock_scale as f32)
                            .round() as i32;
                        let mut ptr_phys_y = (state.interaction.pointer_position.y as f32
                            * dock_scale as f32)
                            .round() as i32;

                        let mut on_popup = false;
                        for dock in &state.docks {
                            if let Some(ref popup) = dock.hover_popup {
                                if event.surface == popup.surface {
                                    ptr_phys_x =
                                        menu_x + (event.position.0 * dock_scale).round() as i32;
                                    ptr_phys_y =
                                        menu_y + (event.position.1 * dock_scale).round() as i32;
                                    on_popup = true;
                                    break;
                                }
                            }
                        }

                        if on_popup
                            || (ptr_phys_x >= menu_x
                                && ptr_phys_x <= menu_x + menu_width
                                && ptr_phys_y >= menu_y
                                && ptr_phys_y <= menu_y + menu_height)
                        {
                            let clicked_idx = ((ptr_phys_y - menu_y) / item_h) as usize;
                            let mut should_close_window_list = true;

                            if let Some(handle_id) = windows.get(clicked_idx) {
                                let close_box_x_start = menu_x + menu_width - item_h;
                                let is_on_close_box = ptr_phys_x >= close_box_x_start;

                                match button {
                                    BTN_LEFT => {
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            if is_on_close_box {
                                                win.handle.close();
                                                should_close_window_list = false;
                                            // Keep window list open so it rerenders
                                            } else if let Some(seat) = &state.wl_seat {
                                                win.handle.activate(seat);
                                                // should_close_window_list remains true for activating a window
                                            }
                                        }
                                    }
                                    BTN_MIDDLE => {
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            win.handle.close();
                                            should_close_window_list = false; // Keep window list open on middle-click close
                                        }
                                    }
                                    _ => {}
                                }
                            }

                            if should_close_window_list {
                                state.hover_state.is_visible = false;
                                state.hover_state.app_id = None;
                            }

                            layer_changed = true;
                            continue;
                        }
                    }
                }

                match button {
                    BTN_LEFT if is_over_icons => {
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
                    BTN_RIGHT if is_over_icons => {
                        layer_changed |= open_context_menu(
                            state,
                            ptr_log_x,
                            apps_in_dock,
                            running_by_app,
                            scale_factor,
                            layout,
                        );
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
