use crate::render::context_menu::{get_hover_menu_bounds};
pub use crate::models::LastState;
use crate::models::WindowDiagnostics;
use crate::app::AppState;

use std::collections::HashMap;

use smithay_client_toolkit::{
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
};

use wayland_client::{
    Connection, QueueHandle
};

use wayland_client::backend::ObjectId;


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
        // Scale layout constants to physical pixels
        let scale_factor = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0);
        let dock_height = 60_usize;
        let box_size = 48_usize;
        let spacing = 12_usize;
        
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

                                let ptr_x = (self.pointer_x as f64) * scale_factor;
                                let ptr_y = (self.pointer_y as f64) * scale_factor;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width &&
                                ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
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

                                let ptr_x = (self.pointer_x as f64) * scale_factor;
                                let ptr_y = (self.pointer_y as f64) * scale_factor;
                                let item_h = (30.0 * scale_factor).round() as usize;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width &&
                                ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
                                    let idx = ((ptr_y - menu_y) as usize) / item_h;
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
                                let launcher_path = crate::handlers::get_launcher_path();

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
                                                .arg(crate::handlers::get_launcher_path())
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

                                            let mut pinned = crate::cache::persistence::load_pinned_apps(); 
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
                                            crate::cache::persistence::save_pinned_apps(&pinned); 
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

                                let ptr_x = (self.pointer_x as f64) * scale_factor;
                                let ptr_y = (self.pointer_y as f64) * scale_factor;
                                let item_h = (30.0 * scale_factor).round() as usize;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width &&
                                ptr_y >= menu_y && ptr_y <= menu_y + menu_height { 
                                    let idx = ((ptr_y - menu_y) as usize) / item_h; 
                                    if let Some(handle_id) = windows.get(idx) { 
                                        let sq_x = menu_x + menu_width - (25.0 * scale_factor);
                                        let sq_y = menu_y + (idx * item_h) as f64 + (5.0 * scale_factor);
                                        let sq_size = 20.0 * scale_factor;
                                        
                                        if ptr_x >= sq_x && ptr_x <= sq_x + sq_size &&
                                        ptr_y >= sq_y && ptr_y <= sq_y + sq_size {
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
                                crate::cache::persistence::save_pinned_apps(&self.pinned_apps);
                            } else {
                                if let Some(old_idx) = self.pinned_apps.iter().position(|x| x == dragged_id) {
                                    self.pinned_apps.remove(old_idx);
                                    crate::cache::persistence::save_pinned_apps(&self.pinned_apps);
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
                                        let launcher_path = crate::handlers::get_launcher_path();
                                        
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
