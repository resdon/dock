//! Main interaction loop.
//!
//! Processes pointer proximity, fade timing, and popup auto-close logic.

pub mod click;
pub mod drag;
pub mod scroll;
pub mod timers;

pub use timers::{reset_all, update_timers};

use std::collections::HashMap;
use std::time::Duration;
use wayland_client::backend::ObjectId;

use crate::AppState;

/// Top-level coordinator for interaction state updates.
///
/// Called every frame from the main event loop. Updates timer state, evaluates
/// popup auto-close conditions, and adjusts fade targets for context menus and
/// window lists. Returns `true` if any UI layer changed and a redraw is needed.
///
/// # Interaction flow
/// 1. Read config parameters (timeouts, fade durations)
/// 2. Track pointer position relative to dock, popups, and menus
/// 3. Update timers based on motion and surface occupancy
/// 4. Evaluate hard close timeouts for context menu and window list
/// 5. Update fade animation targets
/// 6. Handle dock auto-hide/show transitions
pub fn update(
    state: &mut AppState,
    _apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    _scale_factor: f32,
    _layout: (i32, i32, i32, i32),
) -> bool {
    // PARAMETERS from config:
    // ----------
    let hard_timeout = Duration::from_millis(state.config.hard_timeout);
    let menu_time_out = Duration::from_millis(state.config.context_menu_timeout);
    let menu_fade_time: f32 = state.config.context_menu_fade_in;
    // Keep configured shorter delays, but don't leave the window list lingering
    // for more than two seconds after the pointer leaves it.
    let list_time_out =
        Duration::from_millis(state.config.popup_timeout).min(Duration::from_secs(2));
    let list_fade_time: f32 = state.config.popup_fade_in;
    let auto_hide_enabled = state.config.auto_hide_enabled;
    let dock_time_out = Duration::from_millis(1000);
    let dock_time_delay_ms: u64 = 500;
    // ----------

    let mut layer_changed = false;
    let cursor_moved = state.menu_state.cursor_moved;

    // State tracking flags
    let pointer_inside_dock = state.interaction.pointer_inside;

    // FIX (v1.5.5): Detect whether the pointer is over a popup using two methods:
    // 1. Surface-based: the compositor routed the event to the popup surface
    // 2. Geometry-based: the pointer position falls within the popup's bounds
    //
    // The geometry fallback is essential because some compositors (e.g., labwc)
    // route pointer events over subsurfaces to the parent surface, making
    // surface-based detection fail entirely. Without this fallback, the popup
    // never detects hover, cursor movement never triggers redraws, and the
    // highlight/freezing issues persist.
    //
    // Before this fix, only surface-based detection was used. When the compositor
    // routed events to the parent surface, on_window_list was always false, so
    // cursor movement over the popup didn't trigger redraws. The popup only updated
    // when PipeWire happened to capture a new frame — causing the highlight to freeze.
    let pointer_x = state.interaction.pointer_position.x;
    let pointer_y = state.interaction.pointer_position.y;

    let mut pointer_over_context_menu = state.menu_state.pointer_inside_popup;
    let mut pointer_over_window_list = state.window_list_state.pointer_inside_popup;

    // Geometry-based fallback: check if pointer is within any popup's bounds.
    // Popup positions and sizes are in dock-surface logical coordinates.
    if !pointer_over_context_menu || !pointer_over_window_list {
        for dock in &state.docks {
            if !pointer_over_context_menu && state.menu_state.is_open {
                if let Some(ref popup) = dock.context_menu_popup {
                    let px = popup.position.0 as f64;
                    let py = popup.position.1 as f64;
                    let pw = popup.width as f64;
                    let ph = popup.height as f64;
                    if pointer_x >= px
                        && pointer_x < px + pw
                        && pointer_y >= py
                        && pointer_y < py + ph
                    {
                        pointer_over_context_menu = true;
                    }
                }
            }
            if !pointer_over_window_list && state.window_list_state.is_open {
                if let Some(ref popup) = dock.window_list_popup {
                    let px = popup.position.0 as f64;
                    let py = popup.position.1 as f64;
                    let pw = popup.width as f64;
                    let ph = popup.height as f64;
                    if pointer_x >= px
                        && pointer_x < px + pw
                        && pointer_y >= py
                        && pointer_y < py + ph
                    {
                        pointer_over_window_list = true;
                    }
                }
            }
        }
    }

    let on_context_menu = state.menu_state.is_open && pointer_over_context_menu;

    // Window list state
    let window_list_target = state
        .window_list_state
        .target_app_id
        .as_deref()
        .unwrap_or_default();
    let window_count = state.window_count_for_app(window_list_target);

    let on_window_list = state.window_list_state.is_open
        && window_count > 0
        && pointer_over_window_list;

    // Pointer on dock check
    let on_dock = pointer_inside_dock && !on_context_menu && !on_window_list;

    if state.menu_state.just_opened {
        timers::reset_all(state);

        for dock in &mut state.docks {
            dock.menu_fade.duration_secs = menu_fade_time;
            dock.menu_fade.show();
        }

        state.menu_state.just_opened = false;
    }

    // Mouse dragging state
    let is_active_drag = state.is_dragging
        || state.dragged_app_id.is_some()
        || state.dnd_state.current_offer.is_some();

    // --- TRACKING TIMERS ---
    let elapsed = timers::update_timers(
        state,
        cursor_moved,
        is_active_drag,
        on_dock,
        on_context_menu,
        on_window_list,
    );

    // Evaluate Hard Close Timeouts
    let no_motion_timed_out = elapsed.no_motion >= hard_timeout;
    let dock_timed_out = elapsed.dock >= hard_timeout;
    let context_menu_timed_out = elapsed.context_menu >= hard_timeout;
    let window_list_timed_out = elapsed.window_list >= hard_timeout;

    // Cursor idle timeout: close all popups when the cursor hasn't moved
    // for the configured duration, regardless of cursor position.
    let cursor_idle_timeout = Duration::from_millis(state.config.cursor_idle_timeout);
    let cursor_idle_timed_out = elapsed.no_motion >= cursor_idle_timeout;

    // Context menu timers rules
    if state.menu_state.is_open
        && (no_motion_timed_out || dock_timed_out || context_menu_timed_out || cursor_idle_timed_out)
    {
        layer_changed |= state.request_context_menu_close();
    }

    // Window list timers rules
    if state.window_list_state.is_open
        && (no_motion_timed_out || dock_timed_out || window_list_timed_out || cursor_idle_timed_out)
    {
        layer_changed |= state.request_window_list_close();
    }

    // Context Menu Fade Target Update
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        // A popup must not fade merely because the pointer is on the popup
        // rather than the parent dock surface. Hard timeouts still win so a
        // stale open state cannot survive indefinitely. Once a close request
        // sets the fade target to zero, keep that target authoritative until
        // finalize_popup_closures() clears the logical popup state.
        let hard_close = no_motion_timed_out || dock_timed_out || context_menu_timed_out;
        let close_requested = state
            .docks
            .iter()
            .any(|dock| dock.menu_fade.target_alpha <= dock.menu_fade.min_alpha + f32::EPSILON);
        let fading_out = close_requested
            || hard_close
            || (!on_context_menu
                && (elapsed.no_motion >= menu_time_out
                    || elapsed.dock >= menu_time_out
                    || elapsed.context_menu >= menu_time_out));

        let menu_visible_target = !fading_out;

        for dock in &mut state.docks {
            dock.menu_fade.duration_secs = menu_fade_time;
            let was_target_visible = dock.menu_fade.target_alpha > 0.0;
            dock.menu_fade.set_visible(menu_visible_target);
            if was_target_visible != menu_visible_target {
                layer_changed = true;
            }
        }
    } else {
        for dock in &mut state.docks {
            dock.menu_fade.set_visible(false);
        }
    }

    // Window List Fade Target Update
    if state.window_list_state.is_open {
        let hard_close = no_motion_timed_out || dock_timed_out || window_list_timed_out;
        let close_requested = state.docks.iter().any(|dock| {
            dock.window_list_fade.target_alpha <= dock.window_list_fade.min_alpha + f32::EPSILON
        });
        let window_fading_out = close_requested
            || hard_close
            || (!on_window_list
                && (elapsed.no_motion >= list_time_out
                    || elapsed.dock >= list_time_out
                    || elapsed.window_list >= list_time_out));

        let window_visible_target = !window_fading_out && window_count > 0;

        for dock in &mut state.docks {
            dock.window_list_fade.duration_secs = list_fade_time;
            let was_target_visible = dock.window_list_fade.target_alpha > 0.0;
            dock.window_list_fade.set_visible(window_visible_target);
            if was_target_visible != window_visible_target {
                layer_changed = true;
            }
        }

        // Keep logical popup state until the fade reaches zero. This preserves
        // the target geometry for the entire fade and avoids recreate/flicker loops.
    } else {
        for dock in &mut state.docks {
            dock.window_list_fade.set_visible(false);
        }
    }

    // FIX (v1.5.5): If the cursor moved while the window list popup is open,
    // trigger a redraw so the popup can update the hover highlight and capture frame.
    //
    // Before this fix, cursor_moved was only used for timer resets and never set
    // layer_changed = true. So pointer motion over the popup did not trigger a redraw.
    // The popup only updated when PipeWire happened to capture a new frame, causing
    // the highlight to freeze when the captured window was static.
    //
    // The content hash caching in window_list.rs still prevents unnecessary GPU work:
    // if the hover row hasn't changed and the pointer is in the same pixel, the hash
    // will match and rendering is skipped. But at least the draw function is called.
    if cursor_moved && on_window_list {
        layer_changed = true;
    }
    state.menu_state.cursor_moved = false;

    // --- AUTO-HIDE STATE ENGINE ---
    // When auto-hide is disabled in config, keep the dock fully visible and
    // clickable at all times — skip the fade animation and input region shrink.
    if auto_hide_enabled {
        let pointer_near_edge = state.interaction.is_pointer_near;
        let auto_hide_timed_out = elapsed.no_motion >= dock_time_out;

        if is_active_drag {
            if state.hide_state.is_fully_hidden()
                || state.hide_state.mode == crate::graphics::hide::Mode::Hiding
            {
                state.hide_state.show();
                layer_changed = true;
            }
            state.hide_state.hide_timer = None;
        } else if pointer_near_edge && !auto_hide_timed_out {
            if state.hide_state.is_fully_hidden()
                || state.hide_state.mode == crate::graphics::hide::Mode::Hiding
            {
                state.hide_state.show();
                layer_changed = true;
            }
        } else if pointer_inside_dock && !auto_hide_timed_out {
            state.hide_state.hide_timer = None;
        } else {
            state.hide_state.start_hide_timer();
            state.hide_state.update_timer(dock_time_delay_ms);
        }

        if state.hide_state.tick() {
            layer_changed = true;
        }
    } else {
        // Auto-hide disabled: ensure dock stays fully visible
        if !state.hide_state.is_fully_visible() {
            state.hide_state.show();
            layer_changed = true;
        }
        state.hide_state.hide_timer = None;
    }

    // Animate popup fade alphas toward their targets
    for dock in &mut state.docks {
        if dock.menu_fade.tick() {
            layer_changed = true;
        }
        if dock.window_list_fade.tick() {
            layer_changed = true;
        }
    }

    layer_changed
}
