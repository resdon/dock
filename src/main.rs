
pub mod cache;

pub use dockman_lib::models;
pub use dockman_lib::icon_utils;
pub use dockman_lib::terminal_graphics;
pub use dockman_lib::get_icon_path;

use crate::models::WindowDiagnostics;

use wayland_client::Dispatch;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::{Anchor, Layer, LayerShell, LayerSurface},
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_pointer::WlPointer, wl_seat::WlSeat};
use wayland_client::Connection;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;
use wayland_client::backend::ObjectId;

use std::collections::HashMap;
use std::path::PathBuf;

// 1. Mount the files as local root modules
pub mod handlers;
pub mod render;
pub mod modules;


// ...
use modules::persistence;
use crate::modules::world::FontManager;

pub struct MenuState {
    pub x: usize,
    pub y: usize,
    pub target_window: Option<ObjectId>,
    pub target_app_id: Option<String>,
    pub is_open: bool,
}

pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}

pub struct AppState {
	pub connection: Connection,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub pool: SlotPool,
    pub seat_state: SeatState,
    pub layer_surface: Option<LayerSurface>,
    pub current_buffer: Option<Buffer>,
    pub width: u32,
    pub height: u32,
    pub toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,
    pub pointer_x: usize,
    pub pointer_y: usize,
    pub open_windows: HashMap<ObjectId, WindowDiagnostics>,
    pub pinned_apps: Vec<String>,
    pub icon_cache: HashMap<String, (Vec<u8>, u32)>,
    pub menu_state: MenuState,
    pub hover_state: HoverState,
    pub last_interact_time: std::time::Instant,
    pub needs_redraw: bool,
    pub last_mouse_pos: Option<(f64, f64)>,
    pub is_dragging: bool,
    pub drag_start_x: f64,
    pub drag_start_y: f64,
    pub dragged_app_id: Option<String>,
    pub last_drag_draw: std::time::Instant,
    pub sys_scanner: sysinfo::System,
}

impl AppState {
    pub fn update_window_icon(&mut self, window_id: ObjectId) {
        if let Some(window) = self.open_windows.get_mut(&window_id) {
            if window.icon_resolved {
                return;
            }
            
            let mut search_id = if !window.app_id.trim().is_empty() {
                window.app_id.trim().to_string()
            } else {
                window.title.trim().to_string()
            };

            // Normalize taskman / Task Manager title or ID
            let lower_id = search_id.to_lowercase();
            if lower_id.contains("task manager") || lower_id == "taskman" {
                search_id = "taskman".to_string();
            }
            
            // 0. Proactive cleaning: strip suffixes like _1234 (common for dynamic app_ids), preserving steam_icon_<appid>
            if !search_id.starts_with("steam_icon_") {
                if let Some(idx) = search_id.rfind('_') {
                    if search_id[idx+1..].chars().all(|c| c.is_numeric()) {
                        search_id = search_id[..idx].to_string();
                    }
                }
            }
            let original_app_id = window.app_id.clone();

            // Refresh process scanner
            self.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            // Match PID for window if not already matched
            if window.matched_pid.is_none() {
                let target_app = search_id.to_lowercase();
                if let Some((pid, _)) = self.sys_scanner.processes().iter().find(|(_, p)| {
                    let proc_name = p.name().to_string_lossy().to_lowercase();
                    proc_name.contains(&target_app) || target_app.contains(&proc_name)
                }) {
                    window.matched_pid = Some(pid.as_u32());
                }
            }

            let pid_opt = window.matched_pid.map(sysinfo::Pid::from_u32);

            // Priority 1: Check dynamic Steam/Gamescope game details FIRST
            // This prevents gamescope/steam windows from temporarily normalizing to "steam" via Exec match
            if let Some((appid, steam_name, steam_icon_path)) = dockman_lib::resolve_steam_game_details(&search_id, &window.title, &self.sys_scanner, pid_opt) {
                let target_size = 48;
                if let Some((_, _, rgba_data)) = crate::terminal_graphics::load_image_raw_rgba(&steam_icon_path, target_size) {
                    let icon_key = format!("steam_icon_{}", appid);
                    window.app_name = steam_name.clone();
                    window.app_id = icon_key.clone();
                    window.icon_name = icon_key.clone();
                    window.icon_rgba = Some(rgba_data.clone());
                    window.icon_size = target_size;
                    window.icon_resolved = true;
                    self.icon_cache.insert(icon_key.clone(), (rgba_data.clone(), target_size));
                    if self.pinned_apps.contains(&icon_key) {
                        crate::cache::save_cached_icon(&icon_key, target_size, target_size, &rgba_data);
                    }
                    println!("[DEBUG] Resolved Steam Game '{}' AppID: {} -> Icon Path: {:?}", steam_name, appid, steam_icon_path);
                    return;
                }
            }

            // Match against pinned apps strictly by exact match
            for pinned_id in &self.pinned_apps {
                let p_lower = pinned_id.to_lowercase();
                let s_lower = search_id.to_lowercase();
                if p_lower == s_lower 
                    && !p_lower.starts_with("steam_icon_") && !s_lower.starts_with("steam_icon_") 
                {
                    search_id = pinned_id.clone();
                    window.app_id = search_id.clone();
                    break;
                }
            }
            // 1. Try to normalize app_id to a stable .desktop ID if it isn't one already
            if !search_id.is_empty() {
                // If it doesn't look like a standard ID, try to find the desktop file it belongs to
                if !crate::icon_utils::get_icon_from_desktop(&search_id).is_some() {
                    if let Some(resolved_id) = crate::icon_utils::find_desktop_file_by_exec(&search_id) {
                        println!("[DEBUG] Normalized app_id '{}' -> '{}' via Exec match", search_id, resolved_id);
                        search_id = resolved_id;
                        window.app_id = search_id.clone();
                    }
                }
            }

            if !search_id.is_empty() {
                let icon_name = crate::icon_utils::extract_icon_name(&search_id);
                let icon_path = crate::get_icon_path(&search_id);
                println!("[DEBUG] App '{}' (orig: '{}') -> Icon Name: '{}', Path: {:?}", search_id, original_app_id, icon_name, icon_path);
                
                let mut raw_pixels = None;
                let target_size = 48;
                if let Some(path) = icon_path {
                    if let Some((_, _, rgba_data)) = crate::terminal_graphics::load_image_raw_rgba(&path, target_size) {
                        raw_pixels = Some(rgba_data.clone());
                        self.icon_cache.insert(search_id.clone(), (rgba_data.clone(), target_size));
                        if self.pinned_apps.contains(&search_id) {
                            crate::cache::save_cached_icon(&search_id, target_size, target_size, &rgba_data);
                        }
                    }
                }
                window.icon_name = icon_name;
                window.icon_rgba = raw_pixels.clone();
                window.icon_size = target_size;
                // Only mark resolved if we actually found an icon
                if raw_pixels.is_some() {
                    window.icon_resolved = true;
                }
            } else {
                println!("[DEBUG] Could not resolve any ID for window with title '{}'", window.title);
            }
        }
    }

    pub fn draw(&mut self, qh: &wayland_client::QueueHandle<Self>) {
        let box_size = 48;
        let spacing = 12;
        let max_dock_width = 800; // Hard limit for your screen

        // Grouping logic
        let mut apps_in_dock: Vec<String> = self.pinned_apps.clone();
        for window in self.open_windows.values() {
            let id = if !window.app_id.is_empty() {
                window.app_id.clone()
            } else if !window.title.is_empty() {
                window.title.clone()
            } else {
                "Unknown".to_string()
            };
            
            if !apps_in_dock.contains(&id) { 
                apps_in_dock.push(id); 
            }
        }

        // Calculate dynamic width, but clamp to max_dock_width
        let total_items = apps_in_dock.len();
        let calculated_width = if total_items > 0 {
            (total_items * box_size + (total_items + 1) * spacing) as u32
        } else { 100 };
        
        self.width = calculated_width.min(max_dock_width); 
        self.height = if self.menu_state.is_open || self.hover_state.is_visible { 200 } else { 60 };

        if let Some(ref surface) = self.layer_surface {
            surface.set_size(self.width, self.height);
            let compositor = self.compositor_state.wl_compositor();
            let region = compositor.create_region(qh, ());
            
            // 1. The dock itself (bottom 60px of the surface)
            let dock_y = (self.height as i32).saturating_sub(60);
            region.add(0, dock_y, self.width as i32, 60);

            // 2. If hover preview is visible, add its bounds to the input region
            if self.hover_state.is_visible {
                if let Some(ref app_id) = self.hover_state.app_id {
                    let count = self.open_windows.values()
                        .filter(|w| {
                            let id = if !w.app_id.is_empty() { w.app_id.as_str() }
                                     else if !w.title.is_empty() { w.title.as_str() }
                                     else { "Unknown" };
                            id == app_id.as_str()
                        })
                        .count();
                    if count > 0 {
                        let (menu_x, menu_y, menu_width, menu_height) = crate::modules::context_menu::get_hover_menu_bounds(
                            self.hover_state.x, self.width, self.height, count
                        );
                        region.add(menu_x as i32, menu_y as i32, menu_width as i32, menu_height as i32);
                        
                        let gap_y = (menu_y + menu_height) as i32;
                        let dock_top = (self.height as i32).saturating_sub(60);
                        if gap_y < dock_top {
                            region.add(menu_x as i32, gap_y, menu_width as i32, dock_top - gap_y);
                        }
                    }
                }
            }

            // 3. If context menu is open, add its bounds to the input region
            if self.menu_state.is_open {
                let menu_width = crate::modules::context_menu::MENU_WIDTH as i32;
                let menu_height = crate::modules::context_menu::MENU_HEIGHT as i32;
                let menu_x = (self.menu_state.x as i32).min((self.width as i32).saturating_sub(menu_width));
                let menu_y = (self.menu_state.y as i32).saturating_sub(menu_height);
                region.add(menu_x, menu_y, menu_width, menu_height);
            }

            surface.wl_surface().set_input_region(Some(&region));
            // NOTE: Do NOT commit here — we commit below after attaching the buffer.
            // An early commit with no buffer causes the compositor to show blank.
        }

        // 4. Create buffer and draw
        let (buffer, canvas) = self.pool
            .create_buffer(
                self.width as i32,
                self.height as i32,
                (self.width * 4) as i32,
                wayland_client::protocol::wl_shm::Format::Argb8888
            )
            .expect("Failed to create layout buffer");

        render::render_windows(
            canvas, self.width, self.height,
            &self.open_windows,
            &self.pinned_apps,
            &self.icon_cache,
            &self.menu_state,
            &self.hover_state,
            &self.font_manager,
            self.is_dragging,
            self.dragged_app_id.as_ref(),
            self.pointer_x,
            self.pointer_y,
        );

        // 5. Commit the buffer (single commit after both input region and buffer are set)
        if let Some(ref surface) = self.layer_surface {
            buffer.attach_to(surface.wl_surface()).expect("Buffer attach failed");
            surface.wl_surface().damage_buffer(0, 0, self.width as i32, self.height as i32);
            surface.wl_surface().commit();
        }

        self.current_buffer = Some(buffer);
        // Flush is handled by the caller (event loop or handler), not here.
    }
}

impl Dispatch<wayland_client::protocol::wl_region::WlRegion, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &wayland_client::protocol::wl_region::WlRegion,
        _event: <wayland_client::protocol::wl_region::WlRegion as wayland_client::Proxy>::Event,
        _data: &(),
        _conn: &wayland_client::Connection,
        _qhandle: &wayland_client::QueueHandle<Self>,
    ) {
        // WlRegion has no events, so this body can remain empty.
    }
}
// =========================================================================
// Add the missing .draw() orchestration method to bridge render.rs
// =========================================================================
// Replace the `impl AppState` block inside src/main.rs with this:

fn main() {
    println!("[DEBUG] Starting dock...");
    let conn = Connection::connect_to_env().expect("Failed to connect to Wayland display");
    println!("[DEBUG] Connected to Wayland.");
    
    let (globals, mut event_queue) = registry_queue_init::<AppState>(&conn).unwrap();
    let qh = event_queue.handle();
    
    let registry_state = RegistryState::new(&globals);
    let compositor_state = CompositorState::bind(&globals, &qh).expect("Failed to bind compositor");
    let output_state = OutputState::new(&globals, &qh);
    let layer_shell = LayerShell::bind(&globals, &qh).expect("wlr_layer_shell required");
    let shm_state = Shm::bind(&globals, &qh).expect("wl_shm required");
    let pool = SlotPool::new(1024 * 1024 * 4, &shm_state).expect("Failed to create memory pool");

    let seat_state = SeatState::new(&globals, &qh);
    
    // Load font for fontmanager
    let font_path = [
        PathBuf::from("font.ttf"),
        PathBuf::from("/usr/share/dock/font.ttf"),
        PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
    ]
    .into_iter()
    .find(|p| p.exists())
    .expect("No valid font file found on system!");

    // =========================================================================
    // 1. CREATE THE VARIABLES RIGHT BEFORE APPSTATE USES THEM
    // =========================================================================
    let pinned_vector = persistence::load_pinned_apps();
    let mut permanent_icon_cache = HashMap::new();
    for app_id in &pinned_vector {
        if let Some((rgba, size)) = cache::load_cached_icon(app_id) {
            permanent_icon_cache.insert(app_id.clone(), (rgba, size));
        }
    }

    // =========================================================================
    // 2. INITIALIZE THE FULL STATE MATCHING YOUR COMPOSITOR STRUCT
    // =========================================================================
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
        toplevel_manager: None, // This gets bound right below this block
        font_manager: FontManager::from_file(&font_path).expect("Failed to memory-map font file"),
        wl_seat: None,
        wl_pointer: None,
        pointer_x: 0,
        pointer_y: 0,
        open_windows: HashMap::new(),
        pinned_apps: pinned_vector, // Already a Vec<String>
        icon_cache: permanent_icon_cache,                 // Found in scope now!
        menu_state: MenuState {
            x: 0,
            y: 0,
            target_window: None,
            target_app_id: None,
            is_open: false,
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
        drag_start_x: 0.0,
        drag_start_y: 0.0,
        dragged_app_id: None,
        last_drag_draw: std::time::Instant::now(),
        sys_scanner: sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::everything()),
        ),
    };

    // =========================================================================
    // Your existing foreign_toplevel manager binding continues right below here
    // =========================================================================
    state.toplevel_manager = state.registry_state
        .bind_one::<ZwlrForeignToplevelManagerV1, _, _>(&qh, 1..=3, ())
        .ok();
    

    println!("[DEBUG] Doing roundtrip...");
    event_queue.roundtrip(&mut state).unwrap();
    println!("[DEBUG] Roundtrip complete.");

    // Simple Verification Check
    if state.toplevel_manager.is_some() {
        println!("[DEBUG] Active window tracking protocols linked to event loop successfully via direct binding!");
    } else {
        eprintln!("[ERROR] Your compositor does not support zwlr_foreign_toplevel_manager_v1!");
    }
    
    // ... Rest of your layer_surface allocation and blocking_dispatch loop code continues exactly the same

    let raw_surface = state.compositor_state.create_surface(&qh);
	let layer_surface = state.layer_shell.create_layer_surface(
	    &qh,
	    raw_surface,
	    Layer::Top,
	    Some("dock_panel"),
	    None,
	);

	// ADD THIS LINE: This tells the compositor not to focus the dock on click
	layer_surface.set_keyboard_interactivity(
	    smithay_client_toolkit::shell::wlr_layer::KeyboardInteractivity::None
	);
	
    layer_surface.set_size(540, 60);
    layer_surface.set_anchor(Anchor::BOTTOM);
    layer_surface.wl_surface().commit(); 
    state.layer_surface = Some(layer_surface);

    println!("[DEBUG] Starting event loop...");
    loop {
        if let Err(e) = event_queue.blocking_dispatch(&mut state) {
            eprintln!("[WARN] Dispatch error: {}", e);
            break;
        }
    }
}
