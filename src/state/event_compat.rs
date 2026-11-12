//! Event consumption boundary during the migration to the registry-first state
//! architecture.
//!
//! The permanent direction is:
//! listener -> DomainEvent -> resolver -> SystemRegistry -> state projection.
//! This module owns only the application-facing transition and legacy UI
//! compatibility needed while the rest of the dock migrates away from
//! legacy window compatibility data.

use crate::events::{
    BadgeEvent, DomainEvent, DragDropEvent, MediaEvent, OutputEvent, SystemEvent, WindowEvent,
};
use crate::resolvers::ResolverSnapshot;
use crate::state::AppState;
use smithay_client_toolkit::shell::WaylandSurface;
use wayland_client::Proxy;

impl AppState {
    /// Queues a domain event for processing.
    ///
    /// Events are batched and processed together in `process_pending_events()`
    /// to avoid per-event redraws and registry updates.
    pub fn emit_event(&mut self, event: DomainEvent) {
        self.pending_events.push_back(event);
    }

    /// Consumes a batch of domain events. Window discovery is resolved only at
    /// `Done`, then committed atomically to the registry. The live `state/`
    /// projection is rebuilt once after the batch rather than after every
    /// compositor callback.
    ///
    /// Returns `true` if any state changed and a redraw is needed.
    pub fn process_pending_events(&mut self) -> bool {
        let events = std::mem::take(&mut self.pending_events);
        if events.is_empty() {
            return false;
        }

        let should_refresh_processes = events.iter().any(|event| {
            matches!(
                event,
                DomainEvent::Window(WindowEvent::Done { .. })
                    | DomainEvent::Window(WindowEvent::Created { .. })
                    | DomainEvent::Window(WindowEvent::AppIdChanged { .. })
            )
        });
        let refresh_processes = should_refresh_processes
            && self.last_process_scan.elapsed() >= std::time::Duration::from_millis(500);

        if refresh_processes {
            // Process discovery is intentionally outside Wayland callbacks and
            // outside the animation/frame path. Refresh is throttled and shared
            // by the whole event batch.
            self.sys_scanner
                .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
            self.last_process_scan = std::time::Instant::now();
        }

        let resolver_snapshot = if should_refresh_processes {
            Some(ResolverSnapshot::from_system(&self.sys_scanner))
        } else {
            None
        };

        if let Some(snapshot) = resolver_snapshot.as_ref() {
            self.resolver_pipeline.update_snapshot(snapshot.clone());
        }

        let mut registry_changed = false;
        let mut redraw = false;
        for event in events {
            let (registry_dirty, needs_redraw) =
                self.apply_event(event, resolver_snapshot.as_ref());
            registry_changed |= registry_dirty;
            redraw |= needs_redraw;
        }

        if registry_changed {
            self.sync_live_state_from_registry();
        }
        if redraw || registry_changed {
            self.needs_redraw = true;
        }
        if std::env::var_os("DOCK_WINDOW_DEBUG").is_some() {}
        registry_changed || redraw
    }

    /// Compatibility wrapper for `process_pending_events()`.
    ///
    /// Legacy callers use this name during migration to the registry-first architecture.
    pub fn process_pending_events_compat(&mut self) -> bool {
        self.process_pending_events()
    }

    /// Applies a single domain event to the state.
    ///
    /// Returns `(registry_changed, needs_redraw)` indicating whether the
    /// system registry was modified and whether a redraw is needed.
    fn apply_event(
        &mut self,
        event: DomainEvent,
        resolver_snapshot: Option<&ResolverSnapshot>,
    ) -> (bool, bool) {
        match event {
            DomainEvent::Window(event) => self.apply_window_event(event, resolver_snapshot),
            DomainEvent::Output(event) => self.apply_output_event(event),
            DomainEvent::System(event) => self.apply_system_event(event),
            DomainEvent::Media(event) => self.apply_media_event(event),
            DomainEvent::DragDrop(event) => self.apply_dnd_event(event),
            DomainEvent::Badge(BadgeEvent::Update(update)) => {
                let clean_id = crate::events::normalize_desktop_id(&update.desktop_id);
                self.badges.insert(clean_id, update);
                (false, true)
            }
        }
    }

    /// Ensures a `WindowState` entry exists for the given window ID.
    ///
    /// Creates a new entry with default values if it doesn't exist.
    fn ensure_window_state(&mut self, id: &crate::models::system::WindowId) {
        self.windows
            .entry(id.clone())
            .or_insert_with(|| crate::state::WindowState::new(id.clone()));
    }

    /// Updates window state fields from a WindowEvent.
    ///
    /// Handles all window event types: created, closed, title changed,
    /// app_id changed, state changed (focus/minimize), and output changes.
    fn update_window_state_from_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::Created { window_id } => {
                self.ensure_window_state(window_id);
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.mapped = true;
                    window.visible = true;
                    window.is_pending = true;
                }
            }
            WindowEvent::OutputEntered { window_id, output } => {
                self.ensure_window_state(window_id);
                if let Some(window) = self.windows.get_mut(window_id) {
                    if !window.output_ids.contains(output) {
                        window.output_ids.push(output.clone());
                    }
                    window.output_id = window.output_ids.first().cloned();
                }
            }
            WindowEvent::OutputLeft { window_id, output } => {
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.output_ids.retain(|id| id != output);
                    window.output_id = window.output_ids.first().cloned();
                }
            }
            WindowEvent::Closed { window_id } => {
                self.windows.remove(window_id);
                self.window_handles.remove(&window_id.0);
            }
            WindowEvent::Done { window_id } => {
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.is_pending = false;
                }
            }
            WindowEvent::TitleChanged { window_id, title } => {
                self.ensure_window_state(window_id);
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.title = title.clone();
                    window.icon_resolved = false;
                }
            }
            WindowEvent::AppIdChanged { window_id, app_id } => {
                self.ensure_window_state(window_id);
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.app_id = Some(app_id.clone());
                    window.app_name = app_id.to_string();
                    window.icon_resolved = false;
                }
            }
            WindowEvent::StateChanged {
                window_id,
                activated,
                minimized,
            } => {
                self.ensure_window_state(window_id);
                // Clone app_id before mutable borrow so we can trigger animation after
                let app_id_for_anim = self.windows.get(window_id).and_then(|w| w.app_id.clone());
                if let Some(window) = self.windows.get_mut(window_id) {
                    window.focused = *activated;
                    window.foreground = *activated;
                    window.background = !*activated;
                    window.minimized = *minimized;
                    window.visible = !*minimized;
                    window.is_pending = false;
                    window.last_state = if *activated {
                        crate::state::LastState::ReceivedFocus
                    } else {
                        crate::state::LastState::ReportedInactive
                    };
                }
                // Trigger minimize animation whenever any window becomes minimized
                if *minimized {
                    if let Some(ref app_id) = app_id_for_anim {
                        self.minimize_app_id = Some(app_id.0.clone());
                        self.needs_redraw = true;
                    }
                }
                // When a window gains focus, clear focus on all other windows so
                // only the currently focused window reports focused=true. The
                // Wayland foreign toplevel protocol sends activated=true only for
                // the window that gained focus, not activated=false for siblings.
                if *activated {
                    for (id, w) in self.windows.iter_mut() {
                        if *id != *window_id && w.focused {
                            w.focused = false;
                            w.foreground = false;
                            w.background = true;
                        }
                    }
                }
            }
        }
    }

    /// Applies a window event, updating both the registry and live state.
    ///
    /// Returns `(registry_changed, needs_redraw)`.
    fn apply_window_event(
        &mut self,
        event: WindowEvent,
        resolver_snapshot: Option<&ResolverSnapshot>,
    ) -> (bool, bool) {
        let was_closed = matches!(&event, WindowEvent::Closed { .. });
        let closed_app_id = if was_closed {
            match &event {
                WindowEvent::Closed { window_id } => self
                    .system_registry
                    .window_record(window_id.0.clone())
                    .map(|window| window.app_id.clone()),
                _ => None,
            }
        } else {
            None
        };

        let resolver_event = event.clone();
        self.update_window_state_from_event(&event);

        let snapshot = resolver_snapshot
            .cloned()
            .unwrap_or_else(|| self.resolver_pipeline.last_snapshot().clone());
        let action = self
            .resolver_pipeline
            .handle_window_event(resolver_event, &snapshot);
        let changed = self.system_registry.apply_resolution(action);

        if let WindowEvent::AppIdChanged { ref app_id, .. } = event {
            self.request_icon_load(app_id.to_string());
        }

        // Signal PipeWire to probe for new windows when a window is created.
        // The monitor task reacts to this signal and incrementally adds streams
        // for newly opened windows without disrupting existing capture streams.
        if matches!(&event, WindowEvent::Created { .. }) {
            if let Some(pw) = &self.pipewire_capture {
                pw.request_probe();
            }
        }

        if was_closed {
            if let Some(app_id) = closed_app_id.clone() {
                if let Some(app) = self.system_registry.application_record(&app_id) {
                    if let Some(live) = self.applications.get_mut(&app_id) {
                        live.running = app.status.running;
                        live.foreground = app.status.foreground;
                        live.background = app.status.background;
                        live.attention = app.status.attention;
                        live.window_count = app.windows.len();
                        live.process_count = app.processes.len();
                    }
                }
                // Close the window list popup if the target app now has no windows
                if self.window_list_state.is_open
                    && self.window_list_state.target_app_id.as_deref() == Some(app_id.as_str())
                {
                    if self.window_count_for_app(app_id.as_str()) == 0 {
                        self.request_window_list_close();
                    }
                }
            }
            // The live window projection was already updated by
            // update_window_state_from_event(). Avoid a full O(apps+windows+
            // processes+outputs) rebuild on every close event.
            return (false, true);
        }

        (changed, true)
    }

    fn apply_output_event(&mut self, event: OutputEvent) -> (bool, bool) {
        match event {
            OutputEvent::Seen { output, name } | OutputEvent::Updated { output, name } => {
                let record = self.system_registry.ensure_output(output.clone());
                if let Some(name) = name {
                    record.name = Some(name);
                }
                if !self.system.outputs.contains(&output) {
                    self.system.outputs.push(output);
                }
                (true, true)
            }
            OutputEvent::Destroyed { output } => {
                self.system.outputs.retain(|id| id != &output);
                self.output_handles.remove(&output);
                if self.current_output.as_ref().map(Proxy::id) == Some(output.0.clone()) {
                    self.current_output = None;
                }
                self.system_registry.remove_output(&output);
                (true, true)
            }
        }
    }

    fn apply_system_event(&mut self, event: SystemEvent) -> (bool, bool) {
        match event {
            SystemEvent::SurfaceEnter { output } => {
                if let Some(handle) = self.output_handles.get(&output).cloned() {
                    self.current_output = Some(handle);
                }
                (false, true)
            }
            SystemEvent::SurfaceLeave { output } => {
                if self.current_output.as_ref().map(Proxy::id) == Some(output.0.clone()) {
                    self.current_output = None;
                }
                (false, true)
            }
            // FrameRequested is a render scheduling signal, not a domain-state
            // mutation. It only marks the frame as drawable.
            SystemEvent::FrameRequested => {
                self.needs_redraw = true;
                (false, false)
            }
            SystemEvent::LayerConfigured {
                surface_id,
                width,
                height: _height,
            } => {
                let surface_id = surface_id.0.clone();
                let mut found = false;
                for dock in &mut self.docks {
                    if dock.surface.wl_surface().id() == surface_id.clone() {
                        // A zero dimension means the compositor accepted the
                        // client-requested dimension. Preserve that requested
                        // size instead of overwriting it with zero.
                        if width > 0 {
                            dock.width = width;
                        }
                        // Use config dock height, ignoring compositor-reported height
                        dock.height = self.config.dock_height;
                        dock.configured = true;
                        found = true;
                    }
                }
                (false, found)
            }
            SystemEvent::LayerClosed { surface_id } => {
                let surface_id = surface_id.0.clone();
                let mut closed_main = false;
                for dock in &mut self.docks {
                    if dock.surface.wl_surface().id() == surface_id.clone() {
                        closed_main = true;
                        break;
                    }
                }
                if closed_main {
                    self.docks
                        .retain(|d| d.surface.wl_surface().id() != surface_id);
                    (false, true)
                } else {
                    (false, false)
                }
            }
            SystemEvent::FractionalScaleChanged { output, scale } => {
                let new_scale = scale as f64 / 120.0;
                for dock in &mut self.docks {
                    if dock.output.id() == output.0.clone() {
                        dock.scale_factor = new_scale;
                    }
                }
                (false, true)
            }
            SystemEvent::WindowSurfaceChanged { .. } => (false, false),
        }
    }

    fn apply_media_event(&mut self, event: MediaEvent) -> (bool, bool) {
        match event {
            MediaEvent::LinkWindow {
                app_id,
                window_id,
                link,
            } => {
                if let Some(window) = self.system_registry.window_record_mut(window_id.0.clone()) {
                    window.media = Some(link.clone());
                }
                self.media.active_app = app_id.clone();
                self.media.target_window = Some(window_id.clone());
                self.media.media_link = Some(link.clone());
                self.media.is_playing = link.is_playing;
                self.media_state.target_window = Some(window_id.0);
                self.media_state.is_playing = link.is_playing;
                self.media_state.media_link = Some(link);
                (true, true)
            }
            MediaEvent::ClearWindow { window_id } => {
                self.system_registry
                    .update_window_media(window_id.0.clone(), None);
                if self.media.target_window.as_ref() == Some(&window_id) {
                    self.media.clear();
                }
                if self.media_state.target_window.as_ref() == Some(&window_id.0) {
                    self.media_state = Default::default();
                }
                (true, true)
            }
        }
    }

    fn apply_dnd_event(&mut self, event: DragDropEvent) -> (bool, bool) {
        match event {
            DragDropEvent::DataOffer { offer_id } => {
                self.dnd_state.current_offer = Some(offer_id.0);
                self.dnd_state.mime_types.clear();
                (false, true)
            }
            DragDropEvent::OfferMimeType {
                offer_id,
                mime_type,
            } => {
                if self.dnd_state.current_offer.as_ref() == Some(&offer_id.0)
                    && !self.dnd_state.mime_types.iter().any(|m| m == &mime_type)
                {
                    self.dnd_state.mime_types.push(mime_type);
                }
                (false, false)
            }
            DragDropEvent::Enter {
                serial,
                x,
                y,
                offer_id,
            } => {
                self.dnd_state.drag_x = x;
                self.dnd_state.drag_y = y;
                self.dnd_state.current_serial = Some(serial);
                if let Some(id) = offer_id {
                    self.dnd_state.current_offer = Some(id.0);
                }
                self.update_dnd_hover_target(x, y);
                (false, true)
            }
            DragDropEvent::Motion { x, y } => {
                self.dnd_state.drag_x = x;
                self.dnd_state.drag_y = y;
                self.update_dnd_hover_target(x, y);
                (false, true)
            }
            DragDropEvent::Leave => {
                self.dnd_state.current_offer = None;
                self.dnd_state.mime_types.clear();
                self.dnd_state.current_serial = None;
                self.dnd_state.hovered_dock_index = None;
                (false, true)
            }
            DragDropEvent::Drop { x, y, paths } => {
                self.dnd_state.drag_x = x;
                self.dnd_state.drag_y = y;
                self.dnd_state.current_offer = None;
                self.dnd_state.current_serial = None;
                self.dnd_state.mime_types.clear();
                self.dnd_state.hovered_dock_index = None;

                // Compatibility only: the listener no longer invokes handlers.
                // Existing drop behavior remains intact while interaction is
                // migrated to the explicit DropTarget state.
                if !paths.is_empty() {
                    self.handle_file_drop_on_icon(x, y, paths);
                }
                (false, true)
            }
        }
    }
}
