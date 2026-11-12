//! Dock application entry point.
//!
//! Initializes all subsystems (Wayland, GPU, PipeWire, D-Bus) and runs the main
//! event loop that drives input processing, state updates, and frame rendering.

use library::init::{initialize_app, AppContext};
use std::sync::Arc;

// Tracy profiling zones — only active when built with `--features tracy`.
// **FIX (v1.5.5)**: Added Tracy zone instrumentation to the main event loop
// for CPU timeline profiling. Each zone corresponds to a major loop phase.
// Uses the `span!` macro which creates a scoped span that ends on drop.

/// Application entry point.
///
/// # Initialization order
/// 1. Wayland connection and app state (via `init.rs`)
/// 2. GPU rendering via wgpu-api — falls back to CPU rendering
/// 3. PipeWire screen capture service with xdg-desktop-portal
/// 4. Background monitor task for dynamic window detection
/// 5. Main event loop (~60 FPS)
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing so that tracing::info!, tracing::warn!, etc. produce output.
    // Can be overridden by RUST_LOG env var (e.g., RUST_LOG=info ./target/release/dock).
    library::error::init_tracing("dock=info,wgpu_api=warn");

    // DHAT heap profiler — only active when built with `--features dhat-heap`.
    // **FIX (v1.5.5)**: Added DHAT profiler for allocation pattern analysis.
    // Writes dhat-heap.json on exit for viewing in the DHAT web viewer.
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

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

    // GPU rendering via wgpu-api — initializes Vulkan device and queue.
    // The GPU backend is available for features like popup rendering and visual effects.
    // Main dock rendering uses Slint CPU → SHM (correct for headless Wayland).
    // Full GPU rendering requires DMA-BUF export to present frames to the compositor.
    match rt.block_on(wgpu_api::Renderer::new()) {
        Ok(renderer) => {
            state.gpu_context = Some(renderer);
            state.gpu_available = true;
        }
        Err(e) => {
            eprintln!("[main] GPU renderer unavailable, falling back to CPU path: {e}");
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

    // Resource monitor: prints CPU, RAM, VRAM, FPS usage every 2 seconds
    library::services::resource_monitor::start_resource_monitor();

    // FIX (v1.5.5): Frame scheduler enforces 75 FPS animation limit and eliminates
    // unnecessary redraws by tracking whether visual content actually changed.
    let mut frame_scheduler = library::frame_scheduler::FrameScheduler::new();

    let window_list_refresh_interval = std::time::Duration::from_millis(33);
    let mut last_window_list_refresh = std::time::Instant::now();

    // --- MAIN EVENT LOOP ---
    loop {
        // 1. INPUT: Dispatch Wayland socket events from event queue
        #[cfg(feature = "tracy")]
        let _zone_wayland = tracy_client::span!("Wayland Events");
        if let Err(_e) = event_queue.dispatch_pending(&mut state) {
            break;
        }
        if state.window_list_state.is_open
            && last_window_list_refresh.elapsed() >= window_list_refresh_interval
        {
            state.needs_redraw = true;
            frame_scheduler.mark_content_changed();
        }

        // 2. DOMAIN EVENTS: consume the queued listener events through the
        // temporary compatibility projection. This will later become the
        // resolver -> registry pipeline.
        #[cfg(feature = "tracy")]
        let _zone_domain = tracy_client::span!("Domain Events");
        if state.process_pending_events() {
            state.needs_redraw = true;
            frame_scheduler.mark_content_changed();
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
            frame_scheduler.mark_content_changed();
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
        #[cfg(feature = "tracy")]
        let _zone_animation = tracy_client::span!("Animation Update");
        state.update_focus_timer();

        let mut frame_advanced = false;

        // Missing icons use a static fallback frame. Icon loading is background
        // work and must never turn the main thread into a periodic animation loop.

        let mut animated_apps_changed = false;
        for (app_id, anim) in state.animations.iter_mut() {
            if anim.update() {
                if let Some(frame) = anim.current_frame() {
                    // FIX (v1.5.5): Store animated icon frame as Arc<[u8]> so
                    // the cache shares data with Slint and GPU paths.
                    state
                        .icon_cache
                        .insert(app_id.clone(), (Arc::from(frame.rgba.as_slice()), frame.width));
                    animated_apps_changed = true;
                }
            }
        }
        if animated_apps_changed {
            frame_advanced = true;
            state.needs_redraw = true;
            frame_scheduler.mark_content_changed();
            status_sidebars_only = false;
        }

        // --- HOVER & MENU STATE / TARGET UPDATE --- Re-enabled for clicks
        let apps_in_dock = state.get_apps_in_dock_cached();
        if state.sync_dock_surface_width(apps_in_dock.len()) {
            status_sidebars_only = false;
        }
        let running_by_app = state.get_running_by_app_cached();

        // Advance hide/show and popup lifecycle state transitions.
        state.update_animations(&qh);
        if state.finalize_popup_closures() {
            state.needs_redraw = true;
            frame_scheduler.mark_content_changed();
            status_sidebars_only = false;
        }

        let menu_fade_animating = state.docks.iter().any(|dock| {
            (dock.menu_fade.current_alpha - dock.menu_fade.target_alpha).abs() > f32::EPSILON
                && !dock.menu_fade.is_fully_hidden()
        });
        let window_list_fade_animating = state.docks.iter().any(|dock| {
            (dock.window_list_fade.current_alpha - dock.window_list_fade.target_alpha).abs()
                > f32::EPSILON
                && !dock.window_list_fade.is_fully_hidden()
        });

        // Interaction for click handling — re-enabled
        let scale_factor = state.scale_factor as f32;
        let screen_width = state
            .docks
            .first()
            .map(|dock| dock.width as i32)
            .unwrap_or(state.width);

        let dock_height = state.config.dock_height as i32;
        let box_size = state.config.icon_size as i32;
        let spacing = state.config.spacing as i32;
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
            // **FIX (v1.5.5)**: Only set needs_redraw for hover state changes
            // (icon entered/left), not for every interaction::update() call.
            // The compositor sends pointer events continuously, and interaction::update()
            // may return true for timer/fade ticks even during idle. Without this guard,
            // the render loop spins at ~800 FPS.
            let hover_changed = state.interaction.hovered_icon != state.interaction.last_hovered_icon;
            if hover_changed {
                state.interaction.last_hovered_icon = state.interaction.hovered_icon;
                state.needs_redraw = true;
                frame_scheduler.mark_content_changed();
            }
            // **FIX**: interaction::update() can return true for click events that
            // toggle popups (Bluetooth, context menu, etc.) without changing the hover
            // icon. In that case, needs_redraw is set by the click handler, but
            // mark_content_changed() was never called, so the frame scheduler would
            // skip the draw. Mark content changed whenever proximity_changed is true
            // and needs_redraw was set, to ensure popup toggles render immediately.
            if state.needs_redraw {
                frame_scheduler.mark_content_changed();
            }
            // **FIX**: When the cursor moves over the window list popup, interaction::update()
            // returns layer_changed=true but the hover icon doesn't change, so needs_redraw
            // was never set. This caused set_active_row() in the draw function to never be
            // called, so the PipeWire active stream stayed at row 0 and all streams were
            // treated as inactive (1 frame/500ms). Now we trigger a redraw when the window
            // list popup is open and the cursor moved over it.
            if state.window_list_state.is_open && state.window_list_state.cursor_moved {
                state.needs_redraw = true;
                frame_scheduler.mark_content_changed();
            }
            status_sidebars_only = false;
        }

        // **FIX (v1.5.5)**: Tick animation counters each loop iteration so they
        // decay even when the render path is skipped by the frame scheduler.
        // Without this, has_active_animation() returns true forever, causing ~1000 FPS spinning.
        if let Some(ref mut slint_renderer) = state.slint_renderer {
            slint_renderer.tick_animations();
        }
        library::app::dock::tick_gpu_animations();

        // Check if Slint animations are active — trigger animation-driven redraws
        // at the frame scheduler's 75 FPS limit, not immediately.
        let slint_animations_active = state
            .slint_renderer
            .as_ref()
            .map(|r| r.has_active_animation())
            .unwrap_or(false);
        if slint_animations_active {
            state.needs_redraw = true;
            status_sidebars_only = false;
        }

        // **FIX (v1.5.5)**: When the Bluetooth popup is open and scanning, the
        // radar animation needs continuous redraws even though the snapshot hasn't
        // changed. Without this, the radar animation freezes because the frame
        // scheduler skips renders when no content change is detected.
        let bluetooth_scanning_active = state
            .bluetooth_panel_open
            && state.bluetooth_snapshot.discovering;
        if bluetooth_scanning_active {
            state.needs_redraw = true;
            status_sidebars_only = false;
        }

        // FIX (v1.5.5): Frame scheduler enforces 75 FPS animation limit and tracks
        // whether visual content actually changed. This replaces the previous logic
        // that ran at up to ~125 FPS (8ms poll timeout) whenever needs_redraw was true.
        let animations_active = menu_fade_animating
            || window_list_fade_animating
            || slint_animations_active
            || bluetooth_scanning_active
            || animated_apps_changed
            || state.bounce_app_id.is_some()
            || state.click_app_id.is_some()
            || state.minimize_app_id.is_some();
        frame_scheduler.set_animations_active(animations_active);

        // 6. RENDER: Pass visual snapshot to render functions.
        // The first committed buffer is mandatory for every configured surface;
        // popup-only animation must never suppress initial dock presentation.
        #[cfg(feature = "tracy")]
        let _zone_render = tracy_client::span!("Render");
        // **FIX (v1.5.5)**: Check both current_buffer (SHM path) and
        // first_gpu_buffer_committed (DMABUF path) to determine if the
        // initial frame has been rendered. Without this, the GPU path never
        // sets current_buffer, so initial_dock_frame_due stays true forever.
        let initial_dock_frame_due = state
            .docks
            .iter()
            .any(|dock| dock.configured && dock.current_buffer.is_none() && !dock.first_gpu_buffer_committed);

        // FIX (v1.5.5): Use frame scheduler to decide whether to render.
        // Content changes trigger immediate render; animation-driven redraws
        // are capped at 75 FPS. When nothing changed, the draw is skipped entirely.
        let needs_any_redraw = frame_advanced
            || state.needs_redraw
            || initial_dock_frame_due
            || menu_fade_animating
            || window_list_fade_animating;

        // **FIX (v1.5.5)**: initial_dock_frame_due should NOT bypass the frame
        // scheduler. If the scheduler skips a render, the initial buffer will be
        // drawn on the next wakeup. Bypassing caused ~800 FPS spinning because
        // current_buffer was never set (draw was skipped), so initial_dock_frame_due
        // stayed true forever, creating an infinite render loop.
        if initial_dock_frame_due {
            frame_scheduler.mark_content_changed();
        }
        let should_draw = frame_scheduler.should_render(needs_any_redraw);

        if should_draw {
            #[cfg(feature = "tracy")]
            let _zone_draw = tracy_client::span!("Draw");
            // Popup fades are independent subsurfaces. Once a main-dock buffer
            // exists, menu/window-list fades can present independently. Before
            // that first buffer, always render the complete dock surface.
            let popup_only_animation = !initial_dock_frame_due
                && !frame_advanced
                && !full_redraw_was_already_pending
                && !proximity_changed
                && !slint_animations_active
                && !animated_apps_changed
                && (menu_fade_animating || window_list_fade_animating);
            state.draw(&qh, !popup_only_animation && !status_sidebars_only);
            frame_scheduler.on_frame_rendered();
            // **FIX (v1.5.5)**: Clear needs_redraw after drawing to prevent
            // the render loop from spinning at ~9,440 FPS. The flag was only
            // ever set to true, never cleared, so once set it stayed true forever.
            state.needs_redraw = false;
            if state.window_list_state.is_open {
                last_window_list_refresh = std::time::Instant::now();
            }
            library::services::resource_monitor::increment_frame_counter();
            let _ = state.connection.flush();
        }

        // 6. POLL: sleep until Wayland input, a timer, or the next actual
        // animation frame.  When idle, sleep longer to reduce CPU usage.
        // FIX (v1.5.5): Use frame scheduler to compute the optimal poll timeout.
        // This replaces the fixed 8ms timeout that allowed ~125 FPS spinning.
        #[cfg(feature = "tracy")]
        let _zone_poll = tracy_client::span!("Poll");
        let waiting_for_layer_configure =
            state.docks.is_empty() || state.docks.iter().any(|dock| !dock.configured);
        let mut timeout_ms: i32 = if waiting_for_layer_configure {
            // Wait for layer configure — don't render yet
            16
        } else if state.window_list_state.is_open {
            // Window list open — use frame scheduler for rate limiting, but cap
            // at ~30 FPS minimum so PipeWire frames update smoothly.
            // **FIX (v1.5.5)**: Previously hardcoded to 1-16ms, bypassing the
            // frame scheduler entirely and causing ~780 FPS spinning.
            let scheduler_timeout = frame_scheduler.poll_timeout(needs_any_redraw, animations_active);
            let ms = window_list_refresh_interval
                .saturating_sub(last_window_list_refresh.elapsed())
                .as_millis()
                .min(i32::MAX as u128) as i32;
            // Use the frame scheduler timeout when idle, but refresh for PipeWire frames when due
            scheduler_timeout.min(ms.clamp(1, 33))
        } else {
            // FIX (v1.5.5): Frame scheduler determines the timeout based on:
            // - Content changes: immediate wakeup
            // - Active animations: time until next frame at 75 FPS
            // - Idle: 1 second sleep
            frame_scheduler.poll_timeout(needs_any_redraw, animations_active)
        };

        // Keep wakeups responsive while popup fades are in progress.
        if menu_fade_animating || window_list_fade_animating {
            timeout_ms = timeout_ms.min(16);
        }

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

            // Safe wrapper around libc::poll with proper EINTR handling.
            // Retries on EINTR to avoid missing Wayland events.
            loop {
                let poll_res = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
                if poll_res == -1 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::Interrupted {
                        // EINTR: retry the poll
                        continue;
                    }
                    // Other errors: log and skip this iteration
                    tracing::warn!("poll failed: {}", err);
                    break;
                }
                if poll_res > 0 && (pfd.revents & libc::POLLIN) != 0 {
                    let _ = guard.read();
                }
                break;
            }
        }
    }
    Ok(())
}
