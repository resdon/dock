//! Click detection and processing.
//!
//! Handles clicks on dock icons, popup rows, and context menu items.
//! Maps pointer coordinates to UI elements and triggers actions.

use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind};
use std::collections::HashMap;
use wayland_client::backend::ObjectId;

use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::handlers::dock::{get_windows_for_app, handle_dock_press, handle_dock_release};
use crate::handlers::popup::{execute_menu_action, open_context_menu, open_tray_context_menu};
use crate::models::system::WindowId;
use crate::pointer::coordinates::calculate_popup_anchor;
use smithay_client_toolkit::shell::WaylandSurface;

use crate::AppState;

const BTN_LEFT: u32 = 272;
const BTN_RIGHT: u32 = 273;
const BTN_MIDDLE: u32 = 274;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StatusPanel {
    Audio,
    Bluetooth,
    Rclone,
    Process,
}

fn close_other_status_panels(state: &mut AppState, keep_open: Option<StatusPanel>) -> bool {
    let mut changed = false;

    for (panel_flag, panel) in [
        (&mut state.audio_panel_open, StatusPanel::Audio),
        (&mut state.bluetooth_panel_open, StatusPanel::Bluetooth),
        (&mut state.rclone_panel_open, StatusPanel::Rclone),
        (&mut state.process_panel_open, StatusPanel::Process),
    ] {
        if keep_open == Some(panel) {
            continue;
        }
        if *panel_flag {
            *panel_flag = false;
            changed = true;
        }
    }

    if changed {
        state.needs_redraw = true;
    }

    changed
}

fn close_status_panels(state: &mut AppState) -> bool {
    let any_open = state.audio_panel_open
        || state.bluetooth_panel_open
        || state.rclone_panel_open
        || state.process_panel_open;

    if !any_open {
        return false;
    }

    state.audio_panel_open = false;
    state.bluetooth_panel_open = false;
    state.rclone_panel_open = false;
    state.process_panel_open = false;
    state.audio_slider_drag = None;
    state.set_rclone_monitor_enabled(false);
    state.set_process_monitor_enabled(false);
    state.needs_redraw = true;
    true
}

/// Processes all pointer click events (press and release).
///
/// Handles clicks on dock icons, popup rows, context menu items,
/// and widget bar buttons. Maps pointer coordinates to UI elements
/// and triggers the appropriate actions (focus, launch, close, etc.).
///
/// # Returns
/// `true` if any state changed and a redraw is needed.
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

    // Use the same canonical icon hit-test as hover, drag, scroll, and DnD.
    // A Y-only check previously allowed context menus to open on empty dock
    // space and made click targeting disagree with the drawn icon positions.
    let is_over_icons = state
        .get_app_id_at_location(
            state.interaction.pointer_position.x,
            state.interaction.pointer_position.y,
        )
        .is_some();

    let (dock_width, dock_height_val, dock_scale) = state
        .docks
        .first()
        .map(|d| (d.width, d.height, d.scale_factor))
        .unwrap_or((state.width as u32, dock_height as u32, scale_factor as f64));

    let phys_width = (dock_width as f64 * dock_scale).round() as i32;
    let phys_dock_height = (dock_height_val as f64 * dock_scale).round() as i32;
    let item_h = (state.config.popup_row_height as f64 * dock_scale).round() as i32;

    for event in events {
        match event.kind {
            PointerEventKind::Leave { .. } => {}
            PointerEventKind::Press { button, .. } => {
                let event_on_dock_surface = state
                    .docks
                    .iter()
                    .any(|dock| dock.surface.wl_surface() == &event.surface);
                let event_on_widget_bar_surface =
                    state.is_status_widget_bar_surface(&event.surface);
                let event_on_audio_player_surface = state.is_audio_player_surface(&event.surface);
                let event_on_audio_popup = state.docks.iter().any(|dock| {
                    dock.audio_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                let event_on_bluetooth_popup = state.docks.iter().any(|dock| {
                    dock.bluetooth_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                let event_on_rclone_popup = state.docks.iter().any(|dock| {
                    dock.rclone_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                let event_on_process_popup = state.docks.iter().any(|dock| {
                    dock.process_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                if event_on_audio_popup {
                    if button == BTN_LEFT {
                        state.audio_slider_drag = crate::app::audio_popup::slider_target_at(
                            &state.audio_snapshot,
                            event.position.0,
                            event.position.1,
                        );
                    }
                    continue;
                }
                if event_on_bluetooth_popup || event_on_rclone_popup || event_on_process_popup {
                    continue;
                }

                if (event_on_dock_surface
                    || event_on_widget_bar_surface
                    || event_on_audio_player_surface)
                    && button == BTN_LEFT
                    && state.audio_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    if let Some(control) = state.media_control_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    ) {
                        let _ = state
                            .send_audio_action(crate::models::AudioAction::MediaControl(control));
                        continue;
                    }
                    let should_open_audio_panel = !state.audio_panel_open;
                    if should_open_audio_panel {
                        close_other_status_panels(state, Some(StatusPanel::Audio));
                        state.audio_panel_open = true;
                        state.set_rclone_monitor_enabled(false);
                        state.set_process_monitor_enabled(false);
                        state.close_context_menu_now();
                        state.close_window_list_now();
                    } else {
                        state.audio_panel_open = false;
                        state.audio_slider_drag = None;
                    }
                    state.needs_redraw = true;
                    layer_changed = true;
                    continue;
                }

                let event_on_dashed_block_surface = state.docks.iter().any(|dock| {
                    dock.dashed_block_surface
                        .as_ref()
                        .is_some_and(|block| block.surface.wl_surface() == &event.surface)
                });

                // Click on the Bluetooth icon toggles the Bluetooth popup panel.
                // Works from the widget bar and the dashed block.
                let bt_on_widget_bar = state.bluetooth_button_at_surface(
                    &event.surface,
                    event.position.0,
                    event.position.1,
                );
                let bt_on_dashed_block = state.bluetooth_button_at_dashed_block(
                    &event.surface,
                    event.position.0,
                    event.position.1,
                );
                if (event_on_dock_surface
                    || event_on_widget_bar_surface
                    || event_on_dashed_block_surface)
                    && button == BTN_LEFT
                    && (bt_on_widget_bar || bt_on_dashed_block)
                {
                    let should_open_bluetooth_panel = !state.bluetooth_panel_open;
                    if should_open_bluetooth_panel {
                        close_other_status_panels(state, Some(StatusPanel::Bluetooth));
                        state.bluetooth_panel_open = true;
                        state.audio_slider_drag = None;
                        state.set_process_monitor_enabled(false);
                        state.set_rclone_monitor_enabled(false);
                        state.close_context_menu_now();
                        state.close_window_list_now();
                    } else {
                        state.bluetooth_panel_open = false;
                    }
                    state.needs_redraw = true;
                    layer_changed = true;
                    continue;
                }

                if (event_on_dock_surface || event_on_widget_bar_surface)
                    && button == BTN_LEFT
                    && state.rclone_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    let should_open_rclone_panel = !state.rclone_panel_open;
                    if should_open_rclone_panel {
                        close_other_status_panels(state, Some(StatusPanel::Rclone));
                        state.rclone_panel_open = true;
                        state.audio_slider_drag = None;
                        state.set_process_monitor_enabled(false);
                        state.set_rclone_monitor_enabled(true);
                        state.rclone_page = 0;
                        state.close_context_menu_now();
                        state.close_window_list_now();
                    } else {
                        state.rclone_panel_open = false;
                        state.set_rclone_monitor_enabled(false);
                    }
                    state.needs_redraw = true;
                    layer_changed = true;
                    continue;
                }

                if (event_on_dock_surface || event_on_widget_bar_surface)
                    && button == BTN_LEFT
                    && state.process_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    let should_open_process_panel = !state.process_panel_open;
                    if should_open_process_panel {
                        close_other_status_panels(state, Some(StatusPanel::Process));
                        state.process_panel_open = true;
                        state.audio_slider_drag = None;
                        state.set_rclone_monitor_enabled(false);
                        state.set_process_monitor_enabled(true);
                        state.process_page = 0;
                        state.close_context_menu_now();
                        state.close_window_list_now();
                    } else {
                        state.process_panel_open = false;
                        state.set_process_monitor_enabled(false);
                    }
                    state.needs_redraw = true;
                    layer_changed = true;
                    continue;
                }

                // While any status popup is open, clicks outside it only dismiss
                // the popup; they must not accidentally activate a dock icon.
                if state.audio_panel_open
                    || state.bluetooth_panel_open
                    || state.rclone_panel_open
                    || state.process_panel_open
                {
                    continue;
                }

                if let Some(tray_index) =
                    state.tray_item_at_surface(&event.surface, event.position.0, event.position.1)
                {
                    // Right-click opens the dock's shared tray context menu;
                    // use parent-local coordinates for popup placement.
                    if button == BTN_RIGHT {
                        let (anchor_x, anchor_y) = crate::pointer::coordinates::map_coordinates(
                            event,
                            state,
                            None,
                            scale_factor,
                        );
                        layer_changed |= open_tray_context_menu(
                            state,
                            tray_index,
                            anchor_x.round() as i32,
                            anchor_y.round() as i32,
                        );
                    }
                    continue;
                }
                // A left press on the context-menu surface belongs to the
                // menu and must remain open until Release selects the item.
                // Only clicks outside the popup dismiss it.
                // Only dismiss if the context menu popup actually exists (rendered).
                if state.menu_state.is_open && button == BTN_LEFT {
                    let context_popup_exists = state
                        .docks
                        .iter()
                        .any(|dock| dock.context_menu_popup.as_ref().is_some());
                    let on_context_popup = state.docks.iter().any(|dock| {
                        dock.context_menu_popup
                            .as_ref()
                            .is_some_and(|popup| popup.surface == event.surface)
                    });

                    // If the popup doesn't exist (rendering disabled), close the menu state
                    // and let the click fall through to normal dock handling
                    if !context_popup_exists {
                        layer_changed |= state.close_context_menu_now();
                    } else if !on_context_popup {
                        layer_changed |= state.request_context_menu_close();
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
                            // Use the actual popup's dimensions for bounds checking.
                            if let Some((menu_x, menu_y, menu_width, menu_height)) =
                                state.docks.iter().find_map(|dock| {
                                    dock.window_list_popup.as_ref().map(|popup| {
                                        let px =
                                            (popup.position.0 as f64 * dock_scale).round() as i32;
                                        let py =
                                            (popup.position.1 as f64 * dock_scale).round() as i32;
                                        let pw = (popup.width as f64 * dock_scale).round() as i32;
                                        let ph = (popup.height as f64 * dock_scale).round() as i32;
                                        (px, py, pw, ph)
                                    })
                                })
                            {
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

                    if clicked_on_popup {
                        layer_changed = true;
                        continue;
                    } else {
                        layer_changed |= state.request_window_list_close();
                    }
                }

                // Normal dock actions only belong to the dock surface itself.
                // Popup surfaces must never fall through into dock launch/menu
                // handling, even when their bounds overlap an icon.
                if !event_on_dock_surface {
                    continue;
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
                        // Context menu and window list are mutually exclusive.
                        layer_changed |= state.close_window_list_now();
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
                        // Middle-click no longer triggers window list — hover is the primary trigger
                        // (see src/pointer/motion.rs for hover-based window list opening)
                    }
                    _ => {}
                }
            }
            PointerEventKind::Release { button, .. } => {
                let on_rclone_popup = state.docks.iter().any(|dock| {
                    dock.rclone_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                if on_rclone_popup {
                    if button == BTN_LEFT {
                        if crate::app::rclone_popup::minimize_button_at(
                            event.position.0,
                            event.position.1,
                        ) {
                            state.rclone_panel_open = false;
                            state.set_rclone_monitor_enabled(false);
                            state.needs_redraw = true;
                            layer_changed = true;
                            continue;
                        }
                        if let Some(delta) = crate::app::rclone_popup::page_delta_at(
                            event.position.0,
                            event.position.1,
                        ) {
                            let pages = state.rclone_snapshot.mounts.len().div_ceil(8);
                            let max_page = pages.saturating_sub(1);
                            state.rclone_page = if delta < 0 {
                                state.rclone_page.saturating_sub(1)
                            } else {
                                (state.rclone_page + 1).min(max_page)
                            };
                            state.needs_redraw = true;
                            layer_changed = true;
                        }
                    }
                    continue;
                }

                let on_process_popup = state.docks.iter().any(|dock| {
                    dock.process_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                if on_process_popup {
                    if button == BTN_LEFT {
                        if crate::app::process_popup::minimize_button_at(
                            event.position.0,
                            event.position.1,
                        ) {
                            state.process_panel_open = false;
                            state.set_process_monitor_enabled(false);
                            state.needs_redraw = true;
                            layer_changed = true;
                            continue;
                        }
                        if let Some(sort) =
                            crate::app::process_popup::sort_at(event.position.0, event.position.1)
                        {
                            state.process_sort = sort;
                            state.process_page = 0;
                            state.needs_redraw = true;
                            layer_changed = true;
                        } else if let Some(delta) = crate::app::process_popup::page_delta_at(
                            event.position.0,
                            event.position.1,
                        ) {
                            let pages = state.process_snapshot.processes.len().div_ceil(8);
                            let max_page = pages.saturating_sub(1);
                            state.process_page = if delta < 0 {
                                state.process_page.saturating_sub(1)
                            } else {
                                (state.process_page + 1).min(max_page)
                            };
                            state.needs_redraw = true;
                            layer_changed = true;
                        }
                    }
                    continue;
                }

                let on_bluetooth_popup = state.docks.iter().any(|dock| {
                    dock.bluetooth_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                if on_bluetooth_popup {
                    if button == BTN_LEFT {
                        let min_hit = crate::app::bluetooth_popup::minimize_button_at(
                            event.position.0,
                            event.position.1,
                        );
                        if min_hit {
                            state.bluetooth_panel_open = false;
                            state.needs_redraw = true;
                            layer_changed = true;
                            continue;
                        }
                        if let Some(action) = crate::app::bluetooth_popup::action_for_click(
                            &state.bluetooth_snapshot,
                            event.position.0,
                            event.position.1,
                        ) {
                            let _ = state.send_bluetooth_action(action);
                            // **FIX (v1.5.5)**: Trigger immediate redraw so the popup
                            // re-renders with updated state. The Bluetooth monitor sends
                            // a new snapshot asynchronously, but we need to redraw now to
                            // show the action status message and any immediate state changes.
                            // Without this, clicking buttons had no visual feedback.
                            state.needs_redraw = true;
                            layer_changed = true;
                        }
                    }
                    continue;
                }

                let on_audio_popup = state.docks.iter().any(|dock| {
                    dock.audio_popup
                        .as_ref()
                        .is_some_and(|popup| popup.surface == event.surface)
                });
                if on_audio_popup {
                    if button == BTN_LEFT {
                        if crate::app::audio_popup::minimize_button_at(
                            event.position.0,
                            event.position.1,
                        ) {
                            // Hide the mixer popup without toggling the dock or
                            // changing the current PipeWire device/volume state.
                            state.audio_panel_open = false;
                            state.audio_slider_drag = None;
                            state.needs_redraw = true;
                            layer_changed = true;
                            continue;
                        }
                        if let Some(action) = crate::app::audio_popup::action_for_audio_popup_click(
                            &state.audio_snapshot,
                            event.position.0,
                            event.position.1,
                        ) {
                            let _ = state.send_audio_action(action);
                        }
                    }
                    state.audio_slider_drag = None;
                    continue;
                }
                if state.audio_slider_drag.is_some() {
                    state.audio_slider_drag = None;
                }

                let event_on_dock_for_audio = state.docks.iter().any(|dock| {
                    dock.surface.wl_surface() == &event.surface
                        || dock
                            .status_widget_bar
                            .as_ref()
                            .is_some_and(|bar| bar.surface == event.surface)
                        || dock
                            .audio_player_bar
                            .as_ref()
                            .is_some_and(|bar| bar.surface == event.surface)
                        || dock
                            .dashed_block_surface
                            .as_ref()
                            .is_some_and(|block| block.surface.wl_surface() == &event.surface)
                });
                if event_on_dock_for_audio
                    && state.audio_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    continue;
                }
                // Check both the widget bar and the dashed block for the Bluetooth button.
                // Without the dashed block check, a release on the BT icon would reach
                // close_status_panels and immediately undo the Press handler's toggle.
                if event_on_dock_for_audio
                    && (state.bluetooth_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    ) || state.bluetooth_button_at_dashed_block(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    ))
                {
                    continue;
                }
                if event_on_dock_for_audio
                    && state.rclone_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    continue;
                }
                if event_on_dock_for_audio
                    && state.process_button_at_surface(
                        &event.surface,
                        event.position.0,
                        event.position.1,
                    )
                {
                    continue;
                }
                if close_status_panels(state) {
                    layer_changed = true;
                    continue;
                }

                if state.menu_state.target_tray_item.is_some()
                    && state.menu_state.waiting_for_initial_release
                    && state.menu_state.opened_by_button == Some(button)
                {
                    state.menu_state.waiting_for_initial_release = false;
                    layer_changed = true;
                    continue;
                }

                let tray_hit =
                    state.tray_item_at_surface(&event.surface, event.position.0, event.position.1);
                if let Some(index) = tray_hit {
                    // A tray release cannot complete an application drag operation.
                    state.is_dragging = false;
                    state.dragged_app_id = None;

                    let action = match button {
                        BTN_LEFT => state.tray_items.get(index).map(|item| {
                            if item.item_is_menu {
                                crate::models::TrayActionKind::ContextMenu
                            } else {
                                crate::models::TrayActionKind::Activate
                            }
                        }),
                        BTN_MIDDLE => Some(crate::models::TrayActionKind::SecondaryActivate),
                        BTN_RIGHT => Some(crate::models::TrayActionKind::ContextMenu),
                        _ => None,
                    };
                    if let Some(action) = action {
                        if state.menu_state.is_open {
                            layer_changed |= state.request_context_menu_close();
                        }
                        if state.window_list_state.is_open {
                            layer_changed |= state.request_window_list_close();
                        }
                        let (parent_x, parent_y) = crate::pointer::coordinates::map_coordinates(
                            event,
                            state,
                            None,
                            scale_factor,
                        );
                        let _ = state.send_tray_action(
                            index,
                            action,
                            parent_x.round() as i32,
                            parent_y.round() as i32,
                        );
                    }
                    continue;
                }

                let event_on_dock_surface = state
                    .docks
                    .iter()
                    .any(|dock| dock.surface.wl_surface() == &event.surface);
                if state.menu_state.waiting_for_initial_release
                    && state.menu_state.opened_by_button == Some(button)
                {
                    state.menu_state.waiting_for_initial_release = false;
                    layer_changed = true;
                    continue;
                }

                // 1. Handle Context Menu Click on Release
                if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                    let (anchor_x, anchor_y) =
                        if let Some((x, y)) = state.menu_state.anchor_override {
                            (
                                (x as f64 * dock_scale).round() as i32,
                                (y as f64 * dock_scale).round() as i32,
                            )
                        } else {
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
                            (anchor_x, anchor_y)
                        };
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
                        let clicked_idx = ((ptr_phys_y - menu_y) / item_h.max(1))
                            .clamp(0, (item_count - 1).max(0))
                            as usize;
                        let item_type_opt = state
                            .menu_state
                            .items
                            .get(clicked_idx)
                            .map(|item| item.item_type.clone());

                        if let Some(item_type) = item_type_opt {
                            execute_menu_action(state, &item_type);
                        }

                        state.request_context_menu_close();
                        state.is_dragging = false;
                        state.dragged_app_id = None;
                        layer_changed = true;
                        continue;
                    }
                }

                // 2. Handle Window List Click on Release
                if state.window_list_state.is_open {
                    if let Some(ref app_id) = state.window_list_state.target_app_id {
                        let mut windows = get_windows_for_app(app_id, running_by_app);
                        // Sort by title to match the popup row order in window_list.rs.
                        // The popup sorts windows by title, so the click handler must use
                        // the same order for clicked_idx to index the correct window.
                        let titles: std::collections::HashMap<ObjectId, String> = windows
                            .iter()
                            .map(|id| {
                                (
                                    id.clone(),
                                    state
                                        .windows
                                        .get(&WindowId(id.clone()))
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
                        if !windows.is_empty() {
                            let item_count = windows.len() as i32;

                            // Use the actual popup's position and dimensions instead of
                            // recomputing them — the popup calculates frame height dynamically
                            // based on stream aspect ratio, so a fixed constant would be wrong.
                            let (menu_x, menu_y, menu_width, menu_height) = state
                                .docks
                                .iter()
                                .find_map(|dock| {
                                    dock.window_list_popup.as_ref().map(|popup| {
                                        let px =
                                            (popup.position.0 as f64 * dock_scale).round() as i32;
                                        let py =
                                            (popup.position.1 as f64 * dock_scale).round() as i32;
                                        let pw = (popup.width as f64 * dock_scale).round() as i32;
                                        let ph = (popup.height as f64 * dock_scale).round() as i32;
                                        (px, py, pw, ph)
                                    })
                                })
                                .unwrap_or_else(|| {
                                    let anchor_x = calculate_popup_anchor(
                                        state,
                                        app_id,
                                        apps_in_dock,
                                        phys_width,
                                        scale_factor as f64,
                                        layout,
                                    );
                                    let geom = PopupGeometry::compute_bounds(
                                        PopupType::WindowList,
                                        anchor_x,
                                        0,
                                        phys_width,
                                        phys_dock_height,
                                        item_count,
                                        dock_scale as f32,
                                    );
                                    let mw =
                                        (geom.logical_width as f64 * dock_scale).round() as i32;
                                    (geom.x, geom.y, mw, geom.phys_height)
                                });

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
                                // The popup has a capture frame at the top; rows start below it.
                                // Derive frame height from the popup's total height minus the rows area,
                                // matching how window_list.rs and render/popup.rs calculate it.
                                // The popup geometry uses the first stream's aspect ratio, so we must
                                // use the same frame height — not the active stream's — to avoid
                                // rows_y mismatch when hovering over different rows.
                                let phys_item_h =
                                    (state.config.popup_row_height as f32 * dock_scale as f32)
                                        .round() as i32;
                                let rows_phys_h = item_count * phys_item_h;
                                let frame_phys_h = menu_height - rows_phys_h;
                                let rows_y = menu_y + frame_phys_h;
                                let rows_height = menu_height - frame_phys_h;

                                // Only process clicks in the rows area, not the frame area
                                if ptr_phys_y >= rows_y && ptr_phys_y <= rows_y + rows_height {
                                    // Calculate row index the same way window_list.rs does for hover:
                                    // divide the Y offset from the rows area start by the physical row height.
                                    let clicked_idx = ((ptr_phys_y - rows_y).max(0)
                                        / phys_item_h.max(1))
                                    .clamp(0, item_count.saturating_sub(1))
                                        as usize;
                                    let mut should_close_window_list = true;

                                    if let Some(handle_id) = windows.get(clicked_idx) {
                                        let close_btn_size =
                                            (16.0 * dock_scale as f32).round() as i32;
                                        let close_btn_right_padding =
                                            (8.0 * dock_scale as f32).round() as i32;
                                        let close_box_x_start = menu_x + menu_width
                                            - close_btn_right_padding
                                            - close_btn_size;
                                        let is_on_close_box = ptr_phys_x >= close_box_x_start;

                                        match button {
                                            BTN_LEFT => {
                                                if is_on_close_box {
                                                    if let Some(win) =
                                                        state.window_handle(handle_id)
                                                    {
                                                        win.close();
                                                        // Keep the popup open after closing a window
                                                        should_close_window_list = false;
                                                    }
                                                } else if let Some(seat) = &state.wl_seat {
                                                    let win = state
                                                        .window_handles
                                                        .get(handle_id)
                                                        .cloned();
                                                    if let Some(win) = win {
                                                        win.activate(seat);
                                                    }
                                                }
                                            }
                                            BTN_MIDDLE => {
                                                if let Some(win) = state.window_handle(handle_id) {
                                                    win.close();
                                                    // Keep the popup open after closing a window
                                                    should_close_window_list = false;
                                                }
                                            }
                                            _ => {}
                                        }
                                    }

                                    if should_close_window_list {
                                        state.request_window_list_close();
                                    }

                                    layer_changed = true;
                                    continue;
                                } else {
                                    // Click in frame area - just close the popup
                                    state.request_window_list_close();
                                    layer_changed = true;
                                    continue;
                                }
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
                    if let Some(ref popup) = dock.audio_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                    if let Some(ref popup) = dock.bluetooth_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                    if let Some(ref popup) = dock.rclone_popup {
                        if event.surface == popup.surface {
                            on_popup = true;
                            break;
                        }
                    }
                    if let Some(ref popup) = dock.process_popup {
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

                // Allow drag release even if pointer left the dock surface during drag
                let should_handle_dock_release = (event_on_dock_surface || state.is_dragging)
                    && (button == BTN_LEFT || button == BTN_MIDDLE);
                if should_handle_dock_release {
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
