use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use crate::app::AppState;
use crate::models::WindowDiagnostics;
use super::drag::handle_drag_release;

pub fn handle_dock_press(
    state: &mut AppState,
    button: u32,
    ptr_log_x: i32,
    ptr_log_y: i32,
    _apps_in_dock: &[String],
    layout: (i32, i32, i32, i32),
    _scale_factor: f32,
) -> bool {
    let (_, _, spacing, _) = layout;

    if button == 272 {
        // Iterate over docks to access their authoritative pins
        for dock in &state.docks {
            if let Some(ref dock_state) = dock.dock_state {
                for pin in &dock_state.pins {
                    let start_x = pin.x;
                    let box_size = pin.size as i32;
                    let hit_start_x = start_x.saturating_sub(spacing / 2);
                    let hit_end_x = start_x + box_size + (spacing / 2);

                    if ptr_log_x >= hit_start_x && ptr_log_x < hit_end_x {
                        state.dragged_app_id = Some(pin.app_id.clone());
                        state.drag_start_x = ptr_log_x;
                        state.drag_start_y = ptr_log_y;
                        state.is_dragging = false;
                        return false;
                    }
                }
            }
        }
    }
    false
}

pub fn handle_dock_release(
    state: &mut AppState,
    button: u32,
    ptr_log_x: i32,
    is_over_icons: bool,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    layout: (i32, i32, i32, i32),
    _scale_factor: f32,
) -> bool {
    let (_, box_size_layout, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    let mut was_dragging = false;
    if button == 272 && state.is_dragging {
        was_dragging = true;
        if let Some(dragged_id) = state.dragged_app_id.clone() {
            layer_changed |= handle_drag_release(
                state,
                &dragged_id,
                apps_in_dock,
                is_over_icons,
                (box_size_layout, spacing, start_offset_x),
            );
        }
    }

    if !was_dragging && is_over_icons {
        for dock in &state.docks {
            if let Some(ref dock_state) = dock.dock_state {
                for pin in &dock_state.pins {
                    let start_x = pin.x;
                    let box_size = pin.size as i32;
                    let hit_start_x = start_x.saturating_sub(spacing / 2);
                    let hit_end_x = start_x + box_size + (spacing / 2);

                    if ptr_log_x >= hit_start_x && ptr_log_x < hit_end_x {
                        if button == 274 {
                            launch_app(&pin.app_id);
                        } else if button == 272 {
                            let windows = get_windows_for_app(&pin.app_id, running_by_app, &state.open_windows);
                            if let Some(handle_id) = windows.first() {
                                let was_active = state.open_windows
                                    .get(handle_id)
                                    .map_or(false, |w| w.is_activated);
                                if let Some(win) = state.open_windows.get_mut(handle_id) {
                                    if was_active {
                                        win.handle.set_minimized();
                                    } else if let Some(seat) = &state.wl_seat {
                                        win.handle.activate(seat);
                                    }
                                }
                            } else {
                                launch_app(&pin.app_id);
                            }
                        }
                        layer_changed = true;
                        break;
                    }
                }
            }
        }
    }

    state.is_dragging = false;
    state.dragged_app_id = None;
    layer_changed
}

pub fn get_windows_for_app(
    app_id: &str,
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    open_windows: &HashMap<ObjectId, WindowDiagnostics>,
) -> Vec<ObjectId> {
    let target_norm = normalize_app_id(app_id);

    if let Some(windows) = running_by_app.get(app_id) {
        if !windows.is_empty() {
            return windows.clone();
        }
    }
    if let Some(windows) = running_by_app.get(&target_norm) {
        if !windows.is_empty() {
            return windows.clone();
        }
    }

    open_windows
        .iter()
        .filter(|(_, win)| normalize_app_id(&win.app_id) == target_norm)
        .map(|(id, _)| id.clone())
        .collect()
}

pub fn normalize_app_id(app_id: &str) -> String {
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

pub fn launch_app(app_id: &str) {
    let launcher_path = crate::handlers::get_launcher_path();
    let normalized = normalize_app_id(app_id);
    let _ = std::process::Command::new("sh").arg(launcher_path).arg(normalized).spawn();
}
