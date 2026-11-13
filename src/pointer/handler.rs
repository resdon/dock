//! Pointer event handler.
//!
//! Routes pointer events (enter, leave, motion, button) to the appropriate handler.

use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::shell::WaylandSurface;
use wayland_client::{Connection, QueueHandle};

use super::motion;

use crate::interaction;
use crate::interaction::{click, scroll};
use crate::state::AppState;

impl PointerHandler for AppState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wayland_client::protocol::wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        if events.is_empty() {
            return;
        }

        self.last_interact_time = std::time::Instant::now();
        let mut layer_changed = false;
        let scale_factor = self.docks.first().map(|d| d.scale_factor).unwrap_or(1.0) as f32;
        let dock_surface_ptr = self.docks.first().map(|d| d.surface.wl_surface().clone());

        let has_motion = events
            .iter()
            .any(|e| matches!(e.kind, PointerEventKind::Motion { .. }));
        self.menu_state.cursor_moved = has_motion;

        // Step 1: Motion Coordinates & Leave Tracking
        layer_changed |=
            motion::handle_motion_events(self, events, dock_surface_ptr.as_ref(), scale_factor);

        // Step 2: State Retrieval & Layout Metrics
        let running_by_app = self.get_running_by_app_cached();
        let apps_in_dock = self.get_apps_in_dock_cached();

        let dock_height = self.config.dock_height as i32;
        let box_size = self.config.icon_size as i32;
        let spacing = self.config.spacing as i32;

        let total_items = apps_in_dock.len() as i32;
        let dock_width = self
            .docks
            .first()
            .map(|dock| dock.width as i32)
            .unwrap_or(self.width.max(0));
        let icon_content_width = if total_items > 0 {
            total_items * box_size + total_items.saturating_sub(1) * spacing
        } else {
            0
        };
        let start_offset_x = if dock_width > icon_content_width {
            (dock_width - icon_content_width) / 2
        } else {
            0
        };
        let layout = (dock_height, box_size, spacing, start_offset_x);

        // Step 3: Hover & Proximity Checks
        layer_changed |=
            interaction::update(self, scale_factor, layout);

        // Step 4: Scroll Events
        layer_changed |= scroll::handle_scroll_events(
            self,
            events,
            &apps_in_dock,
            &running_by_app,
            scale_factor,
            layout,
        );

        // Step 5: Click & Drag Release
        layer_changed |= click::handle_click_events(
            self,
            events,
            &apps_in_dock,
            &running_by_app,
            scale_factor,
            layout,
        );

        // **FIX (v1.5.5)**: Only set needs_redraw for meaningful state changes,
        // not for every pointer motion event. The compositor sends motion events
        // continuously (sub-pixel jitter), and setting needs_redraw for each one
        // caused the render loop to spin at ~800 FPS. The frame scheduler handles
        // rate-limited redraws for animation-driven updates.
        // Check if the hover state actually changed (not just position).
        let hover_meaningful = self.interaction.hovered_icon.is_some()
            || self.menu_state.is_open
            || self.window_list_state.is_open
            || self.is_dragging
            || self.dragged_app_id.is_some();
        if layer_changed && hover_meaningful {
            self.needs_redraw = true;
        }
    }
}
