pub use crate::models::LastState;
use crate::models::WindowDiagnostics;
use crate::app::AppState;

use std::os::fd::AsFd;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::env;
use std::path::Path;

use smithay_client_toolkit::{
    compositor::{CompositorHandler},
    delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryHandler, RegistryState},
    seat::{
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

use wayland_client::backend::ObjectData;

use wayland_client::protocol::wl_data_device::{Event as DndEvent, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

// Fractional scaling
use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::{self, WpFractionalScaleManagerV1},
    wp_fractional_scale_v1::{self, WpFractionalScaleV1},
};
// -----

// Resolved launcher.sh path
pub fn get_launcher_path() -> String {
    // 1. Dev Mode: Only if launched directly from repo root
    let dev_script = Path::new("./scripts/launcher.sh");
    if dev_script.exists() {
        return dev_script.to_string_lossy().into_owned();
    }

    // 2. User Local Install ($XDG_DATA_HOME/dock/launcher.sh or ~/.local/share/dock/launcher.sh)
    let data_home = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/.local/share", home)
    });
    let user_path = format!("{}/dock/launcher.sh", data_home);

    if Path::new(&user_path).exists() {
        return user_path;
    }

    // 3. System-wide Install (/usr/share/dock/launcher.sh)
    "/usr/share/dock/launcher.sh".to_string()
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

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        // Capture the structural allocations chosen by sctk/compositor
        self.width = configure.new_size.0.max(100);
        
        // Dynamically track what our height should be based on UI overlays
        let target_height = if self.menu_state.is_open || self.hover_state.is_visible { 200 } else { 60 };
        self.height = target_height;
        
        layer.set_size(self.width, self.height);
        layer.set_anchor(Anchor::BOTTOM);
        
        // Flag for redraw instead of calling draw(qh) directly to prevent borrow checker conflicts
        self.needs_redraw = true;
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
                // Kick off the icon load asynchronously without blocking state updates
                state.request_icon_load(app_id.clone());
                state.needs_redraw = true;
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
