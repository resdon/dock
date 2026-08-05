use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use crate::AppState;
use super::dock::{get_windows_for_app, normalize_app_id, launch_app};

pub fn open_context_menu(
    state: &mut AppState,
    ptr_log_x: i32,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
    let dock_scale = state.docks.first().map(|d| d.scale_factor).unwrap_or(scale_factor as f64);
    let dock_width = state.docks.first().map(|d| d.width as i32).unwrap_or(state.width as i32);
    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let ptr_phys_x = (ptr_log_x as f64 * dock_scale).round() as i32;

    let total_apps = apps_in_dock.len();
    if total_apps == 0 {
        return false;
    }

    let box_size = 48;
    let spacing = 8;
    let icon_base_size = (box_size as f64 * dock_scale).round() as i32;
    let icon_spacing = (spacing as f64 * dock_scale).round() as i32;
    let total_icons_width = total_apps as i32 * icon_base_size + (total_apps as i32 - 1).max(0) * icon_spacing;
    let start_x_offset = (phys_width - total_icons_width) / 2;

    for (index, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_x_offset + index as i32 * (icon_base_size + icon_spacing);
        let hit_start_x = start_x - icon_spacing / 2;
        let hit_end_x = start_x + icon_base_size + icon_spacing / 2;

        if ptr_phys_x >= hit_start_x && ptr_phys_x < hit_end_x {
            state.hover_state.is_visible = false;
            state.hover_state.app_id = None;
            state.menu_state.target_app_id = Some(app_id.clone());

            let windows = get_windows_for_app(app_id, running_by_app, &state.open_windows);
            state.menu_state.target_window = windows
                .iter()
                .find(|id| state.open_windows.get(*id).map_or(false, |w| w.is_activated))
                .cloned()
                .or_else(|| windows.first().cloned());

            let is_running = !windows.is_empty();
            let is_pinned = state.pinned_apps.contains(&normalize_app_id(app_id));
            let actions = dockman_lib::get_desktop_actions(app_id);

            let mut items = Vec::new();
            if is_running {
                items.push(crate::ContextMenuItem {
                    label: "Focus".into(),
                    item_type: crate::MenuItemType::Focus,
                });
                items.push(crate::ContextMenuItem {
                    label: "New Instance".into(),
                    item_type: crate::MenuItemType::LaunchNew,
                });
                items.push(crate::ContextMenuItem {
                    label: "Minimize".into(),
                    item_type: crate::MenuItemType::Minimize,
                });
            } else {
                items.push(crate::ContextMenuItem {
                    label: "Launch".into(),
                    item_type: crate::MenuItemType::LaunchNew,
                });
            }

            for action in actions {
                items.push(crate::ContextMenuItem {
                    label: action.name.clone(),
                    item_type: crate::MenuItemType::Action(action),
                });
            }

            let pin_label = if is_pinned { "Unpin from Dock" } else { "Pin to Dock" };
            items.push(crate::ContextMenuItem {
                label: pin_label.into(),
                item_type: crate::MenuItemType::TogglePin,
            });

            if is_running {
                items.push(crate::ContextMenuItem {
                    label: "Quit Application".into(),
                    item_type: crate::MenuItemType::CloseApp,
                });
            }

            state.menu_state.items = items;
            state.menu_state.is_open = true;
            state.menu_state.just_opened = true;
            state.menu_state.opened_by_button = Some(273); // BTN_RIGHT
            state.menu_state.waiting_for_initial_release = true;
            state.needs_redraw = true;

            for dock in &mut state.docks {
                dock.menu_fade.show();
            }

            return true;
        }
    }
    false
}

pub fn check_and_consume_menu_press(
    state: &mut AppState,
    scale_factor: f32,
    phys_width: i32,
    phys_height: i32,
) -> (bool, bool) {
    let total_items = state.menu_state.items.len();
    if total_items == 0 {
        return (false, false);
    }

    let (menu_x, menu_y, menu_width, total_menu_h) =
        state.get_context_menu_bounds(phys_width, phys_height, scale_factor);

    let menu_log_x = (menu_x as f32 / scale_factor).round() as i32;
    let menu_log_y = (menu_y as f32 / scale_factor).round() as i32;
    let menu_log_w = (menu_width as f32 / scale_factor).round() as i32;
    let menu_log_h = (total_menu_h as f32 / scale_factor).round() as i32;

    let ptr_log_x = state.interaction.pointer_position.x as i32;
    let ptr_log_y = state.interaction.pointer_position.y as i32;

    let inside_menu = ptr_log_x >= menu_log_x
        && ptr_log_x <= (menu_log_x + menu_log_w)
        && ptr_log_y >= menu_log_y
        && ptr_log_y <= (menu_log_y + menu_log_h);

    let mut layer_changed = false;
    if !inside_menu && !state.menu_state.just_opened {
        for dock in &mut state.docks {
            dock.menu_fade.hide();
        }
        state.needs_redraw = true;
        layer_changed = true;
    }

    (inside_menu, layer_changed)
}

pub fn handle_menu_release(
    state: &mut AppState,
    phys_width: i32,
    phys_height: i32,
    scale_factor: f32,
) -> bool {
    let total_items = state.menu_state.items.len();
    if total_items == 0 {
        return false;
    }

    let (menu_x, menu_y, menu_width, total_menu_h) =
        state.get_context_menu_bounds(phys_width, phys_height, scale_factor);

    let menu_log_x = (menu_x as f32 / scale_factor).round() as i32;
    let menu_log_y = (menu_y as f32 / scale_factor).round() as i32;
    let menu_log_w = (menu_width as f32 / scale_factor).round() as i32;
    let menu_log_h = (total_menu_h as f32 / scale_factor).round() as i32;

    let ptr_log_x = state.interaction.pointer_position.x as i32;
    let ptr_log_y = state.interaction.pointer_position.y as i32;

    if ptr_log_x >= menu_log_x 
        && ptr_log_x <= (menu_log_x + menu_log_w) 
        && ptr_log_y >= menu_log_y 
        && ptr_log_y <= (menu_log_y + menu_log_h) 
    {
        let item_h_log = ((total_menu_h as f32 / total_items as f32) / scale_factor).round() as i32;
        let clicked_item_idx = ((ptr_log_y - menu_log_y) / item_h_log.max(1)).clamp(0, (total_items - 1) as i32);
        
        let item_type = state.menu_state.items
            .get(clicked_item_idx as usize)
            .map(|item| item.item_type.clone());

        if let Some(item_type) = item_type {
            execute_menu_action(state, &item_type);
        }
        state.is_dragging = false;
        state.dragged_app_id = None;
        for dock in &mut state.docks {
            dock.menu_fade.hide();
        }
        state.needs_redraw = true;
        return true;
    }
    false
}

pub fn execute_menu_action(state: &mut AppState, item_type: &crate::MenuItemType) {
    match item_type {
        crate::MenuItemType::Focus => {
            if let Some(handle_id) = &state.menu_state.target_window {
                if let Some(window_info) = state.open_windows.get_mut(handle_id) {
                    if let Some(seat) = &state.wl_seat { window_info.handle.activate(seat); }
                }
            }
        }
        crate::MenuItemType::LaunchNew => {
            if let Some(app_id) = &state.menu_state.target_app_id {
                launch_app(app_id);
            }
        }
        crate::MenuItemType::Minimize => {
            if let Some(handle_id) = &state.menu_state.target_window {
                if let Some(window_info) = state.open_windows.get_mut(handle_id) {
                    window_info.handle.set_minimized();
                }
            }
        }
        crate::MenuItemType::Action(action) => {
            let _ = std::process::Command::new("sh").arg("-c").arg(&action.exec).spawn();
        }
        crate::MenuItemType::TogglePin => {
            if let Some(app_id) = &state.menu_state.target_app_id {
                let app_id = normalize_app_id(app_id);
                let mut pinned = crate::cache::persistence::load_pinned_apps();
                if pinned.contains(&app_id) {
                    pinned.retain(|x| x != &app_id);
                } else {
                    pinned.push(app_id.clone());
                    if let Some((rgba, size)) = state.icon_cache.get(&app_id) {
                        crate::cache::save_cached_icon(&app_id, *size, *size, rgba);
                    } else if let Some(window_info) = state.open_windows.values().find(|w| w.app_id == app_id) {
                        if let Some(rgba) = &window_info.icon_rgba {
                            crate::cache::save_cached_icon(&app_id, window_info.icon_size, window_info.icon_size, rgba);
                        }
                    }
                }
                crate::cache::persistence::save_pinned_apps(&pinned);
                state.pinned_apps = pinned;
            }
        }
        crate::MenuItemType::CloseApp => {
            if let Some(app_id) = state.menu_state.target_app_id.clone() {
                state.close_application_completely(&app_id);
            }
        }
    }
}
