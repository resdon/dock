// In src/app/draw.rs

use std::collections::HashMap;

use smithay_client_toolkit::shell::WaylandSurface;

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::graphics::fade::FadeAnimation;
use crate::models::WindowDiagnostics;
use crate::render;
use crate::render::dock::DockRenderResources;
use crate::types::{DockState, PinItem, PopupSurface};
use crate::Rect;

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
        let _is_animating =
            (self.hide_state.current_alpha - self.hide_state.target_alpha).abs() >= 0.001;

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

            // Advance fade state timers for the current frame
            let hover_animating = dock.hover_fade.tick();
            let menu_animating = dock.menu_fade.tick();
            let window_list_animating = dock.window_list_fade.tick();
            let hide_animating =
                (self.hide_state.current_alpha - self.hide_state.target_alpha).abs() >= 0.001;

            // Ensure window_list_fade's active animation state keeps the frame loop running
            let fade_animating =
                dock.window_list_fade.current_alpha != dock.window_list_fade.target_alpha;
            let is_animating = hover_animating
                || menu_animating
                || window_list_animating
                || hide_animating
                || fade_animating;
            // -----

            let dock_output = dock.output.clone();

            // 1. Replace the filtered_windows block (around line 46):
            let filtered_windows: HashMap<&ObjectId, &WindowDiagnostics> = self
                .open_windows
                .iter()
                .filter(|(_, win)| win.outputs.contains(&dock_output))
                .collect();

            // Never use HashMap iteration order for visible dock/popup order.
            // WindowDiagnostics::id is stable for the lifetime of the window.
            let mut ordered_windows: Vec<&WindowDiagnostics> =
                filtered_windows.values().copied().collect();
            ordered_windows.sort_by_key(|win| win.id);

            // 2. Update running_by_app to push the borrowed reference (*win):
            let mut running_by_app: HashMap<String, Vec<&WindowDiagnostics>> = HashMap::new();
            for win in &ordered_windows {
                let id = if !win.app_id.is_empty() {
                    win.app_id.clone()
                } else if !win.title.is_empty() {
                    win.title.clone()
                } else {
                    "Unknown".to_string()
                };
                running_by_app.entry(id).or_default().push(*win);
            }

            // Keep window rows deterministic; filtered_windows is a HashMap.
            for windows in running_by_app.values_mut() {
                windows.sort_by_key(|win| win.id);
            }

            // 3. Update apps_in_dock_ptrs:
            let mut apps_in_dock_ptrs: Vec<&str> =
                self.pinned_apps.iter().map(|s| s.as_str()).collect();
            for window in &ordered_windows {
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
            let dock_height: u32 = 60; // Define full logical dock height

            // 1. Determine logical height based on current hide state
            let target_logical_height = dock_height;

            dock.width = dock_width;
            dock.height = target_logical_height;

            let surface = &dock.surface;
            let dock_scale = dock.scale_factor;
            let scale_int = dock_scale.round() as i32;

            // 2. Set surface size dynamically matching current visibility
            surface.set_size(dock_width, target_logical_height);

            // 3. Compute physical screen dimensions based on target height
            let phys_width = (dock_width as f64 * dock_scale).round() as u32;
            let phys_height = (target_logical_height as f64 * dock_scale).round() as u32;

            // =========================================================================
            // WINDOW LIST SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            let show_window_list = self.window_list_state.is_open;
            if show_window_list {
                if let Some(target_app_id) = &self.window_list_state.target_app_id {
                    let app_windows: Vec<&WindowDiagnostics> = running_by_app
                        .get(target_app_id)
                        .cloned()
                        .unwrap_or_default();

                    if !app_windows.is_empty() {
                        let total_apps = apps_in_dock_ptrs.len();
                        let target_index = apps_in_dock_ptrs
                            .iter()
                            .position(|id| id == target_app_id)
                            .unwrap_or(0);

                        let scale = dock_scale as f32;
                        let icon_base_size = (box_size as f32 * scale) as i32;
                        let icon_spacing = (spacing as f32 * scale) as i32;
                        let total_icons_width = total_apps as i32 * icon_base_size
                            + (total_apps as i32 - 1).max(0) * icon_spacing;
                        let start_x_offset = (phys_width as i32 - total_icons_width) / 2;

                        let anchor_x = if total_apps > 0 {
                            let pin_x = start_x_offset
                                + target_index as i32 * (icon_base_size + icon_spacing);
                            pin_x + icon_base_size / 2
                        } else {
                            phys_width as i32 / 2
                        };
                        let anchor_y = 0;

                        let item_count =
                            if let Some(ref target_app) = self.window_list_state.target_app_id {
                                running_by_app.get(target_app).map_or(0, |w| w.len()) as i32
                            } else {
                                0
                            };
                        let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;

                        let geom = PopupGeometry::compute_bounds(
                            PopupType::WindowList,
                            anchor_x,
                            anchor_y,
                            phys_width as i32,
                            phys_dock_height,
                            item_count,
                            dock_scale as f32,
                        );

                        let scale_i32 = dock_scale.round() as i32;

                        let popup = dock.window_list_popup.get_or_insert_with(|| {
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
                                current_app_id: Some(target_app_id.clone()),
                                position: (0, 0),
                            }
                        });

                        // Ensure current_app_id is updated if the popup instance was reused
                        popup.current_app_id = Some(target_app_id.clone());

                        let popup_x = (geom.x as f64 / dock_scale).round() as i32;
                        let popup_y = (geom.y as f64 / dock_scale).round() as i32;
                        popup.position = (popup_x, popup_y);
                        popup.subsurface.set_position(popup_x, popup_y);

                        popup.width = geom.logical_width as u32;
                        popup.height = geom.logical_height as u32;

                        if geom.phys_width > 0 && geom.phys_height > 0 {
                            let (buffer, canvas) = self
                                .pool
                                .create_buffer(
                                    geom.phys_width,
                                    geom.phys_height,
                                    geom.phys_width * 4,
                                    wl_shm::Format::Argb8888,
                                )
                                .expect("Failed to allocate window list popup buffer");

                            // Clear canvas memory to avoid recycled memory garbage
                            canvas.fill(0);
                            let local_ptr_x = ((self.interaction.pointer_position.x * dock_scale)
                                - geom.x as f64)
                                .max(0.0) as i32;
                            let local_ptr_y = ((self.interaction.pointer_position.y * dock_scale)
                                - geom.y as f64)
                                .max(0.0) as i32;

                            let empty_windows: &[&WindowDiagnostics] = &[];
                            let windows = self
                                .window_list_state
                                .target_app_id
                                .as_ref()
                                .and_then(|id| running_by_app.get(id))
                                .map(|v| v.as_slice())
                                .unwrap_or(empty_windows);

                            crate::render::popup::render_window_list_surface(
                                canvas, // Replace 'frame' with your actual canvas/buffer variable name
                                &geom,
                                dock_scale as f32,
                                windows,
                                &mut self.font_manager,
                                local_ptr_x,
                                local_ptr_y,
                            );

                            dock.window_list_fade.apply_alpha_to_canvas(canvas);

                            popup.surface.set_buffer_scale(scale_i32);
                            buffer
                                .attach_to(&popup.surface)
                                .expect("Failed to attach window list buffer");
                            popup
                                .surface
                                .damage_buffer(0, 0, geom.phys_width, geom.phys_height);

                            let compositor = self.compositor_state.wl_compositor();
                            let region = compositor.create_region(qh, ());
                            region.add(0, 0, geom.phys_width, geom.phys_height);
                            popup.surface.set_input_region(Some(&region));
                            region.destroy();

                            popup.surface.commit();
                            popup.current_buffer = Some(buffer);
                        } else if let Some(popup) = dock.window_list_popup.take() {
                            popup.surface.attach(None, 0, 0);
                            popup.surface.commit();
                        }
                    } else if let Some(popup) = dock.window_list_popup.take() {
                        popup.surface.attach(None, 0, 0);
                        popup.surface.commit();
                    }
                } else if let Some(popup) = dock.window_list_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            } else if let Some(popup) = dock.window_list_popup.take() {
                popup.surface.attach(None, 0, 0);
                popup.surface.commit();
            }
            // =========================================================================
            // CONTEXT MENU SUBSURFACE MANAGEMENT & RENDERING
            // =========================================================================
            let show_menu = !dock.menu_fade.is_fully_hidden() && !self.menu_state.items.is_empty();

            if show_menu {
                let _item_count = self.menu_state.items.len();
                let total_apps = apps_in_dock_ptrs.len();

                // Keep the last popup owner while the menu fades out.  Closing a
                // context menu clears menu_state.target_app_id, but the popup can
                // remain visible for the fade animation.  Falling back to index 0
                // here used to make the menu visibly jump to the leftmost icon on
                // click/close.
                let popup_owner = dock
                    .context_menu_popup
                    .as_ref()
                    .and_then(|popup| popup.current_app_id.as_deref());
                let target_app_id = self.menu_state.target_app_id.as_deref().or(popup_owner);

                let target_index = target_app_id
                    .and_then(|app_id| apps_in_dock_ptrs.iter().position(|id| id == &app_id))
                    .unwrap_or(0);

                let scale = dock_scale as f32;
                let icon_base_size = (box_size as f32 * scale) as i32;
                let icon_spacing = (spacing as f32 * scale) as i32;
                let total_icons_width = total_apps as i32 * icon_base_size
                    + (total_apps as i32 - 1).max(0) * icon_spacing;
                let start_x_offset = (phys_width as i32 - total_icons_width) / 2;

                let anchor_x = if total_apps > 0 {
                    let pin_x =
                        start_x_offset + target_index as i32 * (icon_base_size + icon_spacing);
                    pin_x + icon_base_size / 2
                } else {
                    phys_width as i32 / 2
                };
                let anchor_y = 0; // Anchored at the top of the dock to render above it just like the window list

                let item_count = self.menu_state.items.len() as i32;
                let phys_dock_height = (dock_height as f64 * dock_scale).round() as i32;

                let geom = PopupGeometry::compute_bounds(
                    PopupType::ContextMenu,
                    anchor_x,
                    anchor_y,
                    phys_width as i32,
                    phys_dock_height,
                    item_count,
                    dock_scale as f32,
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
                        current_app_id: None,
                        position: (0, 0),
                    }
                });

                if let Some(app_id) = self.menu_state.target_app_id.clone() {
                    popup.current_app_id = Some(app_id);
                }

                let popup_x = (geom.x as f64 / dock_scale).round() as i32;
                let popup_y = (geom.y as f64 / dock_scale).round() as i32;
                popup.position = (popup_x, popup_y);
                popup.subsurface.set_position(popup_x, popup_y);

                popup.width = geom.logical_width as u32;
                popup.height = geom.logical_height as u32;

                if geom.phys_width > 0 && geom.phys_height > 0 {
                    let (buffer, canvas) = self
                        .pool
                        .create_buffer(
                            geom.phys_width,
                            geom.phys_height,
                            geom.phys_width * 4,
                            wl_shm::Format::Argb8888,
                        )
                        .expect("Failed to allocate context menu popup buffer");

                    let local_ptr_x = ((self.interaction.pointer_position.x * dock_scale)
                        - geom.x as f64)
                        .max(0.0) as i32;
                    let local_ptr_y = ((self.interaction.pointer_position.y * dock_scale)
                        - geom.y as f64)
                        .max(0.0) as i32;

                    crate::render::popup::render_context_menu_surface(
                        canvas, // Replace 'frame' with your actual canvas/buffer variable name
                        &geom,
                        dock_scale as f32,
                        &self.menu_state,
                        &mut self.font_manager,
                        local_ptr_x,
                        local_ptr_y,
                    );

                    // --- APPLY CONTEXT MENU FADE ALPHA TO CANVAS ---
                    dock.menu_fade.apply_alpha_to_canvas(canvas);

                    popup.surface.set_buffer_scale(scale_i32);
                    buffer
                        .attach_to(&popup.surface)
                        .expect("Failed to attach context menu buffer");
                    popup
                        .surface
                        .damage_buffer(0, 0, geom.phys_width, geom.phys_height);

                    let compositor = self.compositor_state.wl_compositor();
                    let region = compositor.create_region(qh, ());
                    // Use physical dimensions (geom.phys_width, geom.phys_height) to match buffer scale space
                    region.add(0, 0, geom.phys_width, geom.phys_height);
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
            let total_icons_width =
                total_icons * icon_base_size + (total_icons.saturating_sub(1)) * icon_spacing;
            let start_x_offset = (phys_width as usize).saturating_sub(total_icons_width) / 2;

            let pins: Vec<PinItem> = apps_in_dock
                .iter()
                .enumerate()
                .map(|(i, app_id)| {
                    let x = start_x_offset + i * (icon_base_size + icon_spacing);
                    let y = dock_y + (scaled_dock_height.saturating_sub(icon_base_size)) / 2;
                    let running_windows = running_by_app.get(app_id);
                    let is_running = running_windows.is_some_and(|w| !w.is_empty());
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
            let _is_hidden = self.hide_state.is_fully_hidden();
            let container_start = 0;
            let container_w = dock_width as i32;

            // --- DOCK EXCLUSIVE ZONE ON STATE CHANGE ---
            if self.hide_state.just_became_visible {
                dock.surface.set_exclusive_zone(60);
            } else if self.hide_state.just_became_hidden {
                dock.surface.set_exclusive_zone(0);
            }

            let is_hidden = self.hide_state.is_fully_hidden();

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
            buffer
                .attach_to(surface.wl_surface())
                .expect("Buffer attach failed");
            surface
                .wl_surface()
                .damage_buffer(0, 0, phys_width as i32, phys_height as i32);

            // 5. Single atomic commit for buffer + transparency input region
            surface.wl_surface().commit();

            dock.current_buffer = Some(buffer);
        }
        self.hide_state.just_became_visible = false;
        self.hide_state.just_became_hidden = false;
    }
}
