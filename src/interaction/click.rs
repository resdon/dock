use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::handlers::dock::{get_windows_for_app, handle_dock_press, handle_dock_release};
use crate::handlers::popup::{execute_menu_action, open_context_menu};
use crate::pointer::coordinates::calculate_popup_anchor;
use crate::pointer::motion::{get_hovered_app, IconLayout};

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

    // Pointer coordinates on the layer-surface are dock-local. The old code
    // compared them against screen-height coordinates, which made dock clicks
    // depend on the monitor height.
    let is_over_icons = ptr_log_y >= 0 && ptr_log_y <= dock_height;

    let (dock_width, dock_height_val, dock_scale) = state
        .docks
        .first()
        .map(|d| (d.width, d.height, d.scale_factor))
        .unwrap_or((state.width as u32, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height_val as f64 * dock_scale).round() as i32;
    let item_h = (PopupGeometry::BASE_ITEM_HEIGHT as f64 * dock_scale).round() as i32;

    for event in events {
        match event.kind {
            PointerEventKind::Leave { .. } => {}
            PointerEventKind::Press { button, .. } => {
                // A left press on the context-menu surface belongs to the
                // menu and must remain open until Release selects the item.
                // Only clicks outside the popup dismiss it.
                if state.menu_state.is_open && button == BTN_LEFT {
                    let on_context_popup = state.docks.iter().any(|dock| {
                        dock.context_menu_popup
                            .as_ref()
                            .is_some_and(|popup| popup.surface == event.surface)
                    });

                    if !on_context_popup {
                        state.menu_state.is_open = false;
                        state.menu_state.target_app_id = None;
                        layer_changed = true;
                        continue;
                    }
                }

                // If window list popup is open, check if this press event is on a popup surface or inside its bounds
                if state.window_list_state.is_open {
                    let mut clicked_on_popup = false;

                    for dock in &state.docks {
                        if let Some(ref popup) = dock.window_list_popup {
                            if event.surface == popup.surface {
                                clicked_on_popup = true;
                                break;
                            }
                        }
                        if let Some(ref popup) = dock.hover_popup {
                            if event.surface == popup.surface {
                                clicked_on_popup = true;
                                break;
                            }
                        }
                    }

                    if !clicked_on_popup {
                        let ptr_phys_x = (state.interaction.pointer_position.x as f32
                            * dock_scale as f32)
                            .round() as i32;
                        let ptr_phys_y = (state.interaction.pointer_position.y as f32
                            * dock_scale as f32)
                            .round() as i32;

                        if state.window_list_state.is_open {
                            if let Some(ref app_id) = state.window_list_state.target_app_id {
                                let windows = get_windows_for_app(
                                    app_id,
                                    running_by_app,
                                    &state.open_windows,
                                );
                                if !windows.is_empty() {
                                    let anchor_x = calculate_popup_anchor(
                                        state,
                                        app_id,
                                        apps_in_dock,
                                        phys_width,
                                        scale_factor as f64,
                                        layout,
                                    );
                                    let anchor_y = 0;
                                    let item_count = windows.len() as i32;
                                    let geom = PopupGeometry::compute_bounds(
                                        PopupType::WindowList,
                                        anchor_x,
                                        anchor_y,
                                        phys_width,
                                        phys_dock_height,
                                        item_count,
                                        dock_scale as f32,
                                    );
                                    let menu_x = geom.x;
                                    let menu_y = geom.y;
                                    let menu_width =
                                        (geom.logical_width as f64 * dock_scale).round() as i32;
                                    let menu_height = geom.phys_height;

                                    if ptr_phys_x >= menu_x
                                        && ptr_phys_x <= menu_x + menu_width
                                        && ptr_phys_y >= menu_y
                                        && ptr_phys_y <= menu_y + menu_height
                                    {
                                        clicked_on_popup = true;
                                    }
                                }
                            }
                        }
                    }

                    if clicked_on_popup {
                        layer_changed = true;
                        continue;
                    } else {
                        state.window_list_state.is_open = false;
                        state.window_list_state.target_app_id = None;
                        for dock in &mut state.docks {
                            dock.window_list_fade.hide();
                        }
                        layer_changed = true;
                    }
                }

                // Normal Dock Actions on Press
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
                        // Close window list when opening context menu
                        state.window_list_state.is_open = false;
                        state.window_list_state.target_app_id = None;
                        for dock in &mut state.docks {
                            dock.window_list_fade.hide();
                        }

                        layer_changed |= open_context_menu(
                            state,
                            ptr_log_x,
                            apps_in_dock,
                            running_by_app,
                            scale_factor,
                            layout,
                        );
                    }
                    BTN_MIDDLE if is_over_icons => {
                        if let Some(dock) = state.docks.first_mut() {
                            let apps_refs: Vec<&str> =
                                apps_in_dock.iter().map(|s| s.as_str()).collect();
                            let icon_layout = IconLayout {
                                box_size: 48.0,
                                spacing: 8.0,
                                dock_height: dock_height_val as f32,
                                scale: scale_factor,
                            };
                            if let Some((_, app_id)) = get_hovered_app(
                                &apps_refs,
                                (
                                    state.interaction.pointer_position.x,
                                    state.interaction.pointer_position.y,
                                ),
                                state.interaction.pointer_inside,
                                dock_width,
                                dock_height_val,
                                &icon_layout,
                            ) {
                                let app_string = app_id.to_string();

                                state.menu_state.is_open = false;
                                state.menu_state.target_app_id = None;

                                if state.window_list_state.is_open
                                    && state.window_list_state.target_app_id.as_deref()
                                        == Some(&app_string)
                                {
                                    state.window_list_state.is_open = false;
                                    state.window_list_state.target_app_id = None;
                                    dock.window_list_fade.hide();
                                } else {
                                    state.window_list_state.target_app_id = Some(app_string);
                                    state.window_list_state.is_open = true;
                                    dock.window_list_fade.set_visible(true);
                                    dock.window_list_fade.show();
                                }
                                layer_changed = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            PointerEventKind::Release { button, .. } => {
                if state.menu_state.waiting_for_initial_release
                    && state.menu_state.opened_by_button == Some(button)
                {
                    state.menu_state.waiting_for_initial_release = false;
                    layer_changed = true;
                    continue;
                }

                // 1. Handle Context Menu Click on Release
                if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                    let target_app = state
                        .menu_state
                        .target_app_id
                        .as_deref()
                        .unwrap_or_default();
                    let anchor_x = calculate_popup_anchor(
                        state,
                        target_app,
                        apps_in_dock,
                        phys_width,
                        scale_factor as f64,
                        layout,
                    );
                    let anchor_y = 0;
                    let item_count = state.menu_state.items.len() as i32;
                    let geom = PopupGeometry::compute_bounds(
                        PopupType::ContextMenu,
                        anchor_x,
                        anchor_y,
                        phys_width,
                        phys_dock_height,
                        item_count,
                        dock_scale as f32,
                    );
                    let menu_x = geom.x;
                    let menu_y = geom.y;
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
                                ptr_phys_x = ((event.position.0 + popup.position.0 as f64)
                                    * dock_scale)
                                    .round() as i32;
                                ptr_phys_y = ((event.position.1 + popup.position.1 as f64)
                                    * dock_scale)
                                    .round() as i32;
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
                        state.menu_state.target_app_id = None;
                        state.is_dragging = false;
                        state.dragged_app_id = None;
                        layer_changed = true;
                        continue;
                    }
                }

                // 2. Handle Window List Click on Release
                if state.window_list_state.is_open {
                    if let Some(ref app_id) = state.window_list_state.target_app_id {
                        let windows =
                            get_windows_for_app(app_id, running_by_app, &state.open_windows);
                        if !windows.is_empty() {
                            let anchor_x = calculate_popup_anchor(
                                state,
                                app_id,
                                apps_in_dock,
                                phys_width,
                                scale_factor as f64,
                                layout,
                            );
                            let anchor_y = 0;
                            let item_count = windows.len() as i32;
                            let geom = PopupGeometry::compute_bounds(
                                PopupType::WindowList,
                                anchor_x,
                                anchor_y,
                                phys_width,
                                phys_dock_height,
                                item_count,
                                dock_scale as f32,
                            );
                            let menu_x = geom.x;
                            let menu_y = geom.y;
                            let menu_width =
                                (geom.logical_width as f64 * dock_scale).round() as i32;
                            let menu_height = geom.phys_height;

                            let mut ptr_phys_x = (state.interaction.pointer_position.x as f32
                                * dock_scale as f32)
                                .round() as i32;
                            let mut ptr_phys_y = (state.interaction.pointer_position.y as f32
                                * dock_scale as f32)
                                .round() as i32;

                            let mut on_popup = false;
                            for dock in &state.docks {
                                if let Some(ref popup) = dock.window_list_popup {
                                    if event.surface == popup.surface {
                                        ptr_phys_x = ((event.position.0 + popup.position.0 as f64)
                                            * dock_scale)
                                            .round()
                                            as i32;
                                        ptr_phys_y = ((event.position.1 + popup.position.1 as f64)
                                            * dock_scale)
                                            .round()
                                            as i32;
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
                                            if let Some(win) = state.open_windows.get_mut(handle_id)
                                            {
                                                if is_on_close_box {
                                                    win.handle.close();
                                                    should_close_window_list = false;
                                                } else if let Some(seat) = &state.wl_seat {
                                                    win.handle.activate(seat);
                                                }
                                            }
                                        }
                                        BTN_MIDDLE => {
                                            if let Some(win) = state.open_windows.get_mut(handle_id)
                                            {
                                                win.handle.close();
                                                should_close_window_list = false;
                                            }
                                        }
                                        _ => {}
                                    }
                                }

                                if should_close_window_list {
                                    state.window_list_state.is_open = false;
                                    state.window_list_state.target_app_id = None;
                                    for dock in &mut state.docks {
                                        dock.window_list_fade.hide();
                                    }
                                }

                                layer_changed = true;
                                continue;
                            }
                        }
                    }
                }

                // 3. Catch-all popup release check to prevent fallback to dock
                let mut on_popup = false;
                for dock in &state.docks {
                    if let Some(ref popup) = dock.window_list_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                    if let Some(ref popup) = dock.context_menu_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                    if let Some(ref popup) = dock.hover_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                }

                if on_popup {
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
