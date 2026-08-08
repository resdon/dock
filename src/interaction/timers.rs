use crate::AppState;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, Copy)]
pub struct TimerElapsed {
    pub no_motion: Duration,
    pub dock: Duration,
    pub context_menu: Duration,
    pub window_list: Duration,
}

/// Helper to clear and reset all tracking timers and counters.
pub fn reset_all(state: &mut AppState) {
    state.menu_state.consecutive_no_motion_count = 0;
    state.menu_state.consecutive_on_dock_count = 0;
    state.menu_state.consecutive_on_window_list_count = 0;
    state.menu_state.consecutive_on_context_menu_count = 0;
    state.menu_state.no_motion_timer = None;
    state.menu_state.dock_timer = None;
    state.menu_state.context_menu_timer = None;
    state.menu_state.window_list_timer = None;
}

/// Updates tracking timers for inactivity, dock, context menu, and window list.
pub fn update_timers(
    state: &mut AppState,
    cursor_moved: bool,
    is_active_drag: bool,
    on_dock: bool,
    on_context_menu: bool,
    pointer_on_window_list_target: bool,
) -> TimerElapsed {
    // 1. Inactivity (No Motion) Tracking
    if !cursor_moved && !is_active_drag {
        state.menu_state.consecutive_no_motion_count += 1;
        if state.menu_state.no_motion_timer.is_none() {
            state.menu_state.no_motion_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.no_motion_timer = None;
    }

    // 2. Dock Tracking
    if on_dock {
        state.menu_state.consecutive_on_dock_count += 1;
        if state.menu_state.dock_timer.is_none() {
            state.menu_state.dock_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.dock_timer = None;
    }

    // 3. Context Menu Tracking
    let is_menu_rendering = state.docks.iter().any(|d| d.menu_fade.current_alpha > 0.0);

    if state.menu_state.is_open {
        if on_context_menu {
            state.menu_state.consecutive_on_context_menu_count += 1;
            state.menu_state.context_menu_timer = None;
        } else {
            state.menu_state.consecutive_on_context_menu_count = 0;
            if state.menu_state.context_menu_timer.is_none() {
                state.menu_state.context_menu_timer = Some(Instant::now());
            }
        }
    } else {
        state.menu_state.consecutive_on_context_menu_count = 0;
        // Reset timer when closed and render/fade animation hits alpha 0.0
        if !is_menu_rendering {
            state.menu_state.context_menu_timer = None;
        }
    }

    // 4. Window List Tracking
    let is_window_list_rendering = state
        .docks
        .iter()
        .any(|d| d.window_list_fade.current_alpha > 0.0);

    if state.window_list_state.is_open {
        if pointer_on_window_list_target {
            state.menu_state.consecutive_on_window_list_count += 1;
            state.menu_state.window_list_timer = None;
        } else {
            state.menu_state.consecutive_on_window_list_count = 0;
            if state.menu_state.window_list_timer.is_none() {
                state.menu_state.window_list_timer = Some(Instant::now());
            }
        }
    } else {
        state.menu_state.consecutive_on_window_list_count = 0;
        // Reset timer when closed and render/fade animation hits alpha 0.0
        if !is_window_list_rendering {
            state.menu_state.window_list_timer = None;
        }
    }
    TimerElapsed {
        no_motion: state
            .menu_state
            .no_motion_timer
            .map_or(Duration::ZERO, |t| t.elapsed()),
        dock: state
            .menu_state
            .dock_timer
            .map_or(Duration::ZERO, |t| t.elapsed()),
        context_menu: state
            .menu_state
            .context_menu_timer
            .map_or(Duration::ZERO, |t| t.elapsed()),
        window_list: state
            .menu_state
            .window_list_timer
            .map_or(Duration::ZERO, |t| t.elapsed()),
    }
}
