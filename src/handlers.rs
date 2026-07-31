use crate::modules::context_menu::{MENU_WIDTH, MENU_HEIGHT, MENU_ITEM_HEIGHT, get_hover_menu_bounds};
use std::collections::HashMap;
pub use crate::models::LastState;
use crate::models::WindowDiagnostics;

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

use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};
use wayland_client::backend::ObjectId;
use crate::AppState;

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

impl CompositorHandler for AppState {
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: &WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: &WlOutput) {}
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: i32) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: u32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wayland_client::protocol::wl_surface::WlSurface, _: Transform) {}
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
        layer.set_anchor(Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);

        // Render the buffer configuration cleanly inside the correct dimensions
        self.draw(qh);
    }
}
impl ShmHandler for AppState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl OutputHandler for AppState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: WlOutput) {}
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
        }
    }
}
// =========================================================================
// Corrected Pointer Interaction Tracking Logic
// =========================================================================
impl PointerHandler for AppState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
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
                        if self.is_dragging {
                            // Draw immediately in the handler — blocking_dispatch already
                            // read the latest socket data so this is the freshest position.
                            // Rate-limit to ~4ms (240fps) to avoid overwhelming the compositor.
                            let now = std::time::Instant::now();
                            if now.duration_since(self.last_drag_draw).as_millis() >= 4 {
                                self.last_drag_draw = now;
                                self.draw(qh);
                                let _ = self.connection.flush();
                            }
                        }
                    } else {
                        layer_changed = true; // normal hover updates
                    }
                }
                PointerEventKind::Leave { .. } => {
                    // FIX: When the pointer completely leaves the dock window, clear everything 
                    // instantly. This avoids hanging when no further pointer events are sent.
                    if self.hover_state.is_visible || self.menu_state.is_open {
                        self.hover_state.is_visible = false;
                        self.hover_state.app_id = None;
                        self.menu_state.is_open = false;
                        layer_changed = true;
                    }
                }
                _ => {}
            }
        }

        // --- STEP 2: Unified Layout Metrics ---
        let current_surface_height = self.height as usize; 
        let dock_height = 60; 
        let box_size = 48; 
        let spacing = 12; 

        let mut apps_in_dock = Vec::new(); 
        let mut running_by_app: HashMap<String, Vec<ObjectId>> = HashMap::new(); 
        
        for app_id in &self.pinned_apps { apps_in_dock.push(app_id.clone()); } 
        let mut sorted_windows: Vec<(&ObjectId, &WindowDiagnostics)> = self.open_windows.iter().collect(); 
        sorted_windows.sort_by(|a, b| a.1.app_name.cmp(&b.1.app_name)); 
        
        for (id, win) in &sorted_windows {
            let app_id = if !win.app_id.is_empty() {
                win.app_id.clone()
            } else if !win.title.is_empty() {
                win.title.clone()
            } else {
                "Unknown".to_string()
            };

            running_by_app.entry(app_id.clone()).or_insert_with(Vec::new).push((*id).clone()); 
            if !apps_in_dock.contains(&app_id) { apps_in_dock.push(app_id); } 
        }
        
        let total_items = apps_in_dock.len(); 
        let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 }; 
        let start_offset_x = if (self.width as usize) > content_width { (self.width as usize - content_width) / 2 } else { 0 }; 
        
        let dock_top_bound = current_surface_height.saturating_sub(dock_height);

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
                        self.hover_state.x, self.width, self.height, windows.len()
                    );
                    
                    if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                       self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height {
                        should_be_visible = true;
                        new_app_id = Some(app_id.clone());
                        new_x = self.hover_state.x;
                    }
                }
            }
        }
		// ==========================================
        //  ADD THIS: HOVER STAY-ALIVE GRACE PERIOD
        // ==========================================
        if !should_be_visible && self.hover_state.is_visible {
            // Start the clock the exact frame the mouse leaves a valid hover zone
            if self.hover_state.last_leave_time.is_none() {
                self.hover_state.last_leave_time = Some(std::time::Instant::now());
            }
            
            if let Some(leave_time) = self.hover_state.last_leave_time {
                // Change 400 to whatever millisecond threshold feels best for you
                if leave_time.elapsed().as_millis() < 1000 { 
                    should_be_visible = true; // Force it to stay open
                    new_app_id = self.hover_state.app_id.clone();
                    new_x = self.hover_state.x;
                } else {
                    self.hover_state.last_leave_time = None; // Time's up! Clean up timer
                }
            }
        } else {
            // Mouse is back over an icon or the preview window, reset the clock
            self.hover_state.last_leave_time = None;
        }
        // ==========================================		
        // FIX: Quick dismiss if context menu is open but mouse wiggles away from it inside the dock
        if self.menu_state.is_open {
            let menu_width = MENU_WIDTH as usize;
            let menu_height = MENU_HEIGHT as usize;
            let menu_x = self.menu_state.x.min((self.width as usize).saturating_sub(menu_width));
            let menu_y = self.menu_state.y.saturating_sub(menu_height);
            let leeway = 20;

            if self.pointer_x < menu_x.saturating_sub(leeway)
                || self.pointer_x > menu_x + menu_width + leeway
                || self.pointer_y < menu_y.saturating_sub(leeway)
                || self.pointer_y > menu_y + menu_height + leeway
            {
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

        // --- STEP 4: Instantly Handle Clicks ---
        for event in events {
            if let PointerEventKind::Press { button, .. } = event.kind { 
                if button == 272 { // Left Click
                    


                    // C. Dock Icon Click Handling (Initiate Drag)
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
                } else if button == 273 { // Right Click
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x = start_offset_x + spacing + index * (box_size + spacing); 
                            let end_x = start_x + box_size; 
                            let hit_start_x = start_x.saturating_sub(spacing / 2);
                            let hit_end_x = end_x + (spacing / 2);
                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                println!("[DEBUG] Right-clicked index {}, app_id: '{}'", index, app_id);
                                self.menu_state.is_open = true; 
                                self.menu_state.x = self.pointer_x; 
                                self.menu_state.y = self.pointer_y; 
                                self.menu_state.target_app_id = Some(app_id.clone()); 
                                let windows = running_by_app.get(app_id).map(|v| v.clone()).unwrap_or_default(); 
                                println!("[DEBUG] Windows for app '{}': {:?}", app_id, windows);
                                self.menu_state.target_window = windows.iter()
                                    .find(|id| self.open_windows.get(id).map(|w| w.is_activated).unwrap_or(false)) 
                                    .cloned() 
                                    .or_else(|| windows.first().cloned()); 
                                layer_changed = true;
                                break;
                            }
                        }
                    }
                }
            } else if let PointerEventKind::Release { button, .. } = event.kind {
                if button == 272 { // Left Click Release
                    // A. Context Menu Handling
                    if self.menu_state.is_open { 
                        let menu_width = MENU_WIDTH as usize; 
                        let menu_height = MENU_HEIGHT as usize; 
                        let menu_x = self.menu_state.x.min((self.width as usize).saturating_sub(menu_width)); 
                        let menu_y = self.menu_state.y.saturating_sub(menu_height); 

                        if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                           self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height { 

                            let item_h = MENU_ITEM_HEIGHT as usize; 
                            let clicked_item = (self.pointer_y - menu_y) / item_h; 
                            let app_id = self.menu_state.target_app_id.clone(); 
                            
                            if let Some(app_id) = app_id { 
                                match clicked_item {
                                    0 => {
                                        if let Some(handle_id) = &self.menu_state.target_window { 
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { 
                                                if let Some(seat) = &self.wl_seat { window_info.handle.activate(seat); } 
                                            }
                                        }
                                    },
                                    1 => {
                                        let launcher_path = if std::path::Path::new("./launcher.sh").exists() { "./launcher.sh".to_string() }
                                                            else { "/usr/share/dockman/launcher.sh".to_string() };
                                        println!("[DEBUG] Launching app via context menu: '{}'", app_id);
                                        let _ = std::process::Command::new("sh").arg(launcher_path).arg(app_id).spawn();
                                    },
                                    2 => {
                                        if let Some(handle_id) = &self.menu_state.target_window { 
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { 
                                                window_info.handle.set_minimized(); 
                                            }
                                        }
                                    },
                                    3 => {
                                        if let Some(handle_id) = &self.menu_state.target_window { 
                                            if let Some(window_info) = self.open_windows.get_mut(handle_id) { 
                                                window_info.handle.close(); 
                                            }
                                        }
                                    },
									4 => {
                                        // NORMALIZE ID BEFORE PINNING
                                        let mut app_id = app_id;
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
                                        println!("[DEBUG] Pin toggle request for normalized app_id: '{}'", app_id);
									    if pinned.contains(&app_id) { 
                                            println!("[DEBUG] Removing app from pins: '{}'", app_id);
									        pinned.retain(|x| x != &app_id); 
									        // Optional: you can delete the .raw file here if you want clean-up on unpin
									    } else { 
                                            println!("[DEBUG] Adding app to pins: '{}'", app_id);
									        pinned.push(app_id.clone()); 
									        
									        // INTERCEPT & CACHE IMAGE PERMANENTLY
									        // First check our temporary dynamic icon cache
									        if let Some((rgba, size)) = self.icon_cache.get(&app_id) {
									            crate::cache::save_cached_icon(&app_id, *size, *size, rgba);
									        } 
									        // Fallback: extract directly from the active window payload
									        else if let Some(window_info) = self.open_windows.values().find(|w| w.app_id == app_id) {
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
									},
                                    _ => {}
                                }
                            }
                            self.menu_state.is_open = false; 
                            self.draw(qh); 
                            return;
                        } else {
                            // Clicked outside context menu bounds -> dismiss instantly
                            self.menu_state.is_open = false;
                            layer_changed = true;
                        }
                    }

                    // B. Hover Preview Menu Handling
                    if self.hover_state.is_visible { 
                        let menu_width = 200; 
                        let item_h = 30; 
                        if let Some(ref app_id) = self.hover_state.app_id { 
                            if let Some(windows) = running_by_app.get(app_id) { 
                                let menu_height = windows.len() * item_h; 
                                let menu_x = self.hover_state.x.saturating_sub(menu_width / 2).min((self.width as usize).saturating_sub(menu_width)); 
                                let menu_y = dock_top_bound.saturating_sub(menu_height + 10); 

                                if self.pointer_x >= menu_x && self.pointer_x <= menu_x + menu_width &&
                                   self.pointer_y >= menu_y && self.pointer_y <= menu_y + menu_height { 
                                    let idx = (self.pointer_y - menu_y) / item_h; 
                                    if let Some(handle_id) = windows.get(idx) { 
                                        let sq_x = menu_x + menu_width - 25;
                                        let sq_y = menu_y + idx * item_h + 5;
                                        
                                        if self.pointer_x >= sq_x && self.pointer_x <= sq_x + 20 &&
                                           self.pointer_y >= sq_y && self.pointer_y <= sq_y + 20 {
                                            // Close button clicked
                                            if let Some(win) = self.open_windows.get_mut(handle_id) {
                                                win.handle.close();
                                            }
                                        } else {
                                            // Normal row clicked (activate)
                                            if let Some(win) = self.open_windows.get_mut(handle_id) { 
                                                if let Some(seat) = &self.wl_seat { win.handle.activate(seat); } 
                                            }
                                        }
                                    }
                                    self.hover_state.is_visible = false; 
                                    self.draw(qh); 
                                    return;
                                }
                            }
                        }
                    }

                    let mut was_dragging = false;
                    if self.is_dragging {
                        was_dragging = true;
                        // Handle Drop (Reorder/Pin/Unpin)
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
                                // Dropped outside the dock! Unpin it!
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
                                        let launcher_path = if std::path::Path::new("./launcher.sh").exists() { "./launcher.sh".to_string() }
                                                            else { "/usr/share/dockman/launcher.sh".to_string() };
                                        
                                        // NORMALIZE ID BEFORE LAUNCHING
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

                                        println!("[DEBUG] Launching app via icon click: '{}' (normalized to: '{}')", app_id, normalized_app_id);
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

        if self.needs_redraw {
            self.draw(qh);
            self.needs_redraw = false;
            let _ = self.connection.flush();
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
        qh: &QueueHandle<Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            state.open_windows.entry(toplevel.id()).or_insert_with(|| {
                WindowDiagnostics::new(toplevel.clone())
            });
        }
    }

	// Use the macro here instead of a manual function
    event_created_child!(AppState, ZwlrForeignToplevelManagerV1, [
        // You must find the opcode in the protocol documentation or by looking at the generated code
        // For ToplevelManager, the "toplevel" event is what creates the child.
        // It is typically index 0 in the protocol definition.
        0 => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}
impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for AppState {
    fn event(
        state: &mut Self,
        handle: &ZwlrForeignToplevelHandleV1,
        event: <ZwlrForeignToplevelHandleV1 as Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        println!("[WAYLAND TRACKER EVENT ARRIVED] Handle: {:?}, Got event: {:?}", handle.id(), event);

        state.open_windows.entry(handle.id()).or_insert_with(|| {
            WindowDiagnostics::new(handle.clone())
        });

        match event {
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                println!("[WAYLAND DETECTOR] Window Closed: {:?}", handle);
                state.open_windows.remove(&handle.id());
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                state.update_window_icon(handle.id());
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::Title { title } => {
                println!("[WAYLAND DETECTOR] Title changed: {}", title);
                if let Some(window) = state.open_windows.get_mut(&handle.id()) {
                    window.title = title;
                    window.icon_resolved = false;
                }
                state.update_window_icon(handle.id());
                state.draw(qh);
            }
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                println!("[WAYLAND DETECTOR] AppId registered string payload: '{}'", app_id);
				if let Some(window) = state.open_windows.get_mut(&handle.id()) {
				    window.app_id = app_id.clone();
				    window.app_name = app_id.clone();
				    window.icon_resolved = false;
				}
                state.update_window_icon(handle.id());
				state.draw(qh);
            }
			zwlr_foreign_toplevel_handle_v1::Event::State { state: state_bytes } => {
			    let (activated, minimized) = parse_window_states(&state_bytes);
			    
			    if let Some(window) = state.open_windows.get_mut(&handle.id()) {
			        // The compositor is telling us the current reality. 
			        // Sync our local state to this reality.
			        window.is_activated = activated;
			        window.is_minimized = minimized;
			        
			        // Now that the reality matches our intent, clear pending
			        window.is_pending = false; 
			    }
			    state.draw(qh);
			}
            _ => {println!("[WAYLAND] Unmatched event received: {:?}", event);}
            
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
