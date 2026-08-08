use library::init::{initialize_app, AppContext};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize application state and event queue via init.rs
    let context = initialize_app()?;
    let AppContext {
        mut state,
        mut event_queue,
        qh,
        mut badge_rx,
    } = context;

    // --- MAIN EVENT LOOP ---
    loop {
        // 1. INPUT: Dispatch Wayland socket events from event queue
        if let Err(e) = event_queue.dispatch_pending(&mut state) {
            eprintln!("[WARN] Dispatch error: {}", e);
            break;
        }

        // 2. INTERACTION: Evaluate hitboxes, pointer proximity, and active drag targets
        let is_near = state.check_dock_proximity();
        state.interaction.is_pointer_near = is_near;

        // 3. STATE UPDATE: Drain background channels (Icons & DBus) & update flags
        let mut state_changed = false;
        if state.process_loaded_icons() {
            state_changed = true;
        }

        while let Ok(update) = badge_rx.try_recv() {
            let clean_id = update
                .desktop_id
                .trim_start_matches("application://")
                .trim_end_matches(".desktop")
                .to_lowercase();

            state.badges.insert(clean_id, update);
            state_changed = true;
        }

        if state_changed {
            state.needs_redraw = true;
        }

        // 4. ANIMATION: Step timers, opacity transitions, and active frame tickers
        let was_focus_performed = state.focus_action_performed;
        state.update_focus_timer();

        if was_focus_performed && !state.focus_action_performed {
            eprintln!("[DEBUG] Focus action state reset to false after 1s timeout");
        }

        state.fallback_anim.is_active = state.is_animating();
        let frame_advanced = state.fallback_anim.update();
        state.update_animations(&qh);

        for (app_id, anim) in state.animations.iter_mut() {
            if anim.update() {
                if let Some(frame) = anim.current_frame() {
                    state
                        .icon_cache
                        .insert(app_id.clone(), (frame.rgba.clone(), frame.width));
                    state.needs_redraw = true;
                }
            }
        }

        // --- HOVER & MENU FADE TICKING ---
        let hover_anim_active = state.docks.iter_mut().any(|d| d.hover_fade.tick());
        let menu_anim_active = state.docks.iter_mut().any(|d| d.menu_fade.tick());
        let window_list_anim_active = state.docks.iter_mut().any(|d| d.window_list_fade.tick());

        if state.menu_state.is_open && state.docks.iter().all(|d| d.menu_fade.is_fully_hidden()) {
            state.menu_state.is_open = false;
            state.needs_redraw = true;
        }

        // Do not destroy the window-list subsurface from the legacy hover-fade
        // cleanup path.  Window lists have their own state/fade lifecycle.
        // The old code cleared window_list_popup whenever hover_fade was hidden,
        // which caused the popup to be recreated repeatedly and produced visible
        // flicker while the window list was open.

        let apps_in_dock = state.get_apps_in_dock();
        let running_by_app = state.get_running_by_app();

        let scale_factor = state.scale_factor as f32;
        let screen_width = state.width;

        let dock_height = 60;
        let box_size = 48;
        let spacing = 8;
        let total_items = apps_in_dock.len() as i32;

        let calculated_width = (total_items * box_size + (total_items + 1) * spacing).min(800);
        let start_offset_x = if screen_width > calculated_width {
            (screen_width - calculated_width) / 2
        } else {
            0
        };

        let proximity_changed = library::interaction::update(
            &mut state,
            &apps_in_dock,
            &running_by_app,
            scale_factor,
            (dock_height, box_size, spacing, start_offset_x),
        );

        if proximity_changed {
            state.needs_redraw = true;
        }

        if hover_anim_active || menu_anim_active {
            state.needs_redraw = true;
        }

        // 5. RENDER: Pass visual snapshot to render functions
        if frame_advanced || state.needs_redraw {
            state.draw(&qh);
            state.needs_redraw = false;
            let _ = state.connection.flush();
        }

        // 6. POLL: Calculate dynamic timeout and wait for socket readiness
        let is_hide_animating =
            (state.hide_state.current_alpha - state.hide_state.target_alpha).abs() >= 0.001;
        let timeout_ms = if state.fallback_anim.is_active
            || is_hide_animating
            || hover_anim_active
            || menu_anim_active
            || window_list_anim_active
            || state.needs_redraw
        {
            15
        } else {
            50
        };

        let _ = state.connection.flush();
        if let Some(guard) = state.connection.prepare_read() {
            let raw_fd = std::os::unix::io::AsRawFd::as_raw_fd(&guard.connection_fd());
            let mut pfd = libc::pollfd {
                fd: raw_fd,
                events: libc::POLLIN,
                revents: 0,
            };

            let poll_res = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };

            if poll_res > 0 && (pfd.revents & libc::POLLIN) != 0 {
                let _ = guard.read();
            }
        }
    }
    Ok(())
}
