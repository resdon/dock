//! Popup menu handlers.
//!
//! Handles context menu actions: Launch, Focus, Unpin, Quit, and tray item actions.

use std::collections::HashMap;
use std::time::Instant;
use wayland_client::backend::ObjectId;

use super::dock::{get_windows_for_app, launch_app, normalize_app_id};
use crate::cache;
use crate::types::{ContextMenuItem, MenuItemType};
use crate::AppState;

/// Opens the right-click context menu for an application in the dock.
///
/// Determines which icon was clicked, builds the menu items (Focus, Launch,
/// Unpin, Quit, etc.), and positions the menu at the click location.
///
/// # Returns
/// `true` if the menu was opened successfully.
pub fn open_context_menu(
    state: &mut AppState,
    ptr_log_x: i32,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
    let dock_scale = state
        .docks
        .first()
        .map(|d| d.scale_factor)
        .unwrap_or(scale_factor as f64);
    let dock_width = state
        .docks
        .first()
        .map(|d| d.width as i32)
        .unwrap_or(state.width);
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
    let total_icons_width =
        total_apps as i32 * icon_base_size + (total_apps as i32 - 1).max(0) * icon_spacing;
    let start_x_offset = (phys_width - total_icons_width) / 2;

    for (index, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_x_offset + index as i32 * (icon_base_size + icon_spacing);
        let hit_start_x = start_x - icon_spacing / 2;
        let hit_end_x = start_x + icon_base_size + icon_spacing / 2;

        if ptr_phys_x >= hit_start_x && ptr_phys_x < hit_end_x {
            state.close_window_list_now();
            state.hover_state.is_visible = false;
            state.hover_state.app_id = None;
            state.menu_state.target_app_id = Some(app_id.clone());
            state.menu_state.target_tray_item = None;
            state.menu_state.anchor_override = None;

            let windows = get_windows_for_app(app_id, running_by_app);
            state.menu_state.target_window = windows
                .iter()
                .find(|id| state.window_state(id).is_some_and(|w| w.focused))
                .cloned()
                .or_else(|| windows.first().cloned());

            let is_running = !windows.is_empty();
            let normalized_app = normalize_app_id(app_id);
            let is_pinned = state.pinned_apps.iter().any(|pinned| {
                pinned.eq_ignore_ascii_case(&normalized_app)
                    || (normalized_app.to_ascii_lowercase().contains("amnezia")
                        && pinned.eq_ignore_ascii_case("AmneziaVPN"))
                    || (normalized_app.to_ascii_lowercase().contains("qbittorrent")
                        && pinned.eq_ignore_ascii_case("org.qbittorrent.qBittorrent"))
            });
            let actions = crate::get_desktop_actions(app_id);

            let mut items = Vec::new();
            if is_running {
                items.push(ContextMenuItem {
                    label: "Focus".into(),
                    item_type: MenuItemType::Focus,
                });
                items.push(ContextMenuItem {
                    label: "New Instance".into(),
                    item_type: MenuItemType::LaunchNew,
                });
                items.push(ContextMenuItem {
                    label: "Minimize".into(),
                    item_type: MenuItemType::Minimize,
                });
            } else {
                items.push(ContextMenuItem {
                    label: "Launch".into(),
                    item_type: MenuItemType::LaunchNew,
                });
            }

            for action in actions {
                items.push(ContextMenuItem {
                    label: action.name.clone(),
                    item_type: MenuItemType::Action(action),
                });
            }

            let pin_label = if is_pinned {
                "Unpin from Dock"
            } else {
                "Pin to Dock"
            };
            items.push(ContextMenuItem {
                label: pin_label.into(),
                item_type: MenuItemType::TogglePin,
            });

            if is_running {
                items.push(ContextMenuItem {
                    label: "Quit Application".into(),
                    item_type: MenuItemType::CloseApp,
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

/// Opens the shared context-menu popup for a tray item.
///
/// Menu actions are sent back to the tray item's SNI service through
/// the existing async action queue.
pub fn open_tray_context_menu(
    state: &mut AppState,
    tray_index: usize,
    anchor_x: i32,
    anchor_y: i32,
) -> bool {
    let Some(item) = state.tray_items.get(tray_index).cloned() else {
        return false;
    };

    state.close_window_list_now();
    state.close_context_menu_now();

    let key = item.key.clone();
    let display_name = item.display_name().to_string();
    let mut items = Vec::with_capacity(3);
    items.push(ContextMenuItem {
        label: format!("Open {}", display_name),
        item_type: MenuItemType::TrayActivate(key.clone()),
    });
    items.push(ContextMenuItem {
        label: "Secondary action".into(),
        item_type: MenuItemType::TraySecondaryActivate(key.clone()),
    });
    items.push(ContextMenuItem {
        label: if item.menu_path.is_some() || item.item_is_menu {
            "Open application menu"
        } else {
            "Show tray context menu"
        }
        .into(),
        item_type: MenuItemType::TrayContextMenu(key),
    });

    state.menu_state.target_app_id = None;
    state.menu_state.target_window = None;
    state.menu_state.target_tray_item = Some(item.key);
    state.menu_state.anchor_override = Some((anchor_x, anchor_y));
    state.menu_state.items = items;
    state.menu_state.is_open = true;
    state.menu_state.just_opened = true;
    state.menu_state.opened_by_button = Some(273); // BTN_RIGHT
    state.menu_state.waiting_for_initial_release = true;
    state.menu_state.pointer_inside_popup = false;
    crate::interaction::timers::reset_all(state);
    for dock in &mut state.docks {
        dock.menu_fade.show();
    }
    state.needs_redraw = true;
    true
}

/// Opens the window list popup menu when hovering or clicking a multi-window app.
///
/// Shows the PipeWire capture frame at the top with window rows below it.
/// Resets `active_row` to 0 so the popup defaults to showing the first stream's
/// thumbnail when it opens.
pub fn open_window_list_menu(
    state: &mut AppState,
    _ptr_log_x: i32,
    _apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    _scale_factor: f32,
) -> bool {
    // Clone the target before mutating `state`. Holding a reference into
    // `state.window_list_state` across the mutable calls below causes an
    // E0502 borrow conflict.
    let Some(app_id) = state.window_list_state.target_app_id.clone() else {
        return false;
    };

    let Some(windows) = running_by_app.get(&app_id) else {
        return false;
    };

    if windows.is_empty() {
        return false;
    }

    state.close_context_menu_now();
    crate::interaction::timers::reset_all(state);
    // Reset active_row so the popup defaults to the first stream
    if let Some(ref pw) = state.pipewire_capture {
        pw.set_active_row(0);
    }
    state.window_list_state.is_open = true;
    state.window_list_state.just_opened = false;
    state.window_list_state.target_app_id = Some(app_id);
    state.needs_redraw = true;

    for dock in &mut state.docks {
        dock.window_list_fade.show();
    }

    true
}

/// Checks pointer position relative to menu bounds and handles dismiss logic.
///
/// Closes the menu if the click is outside the menu area.
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
        layer_changed |= state.request_context_menu_close();
    }

    (inside_menu, layer_changed)
}

/// Evaluates menu click coordinates on pointer release and executes selected item.
///
/// Determines which menu row was clicked and triggers the corresponding action.
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
        let clicked_item_idx =
            ((ptr_log_y - menu_log_y) / item_h_log.max(1)).clamp(0, (total_items - 1) as i32);

        let item_type = state
            .menu_state
            .items
            .get(clicked_item_idx as usize)
            .map(|item| item.item_type.clone());

        if let Some(item_type) = item_type {
            execute_menu_action(state, &item_type);
        }
        state.is_dragging = false;
        state.dragged_app_id = None;
        state.request_context_menu_close();
        state.needs_redraw = true;
        return true;
    }
    false
}

/// Dispatches selected menu item action to system or app state.
///
/// Handles Focus, Minimize, Launch, Unpin, Quit, and custom actions.
pub fn execute_menu_action(state: &mut AppState, item_type: &MenuItemType) {
    match item_type {
        MenuItemType::Focus => {
            if let Some(handle_id) = &state.menu_state.target_window {
                if let Some(seat) = &state.wl_seat {
                    let handle = state.window_handles.get(handle_id).cloned();
                    if let Some(handle) = handle {
                        handle.activate(seat);
                    }
                    state.focus_action_performed = true;
                    state.focus_action_time = Some(Instant::now());
                }
                // Store pending focus so the draw loop can verify it succeeded
                if let Some(app_id) = &state.menu_state.target_app_id {
                    state.pending_focus_app_id = Some(app_id.clone());
                }
            }
        }
        MenuItemType::FocusWindow(handle) => {
            if let Some(seat) = &state.wl_seat {
                handle.activate(seat);
                state.focus_action_performed = true;
                state.focus_action_time = Some(Instant::now());
            }
        }
        MenuItemType::LaunchNew => {
            if let Some(app_id) = &state.menu_state.target_app_id {
                launch_app(app_id);
                state.bounce_app_id = Some(app_id.clone());
            }
        }
        MenuItemType::Minimize => {
            if let Some(handle_id) = &state.menu_state.target_window {
                if let Some(handle) = state.window_handle(handle_id) {
                    handle.set_minimized();
                }
                // Trigger minimize animation immediately — set_minimized() is a local action
                if let Some(app_id) = &state.menu_state.target_app_id {
                    state.minimize_app_id = Some(app_id.clone());
                }
                state.needs_redraw = true;
            }
        }
        MenuItemType::Action(action) => {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg(&action.exec)
                .spawn();
        }
        MenuItemType::TogglePin => {
            if let Some(app_id) = &state.menu_state.target_app_id {
                let app_id = normalize_app_id(app_id);
                let mut pinned = cache::persistence::load_pinned_apps();
                if pinned.contains(&app_id) {
                    pinned.retain(|x| x != &app_id);
                } else {
                    pinned.push(app_id.clone());
                    if let Some((rgba, size)) = state.icon_cache.get(&app_id) {
                        // Persist the desktop/icon-theme asset (512px when
                        // available), never the often tiny foreign-toplevel
                        // thumbnail. If it is not loaded yet, the pinned app's
                        // normal async icon request will populate it shortly.
                        cache::save_cached_icon(&app_id, *size, *size, rgba);
                    }
                }
                cache::persistence::save_pinned_apps(&pinned);
                state.pinned_apps = pinned;
            }
        }
        MenuItemType::CloseApp => {
            if let Some(app_id) = state.menu_state.target_app_id.clone() {
                state.close_application_completely(&app_id);
            }
        }
        MenuItemType::TrayActivate(key) => {
            let _ = state.send_tray_action_for_key(
                key,
                crate::models::TrayActionKind::Activate,
                state.interaction.pointer_position.x.round() as i32,
                state.interaction.pointer_position.y.round() as i32,
            );
        }
        MenuItemType::TraySecondaryActivate(key) => {
            let _ = state.send_tray_action_for_key(
                key,
                crate::models::TrayActionKind::SecondaryActivate,
                state.interaction.pointer_position.x.round() as i32,
                state.interaction.pointer_position.y.round() as i32,
            );
        }
        MenuItemType::TrayContextMenu(key) => {
            let _ = state.send_tray_action_for_key(
                key,
                crate::models::TrayActionKind::ContextMenu,
                state.interaction.pointer_position.x.round() as i32,
                state.interaction.pointer_position.y.round() as i32,
            );
        }
    }
}
