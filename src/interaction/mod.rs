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

    // Doubled timing thresholds: start fading at 1000ms, hard close at 2000ms
    let fade_delay = Duration::from_millis(1000);
    let hard_timeout = Duration::from_millis(2000);

    // Pre-check window list hard timeout before calculating raw hover
    let window_list_elapsed_pre = state.menu_state.window_list_timer.map_or(Duration::ZERO, |t| t.elapsed());
    let window_list_hard_timeout_pre = window_list_elapsed_pre >= hard_timeout;

	// 1. Raw Hover Calculation
    let raw_hover_res = update_hover(state, apps_in_dock, running_by_app, scale_factor, layout);
    let pointer_on_window_list_target = raw_hover_res.visible;

    let mut hover_res = raw_hover_res;

    // Suppress window list if hard timed out
    if window_list_hard_timeout_pre {
        hover_res.visible = false;
        state.menu_state.consecutive_on_window_list_count = 0;
    }

    // Window list starts fading out once fade_delay (1000ms) is reached
    let target_hover_visible = hover_res.visible && (window_list_elapsed_pre < fade_delay);

    let visibility_changed = target_hover_visible != state.hover_state.is_visible;
    let app_changed = target_hover_visible && (hover_res.app_id != state.hover_state.app_id);
    let hover_just_opened = target_hover_visible && !state.hover_state.is_visible;

    if visibility_changed || app_changed {
        state.hover_state.is_visible = target_hover_visible;
        
        // Preserve app_id during fade-out to render popup contents continuously
        if hover_res.visible || state.hover_state.app_id.is_none() {
            state.hover_state.app_id = hover_res.app_id;
        }
        layer_changed = true;

        for dock in &mut state.docks {
            dock.hover_fade.set_visible(target_hover_visible);
            if hover_just_opened {
                dock.hover_fade.current_alpha = 0.0; // Start at 0% and fade in to 1.0
            }
        }
    }

    // 2. State tracking flags
    let pointer_inside_dock = state.interaction.pointer_inside;
    let in_valid_leeway = pointer_in_context_menu_leeway(state, apps_in_dock, scale_factor, layout);
    
    let on_context_menu = state.menu_state.is_open 
        && (pointer_on_context_menu(state, apps_in_dock, scale_factor, layout) || in_valid_leeway);

    let on_dock = pointer_inside_dock && !on_context_menu;

    if state.menu_state.just_opened {
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.consecutive_on_window_list_count = 0;
        state.menu_state.consecutive_on_context_menu_count = 0;
        state.menu_state.no_motion_timer = None;
        state.menu_state.dock_timer = None;
        state.menu_state.context_menu_timer = None;
        state.menu_state.window_list_timer = None;

        for dock in &mut state.docks {
            dock.menu_fade.set_visible(true);
            dock.menu_fade.current_alpha = 0.0; // Start at 0% and fade in to 1.0
        }

        state.menu_state.just_opened = false;
    }

    // --- TRACKING COUNTERS & TIMERS ---

    // A. Inactivity tracking: Cursor movement resets inactivity timer
    if !cursor_moved {
        state.menu_state.consecutive_no_motion_count += 1;
        if state.menu_state.no_motion_timer.is_none() {
            state.menu_state.no_motion_timer = Some(Instant::now());
        }
    } else {
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.no_motion_timer = None;
    }

    // B. Dock tracking
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
        state.menu_state.context_menu_timer = None;
    } else {
        state.menu_state.consecutive_on_context_menu_count = 0;
        if state.menu_state.context_menu_timer.is_none() {
            state.menu_state.context_menu_timer = Some(Instant::now());
        }
    }

    // D. Window list tracking
    if pointer_on_window_list_target {
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

    let no_motion_elapsed = state.menu_state.no_motion_timer.map_or(Duration::ZERO, |t| t.elapsed());
    let dock_elapsed = state.menu_state.dock_timer.map_or(Duration::ZERO, |t| t.elapsed());
    let context_menu_elapsed = state.menu_state.context_menu_timer.map_or(Duration::ZERO, |t| t.elapsed());
    let window_list_elapsed = state.menu_state.window_list_timer.map_or(Duration::ZERO, |t| t.elapsed());

    // Evaluate Hard Close Timeouts (>= 2000ms)
    let no_motion_timed_out = no_motion_elapsed >= hard_timeout;
    let dock_timed_out = dock_elapsed >= hard_timeout;
    let window_list_timed_out = window_list_hard_timeout_pre || window_list_elapsed >= hard_timeout;
    let context_menu_timed_out = context_menu_elapsed >= hard_timeout;

    // Hard close context menu when 2000ms threshold is passed
    if state.menu_state.is_open && (no_motion_timed_out || dock_timed_out || window_list_timed_out || context_menu_timed_out) {
        state.menu_state.is_open = false;
        state.menu_state.consecutive_no_motion_count = 0;
        state.menu_state.consecutive_on_dock_count = 0;
        state.menu_state.consecutive_on_window_list_count = 0;
        state.menu_state.consecutive_on_context_menu_count = 0;
        state.menu_state.no_motion_timer = None;
        state.menu_state.dock_timer = None;
        state.menu_state.window_list_timer = None;
        state.menu_state.context_menu_timer = None;
        layer_changed = true;

        for dock in &mut state.docks {
            dock.menu_fade.set_visible(false);
        }
    }

    // Debug logging
    if state.menu_state.last_debug_print.elapsed() >= Duration::from_millis(15) {
        println!(
            "[DEBUG Simultaneous] no_motion: {}ms, dock: {}ms, menu: {}ms, win_list: {}ms",
            no_motion_elapsed.as_millis(),
            dock_elapsed.as_millis(),
            context_menu_elapsed.as_millis(),
            window_list_elapsed.as_millis(),
        );

        state.menu_state.last_debug_print = Instant::now();
    }

    // 3. Context Menu Fade Target Update (Starts fading out at 1000ms mark of inactivity or off-target)
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let fading_out = no_motion_elapsed >= fade_delay
            || dock_elapsed >= fade_delay
            || window_list_elapsed >= fade_delay
            || context_menu_elapsed >= fade_delay;

        let menu_visible_target = !fading_out;

        for dock in &mut state.docks {
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

    // Keep layer redrawing active while alpha is animating back to target
    let animating = state.docks.iter().any(|d| {
        (d.hover_fade.current_alpha - d.hover_fade.target_alpha).abs() > f32::EPSILON
            || (d.menu_fade.current_alpha - d.menu_fade.target_alpha).abs() > f32::EPSILON
    });
    if animating {
        layer_changed = true;
    }

    state.menu_state.cursor_moved = false;

	// --- AUTO-HIDE STATE ENGINE ---
    let pointer_near_edge = state.interaction.is_pointer_near;
    let no_motion_timed_out = no_motion_elapsed >= Duration::from_millis(1000); // DOCK time to start hide/fade

    if pointer_near_edge && !no_motion_timed_out {
        // Reveal dock when pointer touches edge/strip
        if state.hide_state.is_fully_hidden() || state.hide_state.mode == crate::graphics::hide::Mode::Hiding {
            state.hide_state.show();
            layer_changed = true;
        }
    } else if pointer_inside_dock && !no_motion_timed_out {
        // Reset hide timer while actively moving/interacting inside dock area
        state.hide_state.hide_timer = None;
    } else {
        // Pointer left OR pointer stationary > 5000ms: trigger hide timer and fade out
        state.hide_state.start_hide_timer();
        state.hide_state.update_timer(500);
    }

    // Tick alpha animation and transition to Hidden when alpha hits 0.0
    if state.hide_state.tick() {
        layer_changed = true;
    }

	layer_changed //[cite: 9]
}
