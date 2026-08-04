pub mod app;
pub mod cache;
pub mod graphics;
pub mod handlers;
pub mod listeners;
pub mod render;
pub mod resolvers;


use app::AppState;

pub use dockman_lib::models;
pub use dockman_lib::icon_utils;
pub use dockman_lib::terminal_graphics;
pub use dockman_lib::get_icon_path;

use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::{RegistryState},
    seat::SeatState,
    shm::Shm,
};
use smithay_client_toolkit::shell::wlr_layer::{Anchor, Layer, LayerShell};
use smithay_client_toolkit::shm::slot::SlotPool;

use wayland_client::Connection;
use wayland_client::protocol::wl_data_device_manager::WlDataDeviceManager;
use wayland_client::protocol::wl_subcompositor::WlSubcompositor;
use wayland_client::globals::registry_queue_init;

use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use std::collections::HashMap;
use std::path::PathBuf;
use std::fs::File;
use std::io::{BufRead, BufReader};

use libc; // for loop and animation

use crate::cache::persistence;
use crate::graphics::AutoHideState;
use crate::render::font::FontManager;

pub use dockman_lib::DesktopAction;
use dockman_lib::listeners::start_unity_dbus_listener;

use app::types::*;

/// Loads icon_list.txt directly into a fast RAM lookup table ($O(1)$)
fn load_icon_index_map() -> HashMap<String, PathBuf> {
    let mut map = HashMap::new();
    let home = std::env::var("HOME").unwrap_or_default();
    
    let candidate_paths = [
        PathBuf::from(format!("{}/.cache/dockman/icon_list.txt", home)),
        PathBuf::from("./icon_list.txt"),
        PathBuf::from(format!("{}/.local/share/dock/icon_list.txt", home)),
        PathBuf::from("/usr/share/dock/icon_list.txt"),
    ];

    let Some(list_path) = candidate_paths.into_iter().find(|p| p.exists()) else {
        return map;
    };

    if let Ok(file) = File::open(list_path) {
        let reader = BufReader::new(file);
        for line in reader.lines().flatten() {
            let path = PathBuf::from(&line);
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                let key = stem.to_lowercase();
                // Prefer PNG icons when duplicate names exist
                if !map.contains_key(&key) || line.to_lowercase().ends_with(".png") {
                    map.insert(key, path);
                }
            }
        }
    }
    map
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Kick off background indexer
    crate::cache::icon_indexer::spawn_startup_indexer();

    // 2. Pre-load any existing icon index from disk into memory
    let _icon_map = load_icon_index_map();

    // 3. Setup DBus Channel
    let (badge_tx, mut badge_rx) = tokio::sync::mpsc::unbounded_channel();
    
    // 4. Spawn async DBus listener
    let runtime = tokio::runtime::Runtime::new()?;
    runtime.spawn(async move {
        let _ = start_unity_dbus_listener(badge_tx).await;
    });

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
    // Bind fractional scale manager after registry_state and qh are available
    let _fractional_scale_manager: Option<WpFractionalScaleManagerV1> = registry_state.bind_one(&qh, 1..=1, ()).ok();
    let subcompositor = registry_state
        .bind_one::<WlSubcompositor, _, _>(&qh, 1..=1, ())
        .expect("wp_subcompositor not available");

    let pool = SlotPool::new(1024 * 1024 * 16, &shm_state).expect("Failed to create memory pool");
    
    let home = std::env::var("HOME").unwrap_or_default();
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

    let anim_dir = [
        PathBuf::from("assets/24"),
        PathBuf::from(format!("{}/.local/share/dock/24", home)),
        PathBuf::from("/usr/share/dock/24"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .unwrap_or_else(|| PathBuf::from("assets/24"));

    let fallback_anim = dockman_lib::animations::IconAnimation::new(
        anim_dir.to_str().unwrap_or("assets/24"),
        48,
        24,
    );

    let pinned_vector = persistence::load_pinned_apps();
    let mut permanent_icon_cache = HashMap::new();
    for app_id in &pinned_vector {
        if let Some((rgba, size)) = cache::load_cached_icon(app_id) {
            permanent_icon_cache.insert(app_id.clone(), (rgba, size));
        }
    }

    // Channel for background icon loading
    let (icon_tx, icon_rx) = std::sync::mpsc::channel();
    let icon_loader = app::icon_load::IconLoader::new(icon_tx.clone());

    let mut state = AppState {
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
        pointer_x: 0,
        pointer_y: 0,
        open_windows: HashMap::new(),
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
            is_open: false,
            items: Vec::new(),
        },
        hover_state: HoverState {
            x: 0,
            app_id: None,
            is_visible: false,
            last_leave_time: None,
        },
        last_interact_time: std::time::Instant::now(),
        needs_redraw: false,
        last_mouse_pos: None,
        is_dragging: false,
        drag_start_x: 0,
        drag_start_y: 0,
        dragged_app_id: None,
        last_drag_draw: std::time::Instant::now(),
        sys_scanner: sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::everything()),
        ),
        current_output: None,
        docks: Vec::new(),
        fractional_scale_manager: None,
        fractional_scale_notifier: None,
        scale_factor: 1.0,
        fallback_anim,
        dnd_state: DndState::default(),
        data_device_manager: None,
        data_device: None,
        badges: HashMap::new(),
        subcompositor,
        icon_load: icon_loader,
        animations: HashMap::new(),
        hide_state: AutoHideState::new(),
        is_pointer_inside: false,
    };

    state.data_device_manager = state.registry_state
        .bind_one::<WlDataDeviceManager, _, _>(&qh, 1..=3, ())
        .ok();

    state.toplevel_manager = state.registry_state
        .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
        .ok();

    state.fractional_scale_manager = state.registry_state
        .bind_one::<WpFractionalScaleManagerV1, _, _>(&qh, 1..=1, ())
        .ok();

    event_queue.roundtrip(&mut state).unwrap();

    for output in state.output_state.outputs() {
        let raw_surface = state.compositor_state.create_surface(&qh);
        let layer_surface = state.layer_shell.create_layer_surface(
            &qh,
            raw_surface,
            Layer::Top,
            Some("dock_panel"),
            Some(&output),
        );

        let scale_notifier = if let Some(ref manager) = state.fractional_scale_manager {
            Some(manager.get_fractional_scale(layer_surface.wl_surface(), &qh, output.clone()))
        } else {
            None
        };

        // Define physical dimensions prior to setting surface size
        let dock_width = 540;
        let dock_height = 60;
        let scale_factor = 1.0;
        let phys_width = (dock_width as f64 * scale_factor).round() as u32;
        let phys_height = (dock_height as f64 * scale_factor).round() as u32;

        layer_surface.set_keyboard_interactivity(
            smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None
        );
        layer_surface.set_size(phys_width, phys_height);
        layer_surface.set_anchor(Anchor::BOTTOM);
        layer_surface.wl_surface().commit();

        state.docks.push(DockInstance {
            surface: layer_surface,
            output,
            width: dock_width,
            height: dock_height,
            current_buffer: None,
            scale_notifier,
            scale_factor,
            hover_popup: None,
            menu_popup: None,
        });
    }

    // --- MAIN EVENT LOOP ---
    loop {
        // 1. Dispatch any events already in the client queue
        if let Err(e) = event_queue.dispatch_pending(&mut state) {
            eprintln!("[WARN] Dispatch error: {}", e);
            break;
        }

        // 2. Drain background channels (Icons & DBus Badges)
        if state.process_loaded_icons() {
            state.needs_redraw = true;
        }

        while let Ok(update) = badge_rx.try_recv() {
            let clean_id = update.desktop_id
                .trim_start_matches("application://")
                .trim_end_matches(".desktop")
                .to_lowercase();

            state.badges.insert(clean_id, update);
            state.needs_redraw = true;
        }

        // 3. Advance animations, hover timers, and auto-hide transitions
        state.fallback_anim.is_active = state.is_animating();
        let frame_advanced = state.fallback_anim.update();
        let timers_changed = state.update_hover_and_hide_timers();
        state.update_animations(&qh);

        // Advance dynamic per-app animations
        for (app_id, anim) in state.animations.iter_mut() {
            if anim.update() {
                if let Some(frame) = anim.current_frame() {
                    state.icon_cache.insert(app_id.clone(), (frame.rgba.clone(), frame.width));
                    state.needs_redraw = true;
                }
            }
        }

        // 4. Determine socket poll timeout dynamically
        let is_hide_animating = (state.hide_state.current_alpha - state.hide_state.target_alpha).abs() >= 0.001;
        let timeout_ms = if state.fallback_anim.is_active || is_hide_animating || state.needs_redraw {
            15
        } else {
            50 
        };

        // 5. Prepare Wayland Socket Read & Poll
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

        // 6. Dispatch events newly read from socket
        if let Err(e) = event_queue.dispatch_pending(&mut state) {
            eprintln!("[WARN] Dispatch error: {}", e);
            break;
        }

        // 7. Redraw surfaces if needed and FLUSH immediately
        if frame_advanced || timers_changed || state.needs_redraw {
            state.draw(&qh);
            state.needs_redraw = false;
            let _ = state.connection.flush();
        }
    }
    Ok(())
}