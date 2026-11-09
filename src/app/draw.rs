//! Main draw frame coordinator.
//!
//! Orchestrates rendering of all dock surfaces: main dock row, window list popup,
//! context menu, widget bars, audio player bar, and system status bar.

use std::collections::HashMap;

use wayland_client::backend::ObjectId;
use wayland_client::Proxy;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::state::AppState;
use crate::state::WindowState;

use super::context_menu;
use super::dock;
use super::window_list;

pub struct DrawFrameData<'a> {
    pub apps_in_dock: &'a [String],
    pub running_by_app: &'a HashMap<String, Vec<&'a WindowState>>,
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
        let box_size = 48;
        let spacing = 8;

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
        // Dynamic rendering disabled — always redraw every frame
        self.needs_redraw = true;

        // Update the shared window title-to-ID map so the PipeWire monitor task
        // can bind new streams to stable Wayland window IDs. This is called every
        // frame so the monitor always has the latest mapping. The IDs are
        // `protocol_id` values from `foreign_toplevel`.
        //
        // Windows with empty `output_ids` are included — some compositors don't
        // fire `OutputEntered` events for all windows, but they still have open
        // windows that should appear in the popup.
        if let Some(ref pw) = self.pipewire_capture {
            // Build the window-to-ID map sorted by title so that when multiple windows
            // share the same title, they are bound to PipeWire streams in a deterministic
            // order that matches the popup's title-sorted display order. Without this sort,
            // HashMap iteration order is arbitrary, causing stream 0 to bind to a random
            // window among those with the same title, breaking the popup grid mapping.
            let mut window_ids: Vec<(String, u32)> = self.windows.iter().filter_map(|(id, win)| {
                if win.title.is_empty() {
                    None
                } else {
                    Some((win.title.clone(), id.0.protocol_id()))
                }
            }).collect();
            window_ids.sort_by(|(a_title, a_id), (b_title, b_id)| {
                a_title.cmp(b_title).then(a_id.cmp(b_id))
            });
            pw.update_window_ids(&window_ids);
        }

        // Build the canonical app-id mapping before mutably borrowing dock
        // instances. `canonical_running_app_id` reads pinned_apps, so calling
        // it while iterating `&mut self.docks` would create an immutable/mutable
        // borrow conflict. Keep the resolved values local for the draw pass.
        let canonical_app_ids: HashMap<String, String> = self
            .windows
            .values()
            .map(|win| {
                let raw_id = win
                    .app_id
                    .as_ref()
                    .map(|id| id.to_string())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| {
                        if !win.title.is_empty() {
                            win.title.clone()
                        } else {
                            "Unknown".to_string()
                        }
                    });
                let canonical = self.canonical_running_app_id(&raw_id);
                (raw_id, canonical)
            })
            .collect();

        let first_frame_status_pending = false;
        for dock_instance in &mut self.docks {
            if !dock_instance.configured {
                continue;
            }

            // All animations disabled — is_animating always false
            let is_animating = false;

            let dock_output_id = dock_instance.output.id();

            let mut ordered_windows: Vec<&WindowState> = self
                .windows
                .values()
                .filter(|win| {
                    // Include windows on this output, plus windows with no output_ids
                    // (some compositors don't fire OutputEntered for all windows).
                    // Excluding them would make the popup show no windows even though
                    // the app has open windows.
                    win.output_ids.is_empty()
                        || win.output_ids
                            .iter()
                            .any(|id| id.0 == dock_output_id.clone())
                })
                .collect();
            ordered_windows.sort_by_key(|win| win.order_index);

            let mut running_by_app: HashMap<String, Vec<&WindowState>> = HashMap::new();
            for win in &ordered_windows {
                let raw_id = win
                    .app_id
                    .as_ref()
                    .map(|id| id.to_string())
                    .filter(|id| !id.is_empty())
                    .unwrap_or_else(|| {
                        if !win.title.is_empty() {
                            win.title.clone()
                        } else {
                            "Unknown".to_string()
                        }
                    });
                let id = canonical_app_ids.get(&raw_id).cloned().unwrap_or(raw_id);
                running_by_app.entry(id).or_default().push(*win);
            }
            for windows in running_by_app.values_mut() {
                // Sort by title for consistent, predictable popup row order.
                // This ensures the same window always appears in the same row,
                // making PipeWire stream title matching reliable across frames.
                windows.sort_by(|a, b| a.title.cmp(&b.title));
            }

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
            // bluetooth_popup::draw_bluetooth_popup(...);
            // rclone_popup::draw_rclone_popup(...);
            // process_popup::draw_process_popup(...);

            // Main application row, left tray box, right utility box, and the
            // lower system strip are independent visual regions. Only the app
            // row is rendered into the top-level layer-surface buffer.
            let first_buffer = dock_instance.current_buffer.is_none();
            let _status_frame_due = !dock_instance.status_strip_rendered;
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

// Side bars: all disabled
            // side_bars::draw_left_tray_bar(...);
            // super::widget_bar::draw_status_widget_bar(...);
            // side_bars::draw_bottom_system_bar(...);
            dock_instance.status_strip_rendered = true;
        }

        // The first app-row buffer is committed on its own. Schedule one follow-up
        // pass to attach the sibling tray/widget/status surfaces.
        if first_frame_status_pending {
            self.needs_redraw = true;
        }

        self.hide_state.just_became_visible = false;
        self.hide_state.just_became_hidden = false;
    }
}
