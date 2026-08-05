pub mod hover;
pub mod motion;
pub mod proximity;

pub use hover::update_hover;
pub use motion::handle_motion_events;
pub use proximity::{pointer_in_context_menu_leeway, pointer_on_context_menu};

use std::collections::HashMap;
use std::time::{Instant, Duration};
use wayland_client::backend::ObjectId;
use crate::AppState;

/// Top-level coordinator for interaction state updates.
pub fn update(
    state: &mut AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let mut layer_changed = false;

    let cursor_moved = state.menu_state.cursor_moved;

    // Define timeout thresholds (1000ms for no-motion and window list, 2000ms for dock)
    let no_motion_timeout = Duration::from_millis(1000);
    let dock_timeout = Duration::from_millis(2000);
    let window_list_timeout = Duration::from_millis(1000);

    // Check if window list timer has timed out before updating hover
    let window_list_timed_out_pre = state.menu_state.window_list_timer
        .map_or(false, |t| t.elapsed() >= window_list_timeout);

    // 1. Hover Calculation & Fade updates (determines window list / hover visibility)
    let mut hover_res = update_hover(state, apps_in_dock, running_by_app, scale_factor, layout);

    if window_list_timed_out_pre {
        hover_res.visible = false;
        hover_res.app_id = None;
        state.menu_state.window_list_timer = None;
        state.menu_state.consecutive_on_window_list_count = 0;
    }

    let visibility_changed = hover_res.visible != state.hover_state.is_visible;
    let app_changed = hover_res.visible && (hover_res.app_id != state.hover_state.app_id);

    if visibility_changed || app_changed {
        println!(
            "[DEBUG Hover] visibility_changed: {}, app_changed: {}, visible: {}, app_id: {:?}",
            visibility_changed, app_changed, hover_res.visible, hover_res.app_id
        );

        state.hover_state.is_visible = hover_res.visible;
        state.hover_state.app_id = hover_res.app_id;
        layer_changed = true;

        for dock in &mut state.docks {
            dock.hover_fade.set_visible(hover_res.visible);
        }
    }

    // 2. State tracking flags
    let pointer_inside_dock = state.interaction.pointer_inside;
    let on_window_list = state.hover_state.is_visible;
    let on_context_menu = state.menu_state.is_open && pointer_on_context_menu(state, apps_in_dock, scale_factor, layout);
    let in_valid_leeway = pointer_in_context_menu_leeway(state, apps_in_dock, scale_factor, layout);

    // Exclude context menu area from counting as the dock
    let on_dock = pointer_inside_dock && !on_context_menu;

    // If the menu was just opened this frame, reset all counters and timers cleanly
    if state.menu_state.just_opened {
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.consecutive_on_window_list_count = 0;
        state.menu_state.consecutive_on_context_menu_count = 0;
        state.menu_state.no_motion_timer = None;
        state.menu_state.dock_timer = None;
        state.menu_state.context_menu_timer = None;
        state.menu_state.window_list_timer = None;
        state.menu_state.just_opened = false;
    }

    let pointer_on_valid_surface = on_dock || on_window_list || on_context_menu || in_valid_leeway;

    // --- FULLY INDEPENDENT SIMULTANEOUS TRACKING (COUNTERS & TIMERS) ---

    // A. No motion (inactivity) tracking: ticks whenever the mouse stops moving
    if !cursor_moved {
        state.menu_state.consecutive_no_motion_count += 1;
        if state.menu_state.no_motion_timer.is_none() {
            state.menu_state.no_motion_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.no_motion_timer = None;
    }

    // B. Dock tracking (only active when actually on dock and NOT on context menu)
    if on_dock {
        state.menu_state.consecutive_on_dock_count += 1;
        if state.menu_state.dock_timer.is_none() {
            state.menu_state.dock_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.dock_timer = None;
    }

    // C. Context menu tracking
    if on_context_menu {
        state.menu_state.consecutive_on_context_menu_count += 1;
        if state.menu_state.context_menu_timer.is_none() {
            state.menu_state.context_menu_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_on_context_menu_count = 0;
        state.menu_state.context_menu_timer = None;
    }

    // D. Window list tracking (shares the exact same no-motion inactivity logic)
    if on_window_list {
        if !cursor_moved {
            state.menu_state.consecutive_on_window_list_count += 1;
            if state.menu_state.window_list_timer.is_none() {
                state.menu_state.window_list_timer = Some(Instant::now());
            }
        } else {
            state.menu_state.consecutive_on_window_list_count = 0;
            state.menu_state.window_list_timer = None;
        }
    } else {
        state.menu_state.consecutive_on_window_list_count = 0;
        state.menu_state.window_list_timer = None;
    }

    let no_motion_timed_out = state.menu_state.no_motion_timer
        .map_or(false, |t| t.elapsed() >= no_motion_timeout);
    
    let dock_timed_out = state.menu_state.dock_timer
        .map_or(false, |t| t.elapsed() >= dock_timeout);

    let window_list_timed_out = window_list_timed_out_pre || state.menu_state.window_list_timer
        .map_or(false, |t| t.elapsed() >= window_list_timeout);

    // Close context menu if any timeout threshold is met
    if state.menu_state.is_open && (no_motion_timed_out || dock_timed_out || window_list_timed_out) {
        println!(
            "[TIMEOUT] Closing context menu (no_motion_timed_out: {}, dock_timed_out: {}, window_list_timed_out: {}).",
            no_motion_timed_out,
            dock_timed_out,
            window_list_timed_out
        );
        state.menu_state.is_open = false;
        state.menu_state.target_app_id = None;
        state.menu_state.target_window = None;
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.consecutive_on_window_list_count = 0;
        state.menu_state.no_motion_timer = None;
        state.menu_state.dock_timer = None;
        state.menu_state.window_list_timer = None;
        layer_changed = true;
    }

    // --- DEBUG PRINT: Log all independent counters and timers simultaneously ---
    if state.menu_state.last_debug_print.elapsed() >= Duration::from_millis(15) {
        let current_x = state.interaction.pointer_position.x;
        let current_y = state.interaction.pointer_position.y;
        
        let no_motion_ms = state.menu_state.no_motion_timer.map_or(0, |t| t.elapsed().as_millis());
        let dock_ms = state.menu_state.dock_timer.map_or(0, |t| t.elapsed().as_millis());
        let menu_ms = state.menu_state.context_menu_timer.map_or(0, |t| t.elapsed().as_millis());
        let win_list_ms = state.menu_state.window_list_timer.map_or(0, |t| t.elapsed().as_millis());

        println!(
            "[DEBUG Simultaneous] cursor_moved: {}, on_dock: {}, on_menu: {} | Counts [no_motion: {}, dock: {}, menu: {}, win_list: {}] | Timers [no_motion: {}ms, dock: {}ms, menu: {}ms, win_list: {}ms] | ptr: ({}, {})",
            cursor_moved,
            on_dock,
            on_context_menu,
            state.menu_state.consecutive_no_motion_count,
            state.menu_state.consecutive_on_dock_count,
            state.menu_state.consecutive_on_context_menu_count,
            state.menu_state.consecutive_on_window_list_count,
            no_motion_ms,
            dock_ms,
            menu_ms,
            win_list_ms,
            current_x,
            current_y
        );

        state.menu_state.last_debug_print = Instant::now();
    }

    // 3. Context Menu Lifecycle & Fade updates
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let menu_active = !no_motion_timed_out && !dock_timed_out && !window_list_timed_out;

        for dock in &mut state.docks {
            dock.menu_fade.set_visible(menu_active);
        }
    } else {
        for dock in &mut state.docks {
            dock.menu_fade.set_visible(false);
        }
    }

    // Reset cursor_moved at the end of the tick
    state.menu_state.cursor_moved = false;

    layer_changed
}
