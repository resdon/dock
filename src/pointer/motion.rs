//! Pointer motion tracking.
//!
//! Detects which UI element the pointer is over and updates hover state.

use crate::pointer::coordinates::map_coordinates;
use crate::AppState;
use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

/// Handles pointer motion, entry, and leave events.
pub fn handle_motion_events(
    state: &mut AppState,
    events: &[PointerEvent],
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> bool {
    let mut layer_changed = false;

    for event in events {
        let mut is_context_menu_surface = false;
        let mut is_window_list_surface = false;
        let mut is_hover_popup_surface = false;
        let mut is_audio_popup_surface = false;
        let mut is_bluetooth_popup_surface = false;
        let mut is_rclone_popup_surface = false;
        let mut is_process_popup_surface = false;
        let mut is_widget_bar_surface = false;
        let mut is_audio_player_surface = false;
        let mut is_tray_bar_surface = false;
        let mut is_bottom_status_surface = false;

        // Single pass over docks to classify the event surface (#379).
        for dock in &state.docks {
            if dock
                .context_menu_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_context_menu_surface = true;
            }
            if dock
                .window_list_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_window_list_surface = true;
            }
            if dock
                .hover_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_hover_popup_surface = true;
            }
            if dock
                .audio_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_audio_popup_surface = true;
            }
            if dock
                .bluetooth_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_bluetooth_popup_surface = true;
            }
            if dock
                .rclone_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_rclone_popup_surface = true;
            }
            if dock
                .process_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
            {
                is_process_popup_surface = true;
            }
            if dock
                .status_widget_bar
                .as_ref()
                .is_some_and(|bar| bar.surface == event.surface)
            {
                is_widget_bar_surface = true;
            }
            if dock
                .audio_player_bar
                .as_ref()
                .is_some_and(|bar| bar.surface == event.surface)
            {
                is_audio_player_surface = true;
            }
            if dock
                .tray_bar
                .as_ref()
                .is_some_and(|bar| bar.surface == event.surface)
            {
                is_tray_bar_surface = true;
            }
            if dock
                .system_status_bar
                .as_ref()
                .is_some_and(|bar| bar.surface == event.surface)
            {
                is_bottom_status_surface = true;
            }
        }

        let is_popup_surface = is_context_menu_surface
            || is_window_list_surface
            || is_hover_popup_surface
            || is_audio_popup_surface
            || is_bluetooth_popup_surface
            || is_rclone_popup_surface
            || is_process_popup_surface
            || is_widget_bar_surface
            || is_audio_player_surface
            || is_tray_bar_surface
            || is_bottom_status_surface;
        let is_special_surface = is_popup_surface;

        match event.kind {
            smithay_client_toolkit::seat::pointer::PointerEventKind::Leave { .. } => {
                if is_special_surface {
                    if is_audio_player_surface {
                        state.set_audio_player_expanded(&event.surface, false);
                    }
                    if is_audio_popup_surface {
                        let _ = state.audio_slider_drag.take();
                    }
                    if is_context_menu_surface {
                        state.menu_state.pointer_inside_popup = false;
                    }
                    if is_window_list_surface {
                        state.window_list_state.pointer_inside_popup = false;
                    }
                    // Clear the domain flag until an Enter/Motion arrives on
                    // another dock/popup surface. This prevents stale popup
                    // coordinates from keeping the dock alive after a Leave.
                    state.interaction.pointer_inside = false;
                    layer_changed = true;
                    continue;
                }

                if state.interaction.pointer_inside {
                    state.interaction.pointer_inside = false;
                    // Clear hover when pointer leaves the dock
                    if state.interaction.hovered_icon.is_some() {
                        state.interaction.hovered_icon = None;
                    }
                    layer_changed = true;
                }
            }
            smithay_client_toolkit::seat::pointer::PointerEventKind::Enter { .. }
            | smithay_client_toolkit::seat::pointer::PointerEventKind::Motion { .. } => {
                layer_changed |= is_audio_player_surface
                    && state.set_audio_player_expanded(&event.surface, true);
                if is_audio_popup_surface {
                    if let Some(target) = state.audio_slider_drag {
                        if let Some(action) = crate::app::audio_popup::action_for_audio_slider(
                            target,
                            &state.audio_snapshot,
                            event.position.0,
                        ) {
                            let _ = state.send_audio_action(action);
                        }
                    }
                }
                if is_context_menu_surface {
                    state.menu_state.pointer_inside_popup = true;
                }
                if is_window_list_surface {
                    state.window_list_state.pointer_inside_popup = true;
                }

                state.menu_state.cursor_moved = true;
                state.window_list_state.cursor_moved = true;

                if !state.interaction.pointer_inside {
                    state.interaction.pointer_inside = true;
                    layer_changed = true;
                }

                let (mapped_x, mapped_y) =
                    map_coordinates(event, state, dock_surface_ptr, scale_factor);
                let (new_x, new_y) = (mapped_x as f64, mapped_y as f64);

                // **FIX (v1.5.5)**: Always update pointer position for tracking purposes,
                // but do NOT set layer_changed. Position changes alone don't require a
                // redraw — only hover state changes (icon enter/leave) do. The compositor
                // sends motion events continuously (sub-pixel jitter), and setting
                // layer_changed for each one caused the render loop to spin at ~1000 FPS.
                // The position is still tracked for the content hash in window_list.rs
                // and for proximity detection.
                state.interaction.pointer_position.x = new_x;
                state.interaction.pointer_position.y = new_y;

                if state.hide_state.is_fully_hidden() {
                    state.hover_state.is_visible = false;
                    state.hover_state.app_id = None;
                    layer_changed = true;
                }

                if state.dragged_app_id.is_some() && !state.is_dragging {
                    let dx = mapped_x - state.drag_start_x as f32;
                    let dy = mapped_y - state.drag_start_y as f32;
                    let dist_sq = dx * dx + dy * dy;
                    if dist_sq > 25.0 {
                        state.is_dragging = true;
                        layer_changed = true;
                    }
                }

                // Popup surfaces own their pointer events. In particular, a
                // context-menu event must never be allowed to re-target the
                // window list through dock hover logic.
                // Allow hover when the context menu is fading out (not just when fully closed).
                let menu_visible = state.menu_state.is_open
                    && state
                        .docks
                        .iter()
                        .any(|dock| !dock.menu_fade.is_fully_hidden());
                if !is_special_surface && !menu_visible {
                    // Use the same dock-local hitbox as click, drag, scroll, and
                    // DnD handling. Keeping one geometry authority prevents hover
                    // from disagreeing with the actual clickable icon.
                    let hovered_app = state.get_app_id_at_location(new_x as f64, new_y as f64);

                    // Update hovered_icon index for rendering (Slint hover outlines)
                    let apps_in_dock = state.get_apps_in_dock();
                    let new_hovered_index = hovered_app
                        .as_ref()
                        .and_then(|app_id| apps_in_dock.iter().position(|id| id == app_id));
                    if state.interaction.hovered_icon != new_hovered_index {
                        state.interaction.hovered_icon = new_hovered_index;
                        layer_changed = true;
                    }

                    if let Some(app_id) = hovered_app {
                        // Only open the window list popup when the app has open windows.
                        let has_windows = state.window_count_for_app(&app_id) > 0;

                        if has_windows {
                            let target_changed =
                                state.window_list_state.target_app_id.as_deref() != Some(&app_id);
                            let was_open = state.window_list_state.is_open;
                            if target_changed || !was_open {
                                // Only reset global interaction timers when opening a new popup session.
                                // Avoid aggressive resets when switching targets within an active window list.
                                if !was_open {
                                    crate::interaction::timers::reset_all(state);
                                    // FIX (v1.5.5): Signal PipeWire streams that the popup is now open.
                                    if let Some(ref pw) = state.pipewire_capture {
                                        pw.set_window_list_open(true);
                                    }
                                }
                                if let Some(ref pw) = state.pipewire_capture {
                                    pw.set_active_row(0);
                                }
                                state.window_list_state.target_app_id = Some(app_id);
                                state.window_list_state.is_open = true;
                                for dock in &mut state.docks {
                                    dock.window_list_fade.set_visible(true);
                                    dock.window_list_fade.show();
                                }
                                layer_changed = true;
                            }
                        }
                    }
                } else if is_special_surface {
                    // Keep hover highlight when context menu is open for the hovered icon.
                    let keep_hover_for_menu = is_context_menu_surface
                        && state.menu_state.is_open
                        && state.interaction.hovered_icon.is_some();
                    if !keep_hover_for_menu && state.interaction.hovered_icon.is_some() {
                        state.interaction.hovered_icon = None;
                        layer_changed = true;
                    }
                }
            }
            _ => {}
        }
    }

    layer_changed
}
