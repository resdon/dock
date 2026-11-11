//! Main draw frame coordinator.
//!
//! Orchestrates rendering of all dock surfaces: main dock row, window list popup,
//! context menu, widget bars, audio player bar, and system status bar.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::models::system::WindowId;
use crate::state::AppState;
use crate::state::WindowState;

use super::bluetooth_popup;
use super::context_menu;
use super::dock;
use super::window_list;

pub struct DrawFrameData<'a> {
    pub apps_in_dock: &'a [String],
    pub running_by_app: &'a HashMap<String, Vec<ObjectId>>,
    pub windows: &'a HashMap<WindowId, WindowState>,
    pub phys_width: u32,
    pub phys_height: u32,
    pub surface_height: u32,
    pub dock_width: u32,
    pub dock_height: u32,
    pub dock_scale: f64,
    pub box_size: i32,
    pub spacing: i32,
    pub dock_output: &'a ObjectId,
    pub config: &'a DockConfig,
}

impl AppState {
    pub fn draw(&mut self, qh: &QueueHandle<Self>, render_dock: bool) {
        let box_size = self.config.icon_size as i32;
        let spacing = self.config.spacing as i32;

        let apps_in_dock = self.get_apps_in_dock();
        for app_id in &apps_in_dock {
            if !self.icon_cache.contains_key(app_id) {
                self.request_icon_load(app_id.to_string());
            }
        }

        // Do not consume the initial invalidation before the compositor has
        // configured at least one layer surface. The configure event will set
        // it again, but preserving it here makes the deferred path robust.
        if !self.docks.iter().any(|dock| dock.configured) {
            self.needs_redraw = true;
            return;
        }
        // Consume this invalidation before drawing. Any state changes discovered
        // below may set it again to schedule a follow-up frame.
        self.needs_redraw = false;

        // Refresh PipeWire's title-to-ID lookup only after window state changes.
        if let Some(pw) = self.pipewire_capture.as_ref() {
            if self.cached_pipewire_window_ids.is_none() {
                // Build the window-to-ID map sorted by title so that when multiple windows
                // share the same title, they are bound to PipeWire streams in a deterministic
                // order that matches the popup's title-sorted display order. Without this sort,
                // HashMap iteration order is arbitrary, causing stream 0 to bind to a random
                // window among those with the same title, breaking the popup grid mapping.
                let mut window_ids: Vec<(String, u32)> = self
                    .windows
                    .iter()
                    .filter_map(|(id, win)| {
                        if win.title.is_empty() {
                            None
                        } else {
                            Some((win.title.clone(), id.0.protocol_id()))
                        }
                    })
                    .collect();
                window_ids.sort_by(|(a_title, a_id), (b_title, b_id)| {
                    a_title.cmp(b_title).then(a_id.cmp(b_id))
                });
                pw.update_window_ids(&window_ids);
                self.cached_pipewire_window_ids = Some(window_ids);
            }
        }

        let computed_running_by_app;
        let running_by_app = if let Some(cached) = self.cached_running_by_app.as_ref() {
            cached
        } else {
            computed_running_by_app = self.compute_running_by_app_uncached();
            &computed_running_by_app
        };

        for dock_instance in &mut self.docks {
            if !dock_instance.configured {
                continue;
            }

            let is_animating = self.bounce_app_id.is_some()
                || self.click_app_id.is_some()
                || self.minimize_app_id.is_some()
                || self
                    .slint_renderer
                    .as_ref()
                    .is_some_and(|renderer| renderer.has_active_animation())
                || dock::has_active_gpu_animation();

            let dock_output_id = dock_instance.output.id();

            let dock_height: u32 = self.config.dock_height;
            // Width and surface height are negotiated once during initialization
            // and then remain stable. Never call LayerSurface::set_size() from
            // draw(): that schedules another configure and used to create an
            // endless configure -> redraw -> new-SHM-buffer cycle.
            let dock_width = dock_instance.width;
            // Side bars are disabled, so surface height is just the dock height
            let surface_height = dock_height;
            dock_instance.height = surface_height;

            let dock_scale = dock_instance.scale_factor.max(0.25);
            let scale_int = dock_scale.round().max(1.0) as i32;

            let phys_width = (dock_width as f64 * dock_scale).round() as u32;
            let phys_height = (surface_height as f64 * dock_scale).round() as u32;

            if phys_width == 0 || phys_height == 0 {
                continue;
            }

            let frame = DrawFrameData {
                apps_in_dock: &apps_in_dock,
                running_by_app: &running_by_app,
                windows: &self.windows,
                phys_width,
                phys_height,
                surface_height,
                dock_width,
                dock_height,
                dock_scale,
                box_size: box_size as i32,
                spacing: spacing as i32,
                dock_output: &dock_output_id,
                config: &self.config,
            };

            // Popups
            window_list::draw_window_list_popup(
                dock_instance,
                qh,
                &frame,
                &mut self.pool,
                &self.compositor_state,
                self.subcompositor.as_ref(),
                &mut self.font_manager,
                &self.interaction,
                &self.window_list_state,
                self.pipewire_capture.as_ref(),
                self.gpu_context.as_mut(),
                &self.dmabuf_state,
            );
            context_menu::draw_context_menu_popup(
                dock_instance,
                qh,
                &frame,
                &mut self.pool,
                &self.compositor_state,
                self.subcompositor.as_ref(),
                &mut self.font_manager,
                &self.menu_state,
                &self.interaction,
                self.gpu_context.as_mut(),
                &self.dmabuf_state,
            );
            // audio_popup::draw_audio_popup(...);
            // Bluetooth popup — the BT icon on the dashed block opens a subsurface popup.
            if let Some(ref mut bt_renderer) = self.bluetooth_renderer {
                bluetooth_popup::draw_bluetooth_popup(
                    dock_instance,
                    qh,
                    &frame,
                    &mut self.pool,
                    &self.compositor_state,
                    self.subcompositor.as_ref(),
                    bt_renderer,
                    self.bluetooth_panel_open,
                    &self.bluetooth_snapshot,
                    self.tray_items.len(),
                    self.gpu_context.as_mut(),
                    &self.dmabuf_state,
                    &self.config,
                );
            }
            // rclone_popup::draw_rclone_popup(...);
            // process_popup::draw_process_popup(...);

            // Main application row, left tray box, right utility box, and the
            // lower system strip are independent visual regions. Only the app
            // row is rendered into the top-level layer-surface buffer.
            let first_buffer = dock_instance.current_buffer.is_none();
            if render_dock || first_buffer {
                dock::draw_main_dock(
                    dock_instance,
                    qh,
                    &frame,
                    &mut self.pool,
                    &self.compositor_state,
                    self.subcompositor.as_ref(),
                    &mut self.font_manager,
                    &mut self.hide_state,
                    &self.icon_cache,
                    &self.pending_icon_searches,
                    &self.badges,
                    self.is_dragging,
                    self.dragged_app_id.as_ref(),
                    &self.interaction,
                    &self.tray_items,
                    &self.system_bar,
                    &self.bluetooth_snapshot,
                    &mut self.sys_scanner,
                    false,
                    self.gpu_context.as_mut(),
                    &self.dmabuf_state,
                    is_animating,
                    scale_int,
                    self.slint_renderer.as_mut(),
                    self.bounce_app_id.take().as_ref(),
                    self.click_app_id.take().as_ref(),
                    self.minimize_app_id.take().as_ref(),
                    &self.config,
                );
                // Verify pending focus action and trigger click animation if succeeded
                if let Some(ref app_id) = self.pending_focus_app_id {
                    let focus_verified = self.windows.values().any(|w| {
                        w.app_id.as_ref().map(|a| a.as_str() == app_id.as_str()) == Some(true)
                            && w.focused
                    });
                    if focus_verified {
                        self.click_app_id = Some(app_id.clone());
                    }
                }
                self.pending_focus_app_id = None;
            }

            // Render tray icons on their sibling surface alongside the app row.
            super::side_bars::draw_left_tray_bar(
                dock_instance,
                qh,
                &mut self.pool,
                &self.compositor_state,
                self.subcompositor.as_ref(),
                &self.tray_items,
                true,
                self.hide_state.current_alpha,
                &mut self.font_manager,
                self.gpu_context.as_mut(),
                &self.dmabuf_state,
            );

            // Side bars: remaining widgets are disabled.
            // super::widget_bar::draw_status_widget_bar(...);
            // side_bars::draw_bottom_system_bar(...);

            // Dashed block — separate layer surface to the left of the dock
            super::dashed_block::draw_dashed_block(
                dock_instance,
                qh,
                &mut self.pool,
                self.hide_state.current_alpha,
                &mut self.font_manager,
                &self.config,
                self.gpu_context.as_mut(),
                &self.dmabuf_state,
            );

            // Date/time block — separate layer surface to the right of the dock
            super::date_time_block::draw_date_time_block(
                dock_instance,
                qh,
                &mut self.pool,
                self.hide_state.current_alpha,
                &mut self.font_manager,
                &self.config,
                self.gpu_context.as_mut(),
                &self.dmabuf_state,
            );

            dock_instance.status_strip_rendered = true;
        }

        self.hide_state.just_became_visible = false;
        self.hide_state.just_became_hidden = false;
    }
}
