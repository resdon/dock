//! Dock application initialization module.
//!
//! Sets up all Wayland globals, background monitors, and the initial `AppState`.
//!
//! # Initialization order
//! 1. Background monitors (Unity badges, system tray, system bar, audio, Bluetooth, rclone, processes)
//! 2. Wayland connection and global registry
//! 3. Compositor, output, layer-shell, shm, seat bindings
//! 4. Font manager and icon cache
//! 5. `AppState` construction with all fields
//! 6. Slint renderer initialization
//! 7. Optional protocol bindings (data device, toplevel manager, fractional scale)
//! 8. Layer surface creation for each output
//! 9. Roundtrip to dispatch initial layer-shell configure events

use crate::cache::persistence;
use crate::graphics::fade::FadeAnimation;
use crate::graphics::AutoHideState;
use crate::models::BadgeUpdate;
use crate::registry::SystemRegistry;
use crate::render::font::FontManager;
use crate::state::AppState;
use crate::types::*;

use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer, LayerShell};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::{
    compositor::CompositorState, output::OutputState, registry::RegistryState, seat::SeatState,
    shm::Shm,
};

use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::{Connection, Proxy, QueueHandle};

use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::thread;
use std::time::Instant;

/// Context returned by `initialize_app()` containing all initialized state and channels.
///
/// Passed to `main()` to start the event loop.
pub struct AppContext {
    /// Main application state — Wayland, GPU, UI, and interaction state
    pub state: AppState,
    /// Wayland event queue for dispatching socket events
    pub event_queue: wayland_client::EventQueue<AppState>,
    /// Queue handle for creating new Wayland proxies
    pub qh: QueueHandle<AppState>,
    /// Receiver for Unity launcher badge updates (async)
    pub badge_rx: tokio::sync::mpsc::UnboundedReceiver<BadgeUpdate>,
    /// Receiver for system tray icon snapshots (async)
    pub tray_rx: tokio::sync::mpsc::UnboundedReceiver<crate::models::TraySnapshot>,
    /// Receiver for system bar snapshots (clock, keyboard, network rates)
    pub system_bar_rx: std::sync::mpsc::Receiver<crate::models::SystemBarSnapshot>,
    /// Receiver for audio player snapshots (PipeWire/WirePlumber via wpctl).
    ///
    /// The audio monitor thread (`dock-pipewire-audio`) polls `wpctl status`
    /// every 1.5s and sends snapshots only when they change. The main loop
    /// consumes these snapshots and triggers sidecar-only redraws.
    pub audio_rx: std::sync::mpsc::Receiver<crate::models::AudioSnapshot>,
    /// Receiver for Bluetooth device snapshots (BlueZ D-Bus)
    pub bluetooth_rx: std::sync::mpsc::Receiver<crate::models::BluetoothSnapshot>,
    /// Receiver for rclone mount snapshots
    pub rclone_rx: std::sync::mpsc::Receiver<crate::models::RcloneSnapshot>,
    /// Receiver for top process snapshots (CPU, memory)
    pub process_rx: std::sync::mpsc::Receiver<crate::models::ProcessSnapshot>,
}

/// Initialize the entire dock application and return the ready-to-run context.
///
/// # Returns
/// `AppContext` with all state, channels, and the Wayland event queue.
pub fn initialize_app() -> Result<AppContext, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME").unwrap_or_default();
    let log_dir = PathBuf::from(format!("{}/.local/share/dock", home));
    let _ = fs::create_dir_all(&log_dir);

    // 1. Internal App Log Stream
    let app_log_path = log_dir.join("app.log");
    let mut app_logger = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&app_log_path)
        .expect("Failed to initialize app.log");

    writeln!(
        app_logger,
        "[{}] INFO: Dock application starting up.",
        Instant::now().elapsed().as_secs()
    )?;

    // 2. Startup icon indexing is background work. Do not run a periodic
    // mouse-position logger here: that 20 ms file-I/O loop is unrelated to the
    // dock and needlessly wakes a CPU core.

    crate::cache::icon_indexer::spawn_startup_indexer();

    let (badge_tx, badge_rx) = tokio::sync::mpsc::unbounded_channel();
    // Keep the async listener alive independently of initialize_app(). The old
    // code spawned onto a local Runtime and immediately dropped that Runtime,
    // which cancelled its task as soon as initialization returned.
    thread::spawn(move || {
        if let Ok(runtime) = tokio::runtime::Runtime::new() {
            runtime.block_on(async move {
                let _ = crate::listeners::start_unity_dbus_listener(badge_tx).await;
            });
        }
    });

    let (tray_tx, tray_rx) = tokio::sync::mpsc::unbounded_channel();
    let (tray_action_tx, tray_action_rx) = tokio::sync::mpsc::unbounded_channel();
    thread::spawn(move || {
        if let Ok(runtime) = tokio::runtime::Runtime::new() {
            runtime.block_on(async move {
                if let Err(e) =
                    crate::listeners::start_status_notifier_listener(tray_tx, tray_action_rx).await
                {
                }
            });
        }
    });

    let (system_bar_tx, system_bar_rx) = std::sync::mpsc::channel();
    crate::app::system_bar::spawn_system_bar_monitor(system_bar_tx);

    // PipeWire/WirePlumber audio integration is isolated from the Wayland thread.
    // `wpctl` and `playerctl` run in the dedicated monitor worker thread;
    // the render path only reads snapshots from the channel, never blocks on I/O.
    // Audio actions (volume/mute) flow in the opposite direction via audio_action_tx.
    let (audio_tx, audio_rx) = std::sync::mpsc::channel();
    let (audio_action_tx, audio_action_rx) = std::sync::mpsc::channel();
    crate::app::audio::spawn_audio_monitor(audio_tx, audio_action_rx);

    // BlueZ devices and adapter state are read over the system D-Bus on a
    // dedicated worker; no D-Bus roundtrip can block the Wayland event loop.
    let (bluetooth_tx, bluetooth_rx) = std::sync::mpsc::channel();
    let (bluetooth_action_tx, bluetooth_action_rx) = tokio::sync::mpsc::unbounded_channel();
    crate::app::bluetooth::spawn_bluetooth_monitor(bluetooth_tx, bluetooth_action_rx);

    // The rclone and process panels are demand-driven. Their monitors remain
    // asleep while the popups are hidden, then refresh only while a panel is open.
    let (rclone_tx, rclone_rx) = std::sync::mpsc::channel();
    let (rclone_monitor_tx, rclone_monitor_rx) = std::sync::mpsc::channel();
    crate::app::rclone::spawn_rclone_monitor(rclone_tx, rclone_monitor_rx);

    let (process_tx, process_rx) = std::sync::mpsc::channel();
    let (process_monitor_tx, process_monitor_rx) = std::sync::mpsc::channel();
    crate::app::process_monitor::spawn_process_monitor(process_tx, process_monitor_rx);

    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland display");
    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn)
        .expect("Failed to initialize Wayland registry queue");

    let qh = event_queue.handle();
    let registry_state = RegistryState::new(&globals);
    let compositor_state = CompositorState::bind(&globals, &qh).expect("Failed to bind compositor");
    let output_state = OutputState::new(&globals, &qh);
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr_layer_shell required");
    let shm_state = Shm::bind(&globals, &qh).expect("wl_shm required");
    let seat_state = SeatState::new(&globals, &qh);

    let subcompositor = registry_state
        .bind_one::<WlSubcompositor, _, _>(&qh, 1..=1, ())
        .expect("wp_subcompositor not available");

    let pool = SlotPool::new(1024 * 1024 * 16, &shm_state).expect("Failed to create memory pool");

    let user_data_font = format!("{}/.local/share/dock/font.ttf", home);
    let font_path = [
        PathBuf::from("assets/font.ttf"),
        PathBuf::from("font.ttf"),
        PathBuf::from(user_data_font),
        PathBuf::from("/usr/share/dock/font.ttf"),
        PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .expect("No valid font file found!");

    let pinned_vector = persistence::load_pinned_apps();
    let mut permanent_icon_cache = HashMap::new();
    for app_id in &pinned_vector {
        if let Some((rgba, size)) = crate::cache::load_cached_icon(app_id) {
            permanent_icon_cache.insert(app_id.clone(), (rgba, size));
        }
    }

    let (icon_tx, icon_rx) = std::sync::mpsc::channel();
    let icon_loader = crate::app::icon_load::IconLoader::new(icon_tx.clone());

    let mut state = AppState {
        qh: qh.clone(),
        connection: conn.clone(),
        registry_state,
        compositor_state,
        output_state,
        layer_shell,
        shm_state,
        pool,
        seat_state,
        layer_surface: None,
        current_buffer: None,
        width: 100,
        height: 60,
        toplevel_manager: None,
        font_manager: FontManager::from_file(&font_path).expect("Failed to memory-map font file"),
        wl_seat: None,
        wl_pointer: None,
        system_registry: SystemRegistry::new(),
        window_handles: HashMap::new(),
        output_handles: HashMap::new(),
        dnd_offers: HashMap::new(),
        pending_events: std::collections::VecDeque::new(),
        pinned_apps: pinned_vector,
        icon_cache: permanent_icon_cache,
        pending_icon_searches: std::collections::HashSet::new(),
        icon_rx,
        icon_tx,
        menu_state: MenuState {
            x: 0,
            y: 0,
            target_window: None,
            target_app_id: None,
            target_tray_item: None,
            anchor_override: None,
            is_open: false,
            items: Vec::new(),
            opened_by_button: None,
            waiting_for_initial_release: false,
            just_opened: false,
            fade: FadeAnimation::new(0.0, 1.0, crate::config::default_popup_fade_in()),
            consecutive_false_count: 0,
            cursor_moved: false,
            last_pointer_x: 0.0,
            last_pointer_y: 0.0,
            last_debug_print: std::time::Instant::now(),
            consecutive_no_motion_count: 0,
            consecutive_on_dock_count: 0,
            consecutive_on_context_menu_count: 0,
            consecutive_on_window_list_count: 0,
            no_motion_timer: None,
            dock_timer: None,
            context_menu_timer: None,
            window_list_timer: None,
            pointer_inside_popup: false,
            popup_timer: None,
        },
        hover_state: HoverState {
            x: 0,
            app_id: None,
            is_visible: false,
            last_leave_time: None,
        },
        last_interact_time: Instant::now(),
        // The first frame must be explicitly requested. Waiting for a Wayland
        // input event here makes a newly started dock appear frozen.
        needs_redraw: true,
        last_mouse_pos: None,
        is_dragging: false,
        drag_start_x: 0,
        drag_start_y: 0,
        dragged_app_id: None,
        last_drag_draw: Instant::now(),
        sys_scanner: sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_processes(sysinfo::ProcessRefreshKind::everything()),
        ),
        last_process_scan: Instant::now() - std::time::Duration::from_secs(1),
        current_output: None,
        docks: Vec::new(),
        fractional_scale_manager: None,
        fractional_scale_notifier: None,
        scale_factor: 1.0,
        dnd_state: DndState::default(),
        dnd_protocol: crate::types::WaylandDndProtocolState::default(),
        resolver_pipeline: crate::resolvers::ResolverPipeline::default(),
        media_state: crate::types::MediaWindowState::default(),
        system: crate::state::SystemState::default(),
        applications: HashMap::new(),
        windows: HashMap::new(),
        processes: HashMap::new(),
        outputs: HashMap::new(),
        interaction_runtime: crate::state::InteractionRuntimeState::default(),
        dock_runtime: crate::state::DockRuntimeState::default(),
        window_list_runtime: crate::state::WindowListRuntimeState::default(),
        context_menu_runtime: crate::state::ContextMenuRuntimeState::default(),
        telemetry: crate::state::TelemetryState::default(),
        animation_state: crate::state::AnimationState::default(),
        media: crate::state::MediaState::default(),
        data_device_manager: None,
        data_device: None,
        badges: HashMap::new(),
        tray_items: Vec::new(),
        tray_action_tx: tray_action_tx.clone(),
        system_bar: crate::models::SystemBarSnapshot::default(),
        audio_snapshot: crate::models::AudioSnapshot::default(),
        audio_action_tx,
        audio_panel_open: false,
        audio_slider_drag: None,
        bluetooth_snapshot: crate::models::BluetoothSnapshot::default(),
        bluetooth_action_tx,
        bluetooth_panel_open: false,
        rclone_snapshot: crate::models::RcloneSnapshot::default(),
        rclone_monitor_tx,
        rclone_panel_open: false,
        rclone_page: 0,
        process_snapshot: crate::models::ProcessSnapshot::default(),
        process_monitor_tx,
        process_panel_open: false,
        process_page: 0,
        process_sort: crate::models::ProcessSort::Cpu,
        subcompositor: Some(subcompositor),
        icon_load: icon_loader,
        animations: HashMap::new(),
        hide_state: AutoHideState::new(),
        interaction: InteractionState::new(),
        focus_action_performed: true,
        focus_action_time: Some(Instant::now()),
        window_list_state: WindowListState::default(),
        gpu_context: None,
        dmabuf_state: smithay_client_toolkit::dmabuf::DmabufState::new(&globals, &qh),
        config: crate::config::DockConfig::load(),
        slint_renderer: None,
        bounce_app_id: None,
        click_app_id: None,
        pending_focus_app_id: None,
        minimize_app_id: None,
        pipewire_capture: None,
    };

    // Initialize Slint renderer (software renderer - direct buffer rendering)
    state.slint_renderer = match crate::render::slint_dock::SlintDockRenderer::new() {
        Ok(renderer) => {
            Some(renderer)
        }
        Err(e) => {
            None
        }
    };

    state.data_device_manager = state
        .registry_state
        .bind_one::<WlDataDeviceManager, _, _>(&qh, 1..=3, ())
        .ok();

    state.toplevel_manager = state
        .registry_state
        .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
        .ok();

    state.fractional_scale_manager = state
        .registry_state
        .bind_one::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
        .ok();

    event_queue.roundtrip(&mut state).unwrap();
    state.sync_system_registry();

    let available_outputs: Vec<_> = state.output_state.outputs().collect();

    for output in available_outputs {
        let raw_surface = state.compositor_state.create_surface(&qh);
        let layer_surface = state.layer_shell.create_layer_surface(
            &qh,
            raw_surface,
            Layer::Top,
            Some("dock_panel"),
            Some(&output),
        );

        let scale_notifier = state.fractional_scale_manager.as_ref().map(|manager| {
            manager.get_fractional_scale(layer_surface.wl_surface(), &qh, output.clone())
        });

        // The layer surface width belongs only to the app row. The SNI tray and
        // four utility widgets are independent sidecar subsurfaces, while the
        // network/keyboard/clock strip is attached below the app row.
        let app_count = state.get_apps_in_dock().len();
        let dock_width = crate::render::tray::preferred_surface_width(app_count);
        let dock_height = state.config.dock_height;
        // Side bars are disabled; surface height equals dock height only.
        let surface_height = dock_height;
        let scale_factor = 1.0;

        layer_surface.set_keyboard_interactivity(
            smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None,
        );
        // Layer-shell size is in logical coordinates. Buffer dimensions are
        // calculated separately from the negotiated scale during rendering.
        layer_surface.set_size(dock_width, surface_height);
        // Overlay mode: keep the dock above normal windows without reserving
        // any work area. Do not change this on show/hide transitions.
        layer_surface.set_exclusive_zone(0);

        // Set anchor based on config
        let anchor = match state.config.anchor {
            crate::config::DockAnchor::Bottom => Anchor::BOTTOM,
            crate::config::DockAnchor::Top => Anchor::TOP,
            crate::config::DockAnchor::Left => Anchor::LEFT,
            crate::config::DockAnchor::Right => Anchor::RIGHT,
        };
        layer_surface.set_anchor(anchor);

        // Set margin for floating dock effect
        let margin = state.config.margin;
        match state.config.anchor {
            crate::config::DockAnchor::Bottom => {
                layer_surface.set_margin(0, 0, margin, 0);
            }
            crate::config::DockAnchor::Top => {
                layer_surface.set_margin(margin, 0, 0, 0);
            }
            crate::config::DockAnchor::Left => {
                layer_surface.set_margin(0, 0, 0, margin);
            }
            crate::config::DockAnchor::Right => {
                layer_surface.set_margin(0, margin, 0, 0);
            }
        }

        // Anchor configured
        // First commit requests the initial layer-shell configure. A roundtrip
        // after all surfaces are created below guarantees the configure is
        // dispatched before the main loop's first render attempt.
        layer_surface.wl_surface().commit();

        state.docks.push(DockInstance {
            surface: layer_surface,
            output,
            width: dock_width,
            requested_width: dock_width,
            height: surface_height,
            current_buffer: None,
            scale_notifier,
            scale_factor,
            hover_popup: None,
            menu_popup: None,
            window_list_popup: None,
            audio_popup: None,
            bluetooth_popup: None,
            rclone_popup: None,
            process_popup: None,
            tray_bar: None,
            status_widget_bar: None,
            audio_player_bar: None,
            audio_player_expanded: false,
            system_status_bar: None,
            configured: false,
            status_strip_rendered: false,
            context_menu_popup: None,
            dock_state: None,
            // No popup exists at startup. Start popup/hover fades fully hidden
            // so the interaction loop does not schedule an unnecessary initial
            // fade-out and suppress the first main-dock framebuffer.
            hover_fade: FadeAnimation::new_hidden(0.0, 1.0, 1.0),
            menu_fade: FadeAnimation::new_hidden(0.0, 1.0, 1.0),
            window_list_fade: FadeAnimation::new_hidden(0.0, 1.0, 1.0),
            active_hovered_app: None,
        });
    }

    if state.docks.is_empty() {
        // No layer surfaces created
    } else {
        // Do not let the first draw race the compositor's initial configure.
        // Without this, draw() can skip unconfigured docks and clear the
        // initial needs_redraw flag before the configure event is handled.
        event_queue.roundtrip(&mut state)?;
        let _ = state.process_pending_events();
        let configured = state.docks.iter().filter(|dock| dock.configured).count();
        state.needs_redraw = true;
    }

    Ok(AppContext {
        state,
        event_queue,
        qh,
        badge_rx,
        tray_rx,
        system_bar_rx,
        audio_rx,
        bluetooth_rx,
        rclone_rx,
        process_rx,
    })
}
