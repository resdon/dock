pub mod click;
pub mod drag;
pub mod scroll;
pub mod timers;

pub use timers::{reset_all, update_timers};

use std::collections::HashMap;
use std::time::Duration;
use wayland_client::backend::ObjectId;

use crate::geometry::popup::PopupType;
use crate::pointer::proximity::pointer_on_popup;
use crate::AppState;

/// Top-level coordinator for interaction state updates.
pub fn update(
    state: &mut AppState,
    apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    // PARAMETERS:
    // ----------
    // Maximum inactivity duration before popups are closed
    let hard_timeout = Duration::from_millis(10000);
    // Context menu inactivity threshold before fading out
    let menu_time_out = Duration::from_millis(1000);
    // Context menu fade animation speed in seconds
    let menu_fade_time: f32 = 1.0;
    // Window list inactivity threshold before fading out
    let list_time_out = Duration::from_millis(5000);
    // Window list fade animation speed in seconds
    let list_fade_time: f32 = 5.0;
    // Dock inactivity threshold before hiding
    let dock_time_out = Duration::from_millis(1000);
    // Delay in milliseconds before the dock initiates its hide
    let dock_time_delay_ms: u64 = 500;
    // ----------

    let mut layer_changed = false;
    let cursor_moved = state.menu_state.cursor_moved;

    // State tracking flags
    let pointer_inside_dock = state.interaction.pointer_inside;
    // Context menu state
    let target_app = state
        .menu_state
        .target_app_id
        .as_deref()
        .unwrap_or_default();
    let on_context_menu = state.menu_state.is_open
        && pointer_on_popup(
            state,
            PopupType::ContextMenu,
            target_app,
            state.menu_state.items.len(),
            apps_in_dock,
            scale_factor,
            layout,
        );
    // Window list state
    let window_list_target = state
        .window_list_state
        .target_app_id
        .as_deref()
        .unwrap_or_default();
    let window_count = state
        .open_windows
        .iter()
        .filter(|w| w.1.app_id == window_list_target)
        .count();

    let on_window_list = state.window_list_state.is_open
        && window_count > 0
        && (state.window_list_state.pointer_inside_popup
            || pointer_on_popup(
                state,
                PopupType::WindowList,
                window_list_target,
                window_count,
                apps_in_dock,
                scale_factor,
                layout,
            ));

    // Pointer on dock check
    let on_dock = pointer_inside_dock && !on_context_menu && !on_window_list;

    if state.menu_state.just_opened {
        timers::reset_all(state);

        for dock in &mut state.docks {
            dock.menu_fade.duration_secs = menu_fade_time;
            dock.menu_fade.set_visible(true);
            dock.menu_fade.current_alpha = 0.0;
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

    // Context menu timers rules
    if state.menu_state.is_open && (no_motion_timed_out || dock_timed_out || context_menu_timed_out)
    {
        state.menu_state.is_open = false;
        timers::reset_all(state);
        layer_changed = true;

        for dock in &mut state.docks {
            dock.menu_fade.set_visible(false);
        }
    }

    // Window list timers rules
    if state.window_list_state.is_open
        && (no_motion_timed_out || dock_timed_out || window_list_timed_out)
    {
        state.window_list_state.is_open = false;
        state.window_list_state.target_app_id = None;
        timers::reset_all(state);
        layer_changed = true;

        for dock in &mut state.docks {
            dock.window_list_fade.set_visible(false);
        }
    }

    // Context Menu Fade Target Update
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let fading_out = elapsed.no_motion >= menu_time_out
            || elapsed.dock >= menu_time_out
            || elapsed.context_menu >= menu_time_out;

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
        let window_fading_out = elapsed.no_motion >= list_time_out
            || elapsed.dock >= list_time_out
            || elapsed.window_list >= list_time_out;

        let window_visible_target = !window_fading_out && window_count > 0;

        for dock in &mut state.docks {
            dock.window_list_fade.duration_secs = list_fade_time;
            let was_target_visible = dock.window_list_fade.target_alpha > 0.0;
            dock.window_list_fade.set_visible(window_visible_target);
            if was_target_visible != window_visible_target {
                layer_changed = true;
            }
        }

        if !window_visible_target
            && state
                .docks
                .iter()
                .all(|d| d.window_list_fade.current_alpha == 0.0)
        {
            state.window_list_state.is_open = false;
            state.window_list_state.target_app_id = None;
            layer_changed = true;
        }
    } else {
        for dock in &mut state.docks {
            dock.window_list_fade.set_visible(false);
        }
    }

    let animating = state.docks.iter().any(|d| {
        (d.menu_fade.current_alpha - d.menu_fade.target_alpha).abs() > f32::EPSILON
            || (d.window_list_fade.current_alpha - d.window_list_fade.target_alpha).abs()
                > f32::EPSILON
    });
    if animating {
        layer_changed = true;
    }

    state.menu_state.cursor_moved = false;

    // --- AUTO-HIDE STATE ENGINE ---
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

    layer_changed
}
