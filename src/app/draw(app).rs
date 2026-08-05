// In src/app/draw.rs

use std::collections::HashMap;

use smithay_client_toolkit::shell::WaylandSurface;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::app::types::{DockState, PinItem, PopupSurface};
use crate::models::WindowDiagnostics;
use crate::Rect;
use crate::render;
use crate::render::dock::DockRenderResources;
use crate::graphics::fade::FadeAnimation;

use crate::state::AppState;

impl AppState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>) {
        let box_size = 48;
        let spacing = 8; // Unified icon spacing constant across layout calculation and rendering
        let max_dock_width = 800;

        // 1. Compute complete app list ONCE before mutably iterating over self.docks
        let apps_in_dock = self.get_apps_in_dock();

        for app_id in &apps_in_dock {
            if !self.icon_cache.contains_key(app_id) {
                self.request_icon_load(app_id.to_string());
            }
        }

        // =========================================================================
        // PRE-RENDER TICK & BOUNDS CALCULATION
        // =========================================================================
        self.needs_redraw = false;
        let is_animating = (self.hide_state.current_alpha - self.hide_state.target_alpha).abs() >= 0.001;

        // Pre-calculate context menu bounds before borrowing self.docks mutably to prevent E0502 borrow errors
        let show_menu = self.menu_state.is_open && !self.menu_state.items.is_empty();
        let _context_menu_bounds = if show_menu {
            let total_items = apps_in_dock.len();
            let calculated_width = if total_items > 0 {
                (total_items * box_size + (total_items + 1) * spacing) as u32
            } else {
                100
            };
            let dock_width = calculated_width.min(max_dock_width);
            let phys_w = (dock_width as f64 * self.scale_factor).round() as i32;
            let phys_h = (60.0 * self.scale_factor).round() as i32;

            Some(self.get_context_menu_bounds(phys_w, phys_h, self.scale_factor as f32))
        } else {
            None
        };

        // 2. Process each dock display output
        for dock in &mut self.docks {
            if !dock.configured {
                continue;
            }

            let dock_output = dock.output.clone();

            let filtered_windows: HashMap<ObjectId, WindowDiagnostics> = self
                .open_windows
                .iter()
                .filter(|(_, win)| win.outputs.contains(&dock_output))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();

            // Group window diagnostics by app_id
            let mut running_by_app: HashMap<String, Vec<&WindowDiagnostics>> = HashMap::new();
            for win in filtered_windows.values() {
                let id = if !win.app_id.is_empty() {
                    win.app_id.clone()
                } else if !win.title.is_empty() {
                    win.title.clone()
                } else {
                    "Unknown".to_string()
                };
                running_by_app.entry(id).or_default().push(win);
            }

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

            surface.set_size(dock_width, dock_height);

            // Pre-compute physical screen dimensions early for coordinate bounds calculations
            let phys_width = (dock_width as f64 * dock_scale).round() as u32;
            let phys_height = (dock_height as f64 * dock_scale).round() as u32;

			// =========================================================================
            // HOVER WINDOW LIST SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            if !dock.hover_fade.is_fully_hidden() {
                if let Some(ref app_id) = self.hover_state.app_id {
                    let app_windows: Vec<&WindowDiagnostics> = running_by_app
                        .get(app_id)
                        .cloned()
                        .unwrap_or_default();

                    if !app_windows.is_empty() {
                        let total_apps = apps_in_dock_ptrs.len();
                        let hovered_app_index = apps_in_dock_ptrs
                            .iter()
                            .position(|id| id == app_id)
                            .unwrap_or(0);

                        let geometry = crate::geometry::WindowListGeometry::default();
                        let (menu_x, menu_y, p_width, p_height, logical_w, logical_h) = geometry.compute_bounds(
                            phys_width as i32,
                            phys_height as i32,
                            total_apps,
                            hovered_app_index,
                            app_windows.len(),
                            dock_scale,
                        );
                        let scale_i32 = dock_scale.round() as i32;

                        let popup = dock.hover_popup.get_or_insert_with(|| {
                            let surf = self.compositor_state.create_surface(qh);
                            let sub = self
                                .subcompositor
                                .as_ref()
                                .expect("Subcompositor global not bound")
                                .get_subsurface(&surf, surface.wl_surface(), qh, ());
                            sub.set_desync();

                            PopupSurface {
                                subsurface: sub,
                                surface: surf,
                                current_buffer: None,
                                width: 0,
                                height: 0,
                            }
                        });

                        popup.subsurface.set_position(
                            (menu_x as f64 / dock_scale).round() as i32,
                            (menu_y as f64 / dock_scale).round() as i32,
                        );

                        popup.width = logical_w;
                        popup.height = logical_h;

                        if p_width > 0 && p_height > 0 {
                            let (buffer, canvas) = self.pool.create_buffer(
                                p_width,
                                p_height,
                                p_width * 4,
                                wl_shm::Format::Argb8888,
                            ).expect("Failed to allocate hover popup buffer");

                            let mut sorted_windows = app_windows;
                            sorted_windows.sort_by(|a, b| {
                                a.title
                                    .cmp(&b.title)
                                    .then_with(|| std::ptr::from_ref(*a).cmp(&std::ptr::from_ref(*b)))
                            });

                            let local_ptr_x = ((self.interaction.pointer_position.x * dock_scale) - menu_x as f64).max(0.0) as usize;
                            let local_ptr_y = ((self.interaction.pointer_position.y * dock_scale) - menu_y as f64).max(0.0) as usize;

                            render::window_list::render_window_list_surface(
                                canvas,
                                p_width,
                                p_height,
                                dock_scale as f32,
                                &sorted_windows,
                                &mut self.font_manager,
                                local_ptr_x as i32,
                                local_ptr_y as i32,
                            );

                            // --- APPLY HOVER FADE ALPHA TO POPUP CANVAS ---
                            dock.hover_fade.apply_alpha_to_canvas(canvas);

                            popup.surface.set_buffer_scale(scale_i32);
                            buffer.attach_to(&popup.surface).expect("Failed to attach hover buffer");
                            popup.surface.damage_buffer(0, 0, p_width, p_height);

                            let compositor = self.compositor_state.wl_compositor();
                            let region = compositor.create_region(qh, ());
                            region.add(0, 0, logical_w as i32, logical_h as i32);
                            popup.surface.set_input_region(Some(&region));
                            region.destroy();

                            popup.surface.commit();
                            popup.current_buffer = Some(buffer);
                        } else if let Some(popup) = dock.hover_popup.take() {
                            popup.surface.attach(None, 0, 0);
                            popup.surface.commit();
                        }
                    } else if let Some(popup) = dock.hover_popup.take() {
                        popup.surface.attach(None, 0, 0);
                        popup.surface.commit();
                    }
                } else if let Some(popup) = dock.hover_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            } else if let Some(popup) = dock.hover_popup.take() {
                popup.surface.attach(None, 0, 0);
                popup.surface.commit();
            }
			// =========================================================================
            // CONTEXT MENU SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            let show_menu = self.menu_state.is_open && !self.menu_state.items.is_empty();

            if show_menu {
                let item_count = self.menu_state.items.len();
                let total_apps = apps_in_dock_ptrs.len();

                let target_index = self
                    .menu_state
                    .target_app_id
                    .as_ref()
                    .and_then(|app_id| apps_in_dock_ptrs.iter().position(|id| id == app_id))
                    .unwrap_or(0);

                let scale = dock_scale as f32;
                let icon_base_size = (box_size as f32 * scale) as i32;
                let icon_spacing = (spacing as f32 * scale) as i32;
                let total_icons_width = total_apps as i32 * icon_base_size + (total_apps as i32 - 1).max(0) * icon_spacing;
                let start_x_offset = (phys_width as i32 - total_icons_width) / 2;

                let anchor_x = if total_apps > 0 {
                    let pin_x = start_x_offset + target_index as i32 * (icon_base_size + icon_spacing);
                    pin_x + icon_base_size / 2
                } else {
                    phys_width as i32 / 2
                };
                let anchor_y = 0; // Anchored at the top of the dock to render above it just like the window list

                let geom = crate::geometry::context_menu::ContextMenuGeometry::default().compute_bounds(
                    anchor_x,
                    anchor_y,
                    phys_width as i32,
                    phys_height as i32,
                    item_count,
                    dock_scale,
                );

                let scale_i32 = dock_scale.round() as i32;

                let popup = dock.context_menu_popup.get_or_insert_with(|| {
                    let surf = self.compositor_state.create_surface(qh);
                    let sub = self
                        .subcompositor
                        .as_ref()
                        .expect("Subcompositor global not bound")
                        .get_subsurface(&surf, surface.wl_surface(), qh, ());
                    sub.set_desync();

                    PopupSurface {
                        subsurface: sub,
                        surface: surf,
                        current_buffer: None,
                        width: 0,
                        height: 0,
                    }
                });

				popup.subsurface.set_position(geom.x, geom.y);

                popup.width = geom.logical_width as u32;
                popup.height = geom.logical_height as u32;
                
                if geom.phys_width > 0 && geom.phys_height > 0 {
                    let (buffer, canvas) = self
                        .pool
                        .create_buffer(geom.phys_width, geom.phys_height, geom.phys_width * 4, wl_shm::Format::Argb8888)
                        .expect("Failed to allocate context menu popup buffer");

                    let local_ptr_x = ((self.interaction.pointer_position.x * dock_scale) - geom.x as f64).max(0.0) as i32;
                    let local_ptr_y = ((self.interaction.pointer_position.y * dock_scale) - geom.y as f64).max(0.0) as i32;

                    render::context_menu::render_context_menu_surface(
                        canvas,
                        geom.phys_width,
                        geom.phys_height,
                        dock_scale as f32,
                        &self.menu_state,
                        &mut self.font_manager,
                        local_ptr_x,
                        local_ptr_y,
                    );

                    popup.surface.set_buffer_scale(scale_i32);
                    buffer
                        .attach_to(&popup.surface)
                        .expect("Failed to attach context menu buffer");
                    popup.surface.damage_buffer(0, 0, geom.phys_width, geom.phys_height);

                    let compositor = self.compositor_state.wl_compositor();
                    let region = compositor.create_region(qh, ());
                    region.add(0, 0, geom.logical_width as i32, geom.logical_height as i32);
                    popup.surface.set_input_region(Some(&region));
                    region.destroy();

                    popup.surface.commit();
                    popup.current_buffer = Some(buffer);
                } else if let Some(popup) = dock.context_menu_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            } else if let Some(popup) = dock.context_menu_popup.take() {
                popup.surface.attach(None, 0, 0);
                popup.surface.commit();
            }
            // =========================================================================
            // MAIN DOCK RENDER
            // =========================================================================
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

            // Upstream layout calculation for the pure renderer
            let scale = dock_scale as f32;
            let icon_base_size = (box_size as f32 * scale) as usize;
            let icon_spacing = (spacing as f32 * scale) as usize;
            let scaled_dock_height = (dock_height as f64 * dock_scale).round() as usize;
            let dock_y = (phys_height as usize).saturating_sub(scaled_dock_height);

            let total_icons = apps_in_dock.len();
            let total_icons_width = total_icons * icon_base_size + (total_icons.saturating_sub(1)) * icon_spacing;
            let start_x_offset = (phys_width as usize).saturating_sub(total_icons_width) / 2;

            let pins: Vec<PinItem> = apps_in_dock
                .iter()
                .enumerate()
                .map(|(i, app_id)| {
                    let x = start_x_offset + i * (icon_base_size + icon_spacing);
                    let y = dock_y + (scaled_dock_height.saturating_sub(icon_base_size)) / 2;
                    let running_windows = running_by_app.get(app_id);
                    let is_running = running_windows.map_or(false, |w| !w.is_empty());
                    let running_count = running_windows.map_or(0, |w| w.len());

                    PinItem {
                        app_id: app_id.clone(),
                        x: x as i32,
                        y: y as i32,
                        size: icon_base_size,
                        scale_factor: scale,
                        is_running,
                        is_activated: false,
                        running_count,
                        badge_count: None,
                    }
                })
                .collect();

            let dock_state = DockState {
                pins,
                hovered_index: None,
                current_visibility_alpha: self.hide_state.current_alpha,
                panel_rect: Rect {
                    x: 0.0,
                    y: dock_y as f64,
                    width: phys_width as f64,
                    height: scaled_dock_height as f64,
                },
                fade: FadeAnimation::new(0.0, 1.0, 0.25),
            };

            dock.dock_state = Some(dock_state.clone());

            let mut render_res = DockRenderResources {
                phys_width,
                phys_height,
                running_by_app: &running_by_app,
                icon_cache: &self.icon_cache,
                badges: &self.badges,
                font_manager: &mut self.font_manager,
                fallback_anim: &self.fallback_anim,
                is_dragging: self.is_dragging,
                dragged_app_id: self.dragged_app_id.as_ref(),
                pointer_position: (
                    self.interaction.pointer_position.x as i32,
                    self.interaction.pointer_position.y as i32,
                ),
            };

            // 1. Render standard dock surface
            render::render_dock_surface(canvas, &dock_state, &mut render_res);

            // 2. Apply auto-hide transparency overlay
            self.hide_state.apply_alpha_to_canvas(canvas);

            // 3. Update input region for THIS dock
            let is_hidden = self.hide_state.is_fully_hidden();
            let container_start = 0;
            let container_w = dock_width as i32;

            dock.update_input_region(
                &self.compositor_state,
                is_hidden,
                container_start,
                container_w,
                qh,
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
