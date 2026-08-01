use crate::modules::context_menu::{get_hover_menu_bounds};
pub use crate::models::LastState;
use crate::models::WindowDiagnostics;

use std::collections::HashMap;
use std::os::fd::AsFd;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use smithay_client_toolkit::{
    compositor::{CompositorHandler},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
        Capability, SeatHandler, SeatState,
    },
    shell::wlr_layer::{Anchor, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
    shm::{Shm, ShmHandler},
};

use wayland_client::event_created_child;
use wayland_client::{
    protocol::{
        wl_output::{Transform, WlOutput},
        wl_seat::WlSeat,
    },
    Connection, Dispatch, Proxy, QueueHandle,
};

use wayland_client::backend::{ObjectData, ObjectId};

use wayland_client::protocol::wl_data_device::{Event as DndEvent, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use crate::AppState;

// Fractional scaling
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::{self, WpFractionalScaleManagerV1},
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
// -----

// Resolved launcher.sh path
pub fn get_launcher_path() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let local_share_path = format!("{}/.local/share/dock/launcher.sh", home);

    if std::path::Path::new("./launcher.sh").exists() {
        "./launcher.sh".to_string()
    } else if std::path::Path::new(&local_share_path).exists() {
        local_share_path
    } else {
        "/usr/share/dock/launcher.sh".to_string()
    }
}

fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;
    
    // The state event sends a list of u32s. 
    // Protocol zwlr_foreign_toplevel_handle_v1::State:
    // 0 = Maximize, 1 = Minimize, 2 = Activated, 3 = Fullscreen
    for chunk in state_bytes.chunks_exact(4) {
        let value = u32::from_ne_bytes(chunk.try_into().unwrap());
        match value {
            2 => activated = true, 
            1 => minimized = true, 
            _ => {} // Ignore Maximize (0) and Fullscreen (3) for now
        }
    }
    (activated, minimized)
}
/// Parses a `text/uri-list` string into valid local PathBuf instances.
pub fn parse_uri_list(buffer: &str) -> Vec<PathBuf> {
    buffer
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let path_str = if let Some(stripped) = line.strip_prefix("file://localhost") {
                stripped
            } else if let Some(stripped) = line.strip_prefix("file://") {
                stripped
            } else {
                line
            };

            percent_decode(path_str).map(PathBuf::from)
        })
        .collect()
}
// Helper
fn percent_decode(input: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next()?;
            let h2 = chars.next()?;
            
            // Store array in a local variable so the borrow lasts long enough for `from_utf8`
            let hex_bytes = [h1, h2];
            let hex_str = std::str::from_utf8(&hex_bytes).ok()?;
            let byte = u8::from_str_radix(hex_str, 16).ok()?;
            
            bytes.push(byte);
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8(bytes).ok()
}
// --- Dispatch for WlDataDeviceManager ---
impl Dispatch<WlDataDeviceManager, ()> for AppState {
    fn event(
        _state: &mut Self,
        _manager: &WlDataDeviceManager,
        _event: <WlDataDeviceManager as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

// --- Collect MIME types as they arrive ---
impl Dispatch<WlDataOffer, ()> for AppState {
    fn event(
        state: &mut Self,
        _offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            state.dnd_state.mime_types.push(mime_type);
        }
    }
}

// --- Dispatch for WlDataDevice (Full DnD Handler) ---
impl Dispatch<WlDataDevice, ()> for AppState {
    fn event(
        state: &mut Self,
        _device: &WlDataDevice,
        event: DndEvent,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            DndEvent::DataOffer { id } => {
                state.dnd_state.current_offer = Some(id);
                state.dnd_state.mime_types.clear();
            }

            DndEvent::Enter { serial, x, y, .. } => {
                state.dnd_state.drag_x = x;
                state.dnd_state.drag_y = y;

                if let Some(ref offer) = state.dnd_state.current_offer {
                    let has_uri_list = state.dnd_state.mime_types.iter().any(|m| m == "text/uri-list");

                    if has_uri_list {
                        offer.accept(serial, Some("text/uri-list".to_string()));
                        if offer.version() >= 3 {
                            offer.set_actions(DndAction::Copy, DndAction::Copy);
                        }
                    } else {
                        offer.accept(serial, None);
                    }
                }

                state.update_dnd_hover_target(x, y);
            }

            DndEvent::Motion { x, y, .. } => {
                state.dnd_state.drag_x = x;
                state.dnd_state.drag_y = y;
                state.update_dnd_hover_target(x, y);
            }

            DndEvent::Leave => {
                state.dnd_state.current_offer = None;
                state.dnd_state.mime_types.clear();
                state.dnd_state.hovered_dock_index = None;
                state.needs_redraw = true;
            }

            DndEvent::Drop => {
                let drop_x = state.dnd_state.drag_x;
                let drop_y = state.dnd_state.drag_y;

                eprintln!("[DnD DEBUG] Drop event received at x={:.1}, y={:.1}", drop_x, drop_y);

                if let Some(offer) = state.dnd_state.current_offer.take() {
                    let has_uri_list = state.dnd_state.mime_types.iter().any(|m| m == "text/uri-list");

                    if has_uri_list {
                        if let Ok((read_pipe, write_pipe)) = os_pipe::pipe() {
                            offer.receive("text/uri-list".to_string(), write_pipe.as_fd());
                            
                            let _ = _conn.flush();
                            drop(write_pipe);

                            let mut reader = read_pipe;
                            let mut buffer = String::new();
                            match reader.read_to_string(&mut buffer) {
                                Ok(bytes_read) => {
                                    eprintln!("[DnD DEBUG] Read {} bytes from pipe", bytes_read);
                                    eprintln!("[DnD DEBUG] Raw URI payload:\n--- START ---\n{}\n--- END ---", buffer.trim());
                                    
                                    let paths = parse_uri_list(&buffer);
                                    eprintln!("[DnD DEBUG] Parsed {} path(s): {:?}", paths.len(), paths);

                                    if !paths.is_empty() {
                                        state.handle_file_drop_on_icon(drop_x, drop_y, paths);
                                    } else {
                                        eprintln!("[DnD DEBUG] Warning: Parsed paths list was empty!");
                                    }
                                }
                                Err(e) => eprintln!("[DnD DEBUG] Error reading pipe: {}", e),
                            }
                        }

                        if offer.version() >= 3 {
                            offer.finish();
                        }
                    } else {
                        eprintln!("[DnD DEBUG] Dropped data does not contain 'text/uri-list' MIME type");
                    }
                    offer.destroy();
                } else {
                    eprintln!("[DnD DEBUG] No active offer found on drop!");
                }

                state.dnd_state.hovered_dock_index = None;
                state.needs_redraw = true;
            }

            _ => {}
        }
    }

    // Required so wayland-client can instantiate incoming WlDataOffer proxies (opcode 0)
    fn event_created_child(
        opcode: u16,
        qh: &QueueHandle<Self>,
    ) -> Arc<dyn ObjectData> {
        match opcode {
            0 => qh.make_data::<WlDataOffer, _>(()),
            _ => unreachable!(),
        }
    }
}
// -----Fractional scale manager handler--------
impl Dispatch<WpFractionalScaleManagerV1, ()> for AppState {
    fn event(
        _state: &mut Self,
        _proxy: &WpFractionalScaleManagerV1,
        _event: wp_fractional_scale_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        // Manager object emits no events directly
    }
}

// -----Fractional scale notifier handler--------
impl Dispatch<WpFractionalScaleV1, WlOutput> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &WpFractionalScaleV1,
        event: wp_fractional_scale_v1::Event,
        data: &WlOutput, // `data` is now the WlOutput bound to this notifier
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            wp_fractional_scale_v1::Event::PreferredScale { scale } => {
                // Scale is provided in 120ths (120 = 1.0x, 150 = 1.25x, 180 = 1.5x)
                let new_scale = scale as f64 / 120.0;
                
                // Find the specific dock instance attached to this WlOutput
                if let Some(dock) = state.docks.iter_mut().find(|d| d.output == *data) {
                    if (dock.scale_factor - new_scale).abs() > f64::EPSILON {
                        dock.scale_factor = new_scale;
                        state.needs_redraw = true;
                    }
                }
            }
            _ => {}
        }
    }
}

// =========================================================================
// Registry Handler to Bind Globals
// =========================================================================
impl RegistryHandler<AppState> for AppState {
    fn new_global(
        _data: &mut AppState,
        _conn: &Connection,
        _qh: &QueueHandle<AppState>,
        _name: u32,
        interface: &str,
        version: u32,
    ) {
        eprintln!("[DEBUG] Global detected: {} (v{})", interface, version);
    }
    fn remove_global(_data: &mut AppState, _conn: &Connection, _qh: &QueueHandle<AppState>, _name: u32, _interface: &str) {}
}

// =========================================================================
// Existing SCTK Registry Glue Implementations
// =========================================================================
impl ProvidesRegistryState for AppState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers!(OutputState, SeatState);
}

// =========================================================================
// Compositor Handler - Track dock surface output
// =========================================================================
impl CompositorHandler for AppState {
    fn surface_enter(&mut self, _: &Connection, _qh: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, output: &WlOutput) {
        println!("[DOCK MONITOR] Surface entered output: {:?}", output);
        self.current_output = Some(output.clone());
        self.needs_redraw = true;
    }

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, output: &WlOutput) {
        if self.current_output.as_ref() == Some(output) {
            self.current_output = None;
        }
    }

    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: i32) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: u32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: Transform) {}
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, output: WlOutput) {
        if self.current_output.as_ref() == Some(&output) {
            self.current_output = None;
        }
    }
}

impl LayerShellHandler for AppState {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {}

    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, configure: LayerSurfaceConfigure, _: u32) {
        // Capture the structural allocations chosen by sctk/compositor
        self.width = configure.new_size.0.max(100);
        
        // Dynamically track what our height should be based on UI overlays
        let target_height = if self.menu_state.is_open || self.hover_state.is_visible { 200 } else { 60 };
        self.height = target_height;

        layer.set_size(self.width, self.height);
        layer.set_anchor(Anchor::BOTTOM);

        // Render the buffer configuration cleanly inside the correct dimensions
        self.draw(qh);
    }
}
impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl SeatHandler for AppState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat) {}

    fn new_capability(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            println!("[INPUT DETECTOR] Mouse Pointer Capability Registered!");

            if let Some(ref ddm) = self.data_device_manager {
                self.data_device = Some(ddm.get_data_device(&seat, qh, ()));
            }

            let wl_pointer = self.seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to secure pointer handle");

            self.wl_pointer = Some(wl_pointer);
            self.wl_seat = Some(seat);
        }
    }

    fn remove_capability(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _seat: WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            println!("[INPUT DETECTOR] Mouse Pointer Capability Unplugged!");
            self.wl_pointer = None;
            self.data_device = None;
        }
    }
}

// =========================================================================
// Corrected Pointer Interaction Tracking Logic (With Window List Scroll)
// =========================================================================
impl PointerHandler for AppState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        if events.is_empty() { return; }

        self.last_interact_time = std::time::Instant::now(); 
        let mut layer_changed = false;

        // --- STEP 1: Parse the Frame Packet & Handle Instant Leave ---
        for event in events {
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    self.pointer_x = event.position.0 as usize;
                    self.pointer_y = event.position.1 as usize;
                    
                    if self.dragged_app_id.is_some() {
                        let dx = event.position.0 - self.drag_start_x;
                        let dy = event.position.1 - self.drag_start_y;
                        if (dx * dx + dy * dy).sqrt() > 5.0 {
                            self.is_dragging = true;
                        }
                        // Inside Step 1 of pointer_frame:
                        if self.is_dragging {
                            let now = std::time::Instant::now();
                            if now.duration_since(self.last_drag_draw).as_millis() >= 4 {
                                self.last_drag_draw = now;
                                layer_changed = true; // ✅ Set layer_changed instead of calling self.draw(qh) directly
                            }
                        }
                    } else {
                        layer_changed = true; // normal hover updates
                    }
                }
                PointerEventKind::Leave { .. } => {
                    // Immediately dismiss hover state when pointer leaves the surface entirely
                    if self.hover_state.is_visible {
                        self.hover_state.is_visible = false;
                        self.hover_state.app_id = None;
                        layer_changed = true;
                    }
                    self.hover_state.last_leave_time = None;

                    if self.menu_state.is_open {
                        self.menu_state.is_open = false;
                        layer_changed = true;
                    }
                }
                _ => {}
            }
        }

        // --- STEP 2: Unified Layout Metrics ---
        // Get scale factor from the first dock (all docks should have same scale)
        let scale_factor = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0);
        
        // Scale layout constants to physical pixels
        let dock_height = (60.0 * scale_factor).round() as usize;
        let box_size = (48.0 * scale_factor).round() as usize;
        let spacing = (12.0 * scale_factor).round() as usize;
        
        // Scale context menu dimensions to match the rendered surface bounds
        let _menu_width = (180.0 * scale_factor).round() as usize;
        let _menu_item_height = (30.0 * scale_factor).round() as usize;

        // Populate map of running windows by app_id
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new(); 
        for (id, window) in &self.open_windows {
            let app_id = if !window.app_id.is_empty() {
                window.app_id.clone()
            } else if !window.title.is_empty() {
                window.title.clone()
            } else {
                "Unknown".to_string()
            };
            running_by_app.entry(app_id).or_default().push(id.clone());
        }
        // ✅ FIX: Sort each app's window vector so click index matches render index
        for windows in running_by_app.values_mut() {
            windows.sort_by(|a, b| {
                let win_a = self.open_windows.get(a);
                let win_b = self.open_windows.get(b);
                let title_a = win_a.map(|w| w.title.as_str()).unwrap_or("");
                let title_b = win_b.map(|w| w.title.as_str()).unwrap_or("");
                title_a.cmp(title_b)
            });
        }

        // Gather and sort active windows across all outputs
        let mut sorted_windows: Vec<&WindowDiagnostics> = self.open_windows.values().collect();
        sorted_windows.sort_by(|a, b| {
            a.app_name.cmp(&b.app_name)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| std::ptr::from_ref(*a).cmp(&std::ptr::from_ref(*b)))
        });

        let mut apps_in_dock: Vec<String> = self.pinned_apps.clone();
        for window in sorted_windows {
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
        
        let total_items = apps_in_dock.len(); 
        let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 }; 
        let start_offset_x = if (self.width as usize) > content_width { (self.width as usize - content_width) / 2 } else { 0 }; 
        
        let phys_surface_height = (self.height as f64 * scale_factor).round() as usize;
        let dock_top_bound = phys_surface_height.saturating_sub(dock_height);

        // --- STEP 3: Unified Hover & Bounds Tracking ---
        let mut should_be_visible = false;
        let mut new_app_id = None;
        let mut new_x = self.hover_state.x;

        let is_over_icons = self.pointer_y >= dock_top_bound;

        if is_over_icons {
            for (index, app_id) in apps_in_dock.iter().enumerate() {
                let start_x = start_offset_x + spacing + index * (box_size + spacing);
                let end_x = start_x + box_size;
                let hit_start_x = start_x.saturating_sub(spacing / 2);
                let hit_end_x = end_x + (spacing / 2);
                
                if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x {
                    should_be_visible = true;
                    new_x = start_x + box_size / 2;
                    new_app_id = Some(app_id.clone());
                    break;
                }
            }
        }

        if !should_be_visible && self.hover_state.is_visible {
            if let Some(ref app_id) = self.hover_state.app_id {
                if let Some(windows) = running_by_app.get(app_id) {
                    let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                        self.hover_state.x,
                        self.width as usize,
                        self.height as usize,
                        windows.len(),
                        scale_factor,
                    );
                    
                    // Convert logical pointer coordinates to physical pixels to match menu bounds
                    let ptr_x = (self.pointer_x as f64) * scale_factor;
                    let ptr_y = (self.pointer_y as f64) * scale_factor;

                    // 1. Strict menu bounds (in physical pixels)
                    let inside_menu = ptr_x >= menu_x 
                        && ptr_x <= menu_x + menu_width
                        && ptr_y >= menu_y 
                        && ptr_y <= menu_y + menu_height;

                    // 2. Gap bounds (bridges menu_bottom to dock_top)
                    let menu_bottom = menu_y + menu_height;
                    let inside_gap = ptr_x >= menu_x 
                        && ptr_x <= menu_x + menu_width
                        && ptr_y >= menu_bottom 
                        && ptr_y <= (dock_top_bound as f64);

                    if inside_menu || inside_gap {
                        should_be_visible = true;
                        new_app_id = Some(app_id.clone());
                        new_x = self.hover_state.x;
                    }
                }
            }
        }

        // --- Context Menu Dismissal Leeway & Hit Testing ---
        if self.menu_state.is_open && !self.menu_state.items.is_empty() {
            let scale_factor = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0);
            let phys_width = (self.width as f64 * scale_factor).round() as usize;
            let phys_height = (self.height as f64 * scale_factor).round() as usize;

            let (menu_x, menu_y, menu_width, total_menu_h) = self.get_context_menu_bounds(phys_width, phys_height, scale_factor);
            
            // Convert logical pointer to physical pixels
            let ptr_x = (self.pointer_x as f64) * scale_factor;
            let ptr_y = (self.pointer_y as f64) * scale_factor;
            let leeway = (20.0 * scale_factor).round() as f64;

            let inside_extended = ptr_x >= (menu_x as f64) - leeway
                && ptr_x <= (menu_x + menu_width) as f64 + leeway
                && ptr_y >= (menu_y as f64) - leeway
                && ptr_y <= (menu_y + total_menu_h) as f64 + leeway;

            if !inside_extended {
                self.menu_state.is_open = false;
                layer_changed = true;
            }
        }

        if should_be_visible != self.hover_state.is_visible || new_app_id != self.hover_state.app_id {
            self.hover_state.is_visible = should_be_visible;
            self.hover_state.app_id = new_app_id;
            self.hover_state.x = new_x;
            layer_changed = true;
        }
        // --- STEP 3.5: Scroll-to-Cycle Windows ---
        for event in events {
            if let PointerEventKind::Axis { vertical, .. } = &event.kind {
                let scroll_val = if vertical.discrete != 0 {
                    vertical.discrete as f64
                } else {
                    vertical.absolute
                };

                if scroll_val != 0.0 {
                    let mut target_app: Option<String> = None;

                    // 1. Check if hovering over dock icons
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index * (box_size + spacing);
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = start_x + box_size + (spacing / 2);

                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x {
                                target_app = Some(app_id.clone());
                                break;
                            }
                        }
                    }

                    // 2. Check if hovering over the hover preview popup menu
                    if target_app.is_none() && self.hover_state.is_visible {
                        if let Some(ref app_id) = self.hover_state.app_id {
                            if let Some(windows) = running_by_app.get(app_id) {
                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.hover_state.x,
                                    self.width as usize,
                                    self.height as usize,
                                    windows.len(),
                                    scale_factor,
                                );

                                if (self.pointer_x as f64) >= menu_x && (self.pointer_x as f64) <= menu_x + menu_width &&
                                   (self.pointer_y as f64) >= menu_y && (self.pointer_y as f64) <= menu_y + menu_height {
                                    target_app = Some(app_id.clone());
                                }
                            }
                        }
                    }

                    if let Some(app_id) = target_app {
                        self.cycle_window_for_app(&app_id, scroll_val < 0.0);
                        layer_changed = true;
                    }
                }
            }
        }

        // --- STEP 4: Instantly Handle Clicks ---
        for event in events {
            if let PointerEventKind::Press { button, .. } = event.kind { 
                if button == 272 { // Left Click
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index * (box_size + spacing); 
                            let end_x = start_x + box_size; 
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = end_x + (spacing / 2);
                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                self.dragged_app_id = Some(app_id.clone());
                                self.drag_start_x = self.pointer_x as f64;
                                self.drag_start_y = self.pointer_y as f64;
                                self.is_dragging = false;
                                break;
                            }
                        }
                    }
                // In PointerHandler::pointer_frame during Right Click press (button == 273):
                } else if button == 273 { // Right Click
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index * (box_size + spacing); 
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = start_x + box_size + (spacing / 2);
                            
                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                self.menu_state.is_open = true; 
                                self.menu_state.x = self.pointer_x; 
                                self.menu_state.y = self.pointer_y; 
                                self.menu_state.target_app_id = Some(app_id.clone()); 
                                let windows = running_by_app.get(app_id).cloned().unwrap_or_default(); 
                                self.menu_state.target_window = windows.iter()
                                    .find(|id| self.open_windows.get(id).map(|w| w.is_activated).unwrap_or(false)) 
                                    .cloned() 
                                    .or_else(|| windows.first().cloned()); 

                                let is_running = !windows.is_empty();
                                let is_pinned = self.pinned_apps.contains(app_id);

                                // Load Desktop Actions from .desktop file
                                let actions = dockman_lib::get_desktop_actions(app_id);

                                let mut items = Vec::new();
                                if is_running {
                                    items.push(crate::ContextMenuItem {
                                        label: "Focus".to_string(),
                                        item_type: crate::MenuItemType::Focus,
                                    });
                                    items.push(crate::ContextMenuItem {
                                        label: "New Instance".to_string(),
                                        item_type: crate::MenuItemType::LaunchNew,
                                    });
                                    items.push(crate::ContextMenuItem {
                                        label: "Minimize".to_string(),
                                        item_type: crate::MenuItemType::Minimize,
                                    });
                                } else {
                                    items.push(crate::ContextMenuItem {
                                        label: "Launch".to_string(),
                                        item_type: crate::MenuItemType::LaunchNew,
                                    });
                                }

                                // Add Actions
                                for action in actions {
                                    items.push(crate::ContextMenuItem {
                                        label: action.name.clone(),
                                        item_type: crate::MenuItemType::Action(action),
                                    });
                                }

                                // Toggle Pin
                                let pin_label = if is_pinned { "Unpin from Dock" } else { "Pin to Dock" };
                                items.push(crate::ContextMenuItem {
                                    label: pin_label.to_string(),
                                    item_type: crate::MenuItemType::TogglePin,
                                });

                                // Full Application Close
                                if is_running {
                                    items.push(crate::ContextMenuItem {
                                        label: "Quit Application".to_string(),
                                        item_type: crate::MenuItemType::CloseApp,
                                    });
                                }

                                self.menu_state.items = items;
                                layer_changed = true;
                                break;
                            }
                        }
                    }
                } else if button == 274 { // Middle Click
                    let mut handled = false;

                    // 1. Hover Preview Window Item: Middle-click to close window instance
                    if self.hover_state.is_visible {
                        if let Some(ref app_id) = self.hover_state.app_id {
                            if let Some(windows) = running_by_app.get(app_id) {
                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.hover_state.x,
                                    self.width as usize,
                                    self.height as usize,
                                    windows.len(),
                                    scale_factor,
                                );

                                if (self.pointer_x as f64) >= menu_x && (self.pointer_x as f64) <= menu_x + menu_width &&
                                   (self.pointer_y as f64) >= menu_y && (self.pointer_y as f64) <= menu_y + menu_height {
                                    let item_h = 30;
                                    let idx = (((self.pointer_y as f64) - menu_y) as usize) / item_h;
                                    if let Some(handle_id) = windows.get(idx) {
                                        if let Some(win) = self.open_windows.get_mut(handle_id) {
                                            win.handle.close();
                                            handled = true;
                                            layer_changed = true;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // 2. Dock Icon: Middle-click to launch fresh separate instance
                    if !handled && is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index * (box_size + spacing);
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = start_x + box_size + (spacing / 2);

                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x {
                                let launcher_path = get_launcher_path();

                                let mut normalized_app_id = app_id.clone();
                                if !normalized_app_id.starts_with("steam_icon_") {
                                    if let Some(idx) = normalized_app_id.rfind('_') {
                                        if normalized_app_id[idx+1..].chars().all(|c| c.is_numeric()) {
                                            normalized_app_id = normalized_app_id[..idx].to_string();
                                        }
                                    }
                                }
                                if normalized_app_id.to_lowercase().contains("transmission") {
                                    normalized_app_id = "transmission-gtk".to_string();
                                }

                                let _ = std::process::Command::new("sh")
                                    .arg(launcher_path)
                                    .arg(normalized_app_id)
                                    .spawn();

                                layer_changed = true;
                                break;
                            }
                        }
                    }
                }
            } else if let PointerEventKind::Release { button, .. } = event.kind {
                if button == 272 { // Left Click Release
                    // A. Context Menu Handling
                    if self.menu_state.is_open && !self.menu_state.items.is_empty() { 
                        let scale_factor = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0);
                        let phys_width = (self.width as f64 * scale_factor).round() as usize;
                        let phys_height = (self.height as f64 * scale_factor).round() as usize;
                        let item_h = (30.0 * scale_factor).round() as usize;

                        let (menu_x, menu_y, menu_width, total_menu_h) = self.get_context_menu_bounds(phys_width, phys_height, scale_factor);
                        let ptr_x = (self.pointer_x as f64) * scale_factor;
                        let ptr_y = (self.pointer_y as f64) * scale_factor;

                        if ptr_x >= menu_x as f64 && ptr_x <= (menu_x + menu_width) as f64 &&
                           ptr_y >= menu_y as f64 && ptr_y <= (menu_y + total_menu_h) as f64 { 

                            let clicked_item_idx = ((ptr_y - menu_y as f64) as usize) / item_h; 
                            if let Some(item) = self.menu_state.items.get(clicked_item_idx) {
                                match &item.item_type {
                                    crate::MenuItemType::Focus => {
                                        if let Some(handle_id) = &self.menu_state.target_window { 
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { 
                                                if let Some(seat) = &self.wl_seat { window_info.handle.activate(seat); } 
                                            }
                                        }
                                    },
                                    crate::MenuItemType::LaunchNew => {
                                        if let Some(app_id) = &self.menu_state.target_app_id {
                                            let _ = std::process::Command::new("sh")
                                                .arg(get_launcher_path())
                                                .arg(app_id)
                                                .spawn();
                                        }
                                    },
                                    crate::MenuItemType::Minimize => {
                                        if let Some(handle_id) = &self.menu_state.target_window { 
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { 
                                                window_info.handle.set_minimized(); 
                                            }
                                        }
                                    },
                                    crate::MenuItemType::Action(action) => {
                                        let _ = std::process::Command::new("sh")
                                            .arg("-c")
                                            .arg(&action.exec)
                                            .spawn();
                                    },
                                    crate::MenuItemType::TogglePin => {
                                        if let Some(app_id) = &self.menu_state.target_app_id {
                                            let mut app_id = app_id.clone();
                                            if !app_id.starts_with("steam_icon_") {
                                                if let Some(idx) = app_id.rfind('_') {
                                                    if app_id[idx+1..].chars().all(|c| c.is_numeric()) {
                                                        app_id = app_id[..idx].to_string();
                                                    }
                                                }
                                            }
                                            if app_id.to_lowercase().contains("transmission") {
                                                app_id = "transmission-gtk".to_string();
                                            }

                                            let mut pinned = crate::modules::persistence::load_pinned_apps(); 
                                            if pinned.contains(&app_id) { 
                                                pinned.retain(|x| x != &app_id); 
                                            } else { 
                                                pinned.push(app_id.clone()); 
                                                if let Some((rgba, size)) = self.icon_cache.get(&app_id) {
                                                    crate::cache::save_cached_icon(&app_id, *size, *size, rgba);
                                                } else if let Some(window_info) = self.open_windows.values().find(|w| w.app_id == app_id) {
                                                    if let Some(rgba) = &window_info.icon_rgba {
                                                        crate::cache::save_cached_icon(
                                                            &app_id, 
                                                            window_info.icon_size, 
                                                            window_info.icon_size, 
                                                            rgba
                                                        );
                                                    }
                                                }
                                            } 
                                            crate::modules::persistence::save_pinned_apps(&pinned); 
                                            self.pinned_apps = pinned; 
                                        }
                                    },
                                    crate::MenuItemType::CloseApp => {
                                        if let Some(app_id) = self.menu_state.target_app_id.clone() {
                                            self.close_application_completely(&app_id);
                                        }
                                    },
                                }
                            }

                            self.is_dragging = false;
                            self.dragged_app_id = None;
                            self.menu_state.is_open = false; 
                            layer_changed = true;
                            break;
                        } else {
                            self.menu_state.is_open = false;
                            layer_changed = true;
                        }
                    }

                    // B. Hover Preview Menu Handling
                    if self.hover_state.is_visible { 
                        if let Some(ref app_id) = self.hover_state.app_id { 

                            if let Some(windows) = running_by_app.get(app_id) {
                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.hover_state.x,
                                    self.width as usize,
                                    self.height as usize,
                                    windows.len(),
                                    scale_factor,
                                );
                                if (self.pointer_x as f64) >= menu_x && (self.pointer_x as f64) <= menu_x + menu_width &&
                                   (self.pointer_y as f64) >= menu_y && (self.pointer_y as f64) <= menu_y + menu_height { 
                                    let item_h = 30;
                                    let idx = (((self.pointer_y as f64) - menu_y) as usize) / item_h; 
                                    if let Some(handle_id) = windows.get(idx) { 
                                        let sq_x = (menu_x as f64) + (menu_width as f64) - 25.0;
                                        let sq_y = (menu_y as f64) + ((idx * item_h) as f64) + 5.0;
                                        
                                        if (self.pointer_x as f64) >= sq_x && (self.pointer_x as f64) <= sq_x + 20.0 &&
                                           (self.pointer_y as f64) >= sq_y && (self.pointer_y as f64) <= sq_y + 20.0 {
                                            if let Some(win) = self.open_windows.get_mut(handle_id) {
                                                win.handle.close();
                                            }
                                        } else {
                                            if let Some(win) = self.open_windows.get_mut(handle_id) { 
                                                if let Some(seat) = &self.wl_seat { win.handle.activate(seat); } 
                                            }
                                        }
                                    }
                                    self.hover_state.is_visible = false; 
                                    layer_changed = true;
                                    break;
                                }
                            }
                        }
                    }

                    let mut was_dragging = false;
                    if self.is_dragging {
                        was_dragging = true;
                        if let Some(dragged_id) = &self.dragged_app_id {
                            if is_over_icons {
                                let mut dropped_idx = None;
                                for (index, _) in apps_in_dock.iter().enumerate() {
                                    let start_x = start_offset_x + spacing + index * (box_size + spacing);
                                    let hit_start_x = start_x.saturating_sub(spacing / 2);
                                    let hit_end_x = start_x + box_size + (spacing / 2);
                                    if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x {
                                        dropped_idx = Some(index);
                                        break;
                                    }
                                }
                                if dropped_idx.is_none() && self.pointer_x >= start_offset_x {
                                    dropped_idx = Some(apps_in_dock.len().saturating_sub(1));
                                }
                                
                                if let Some(target_idx) = dropped_idx {
                                    if let Some(target_app_id) = apps_in_dock.get(target_idx) {
                                        let old_idx_opt = self.pinned_apps.iter().position(|x| x == dragged_id);
                                        let new_idx_opt = self.pinned_apps.iter().position(|x| x == target_app_id);
                                        
                                        match (old_idx_opt, new_idx_opt) {
                                            (Some(old_idx), Some(new_idx)) => {
                                                if old_idx != new_idx {
                                                    let app = self.pinned_apps.remove(old_idx);
                                                    self.pinned_apps.insert(new_idx, app);
                                                    layer_changed = true;
                                                }
                                            },
                                            (None, Some(new_idx)) => {
                                                self.pinned_apps.insert(new_idx, dragged_id.clone());
                                                layer_changed = true;
                                            },
                                            (Some(old_idx), None) => {
                                                let app = self.pinned_apps.remove(old_idx);
                                                self.pinned_apps.push(app);
                                                layer_changed = true;
                                            },
                                            (None, None) => {
                                                self.pinned_apps.push(dragged_id.clone());
                                                layer_changed = true;
                                            }
                                        }
                                    }
                                }
                                crate::modules::persistence::save_pinned_apps(&self.pinned_apps.iter().cloned().collect());
                            } else {
                                if let Some(old_idx) = self.pinned_apps.iter().position(|x| x == dragged_id) {
                                    self.pinned_apps.remove(old_idx);
                                    crate::modules::persistence::save_pinned_apps(&self.pinned_apps.iter().cloned().collect());
                                    layer_changed = true;
                                }
                            }
                        }
                    }

                    if !was_dragging {
                        // C. Dock Icon Click Handling (Execute Launch/Focus)
                        if is_over_icons {
                            for (index, app_id) in apps_in_dock.iter().enumerate() {
                                let start_x = start_offset_x + spacing + index * (box_size + spacing); 
                                let end_x = start_x + box_size; 
                                let hit_start_x = start_x.saturating_sub(spacing / 2);
                                let hit_end_x = end_x + (spacing / 2);
                                if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                    if let Some(windows) = running_by_app.get(app_id) { 
                                        if let Some(handle_id) = windows.first() { 
                                            let was_active = self.open_windows.get(handle_id).map(|w| w.is_activated).unwrap_or(false); 
                                            if let Some(win) = self.open_windows.get_mut(handle_id) { 
                                                if was_active { win.handle.set_minimized(); } 
                                                else { if let Some(seat) = &self.wl_seat { win.handle.activate(seat); } } 
                                            }
                                        }
                                    } else {
                                        let launcher_path = get_launcher_path();
                                        
                                        let mut normalized_app_id = app_id.clone();
                                        if !normalized_app_id.starts_with("steam_icon_") {
                                            if let Some(idx) = normalized_app_id.rfind('_') {
                                                if normalized_app_id[idx+1..].chars().all(|c| c.is_numeric()) {
                                                    normalized_app_id = normalized_app_id[..idx].to_string();
                                                }
                                            }
                                        }
                                        if normalized_app_id.to_lowercase().contains("transmission") {
                                            normalized_app_id = "transmission-gtk".to_string();
                                        }

                                        let _ = std::process::Command::new("sh").arg(launcher_path).arg(normalized_app_id).spawn();
                                    }
                                    break;
                                }
                            }
                        }
                    }
                    
                    self.is_dragging = false;
                    self.dragged_app_id = None;
                }
            }
        }

        // --- STEP 5: Request Render Pipeline Synchronously ---
        if layer_changed {
            self.needs_redraw = true; 
        }
    }
}

// =========================================================================
// Foreign Toplevel Event Handlers
// =========================================================================
impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for AppState {
    fn event(
        state: &mut Self,
        _proxy: &ZwlrForeignToplevelManagerV1,
        event: zwlr_foreign_toplevel_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            let window_id = toplevel.id().protocol_id() as u64;
            state.open_windows.entry(toplevel.id()).or_insert_with(|| {
                WindowDiagnostics::new(window_id, toplevel.clone())
            });
        }
    }

    event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        0 => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}
// =========================================================================
// ZwlrForeignToplevelHandleV1 Dispatcher
// =========================================================================
impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let window_id = handle.id().protocol_id() as u64;
        state.open_windows.entry(handle.id()).or_insert_with(|| {
            WindowDiagnostics::new(window_id, handle.clone())
        });

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::OutputEnter { output } => {
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    if !window.outputs.contains(&output) {
                        window.outputs.push(output);
                    }
                }
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::OutputLeave { output } => {
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.outputs.retain(|o| o != &output);
                }
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                state.open_windows.remove(&handle.id());
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.update_window_icon(handle.id());
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.title = title;
                    window.icon_resolved = false;
                }
                state.update_window_icon(handle.id());
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.app_id = app_id.clone();
                    window.app_name = app_id.clone();
                    window.icon_resolved = false;
                }
                state.update_window_icon(handle.id());
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
                let (activated, minimized) = parse_window_states(&state_bytes);
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.is_activated = activated;
                    window.is_minimized = minimized;
                    window.is_pending = false; 
                }
                state.needs_redraw = true; // ✅ Changed from state.draw(qh)
            }
            _ => {}
        }
    }
}

// =========================================================================
// Standard SCTK Macro Framework Delegates
// =========================================================================
delegate_registry!(AppState);
delegate_compositor!(AppState);
delegate_output!(AppState);
delegate_layer!(AppState);
delegate_shm!(AppState);
delegate_seat!(AppState);
delegate_pointer!(AppState);
