//! # Dock Icon Click Handler
//!
//! Handles click events on dock icons, determining whether to focus,
//! launch, or activate windows based on the app's running state.
//!
//! ## Click behavior
//! - **Left-click on running app**: Focus the window whose PipeWire stream
//!   is currently displayed in the popup (via `active_row`). If no PipeWire
//!   match is found, falls back to the focused window.
//! - **Left-click on unlaunched app**: Launch the app with bounce animation
//! - **Middle-click**: Launch a new instance with bounce animation
//! - **Right-click**: Open context menu (handled by `click.rs`)
//! - **Drag**: Reorder icons in the dock

use crate::interaction::drag::handle_drag_release;
use crate::AppState;
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

/// Handle pointer press on the dock area.
///
/// Starts a drag operation when the user presses on a dock icon.
/// Returns `true` if the event was consumed.
pub fn handle_dock_press(
    state: &mut AppState,
    button: u32,
    ptr_log_x: i32,
    ptr_log_y: i32,
    _apps_in_dock: &[String],
    _layout: (i32, i32, i32, i32),
    _scale_factor: f32,
) -> bool {
    if button == 272 {
        if let Some(app_id) = state.get_app_id_at_location(ptr_log_x as f64, ptr_log_y as f64) {
            state.dragged_app_id = Some(app_id.clone());
            state.drag_start_x = ptr_log_x;
            state.drag_start_y = ptr_log_y;
        }
    }
    false
}

#[allow(clippy::too_many_arguments)]
/// Handle pointer release on the dock area.
///
/// Determines whether the click was a drag, single-click, or double-click,
/// and performs the appropriate action (focus window, launch app, reorder icons).
///
/// # Arguments
/// * `button` - Button code: 272=left, 274=middle, 273=right
/// * `is_over_icons` - Whether the pointer is over the icon row (not empty space)
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
    let (box_size_layout, spacing, start_offset_x) = (layout.1, layout.2, layout.3);
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
        if let Some(app_id) =
            state.get_app_id_at_location(ptr_log_x as f64, state.interaction.pointer_position.y)
        {
            match button {
                274 => {
                    // Middle-click: launch a new instance
                    launch_app(&app_id);
                    state.bounce_app_id = Some(app_id.clone());
                }
                272 => {
                    // Always trigger click animation on left click
                    state.click_app_id = Some(app_id.clone());
                    let mut windows = get_windows_for_app(&app_id, running_by_app);
                    if windows.is_empty() {
                        launch_app(&app_id);
                        state.bounce_app_id = Some(app_id.clone());
                    } else {
                        // Sort by title to match the popup row order in window_list.rs.
                        let titles: HashMap<ObjectId, String> = windows
                            .iter()
                            .map(|id| {
                                (
                                    id.clone(),
                                    state
                                        .window_state(id)
                                        .map(|w| w.title.clone())
                                        .unwrap_or_default(),
                                )
                            })
                            .collect();
                        windows.sort_by(|a, b| {
                            titles
                                .get(a)
                                .unwrap_or(&String::new())
                                .cmp(titles.get(b).unwrap_or(&String::new()))
                        });

                        // Match each window to its PipeWire stream, then focus the window
                        // whose stream is currently displayed (active_row). This ensures
                        // clicking the dock icon focuses the same window whose capture is shown.
                        let target_window = if let Some(service) = &state.pipewire_capture {
                            let active_row = *service.active_row.lock();
                            // Build a list of (window_id, stream_idx) pairs, matching
                            // windows to streams the same way window_list.rs does.
                            let matched: Vec<(&ObjectId, usize)> = windows
                                .iter()
                                .filter_map(|wid| {
                                    let window_id = wid.protocol_id();
                                    let stream_idx = if let Some(ws) = state.window_state(wid) {
                                        if ws.title.is_empty() {
                                            None
                                        } else {
                                            // Primary: check persistent binding registry
                                            service.get_bound_stream(window_id).or_else(|| {
                                                // Secondary: fall back to title match
                                                service.find_stream_by_title(&ws.title, &[])
                                            })
                                        }
                                    } else {
                                        None
                                    };
                                    stream_idx.map(|idx| (wid, idx))
                                })
                                .collect();
                            // Find the window whose stream matches the active row
                            matched
                                .iter()
                                .find(|(_, idx)| *idx == active_row)
                                .map(|(wid, _)| *wid)
                                // Fallback: if no PipeWire match, use the focused window
                                .or_else(|| {
                                    windows.iter().find(|id| {
                                        state.window_state(id).is_some_and(|w| w.focused)
                                    })
                                })
                                .or(windows.first())
                        } else {
                            // No PipeWire service; fall back to focused window
                            windows
                                .iter()
                                .find(|id| state.window_state(id).is_some_and(|w| w.focused))
                                .or(windows.first())
                        };

                        if let Some(handle_id) = target_window {
                            let was_active =
                                state.window_state(handle_id).is_some_and(|w| w.focused);
                            if was_active {
                                if let Some(win) = state.window_handle(handle_id) {
                                    win.set_minimized();
                                }
                                // Trigger minimize animation immediately — set_minimized() is a local action
                                state.minimize_app_id = Some(app_id.clone());
                                state.needs_redraw = true;
                            } else if let Some(seat) = &state.wl_seat {
                                let handle = state.window_handles.get(handle_id).cloned();
                                if let Some(win) = handle {
                                    win.activate(seat);
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            layer_changed = true;
        }
    }

    state.is_dragging = false;
    state.dragged_app_id = None;

    // After a drag, if the popup is open, reset active_row so the stream
    // matches the first window of the current target app.
    if was_dragging && state.window_list_state.is_open {
        if let Some(ref pw) = state.pipewire_capture {
            pw.set_active_row(0);
        }
    }

    layer_changed
}

/// Look up all running window IDs for a given app.
pub fn get_windows_for_app(
    app_id: &str,
    running_by_app: &HashMap<String, Vec<ObjectId>>,
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

    Vec::new()
}

/// Normalize an app ID for matching against Wayland window app_ids.
///
/// Handles Steam app IDs (strips `steam_icon_` prefix) and other edge cases.
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

/// Launch an application by its app ID.
///
/// Delegates to `launch_app_with_files` with no file arguments.
pub fn launch_app(app_id: &str) {
    launch_app_with_files(app_id, &[]);
}

/// Launch an application through the dock-owned script. Clear scale overrides
/// at the child-process boundary as a second layer of protection; launcher.sh
/// repeats this before invoking gtk-launch or the application binary.
pub fn launch_app_with_files(app_id: &str, files: &[std::path::PathBuf]) {
    const SCALE_ENV: &[&str] = &[
        "GDK_SCALE",
        "GDK_DPI_SCALE",
        "GTK_SCALE",
        "GTK_DPI_SCALE",
        "QT_SCALE_FACTOR",
        "QT_SCREEN_SCALE_FACTORS",
        "QT_AUTO_SCREEN_SCALE_FACTOR",
        "QT_ENABLE_HIGHDPI_SCALING",
        "QT_FONT_DPI",
        "QT_DEVICE_PIXEL_RATIO",
        "ELM_SCALE",
        "WINIT_X11_SCALE_FACTOR",
        "FLTK_SCALING",
    ];

    let Some(launcher_path) = crate::handlers::get_launcher_path() else {
        tracing::error!("Launcher script not found; cannot launch app '{}'", app_id);
        return;
    };
    let normalized = normalize_app_id(app_id);
    let mut command = std::process::Command::new("bash");
    command.arg(&launcher_path).arg(&normalized);
    for file in files {
        command.arg(file);
    }
    let preserve_scale = std::env::var("DOCK_PRESERVE_APP_SCALE")
        .map(|value| value == "1")
        .unwrap_or(false);
    if !preserve_scale {
        for &key in SCALE_ENV {
            command.env_remove(key);
        }
        // Some toolkits treat absent variables as permission to inherit a
        // desktop-wide scale setting. Start children at 1x; the compositor
        // still applies output scaling to each app surface.
        command.env("GDK_SCALE", "1").env("GDK_DPI_SCALE", "1");
        command
            .env("QT_SCALE_FACTOR", "1")
            .env("QT_AUTO_SCREEN_SCALE_FACTOR", "0");
    }

    if let Err(_error) = command.spawn() {
        // Failed to start launcher
    }
}
