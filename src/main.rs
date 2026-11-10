//! Dock application entry point.
//!
//! Initializes all subsystems (Wayland, GPU, PipeWire, D-Bus) and runs the main
//! event loop that drives input processing, state updates, and frame rendering.

use library::init::{initialize_app, AppContext};

/// Application entry point.
///
/// # Initialization order
/// 1. Wayland connection and app state (via `init.rs`)
/// 2. GPU rendering via wgpu-api — falls back to CPU rendering
/// 3. PipeWire screen capture service with xdg-desktop-portal
/// 4. Background monitor task for dynamic window detection
/// 5. Main event loop (~60 FPS)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Catch panics and print the location so crashes can be diagnosed
    std::panic::set_hook(Box::new(|info| {
        if let Some(location) = info.location() {
            eprintln!(
                "[PANIC] {}:{}:{}",
                location.file(),
                location.line(),
                location.column()
            );
        }
        if let Some(msg) = info.payload().downcast_ref::<&str>() {
            eprintln!("[PANIC] Message: {}", msg);
        } else if let Some(msg) = info.payload().downcast_ref::<String>() {
            eprintln!("[PANIC] Message: {}", msg);
        }
        eprintln!("[PANIC] Full info: {:?}", info);
    }));

    // Initialize application state and event queue via init.rs
    let context = initialize_app()?;
    let AppContext {
        mut state,
        mut event_queue,
        qh,
        mut badge_rx,
        mut tray_rx,
        system_bar_rx,
        audio_rx,
        bluetooth_rx,
        rclone_rx,
        process_rx,
    } = context;

    // Keep the tokio runtime for GPU init, PipeWire and other async tasks
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| format!("Failed to create tokio runtime: {}", e))?;

    // GPU rendering via wgpu-api — initializes Vulkan device and queue
    match rt.block_on(wgpu_api::Renderer::new()) {
        Ok(renderer) => {
            state.gpu_context = Some(renderer);
            state.gpu_available = true;
            eprintln!("[GPU] Renderer initialized successfully");
        }
        Err(e) => {
            eprintln!("[GPU] Init failed: {}, falling back to CPU rendering", e);
            state.gpu_available = false;
        }
    }

    // Initialize PipeWire window capture service for the window list popup.
    //
    // The service captures live video frames from all open Wayland windows via
    // XDG Desktop Portal. Each window gets a dedicated stream thread that
    // continuously receives video buffers. The capture runs in the background
    // so the dock shows immediately even when the portal is slow to respond.
    //
    // Two tokio tasks are spawned:
    // 1. `init_capture_service` — one-time capture of all windows on startup
    // 2. `start_monitor_inner` — background monitor that adds/removes streams
    //    when windows open/close (reacts to probe_signal notifications)
    let pipewire_service =
        std::sync::Arc::new(library::services::pipewire_capture::PipewireCaptureService::new());
    pipewire_service.set_scale_factor(state.config.pipewire_scale_factor);
    let max_windows = 20;
    let (connection, desktop) = rt.block_on(async {
        use zbus::Proxy;
        let connection = match zbus::Connection::session().await {
            Ok(c) => c,
            Err(_e) => return (None, None),
        };
        let desktop = match Proxy::new(
            &connection,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.ScreenCast",
        )
        .await
        {
            Ok(d) => d,
            Err(_e) => return (None, None),
        };
        (Some(connection), Some(desktop))
    });
    // Store the service in AppState so the render thread can access frames.
    state.pipewire_capture = Some(pipewire_service.clone());

    // Start PipeWire capture and monitor in the background.
    // `init_capture_service` captures all windows once on startup.
    // `start_monitor_inner` runs indefinitely, reacting to window create/destroy events.
    if let (Some(conn), Some(dsk)) = (connection, desktop) {
        let init_service = pipewire_service.clone();
        let init_conn = conn.clone();
        let init_dsk = dsk.clone();
        rt.spawn(async move {
            let _ = library::services::pipewire_capture::init_capture_service(
                &init_service,
                &init_conn,
                &init_dsk,
                max_windows,
            )
            .await;
        });
        let monitor_service = pipewire_service.clone();
        rt.spawn(async move {
            library::services::pipewire_capture::start_monitor_inner(
                &monitor_service,
                conn,
                dsk,
                max_windows,
            )
            .await;
        });
    }

    // Resource monitor: prints CPU, RAM, VRAM usage every 2 seconds
    library::services::resource_monitor::start_resource_monitor();

    // --- MAIN EVENT LOOP ---
    loop {
        // 1. INPUT: Dispatch Wayland socket events from event queue
        if let Err(_e) = event_queue.dispatch_pending(&mut state) {
            break;
        }

        // 2. DOMAIN EVENTS: consume the queued listener events through the
        // temporary compatibility projection. This will later become the
        // resolver -> registry pipeline.
        if state.process_pending_events() {
            state.needs_redraw = true;
        }
        // If a Wayland/configure/domain event already requested a complete
        // frame, a simultaneous status update must not downgrade it to a
        // sidecar-only render.
        let full_redraw_was_already_pending = state.needs_redraw;

        // 3. INTERACTION: Re-enabled for click handling
        let is_near = state.check_dock_proximity();
        state.interaction.is_pointer_near = is_near;

        // 4. STATE UPDATE: Drain background channels (Icons & DBus) & update flags
        let mut state_changed = false;
        let mut status_sidebars_only = false;
        if state.process_loaded_icons() {
            state_changed = true;
        }

        while let Ok(update) = badge_rx.try_recv() {
            state.emit_event(library::events::DomainEvent::Badge(
                library::events::BadgeEvent::Update(update),
            ));
            state_changed = true;
        }

        while let Ok(snapshot) = tray_rx.try_recv() {
            if state.apply_tray_snapshot(snapshot) {
                status_sidebars_only = true;
            }
        }

        // Consume only the newest status-bar snapshot. The sampler runs off
        // the Wayland thread and emits only when displayed values change.
        let mut latest_system_bar = None;
        while let Ok(snapshot) = system_bar_rx.try_recv() {
            latest_system_bar = Some(snapshot);
        }
        if let Some(snapshot) = latest_system_bar {
            if state.system_bar != snapshot {
                state.system_bar = snapshot;
                for dock in &mut state.docks {
                    dock.status_strip_rendered = false;
                }
                // The bottom status strip is intentionally independent from
                // the dock fade and remains fully opaque while the dock is hidden.
                // Redraw it even during auto-hide so rates/time do not go stale.
                // This is a sidecar-only invalidation; don't rerender app icons.
                status_sidebars_only = true;
            }
        }

        let mut latest_audio = None;
        while let Ok(snapshot) = audio_rx.try_recv() {
            latest_audio = Some(snapshot);
        }
        if let Some(snapshot) = latest_audio {
            if state.audio_snapshot != snapshot {
                state.audio_snapshot = snapshot;
                for dock in &mut state.docks {
                    dock.status_strip_rendered = false;
                }
                // Refresh the always-visible player card even when its mixer
                // popup is closed, without repainting the app icon row.
                status_sidebars_only = true;
                if state.audio_panel_open && !state.hide_state.is_fully_hidden() {
                    state_changed = true;
                }
            }
        }

        let mut latest_bluetooth = None;
        while let Ok(snapshot) = bluetooth_rx.try_recv() {
            latest_bluetooth = Some(snapshot);
        }
        if let Some(snapshot) = latest_bluetooth {
            if state.bluetooth_snapshot != snapshot {
                state.bluetooth_snapshot = snapshot;
                for dock in &mut state.docks {
                    dock.status_strip_rendered = false;
                }
                status_sidebars_only = true;
                if state.bluetooth_panel_open && !state.hide_state.is_fully_hidden() {
                    state_changed = true;
                }
            }
        }

        let mut latest_rclone = None;
        while let Ok(snapshot) = rclone_rx.try_recv() {
            latest_rclone = Some(snapshot);
        }
        if let Some(snapshot) = latest_rclone {
            if state.rclone_snapshot != snapshot {
                state.rclone_snapshot = snapshot;
                if state.rclone_panel_open && !state.hide_state.is_fully_hidden() {
                    state_changed = true;
                }
                let page_count = (state.rclone_snapshot.mounts.len()
                    + library::render::rclone::ROWS_PER_PAGE
                    - 1)
                    / library::render::rclone::ROWS_PER_PAGE;
                state.rclone_page = state.rclone_page.min(page_count.saturating_sub(1));
            }
        }

        let mut latest_processes = None;
        while let Ok(snapshot) = process_rx.try_recv() {
            latest_processes = Some(snapshot);
        }
        if let Some(snapshot) = latest_processes {
            if state.process_snapshot != snapshot {
                state.process_snapshot = snapshot;
                if state.process_panel_open && !state.hide_state.is_fully_hidden() {
                    state_changed = true;
                }
                let page_count = (state.process_snapshot.processes.len()
                    + library::render::processes::ROWS_PER_PAGE
                    - 1)
                    / library::render::processes::ROWS_PER_PAGE;
                state.process_page = state.process_page.min(page_count.saturating_sub(1));
            }
        }

        if state_changed || status_sidebars_only {
            state.needs_redraw = true;
        }
        if state_changed || full_redraw_was_already_pending {
            // A domain/UI event or earlier Wayland configure may have changed
            // application-row content as well.
            status_sidebars_only = false;
        }

        // 5. INTERACTION/ANIMATION: interaction decides the new targets first;
        // then each animation clock is advanced exactly once.  Advancing fades
        // before interaction used to leave a newly-started fade waiting for the
        // next polling cycle and could make popups appear frozen.
        state.update_focus_timer();

        let mut frame_advanced = false;

        // Missing icons use a static fallback frame. Icon loading is background
        // work and must never turn the main thread into a periodic animation loop.

        let mut animated_apps_changed = false;
        for (app_id, anim) in state.animations.iter_mut() {
            if anim.update() {
                if let Some(frame) = anim.current_frame() {
                    state
                        .icon_cache
                        .insert(app_id.clone(), (frame.rgba.clone(), frame.width));
                    animated_apps_changed = true;
                }
            }
        }
        if animated_apps_changed {
            frame_advanced = true;
            state.needs_redraw = true;
            status_sidebars_only = false;
        }

        // --- HOVER & MENU STATE / TARGET UPDATE --- Re-enabled for clicks
        let apps_in_dock = state.get_apps_in_dock();
        if state.sync_dock_surface_width(apps_in_dock.len()) {
            status_sidebars_only = false;
        }
        let running_by_app = state.get_running_by_app();

        // Hide/show animations disabled
        // state.update_animations(&qh);

        // Animation flags all false — animations disabled
        let _hover_anim_active = false;
        let _menu_anim_active = false;
        let _window_list_anim_active = false;
        // state.finalize_popup_closures(); // disabled

        let _hide_animating = false;
        let _menu_fade_animating = false;
        let _window_list_fade_animating = false;

        // Interaction for click handling — re-enabled
        let scale_factor = state.scale_factor as f32;
        let screen_width = state
            .docks
            .first()
            .map(|dock| dock.width as i32)
            .unwrap_or(state.width);

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
            status_sidebars_only = false;
        }

        // Check if Slint animations are active — force redraw to advance animation frames
        let slint_animations_active = state
            .slint_renderer
            .as_ref()
            .map(|r| r.has_active_animation())
            .unwrap_or(false);
        if slint_animations_active {
            state.needs_redraw = true;
            status_sidebars_only = false;
        }

        // 6. RENDER: Pass visual snapshot to render functions.
        // The first committed buffer is mandatory for every configured surface;
        // popup-only animation must never suppress initial dock presentation.
        let initial_dock_frame_due = state
            .docks
            .iter()
            .any(|dock| dock.configured && dock.current_buffer.is_none());
        if frame_advanced || state.needs_redraw || initial_dock_frame_due {
            // Popup fades are independent subsurfaces. Once a main-dock buffer
            // exists, menu/window-list fades can present independently. Before
            // that first buffer, always render the complete dock surface.
            // All animations disabled — popup_only_animation always false
            let popup_only_animation = false;
            if initial_dock_frame_due {}
            state.draw(&qh, !popup_only_animation && !status_sidebars_only);
            // AppState::draw owns redraw invalidation. Do not clear the flag
            // here: a surface can still be awaiting its first layer configure.
            let _ = state.connection.flush();
        }

        // 6. POLL: sleep until Wayland input, a timer, or the next actual
        // animation frame.  Fixed 60 FPS target for smooth rendering.
        let mut timeout_ms: i32 = 16;

        let waiting_for_layer_configure =
            state.docks.is_empty() || state.docks.iter().any(|dock| !dock.configured);
        if state.needs_redraw && !waiting_for_layer_configure {
            timeout_ms = 0;
        }

        // Animation-based timeout adjustment disabled
        // hide_animating, menu_fade_animating, etc. are all false

        for anim in state.animations.values() {
            if anim.is_active {
                let ms = anim
                    .time_until_next_frame()
                    .as_millis()
                    .min(i32::MAX as u128) as i32;
                timeout_ms = timeout_ms.min(ms.max(0));
            }
        }

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
