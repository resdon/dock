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

        // 1. Gather all dock apps & queue missing icons
        let mut apps_in_dock = self.pinned_apps.clone();
        for win in self.open_windows.values() {
            if !apps_in_dock.contains(&win.app_id) {
                apps_in_dock.push(win.app_id.clone());
            }
        }

        for app_id in &apps_in_dock {
            if !self.icon_cache.contains_key(app_id) {
                self.request_icon_load(app_id.to_string());
            }
        }

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

            let surface = &dock.surface;
            let dock_scale = dock.scale_factor;
            let scale_int = dock_scale.round() as i32;

            // Update main dock surface size & input region
            surface.set_size(dock_width, dock_height);
            let compositor = self.compositor_state.wl_compositor();
            let region = compositor.create_region(qh, ());
            region.add(0, 0, dock_width as i32, dock_height as i32);
            surface.wl_surface().set_input_region(Some(&region));
            region.destroy();

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

                        popup.subsurface.set_position((menu_x as f32 / dock_scale as f32).round() as i32, (menu_y as f32 / dock_scale as f32).round() as i32);
                        popup.width = (menu_w as f32 / dock_scale as f32).round() as u32;
                        popup.height = (menu_h as f32 / dock_scale as f32).round() as u32;

                        let p_width = menu_w as i32;
                        let p_height = menu_h as i32;

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

            surface.wl_surface().set_buffer_scale(scale_int);
            buffer.attach_to(surface.wl_surface()).expect("Buffer attach failed");
            surface.wl_surface().damage_buffer(0, 0, phys_width as i32, phys_height as i32);
            surface.wl_surface().commit();

            dock.current_buffer = Some(buffer);
        }
    }
}