use crate::render::window_list::get_hover_menu_bounds;
pub use crate::models::LastState;
use crate::models::WindowDiagnostics;
use crate::app::AppState;

use std::collections::HashMap;

use smithay_client_toolkit::{
    seat::{
        pointer::{PointerEvent, PointerEventKind, PointerHandler},
    },
    shell::WaylandSurface,
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

        let scale_factor: f32 = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0) as f32;

        // --- Build running_by_app early for coordinate mapping ---
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
        for windows in running_by_app.values_mut() {
            windows.sort_by(|a, b| {
                let win_a = self.open_windows.get(a);
                let win_b = self.open_windows.get(b);
                let title_a = win_a.map(|w| w.title.as_str()).unwrap_or("");
                let title_b = win_b.map(|w| w.title.as_str()).unwrap_or("");
                title_a.cmp(title_b)
            });
        }

        // Helper to map surface-local popup coordinates to dock-local logical coordinates
        let dock_surface_ptr = self.docks.first().map(|d| d.surface.wl_surface());

        let map_coordinates = |event: &PointerEvent, state: &AppState, running_by_app: &HashMap<String, Vec<ObjectId>>| -> (f32, f32) {
            let (px, py) = event.position;
            
            if let Some(dock_surf) = dock_surface_ptr {
                let is_main_surface = event.surface == *dock_surf;
                if !is_main_surface {
                    // Check context menu first so it takes precedence over hover state
                    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
                        let phys_width = (state.width as f32 * scale_factor).round() as i32;
                        let phys_height = (state.height as f32 * scale_factor).round() as i32;
                        let (menu_x, menu_y, _, _) = state.get_context_menu_bounds(phys_width, phys_height, scale_factor);
                        return (px as f32 + (menu_x as f32 / scale_factor), py as f32 + (menu_y as f32 / scale_factor));
                    }

                    // Inside map_coordinates closure:
                    if state.hover_state.is_visible {
                        if let Some(ref app_id) = state.hover_state.app_id {
                            // Use `self.` instead of `state.`
                            let apps_in_dock = &self.pinned_apps; 

                            // Safely calculate the number of open windows for this specific app
                            // Note: Adjust `w.app_id` to whatever the actual field name is in WindowDiagnostics
                            let win_count = self.open_windows
                                .values()
                                .filter(|w| w.app_id == *app_id)
                                .count()
                                .max(1);

                            let total_apps = apps_in_dock.len();
                            let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                            // Explicitly mark literals as _f64 to fix the E0689 rounding error
                            let menu_w = (180.0_f64 * self.scale_factor).round() as i32;
                            let menu_h = (win_count as f64 * 30.0_f64 * self.scale_factor).round() as i32;
                            let scale_i32 = self.scale_factor.round() as i32;

                            let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                self.width as i32,
                                self.height as i32,
                                total_apps,
                                hovered_app_index,
                                menu_w,
                                menu_h,
                                scale_i32,
                            );
                            return (px as f32 + (menu_x as f32 / scale_factor), py as f32 + (menu_y as f32 / scale_factor));
                        }
                    }
                }
            }

            (px as f32, py as f32)
        };
        // --- STEP 1: Parse the Frame Packet & Handle Instant Leave ---
        for event in events {
            match event.kind {
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    let (mapped_x, mapped_y) = map_coordinates(event, self, &running_by_app);
                    self.pointer_x = mapped_x as i32;
                    self.pointer_y = mapped_y as i32;
                    
                    if self.dragged_app_id.is_some() {
                        let dx = mapped_x - self.drag_start_x as f32;
                        let dy = mapped_y - self.drag_start_y as f32;
                        if (dx * dx + dy * dy).sqrt() > 5.0 {
                            self.is_dragging = true;
                        }
                        if self.is_dragging {
                            let now = std::time::Instant::now();
                            if now.duration_since(self.last_drag_draw).as_millis() >= 4 {
                                self.last_drag_draw = now;
                                layer_changed = true;
                            }
                        }
                    } else {
                        layer_changed = true;
                    }
                }
                PointerEventKind::Leave { .. } => {
                    if let Some(dock_surf) = dock_surface_ptr {
                        if event.surface == *dock_surf {
                            // Defer dismissal to STEP 3 geometry/leeway checks 
                            // to allow smooth mouse transition into subsurfaces.
                        } else {
                            if self.hover_state.is_visible {
                                self.hover_state.is_visible = false;
                                self.hover_state.app_id = None;
                                layer_changed = true;
                                self.needs_redraw = true; // Force redraw to clear old frames
                            }
                            self.hover_state.last_leave_time = None;

                            if self.menu_state.is_open {
                                self.menu_state.is_open = false;
                                layer_changed = true;
                                self.needs_redraw = true; // Force redraw to clear old frames
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // --- STEP 2: Unified Layout Metrics ---
        let dock_height: i32 = 60 as i32;
        let box_size: i32 = 48 as i32;
        let spacing: i32 = 12 as i32;
        
        let _menu_width = (180.0 * scale_factor as f32).round() as i32;
        let _menu_item_height = (30.0 * scale_factor as f32).round() as i32;
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
        // ✅ FIX: Sort each app's window vector so click index as i32 matches render index as i32
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
        
        let total_items: i32 = apps_in_dock.len() as i32; 
        let content_width: i32 = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 }; 
        let start_offset_x: i32 = if (self.width as i32) > content_width { (self.width as i32 - content_width) / 2 } else { 0 }; 
        
        let phys_surface_height = (self.height as f32 * scale_factor as f32).round() as i32;
        let dock_top_bound = phys_surface_height.saturating_sub(dock_height as i32);

        // --- STEP 3: Unified Hover & Bounds Tracking ---
        let mut should_be_visible = false;
        let mut new_app_id = None;
        let mut new_x = self.hover_state.x;

        let is_over_icons = self.pointer_y >= dock_top_bound as i32;

        if is_over_icons {
            for (index, app_id) in apps_in_dock.iter().enumerate() {
                let start_x: i32 = start_offset_x as i32 + spacing as i32 + index as i32 * (box_size as i32 + spacing as i32); 
                let end_x: i32 = start_x + box_size as i32;
                let hit_start_x: i32 = start_x.saturating_sub(spacing as i32 / 2);
                let hit_end_x: i32 = end_x + (spacing as i32 / 2) as i32;
                
                if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x {
                    should_be_visible = true;
                    let new_x: i32 = start_x as i32 + box_size as i32 / 2;
                    new_app_id = Some(app_id.clone());
                    break;
                }
            }
        }

        if !should_be_visible && self.hover_state.is_visible {
            if let Some(ref app_id) = self.hover_state.app_id {
                if let Some(windows) = running_by_app.get(app_id) {
                    // Use `self.` instead of `state.`
                    let apps_in_dock = &self.pinned_apps; 

                    // Safely calculate the number of open windows for this specific app
                    // Note: Adjust `w.app_id` to whatever the actual field name is in WindowDiagnostics
                    let win_count = self.open_windows
                        .values()
                        .filter(|w| w.app_id == *app_id)
                        .count()
                        .max(1);

                    let total_apps = apps_in_dock.len();
                    let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                    // Explicitly mark literals as _f64 to fix the E0689 rounding error
                    let menu_w = (180.0_f64 * self.scale_factor).round() as i32;
                    let menu_h = (win_count as f64 * 30.0_f64 * self.scale_factor).round() as i32;
                    let scale_i32 = self.scale_factor.round() as i32;

                    let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                        self.width as i32,
                        self.height as i32,
                        total_apps,
                        hovered_app_index,
                        menu_w,
                        menu_h,
                        scale_i32,
                    );
                    
                    // Convert logical pointer coordinates to physical pixels to match menu bounds
                    let ptr_x: i32 = ((self.pointer_x as f32) * scale_factor as f32) as i32;
                    let ptr_y: i32 = ((self.pointer_y as f32) * scale_factor as f32) as i32;

                    // Strict menu bounds only (removes the gap leeway)
                    let inside_menu = ptr_x >= menu_x
                        && ptr_x <= (menu_x + menu_width)
                        && ptr_y >= menu_y
                        && ptr_y <= (menu_y + menu_height);

                    if inside_menu {
                        should_be_visible = true;
                        new_app_id = Some(app_id.clone());
                        new_x = self.hover_state.x;
                    }
                }
            }
        }

        // --- Context Menu Dismissal Leeway & Hit Testing ---
        if self.menu_state.is_open && !self.menu_state.items.is_empty() {
            let scale_factor = (self.docks.first().map(|d| d.scale_factor as f32).unwrap_or(1.0)) as f32;
            let phys_width = (self.width as f32 * scale_factor as f32).round() as i32;
            let phys_height = (self.height as f32 * scale_factor as f32).round() as i32;

            let (menu_x, menu_y, menu_width, total_menu_h) = self.get_context_menu_bounds(phys_width, phys_height, scale_factor as f32);
            
            // Convert logical pointer to physical pixels
            let ptr_x: i32 = ((self.pointer_x as f32) * scale_factor as f32) as i32;
            let ptr_y: i32 = ((self.pointer_y as f32) * scale_factor as f32) as i32;
            let leeway = (20.0 * scale_factor as f32).round() as i32;

            let inside_extended: bool = ptr_x >= (menu_x as i32) - leeway
                && ptr_x <= (menu_x + menu_width) as i32 + leeway
                && ptr_y >= (menu_y as i32) - leeway
                && ptr_y <= (menu_y + total_menu_h) as i32 + leeway;

            if !inside_extended {
                self.menu_state.is_open = false;
                layer_changed = true;
                self.needs_redraw = true; // Force compositor frame update to clear garbage
            }
        }

        // --- 1-Second Delay Applied Only When Leaving Main Dock ---
        let mut effective_should_be_visible = should_be_visible;

        // Check if cursor is strictly outside the main dock bounding box area (using i32 since pointer coordinates are i32)
        let pointer_on_main_dock = self.pointer_x >= 0 
            && self.pointer_x <= self.width as i32 
            && self.pointer_y >= 0 
            && self.pointer_y <= self.height as i32;

        if !effective_should_be_visible && self.hover_state.is_visible {
            // Only trigger the delay if the cursor is completely off the main dock
            if !pointer_on_main_dock {
                let leave_time = *self.hover_state.last_leave_time.get_or_insert_with(std::time::Instant::now);
                
                if leave_time.elapsed() < std::time::Duration::from_secs(1) {
                    effective_should_be_visible = true;
                    self.needs_redraw = true; // Keep frame loop active during grace period
                } else {
                    self.hover_state.last_leave_time = None;
                }
            } else {
                // If still on the main dock context, reset timer and dismiss immediately if requested
                self.hover_state.last_leave_time = None;
            }
        } else if effective_should_be_visible {
            self.hover_state.last_leave_time = None;
        }

        if effective_should_be_visible != self.hover_state.is_visible || new_app_id != self.hover_state.app_id {
            self.hover_state.is_visible = effective_should_be_visible;
            self.hover_state.app_id = new_app_id;
            self.hover_state.x = new_x;
            layer_changed = true;
            self.needs_redraw = true; // Force compositor frame update to clear garbage
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
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
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
                                // Use `self.` instead of `state.`
                                let apps_in_dock = &self.pinned_apps; 

                                // Safely calculate the number of open windows for this specific app
                                // Note: Adjust `w.app_id` to whatever the actual field name is in WindowDiagnostics
                                let win_count = self.open_windows
                                    .values()
                                    .filter(|w| w.app_id == *app_id)
                                    .count()
                                    .max(1);

                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                                // Explicitly mark literals as _f64 to fix the E0689 rounding error
                                let menu_w = (180.0_f64 * self.scale_factor).round() as i32;
                                let menu_h = (win_count as f64 * 30.0_f64 * self.scale_factor).round() as i32;
                                let scale_i32 = self.scale_factor.round() as i32;

                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.width as i32,
                                    self.height as i32,
                                    total_apps,
                                    hovered_app_index,
                                    menu_w,
                                    menu_h,
                                    scale_i32,
                                );
                                let ptr_x: i32 = ((self.pointer_x as f32) * scale_factor as f32) as i32;
                                let ptr_y: i32 = ((self.pointer_y as f32) * scale_factor as f32) as i32;

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
                            let start_x: i32 = start_offset_x + spacing + index as i32 * (box_size + spacing); 
                            let end_x: i32 = start_x + box_size; 
                            let hit_start_x: i32 = start_x.saturating_sub(spacing / 2);
                            let hit_end_x: i32 = end_x + (spacing / 2);
                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                self.dragged_app_id = Some(app_id.clone());
                                self.drag_start_x = self.pointer_x as i32;
                                self.drag_start_y = self.pointer_y as i32;
                                self.is_dragging = false;
                                break;
                            }
                        }
                    }
                // In PointerHandler::pointer_frame during Right Click press (button == 273):
                } else if button == 273 { // Right Click
                    if is_over_icons {
                        for (index, app_id) in apps_in_dock.iter().enumerate() {
                            let start_x: i32 = start_offset_x + spacing + index as i32 as i32 * (box_size + spacing as i32); 
                            let hit_start_x: i32 = start_x.saturating_sub(spacing as i32 / 2);
                            let hit_end_x: i32 = start_x + box_size as i32 + (spacing as i32 / 2);
                            
                            if self.pointer_x >= hit_start_x && self.pointer_x <= hit_end_x { 
                                self.hover_state.is_visible = false;
                                self.hover_state.app_id = None;
                                self.menu_state.is_open = true;
                                self.needs_redraw = true;
                                self.menu_state.x = self.pointer_x as usize; 
                                self.menu_state.y = self.pointer_y as usize;
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
                                // Use `self.` instead of `state.`
                                let apps_in_dock = &self.pinned_apps; 

                                // Safely calculate the number of open windows for this specific app
                                // Note: Adjust `w.app_id` to whatever the actual field name is in WindowDiagnostics
                                let win_count = self.open_windows
                                    .values()
                                    .filter(|w| w.app_id == *app_id)
                                    .count()
                                    .max(1);

                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                                // Explicitly mark literals as _f64 to fix the E0689 rounding error
                                let menu_w = (180.0_f64 * self.scale_factor).round() as i32;
                                let menu_h = (win_count as f64 * 30.0_f64 * self.scale_factor).round() as i32;
                                let scale_i32 = self.scale_factor.round() as i32;

                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.width as i32,
                                    self.height as i32,
                                    total_apps,
                                    hovered_app_index,
                                    menu_w,
                                    menu_h,
                                    scale_i32,
                                );
                                let ptr_x: i32 = ((self.pointer_x as f32) * scale_factor as f32) as i32;
                                let ptr_y: i32 = ((self.pointer_y as f32) * scale_factor as f32) as i32;
                                let item_h: i32 = (30.0 * scale_factor as f32).round() as i32;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width &&
                                ptr_y >= menu_y && ptr_y <= menu_y + menu_height {
                                    let idx: usize = ((ptr_y - menu_y) as usize) / item_h as usize;
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
                            let start_x = start_offset_x + spacing + index as i32 * (box_size + spacing);
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
                        let phys_width = (self.width as f32 * scale_factor as f32).round() as i32;
                        let phys_height = (self.height as f32 * scale_factor as f32).round() as i32;
                        let item_h = (30.0 * scale_factor as f32).round() as i32;

                        let (menu_x, menu_y, menu_width, total_menu_h) = self.get_context_menu_bounds(phys_width, phys_height, scale_factor as f32);
                        let ptr_x = (self.pointer_x as f32 * scale_factor as f32).round() as i32;
                        let ptr_y = (self.pointer_y as f32 * scale_factor as f32).round() as i32;

                        if ptr_x >= menu_x && ptr_x <= (menu_x + menu_width) &&
                           ptr_y >= menu_y && ptr_y <= (menu_y + total_menu_h) { 

                            let clicked_item_idx = ((ptr_y - menu_y) as i32) / item_h; 
                            if let Some(item) = self.menu_state.items.get(clicked_item_idx as usize) {
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
                                // Use `self.` instead of `state.`
                                let apps_in_dock = &self.pinned_apps; 

                                // Safely calculate the number of open windows for this specific app
                                // Note: Adjust `w.app_id` to whatever the actual field name is in WindowDiagnostics
                                let win_count = self.open_windows
                                    .values()
                                    .filter(|w| w.app_id == *app_id)
                                    .count()
                                    .max(1);

                                let total_apps = apps_in_dock.len();
                                let hovered_app_index = apps_in_dock.iter().position(|id| id == app_id).unwrap_or(0);

                                // Explicitly mark literals as _f64 to fix the E0689 rounding error
                                let menu_w = (180.0_f64 * self.scale_factor).round() as i32;
                                let menu_h = (win_count as f64 * 30.0_f64 * self.scale_factor).round() as i32;
                                let scale_i32 = self.scale_factor.round() as i32;

                                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                                    self.width as i32,
                                    self.height as i32,
                                    total_apps,
                                    hovered_app_index,
                                    menu_w,
                                    menu_h,
                                    scale_i32,
                                );
                                let ptr_x: i32 = (self.pointer_x as f32 * scale_factor as f32).round() as i32;
                                let ptr_y: i32 = (self.pointer_y as f32 * scale_factor as f32).round() as i32;
                                let item_h: i32 = (30.0 * scale_factor as f32).round() as i32;

                                if ptr_x >= menu_x && ptr_x <= menu_x + menu_width &&
                                ptr_y >= menu_y && ptr_y <= menu_y + menu_height { 
                                    let idx = ((ptr_y - menu_y) as i32) / item_h; 
                                    if let Some(handle_id) = windows.get(idx as usize) { 
                                        let sq_x = (menu_x as f32 + menu_width as f32 - (25.0 * scale_factor as f32)) as i32;
                                        let sq_y = (menu_y as f32 + idx as f32 * item_h as f32 + (5.0 * scale_factor as f32)) as i32;
                                        let sq_size = (20.0 * scale_factor as f32) as i32;
                                        
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
                                    let start_x = start_offset_x as i32 + spacing as i32 + index as i32 * (box_size as i32 + spacing as i32);
                                    let hit_start_x = (start_x.saturating_sub(spacing as i32 / 2)) as i32;
                                    let hit_end_x = (start_x as i32 + box_size as i32 + (spacing as i32 / 2)) as i32;
                                    if self.pointer_x as i32 >= hit_start_x && self.pointer_x as i32 <= hit_end_x {
                                        dropped_idx = Some(index as i32);
                                        break;
                                    }
                                }
                                if dropped_idx.is_none() && self.pointer_x as i32 >= start_offset_x as i32 {
                                    dropped_idx = Some(apps_in_dock.len().saturating_sub(1) as i32);
                                }
                                
                                if let Some(target_idx) = dropped_idx {
                                    if let Some(target_app_id) = apps_in_dock.get(target_idx as usize) {
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
                                let start_x: i32 = start_offset_x as i32 + spacing as i32 + index as i32 * (box_size as i32 + spacing as i32); 
                                let end_x: i32 = start_x + box_size as i32; 
                                let hit_start_x: i32 = (start_x.saturating_sub(spacing as i32 / 2)) as i32;
                                let hit_end_x: i32 = end_x + (spacing as i32 / 2);
                                if self.pointer_x as i32 >= hit_start_x && self.pointer_x as i32 <= hit_end_x { 
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
