use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};

use crate::app::AppState;
use crate::render::window_list::get_hover_menu_bounds;
use super::drag::handle_drag_release;

pub fn handle_click_events(
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
    let is_over_icons = state.interaction.pointer_position.y >= dock_top_bound.into();

    for event in events {
        match event.kind {
            PointerEventKind::Press { button, .. } => match button {
                272 => { // Left Click Press
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
                            let end_x = start_x + box_size;
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = end_x + (spacing / 2);

                            if state.interaction.pointer_position.x >= hit_start_x.into() && state.interaction.pointer_position.x <= hit_end_x.into() {
                                state.dragged_app_id = Some(app_id.clone());
                                state.drag_start_x = state.interaction.pointer_position.x as i32;
                                state.drag_start_y = state.interaction.pointer_position.y as i32;
                                state.is_dragging = false;
                                break;
                            }
                        }
                    }
                }
                273 => { // Right Click Press
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = start_x + box_size + (spacing / 2);

                            if state.interaction.pointer_position.x >= hit_start_x.into() && state.interaction.pointer_position.x <= hit_end_x.into() {
                                state.hover_state.is_visible = false;
                                state.hover_state.app_id = None;
                                state.menu_state.is_open = true;
                                state.needs_redraw = true;
                                state.menu_state.x = state.interaction.pointer_position.x as usize;
                                state.menu_state.y = state.interaction.pointer_position.y as usize;
                                state.menu_state.target_app_id = Some(app_id.clone());

                                let windows = running_by_app.get(app_id).cloned().unwrap_or_default();
                                state.menu_state.target_window = windows.iter()
                                    .find(|id| state.open_windows.get(id).map(|w| w.is_activated).unwrap_or(false))
                                    .cloned()
                                    .or_else(|| windows.first().cloned());

                                let is_running = !windows.is_empty();
                                let is_pinned = state.pinned_apps.contains(app_id);
                                let actions = dockman_lib::get_desktop_actions(app_id);

                                let mut items = Vec::new();
                                if is_running {
                                    items.push(crate::ContextMenuItem { label: "Focus".into(), item_type: crate::MenuItemType::Focus });
                                    items.push(crate::ContextMenuItem { label: "New Instance".into(), item_type: crate::MenuItemType::LaunchNew });
                                    items.push(crate::ContextMenuItem { label: "Minimize".into(), item_type: crate::MenuItemType::Minimize });
                                } else {
                                    items.push(crate::ContextMenuItem { label: "Launch".into(), item_type: crate::MenuItemType::LaunchNew });
                                }

                                for action in actions {
                                    items.push(crate::ContextMenuItem { label: action.name.clone(), item_type: crate::MenuItemType::Action(action) });
                                }

                                let pin_label = if is_pinned { "Unpin from Dock" } else { "Pin to Dock" };
                                items.push(crate::ContextMenuItem { label: pin_label.into(), item_type: crate::MenuItemType::TogglePin });

                                if is_running {
                                    items.push(crate::ContextMenuItem { label: "Quit Application".into(), item_type: crate::MenuItemType::CloseApp });
                                }

                                state.menu_state.items = items;
                                layer_changed = true;
                                break;
                            }
                        }
                    }
                }
                274 => { // Middle Click Press
                    let mut handled = false;
                    if state.hover_state.is_visible {
                        if let Some(ref app_id) = state.hover_state.app_id {
                            if let Some(windows) = running_by_app.get(app_id) {
                                // Correct implementation:
                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);
                                let win_count = state.open_windows.values().filter(|w| w.app_id == *app_id).count().max(1);

                                let menu_w = (180.0_f64 * state.scale_factor).round() as i32;
                                let menu_h = (win_count as f64 * 30.0_f64 * state.scale_factor).round() as i32;
                                let scale_i32 = state.scale_factor.round() as i32;

                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    state.width as i32, state.height as i32, total_apps, hovered_app_index, menu_w, menu_h, scale_i32,
                                );
                                let ptr_x = (state.interaction.pointer_position.x as f32 * scale_factor) as i32;
                                let ptr_y = (state.interaction.pointer_position.y as f32 * scale_factor) as i32;
                                let item_h = (30.0 * scale_factor).round() as i32;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width && ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
                                    let idx = ((ptr_y - menu_y) as usize) / item_h as usize;
                                    if let Some(handle_id) = windows.get(idx) {
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            win.handle.close();
                                            handled = true;
                                            layer_changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if !handled && is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = start_x + box_size + (spacing / 2);

                            if state.interaction.pointer_position.x >= hit_start_x.into() && state.interaction.pointer_position.x <= hit_end_x.into() {
                                launch_app(app_id);
                                layer_changed = true;
                                break;
                            }
                        }
                    }
                }
                _ => {}
            },
            PointerEventKind::Release { button, .. } => {
                if button == 272 { // Left Click Release
                    // Context Menu
                    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                        let phys_width = (state.width as f32 * scale_factor).round() as i32;
                        let phys_height = (state.height as f32 * scale_factor).round() as i32;
                        let item_h = (30.0 * scale_factor).round() as i32;

                        let (menu_x, menu_y, menu_width, total_menu_h) = state.get_context_menu_bounds(phys_width, phys_height, scale_factor);
                        let ptr_x = (state.interaction.pointer_position.x as f32 * scale_factor).round() as i32;
                        let ptr_y = (state.interaction.pointer_position.y as f32 * scale_factor).round() as i32;

                        if ptr_x >= menu_x && ptr_x <= (menu_x + menu_width) && ptr_y >= menu_y && ptr_y <= (menu_y + total_menu_h) {
                            let clicked_item_idx = (ptr_y - menu_y) / item_h;
                            let item_type = state.menu_state.items
                                .get(clicked_item_idx as usize)
                                .map(|item| item.item_type.clone());

                            if let Some(item_type) = item_type {
                                execute_menu_action(state, &item_type);
                            }
                            state.is_dragging = false;
                            state.dragged_app_id = None;
                            state.menu_state.is_open = false;
                            layer_changed = true;
                            break;
                        } else {
                            state.menu_state.is_open = false;
                            layer_changed = true;
                        }
                    }

                    // Hover Preview Window
                    if state.hover_state.is_visible {
                        if let Some(ref app_id) = state.hover_state.app_id {
                            if let Some(windows) = running_by_app.get(app_id) {
                                // Correct implementation:
                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);
                                let win_count = state.open_windows.values().filter(|w| w.app_id == *app_id).count().max(1);

                                let menu_w = (180.0_f64 * state.scale_factor as f64).round() as i32;
                                let menu_h = (win_count as f64 * 30.0_f64 * state.scale_factor as f64).round() as i32;
                                let scale_i32 = state.scale_factor.round() as i32;

                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    state.width as i32, state.height as i32, total_apps, hovered_app_index, menu_w, menu_h, scale_i32,
                                );
                                let ptr_x = (state.interaction.pointer_position.x as f32 * scale_factor).round() as i32;
                                let ptr_y = (state.interaction.pointer_position.y as f32 * scale_factor).round() as i32;
                                let item_h = (30.0 * scale_factor).round() as i32;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width && ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
                                    let idx = (ptr_y - menu_y) / item_h;
                                    if let Some(handle_id) = windows.get(idx as usize) {
                                        let sq_x = (menu_x as f32 + menu_width as f32 - (25.0 * scale_factor)) as i32;
                                        let sq_y = (menu_y as f32 + idx as f32 * item_h as f32 + (5.0 * scale_factor)) as i32;
                                        let sq_size = (20.0 * scale_factor) as i32;

                                        if ptr_x >= sq_x && ptr_x <= sq_x + sq_size && ptr_y >= sq_y && ptr_y <= sq_y + sq_size {
                                            if let Some(win) = state.open_windows.get_mut(handle_id) {
                                                win.handle.close();
                                            }
                                        } else if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            if let Some(seat) = &state.wl_seat {
                                                win.handle.activate(seat);
                                            }
                                        }
                                    }
                                    state.hover_state.is_visible = false;
                                    layer_changed = true;
                                    break;
                                }
                            }
                        }
                    }

                    // Drag Release Handling
                    let mut was_dragging = false;
                    if state.is_dragging {
                        was_dragging = true;
                        if let Some(dragged_id) = state.dragged_app_id.clone() {
                            layer_changed |= handle_drag_release(state, &dragged_id, apps_in_dock, is_over_icons, (box_size, spacing, start_offset_x));
                        }
                    }

                    // Standard Dock Icon Click
                    if !was_dragging && is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
                            let end_x = start_x + box_size;
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = end_x + (spacing / 2);

                            if state.interaction.pointer_position.x >= hit_start_x.into() && state.interaction.pointer_position.x <= hit_end_x.into() {
                                if let Some(windows) = running_by_app.get(app_id) {
                                    if let Some(handle_id) = windows.first() {
                                        let was_active = state.open_windows.get(handle_id).map(|w| w.is_activated).unwrap_or(false);
                                        if let Some(win) = state.open_windows.get_mut(handle_id) {
                                            if was_active {
                                                win.handle.set_minimized();
                                            } else if let Some(seat) = &state.wl_seat {
                                                win.handle.activate(seat);
                                            }
                                        }
                                    }
                                } else {
                                    launch_app(app_id);
                                }
                                break;
                            }
                        }
                    }

                    state.is_dragging = false;
                    state.dragged_app_id = None;
                }
            }
            _ => {}
        }
    }

    layer_changed
}

fn execute_menu_action(state: &mut AppState, item_type: &crate::MenuItemType) {
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

fn normalize_app_id(app_id: &str) -> String {
    let mut id = app_id.to_string();
    if !id.starts_with("steam_icon_") {
        if let Some(idx) = id.rfind('_') {
            if id[idx + 1..].chars().all(|c| c.is_numeric()) {
                id = id[..idx].to_string();
            }
        }
    }
    if id.to_lowercase().contains("transmission") {
        id = "transmission-gtk".to_string();
    }
    id
}

fn launch_app(app_id: &str) {
    let launcher_path = crate::handlers::get_launcher_path();
    let normalized = normalize_app_id(app_id);
    let _ = std::process::Command::new("sh").arg(launcher_path).arg(normalized).spawn();
}
