// In src/app/draw.rs

use std::collections::HashMap;

use smithay_client_toolkit::shell::WaylandSurface;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::app::types::PopupSurface;
use crate::models::WindowDiagnostics;
use crate::render;
use crate::render::context_menu::get_context_menu_bounds;
use crate::render::window_list::get_hover_menu_bounds;

use super::state::AppState;

impl AppState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let box_size = 48;
        let spacing = 12;
        let max_dock_width = 800;

        // ✅ 1. Compute complete app list ONCE before mutably iterating over self.docks
        let apps_in_dock = self.get_apps_in_dock();

        for app_id in &apps_in_dock {
            if !self.icon_cache.contains_key(app_id) {
                self.request_icon_load(app_id.to_string());
            }
        }

        // =========================================================================
        // PRE-RENDER TICK (Run ONCE before iterating over surfaces/docks)
        // =========================================================================
        self.needs_redraw = false;
        let is_animating = (self.hide_state.current_alpha - self.hide_state.target_alpha).abs() >= 0.001;

        // 2. Process each dock display output
        for dock in &mut self.docks {
            let menu_is_open = self.menu_state.is_open;
            let menu_x_val = self.menu_state.x;
            let menu_y_val = self.menu_state.y;
            let menu_items_len = self.menu_state.items.len();
            let dock_output = dock.output.clone();

            let filtered_windows: HashMap<&ObjectId, &WindowDiagnostics> = self
                .open_windows
                .iter()
                .filter(|(_, win)| win.outputs.contains(&dock_output))
                .collect();

            let mut apps_in_dock_ptrs: Vec<&str> = self.pinned_apps.iter().map(|s| s.as_str()).collect();
            for window in filtered_windows.values() {
                let id = if !window.app_id.is_empty() {
                    window.app_id.as_str()
                } else if !window.title.is_empty() {
                    window.title.as_str()
                } else {
                    "Unknown"
                };

                if !apps_in_dock_ptrs.contains(&id) {
                    apps_in_dock_ptrs.push(id);
                }
            }

            let total_items = apps_in_dock_ptrs.len();
            let calculated_width = if total_items > 0 {
                (total_items * box_size + (total_items + 1) * spacing) as u32
            } else {
                100
            };

            let dock_width = calculated_width.min(max_dock_width);
            let dock_height = 60;

            dock.width = dock_width;
            dock.height = dock_height;

            // Update AppState dimensions
            self.width = dock_width as i32;
            self.height = dock_height as i32;

            let surface = &dock.surface;
            let dock_scale = dock.scale_factor;
            let scale_int = dock_scale.round() as i32;

            surface.set_size(dock_width, dock_height);

            // =========================================================================
            // HOVER WINDOW LIST SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            let show_hover = self.hover_state.is_visible;
            if show_hover {
                if let Some(ref app_id) = self.hover_state.app_id {
                    let count = filtered_windows
                        .values()
                        .filter(|w| {
                            let id = if !w.app_id.is_empty() { w.app_id.as_str() } else { "Unknown" };
                            id == app_id.as_str()
                        })
                        .count();

                    if count > 0 {
                        // 1. Retrieve the complete dock app list (pinned + active unpinned)
                        let total_apps = apps_in_dock.len();
                        let hovered_app_index = apps_in_dock
                            .iter()
                            .position(|id| id == app_id)
                            .unwrap_or(0);

                        // 2. Count open windows for this app (handling empty app_id fallback)
                        let win_count = self
                            .open_windows
                            .values()
                            .filter(|w| {
                                let win_id = if !w.app_id.is_empty() { &w.app_id } else { "Unknown" };
                                win_id == app_id
                            })
                            .count()
                            .max(1);

                        // 3. Scale menu dimensions
                        let menu_w = (180.0_f64 * dock_scale).round() as i32;
                        let menu_h = (win_count as f64 * 30.0_f64 * dock_scale).round() as i32;
                        let scale_i32 = dock_scale.round() as i32;

                        // 4. Calculate popup placement using dynamic dock dimensions instead of hardcoded 100x60
                        let (menu_x, menu_y, _menu_width, _menu_height) = get_hover_menu_bounds(
                            dock_width as i32,  // Or self.width if updated to dynamic dock_width
                            dock_height as i32, // Or self.height if updated to dynamic dock_height
                            total_apps,
                            hovered_app_index,
                            menu_w,
                            menu_h,
                            scale_i32,
                        );

                        // 5. Lazily initialize Wayland subsurface popup
                        let popup = dock.hover_popup.get_or_insert_with(|| {
                            let surf = self.compositor_state.create_surface(qh);
                            let sub = self.subcompositor.get_subsurface(&surf, surface.wl_surface(), qh, ());
                            sub.set_desync();
                            
                            // IMPORTANT: Ensure `subsurface` is declared ABOVE `surface` in src/app/types.rs!
                            PopupSurface {
                                subsurface: sub,
                                surface: surf,
                                current_buffer: None,
                                width: 0,
                                height: 0,
                            }
                        });

                        // 6. Convert physical positions back to surface coordinates
                        popup.subsurface.set_position(
                            (menu_x as f64 / dock_scale).round() as i32,
                            (menu_y as f64 / dock_scale).round() as i32,
                        );
                        popup.width = (menu_w as f64 / dock_scale).round() as u32;
                        popup.height = (menu_h as f64 / dock_scale).round() as u32;

                        let p_width = menu_w;
                        let p_height = menu_h;

                        if p_width > 0 && p_height > 0 {
                            let (buffer, canvas) = self.pool.create_buffer(
                                p_width,
                                p_height,
                                p_width * 4,
                                wl_shm::Format::Argb8888,
                            ).expect("Failed to allocate hover popup buffer");

                            let mut app_windows: Vec<&WindowDiagnostics> = filtered_windows
                                .values()
                                .filter(|w| {
                                    let id = if !w.app_id.is_empty() { w.app_id.as_str() } else { "Unknown" };
                                    id == app_id.as_str()
                                })
                                .copied()
                                .collect();

                            app_windows.sort_by(|a, b| {
                                a.title
                                    .cmp(&b.title)
                                    .then_with(|| std::ptr::from_ref(*a).cmp(&std::ptr::from_ref(*b)))
                            });
                            
                            let local_ptr_x = ((self.pointer_x as f32 * dock_scale as f32) - menu_x as f32).max(0.0) as usize;
                            let local_ptr_y = ((self.pointer_y as f32 * dock_scale as f32) - menu_y as f32).max(0.0) as usize;
                            
                            render::window_list::render_window_list_surface(
                                canvas,
                                p_width,
                                p_height,
                                dock_scale as f32,
                                &app_windows,
                                &self.font_manager,
                                local_ptr_x as i32,
                                local_ptr_y as i32,
                            );
                            
                            popup.surface.set_buffer_scale(scale_int);
                            buffer.attach_to(&popup.surface).expect("Failed to attach hover buffer");
                            popup.surface.damage_buffer(0, 0, p_width, p_height);

                            // --- HOVER INPUT REGION ---
                            let compositor = self.compositor_state.wl_compositor();
                            let region = compositor.create_region(qh, ());

                            // Input regions MUST be defined in logical coordinates
                            let logical_w = (p_width as f32 / dock_scale as f32).round() as i32;
                            let logical_h = (p_height as f32 / dock_scale as f32).round() as i32;
                            region.add(0, 0, logical_w, logical_h);
                            popup.surface.set_input_region(Some(&region));
                            region.destroy();

                            popup.surface.commit();
                            popup.current_buffer = Some(buffer);
                        } else {
                            if let Some(popup) = dock.hover_popup.take() {
                                popup.surface.attach(None, 0, 0);
                                popup.surface.commit();
                            }
                        }
                    } else {
                        if let Some(popup) = dock.hover_popup.take() {
                            popup.surface.attach(None, 0, 0);
                            popup.surface.commit();
                        }
                    }
                } else {
                    if let Some(popup) = dock.hover_popup.take() {
                        popup.surface.attach(None, 0, 0);
                        popup.surface.commit();
                    }
                }
            } else {
                if let Some(popup) = dock.hover_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            }

            // =========================================================================
            // CONTEXT MENU SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            if menu_is_open {
                let (menu_x, menu_y, menu_w, menu_h) = get_context_menu_bounds(
                    menu_x_val as i32,
                    menu_y_val as i32,
                    dock_width as i32,
                    dock_height as i32,
                    menu_items_len as i32,
                    dock_scale as f32,
                );

                let popup = dock.menu_popup.get_or_insert_with(|| {
                    let surf = self.compositor_state.create_surface(qh);
                    let sub = self.subcompositor.get_subsurface(&surf, surface.wl_surface(), qh, ());
                    sub.set_desync();
                    PopupSurface {
                        subsurface: sub, // Keep subsurface declared before surface to match types.rs
                        surface: surf,
                        current_buffer: None,
                        width: 0,
                        height: 0,
                    }
                });

                // --- ENSURE CONTEXT MENU STACKS ABOVE THE WINDOW LIST ---
                if let Some(ref hover_p) = dock.hover_popup {
                    popup.subsurface.place_above(&hover_p.surface);
                }

                popup.subsurface.set_position((menu_x as f64 / dock_scale).round() as i32, (menu_y as f64 / dock_scale).round() as i32);
                popup.width = (menu_w as f64 / dock_scale).round() as u32;
                popup.height = (menu_h as f64 / dock_scale).round() as u32;

                let p_width = (menu_w as f32).round() as i32;
                let p_height = (menu_h as f32).round() as i32;

                // 1. ADD ZERO-DIMENSION GUARD
                if p_width > 0 && p_height > 0 {
                    let (buffer, canvas) = self.pool.create_buffer(
                        p_width,
                        p_height,
                        p_width * 4,
                        wl_shm::Format::Argb8888,
                    ).expect("Failed to allocate context menu buffer");

                    let local_ptr_x = ((self.pointer_x as f64 * dock_scale) - menu_x as f64).max(0.0) as usize;
                    let local_ptr_y = ((self.pointer_y as f64 * dock_scale) - menu_y as f64).max(0.0) as usize;
                    
                    render::context_menu::render_context_menu_surface(
                        canvas,
                        p_width as i32,
                        p_height as i32,
                        dock_scale as f32,
                        &self.menu_state,
                        &self.font_manager,
                        local_ptr_x as i32,
                        local_ptr_y as i32,
                    );

                    popup.surface.set_buffer_scale(scale_int);
                    buffer.attach_to(&popup.surface).expect("Failed to attach menu buffer");
                    popup.surface.damage_buffer(0, 0, p_width, p_height);

                    // --- CONTEXT MENU INPUT REGION ---
                    let compositor = self.compositor_state.wl_compositor();
                    let region = compositor.create_region(qh, ());
                    
                    // 2. USE LOGICAL COORDINATES FOR WAYLAND INPUT
                    let logical_w = (p_width as f64 / dock_scale).round() as i32;
                    let logical_h = (p_height as f64 / dock_scale).round() as i32;
                    
                    region.add(0, 0, logical_w, logical_h);
                    popup.surface.set_input_region(Some(&region));
                    region.destroy();

                    popup.surface.commit();
                    popup.current_buffer = Some(buffer);
                } else {
                    if let Some(popup) = dock.menu_popup.take() {
                        popup.surface.attach(None, 0, 0);
                        popup.surface.commit();
                    }
                }
            } else {
                if let Some(popup) = dock.menu_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            }

            // =========================================================================
            // MAIN DOCK RENDER
            // =========================================================================
            let phys_width = (dock_width as f64 * dock_scale).round() as u32;
            let phys_height = (dock_height as f64 * dock_scale).round() as u32;
            let stride = phys_width * 4;

            if phys_width == 0 || phys_height == 0 {
                continue;
            }

            dock.current_buffer = None;

            let (buffer, canvas) = self
                .pool
                .create_buffer(
                    phys_width as i32,
                    phys_height as i32,
                    stride as i32,
                    wl_shm::Format::Argb8888,
                )
                .expect("Failed to allocate SHM buffer");

            let render_windows_map: HashMap<ObjectId, WindowDiagnostics> = filtered_windows
                .iter()
                .map(|(k, v)| ((*k).clone(), (*v).clone()))
                .collect();

            // 1. Render standard dock surface
            render::render_dock_surface(
                canvas,
                phys_width,
                phys_height,
                dock_scale,
                &render_windows_map,
                &self.pinned_apps,
                &self.icon_cache,
                &self.font_manager,
                self.is_dragging,
                self.dragged_app_id.as_ref(),
                self.pointer_x,
                self.pointer_y,
                &self.fallback_anim,
                &self.badges,
            );

            // 2. Apply auto-hide transparency overlay
            self.hide_state.apply_alpha_to_canvas(canvas);

            // 3. Update input region for THIS dock using disjoint field borrows
            let is_hidden = self.hide_state.is_fully_hidden();
            let container_start = 0;             
            let container_w = dock_width as i32; // ✅ Use dynamic dock_width instead of self.width

            dock.update_input_region(
                &self.compositor_state, 
                is_hidden, 
                container_start, 
                container_w, 
                qh
            );

            // 4. Request next frame BEFORE commit if animation is active
            if is_animating {
                surface.wl_surface().frame(qh, surface.wl_surface().clone());
            }

            surface.wl_surface().set_buffer_scale(scale_int);
            buffer.attach_to(surface.wl_surface()).expect("Buffer attach failed");
            surface.wl_surface().damage_buffer(0, 0, phys_width as i32, phys_height as i32);

            // 5. Single atomic commit for buffer + transparency input region
            surface.wl_surface().commit();

            dock.current_buffer = Some(buffer);
        }
    }
}