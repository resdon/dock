//! Application state management.
//!
//! Contains `AppState` — the single source of truth for the entire dock application.
//! All subsystems (Wayland, GPU, PipeWire, D-Bus, UI) share state through this struct.
//!
//! # Submodules
//! - `dock` — dock surface width and layout calculations
//! - `popup` — popup open/close logic
//! - `event_compat` — Wayland event compatibility layer
//! - `interaction` — interaction state helpers
//! - `animation` — icon click/bounce animations
//! - `window` — window state tracking
//! - `media` — media player state
//! - `system` — system output state
//! - `telemetry` — performance metrics

use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use crate::animations::IconAnimation;
use smithay_client_toolkit::{
    compositor::CompositorState,
    dmabuf::DmabufState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::LayerShell,
    shell::WaylandSurface,
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::WlDataDeviceManager,
    wl_data_offer::WlDataOffer, wl_output::WlOutput, wl_pointer::WlPointer, wl_seat::WlSeat,
    wl_subcompositor::WlSubcompositor, wl_surface::WlSurface,
};
use wayland_client::{Connection, Proxy};

use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use crate::graphics::hide::AutoHideState;
use crate::models::BadgeUpdate;
use crate::registry::SystemRegistry;
use crate::render::font::FontManager;

use crate::app::icon_load::IconLoader;
use crate::events::DomainEvent;
use crate::models::system::{AppId, OutputId, ProcessId, WindowId};
use crate::resolvers::ResolverPipeline;
use crate::types::WaylandDndProtocolState;
use crate::types::{
    DndState, DockInstance, HoverState, InteractionState, MenuState, WindowListState,
};

mod animation;
mod application;
mod context_menu;
mod dock_runtime;
mod event_compat;
mod interaction_runtime;
mod media;
mod process;
mod system;
mod telemetry;
mod window;
mod window_list;

pub use animation::{AnimationKind, AnimationState};
pub use application::ApplicationState;
pub use context_menu::ContextMenuRuntimeState;
pub use dock_runtime::DockRuntimeState;
pub use interaction_runtime::{DropTarget, InputTarget, InteractionRuntimeState};
pub use media::MediaState;
pub use process::{ProcessHealthState, ProcessState, ProcessTelemetry};
pub use system::{LiveOutputState, SystemState};
pub use telemetry::{
    CpuTelemetry, FrameTelemetry, GpuTelemetry, MemoryTelemetry, NetworkTelemetry, TelemetryState,
};
pub use window::{LastState, WindowAreas, WindowInputState, WindowState, WindowType};
pub use window_list::WindowListRuntimeState;

pub(crate) mod dock;
mod interaction;
mod popup;

/// Result returned by the background icon loader thread.
pub struct IconLoadResult {
    /// Application ID this icon belongs to
    pub app_id: String,
    /// Decoded icon pixels (RGBA format)
    pub rgba: Vec<u8>,
    /// Icon width/height in pixels
    pub size: u32,
    /// Optional animated icon frames (e.g., for launch animation)
    pub animation: Option<IconAnimation>,
}

/// The single source of truth for the entire dock application.
///
/// Contains all Wayland protocol state, UI state, interaction state,
/// background channel senders, and rendering state. Accessed from the
/// main event loop in `main.rs`.
pub struct AppState {
    // === Wayland Protocol & Compositor Core ===
    pub connection: Connection,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub qh: smithay_client_toolkit::reexports::client::QueueHandle<AppState>,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub pool: SlotPool,
    pub seat_state: SeatState,
    pub subcompositor: Option<WlSubcompositor>, // Updated: wrapped in Option
    pub layer_surface: Option<smithay_client_toolkit::shell::wlr_layer::LayerSurface>,
    pub current_buffer: Option<Buffer>,
    pub width: i32,
    pub height: i32,
    pub toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,

    // Application & Dock State Management
    pub interaction: InteractionState,
    pub system_registry: SystemRegistry,
    /// Wayland protocol handles kept by the adapter layer; domain events only
    /// carry stable IDs.
    pub window_handles: HashMap<ObjectId, wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1>,
    pub output_handles: HashMap<OutputId, WlOutput>,
    pub dnd_offers: HashMap<ObjectId, WlDataOffer>,
    /// Wayland-only DnD negotiation state; domain DnD events never expose the offer object.
    pub dnd_protocol: WaylandDndProtocolState,
    pub pending_events: VecDeque<DomainEvent>,
    /// Cached resolver pipeline. Expensive discovery is performed only when a window
    /// reaches a stable Done boundary, never from the compositor callback itself.
    pub resolver_pipeline: ResolverPipeline,
    pub pinned_apps: Vec<String>,
    pub hover_state: HoverState,
    pub last_interact_time: std::time::Instant,
    pub needs_redraw: bool,
    pub last_mouse_pos: Option<(i32, i32)>,
    pub is_dragging: bool,
    pub drag_start_x: i32,
    pub drag_start_y: i32,
    pub dragged_app_id: Option<String>,
    pub last_drag_draw: std::time::Instant,
    pub sys_scanner: sysinfo::System,
    /// Process discovery is throttled because it is external-system I/O and
    /// must never run at animation/frame cadence.
    pub last_process_scan: std::time::Instant,
    pub current_output: Option<WlOutput>,
    pub docks: Vec<DockInstance>,

    // Fractional Scaling
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub fractional_scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,

    // Drag-and-Drop & Animations
    pub dnd_state: DndState,
    /// Legacy media presentation state; kept while consumers migrate to `media`.
    pub media_state: crate::types::MediaWindowState,

    // Live domain state populated from listeners/resolvers/registry.
    pub system: SystemState,
    pub applications: HashMap<AppId, ApplicationState>,
    pub windows: HashMap<WindowId, WindowState>,
    pub processes: HashMap<ProcessId, ProcessState>,
    pub outputs: HashMap<crate::models::system::OutputId, LiveOutputState>,
    pub interaction_runtime: InteractionRuntimeState,
    pub dock_runtime: DockRuntimeState,
    pub window_list_runtime: WindowListRuntimeState,
    pub context_menu_runtime: ContextMenuRuntimeState,
    pub telemetry: TelemetryState,
    pub animation_state: AnimationState,
    pub media: MediaState,
    pub data_device_manager: Option<WlDataDeviceManager>,
    pub data_device: Option<WlDataDevice>,

    // App Integrations & DBus Badges
    pub badges: HashMap<String, BadgeUpdate>,
    pub tray_items: Vec<crate::models::TrayItem>,
    pub tray_action_tx: tokio::sync::mpsc::UnboundedSender<crate::models::TrayAction>,
    pub system_bar: crate::models::SystemBarSnapshot,
    pub audio_snapshot: crate::models::AudioSnapshot,
    pub audio_action_tx: std::sync::mpsc::Sender<crate::models::AudioAction>,
    pub audio_panel_open: bool,
    pub audio_slider_drag: Option<crate::models::AudioSliderTarget>,
    pub bluetooth_snapshot: crate::models::BluetoothSnapshot,
    pub bluetooth_action_tx: tokio::sync::mpsc::UnboundedSender<crate::models::BluetoothAction>,
    pub bluetooth_panel_open: bool,
    /// PID of the spawned bluetooth-widget-complete process, if currently running.
    /// Used to close the widget when clicking the BT icon on the dashed block again.
    pub bluetooth_widget_pid: Option<u32>,
    pub rclone_snapshot: crate::models::RcloneSnapshot,
    pub rclone_monitor_tx: Sender<bool>,
    pub rclone_panel_open: bool,
    pub rclone_page: usize,
    pub process_snapshot: crate::models::ProcessSnapshot,
    pub process_monitor_tx: Sender<bool>,
    pub process_panel_open: bool,
    pub process_page: usize,
    pub process_sort: crate::models::ProcessSort,

    // Async Icon Loading & Caching
    pub icon_cache: HashMap<String, (Vec<u8>, u32)>,
    pub pending_icon_searches: HashSet<String>,
    pub icon_rx: Receiver<IconLoadResult>,
    pub icon_tx: Sender<IconLoadResult>,
    pub icon_load: IconLoader,
    pub animations: HashMap<String, IconAnimation>,

    // Autohide System State
    pub hide_state: AutoHideState,

    // GPU Rendering (wgpu-api)
    pub gpu_context: Option<wgpu_api::Renderer>,
    /// GPU is available (initialized successfully)
    pub gpu_available: bool,
    pub dmabuf_state: DmabufState,

    // Dock configuration
    pub config: crate::config::DockConfig,

    // Slint dock renderer (software renderer - direct buffer rendering)
    pub slint_renderer: Option<crate::render::slint_dock::SlintDockRenderer>,

    /// App ID that should trigger a bounce animation on the next render frame
    pub bounce_app_id: Option<String>,
    /// App ID that should trigger a click animation on the next render frame
    pub click_app_id: Option<String>,
    /// App ID waiting for focus verification before playing click animation
    pub pending_focus_app_id: Option<String>,
    /// App ID that should trigger a minimize (shrink) animation on the next render frame
    pub minimize_app_id: Option<String>,

    // Focus check
    pub focus_action_performed: bool,
    pub focus_action_time: Option<Instant>,
    // Popups
    pub window_list_state: WindowListState,
    pub menu_state: MenuState,

    // PipeWire window capture service for the window list popup.
    //
    // Captures live video frames from all open Wayland windows via XDG Desktop
    // Portal. Each window has a dedicated stream thread. The service provides
    // RGBA thumbnails for the popup grid and handles window-to-stream matching
    // via the persistent binding registry.
    pub pipewire_capture: Option<std::sync::Arc<crate::services::pipewire_capture::PipewireCaptureService>>,
}

impl AppState {
    pub fn apply_tray_snapshot(&mut self, snapshot: crate::models::TraySnapshot) -> bool {
        let changed = self.tray_items.len() != snapshot.items.len()
            || self
                .tray_items
                .iter()
                .zip(snapshot.items.iter())
                .any(|(a, b)| {
                    a.key != b.key
                        || a.service_name != b.service_name
                        || a.object_path != b.object_path
                        || a.interface_name != b.interface_name
                        || a.title != b.title
                        || a.status != b.status
                        || a.icon_name != b.icon_name
                        || a.tooltip != b.tooltip
                        || a.item_is_menu != b.item_is_menu
                        || a.menu_path != b.menu_path
                        || a.icon
                            .as_ref()
                            .map(|i| (i.width, i.height, i.rgba.as_slice()))
                            != b.icon
                                .as_ref()
                                .map(|i| (i.width, i.height, i.rgba.as_slice()))
                });

        let _item_count = snapshot.items.len();
        self.tray_items = snapshot.items;
        if changed {
            for dock in &mut self.docks {
                dock.status_strip_rendered = false;
            }
            let _icon_count = self
                .tray_items
                .iter()
                .filter(|item| item.icon.is_some())
                .count();
        }
        changed
    }

    pub fn tray_item_at_surface(
        &self,
        surface: &WlSurface,
        local_x: f64,
        local_y: f64,
    ) -> Option<usize> {
        let is_tray_surface = self.docks.iter().any(|dock| {
            dock.tray_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        });
        let tray_height = crate::render::tray::tray_logical_size(self.tray_items.len()).1;
        if !is_tray_surface
            || self.tray_items.is_empty()
            || local_x < 0.0
            || local_y < 0.0
            || local_y >= tray_height as f64
        {
            return None;
        }
        crate::render::tray::tray_item_at(
            local_x.round() as i32,
            local_y.round() as i32,
            self.tray_items.len(),
        )
    }

    pub fn is_status_tray_bar_surface(&self, surface: &WlSurface) -> bool {
        self.docks.iter().any(|dock| {
            dock.tray_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        })
    }

    pub fn is_bottom_system_status_surface(&self, surface: &WlSurface) -> bool {
        self.docks.iter().any(|dock| {
            dock.system_status_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        })
    }

    pub fn send_tray_action(
        &self,
        index: usize,
        kind: crate::models::TrayActionKind,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(item) = self.tray_items.get(index) else {
            return false;
        };
        self.tray_action_tx
            .send(crate::models::TrayAction {
                key: item.key.clone(),
                service_name: item.service_name.clone(),
                object_path: item.object_path.clone(),
                interface_name: item.interface_name.clone(),
                menu_path: item.menu_path.clone(),
                kind,
                x,
                y,
            })
            .is_ok()
    }

    pub fn send_tray_action_for_key(
        &self,
        key: &str,
        kind: crate::models::TrayActionKind,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(index) = self.tray_items.iter().position(|item| item.key == key) else {
            return false;
        };
        self.send_tray_action(index, kind, x, y)
    }

    fn widget_button_at_surface(&self, surface: &WlSurface, x: f64, y: f64, index: usize) -> bool {
        let on_widget_bar = self.docks.iter().any(|dock| {
            dock.status_widget_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        });
        on_widget_bar
            && crate::render::widget_bar::button_at(index, x.round() as i32, y.round() as i32)
    }

    pub fn is_status_widget_bar_surface(&self, surface: &WlSurface) -> bool {
        self.docks.iter().any(|dock| {
            dock.status_widget_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        })
    }

    pub fn is_audio_player_surface(&self, surface: &WlSurface) -> bool {
        self.docks.iter().any(|dock| {
            dock.audio_player_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
        })
    }

    pub fn audio_player_expanded_on_surface(&self, surface: &WlSurface) -> bool {
        self.docks.iter().any(|dock| {
            dock.audio_player_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
                && dock.audio_player_expanded
        })
    }

    pub fn set_audio_player_expanded(&mut self, surface: &WlSurface, expanded: bool) -> bool {
        for dock in &mut self.docks {
            if dock
                .audio_player_bar
                .as_ref()
                .is_some_and(|bar| bar.surface.id() == surface.id())
            {
                if dock.audio_player_expanded != expanded {
                    dock.audio_player_expanded = expanded;
                    return true;
                }
                return false;
            }
        }
        false
    }

    pub fn audio_button_at_surface(&self, surface: &WlSurface, x: f64, y: f64) -> bool {
        if self.is_audio_player_surface(surface) {
            return crate::render::audio_player::player_area_contains(
                x.round() as i32,
                y.round() as i32,
                self.audio_player_expanded_on_surface(surface),
            );
        }
        self.widget_button_at_surface(surface, x, y, crate::render::widget_bar::AUDIO_INDEX)
    }

    pub fn media_control_at_surface(
        &self,
        surface: &WlSurface,
        x: f64,
        y: f64,
    ) -> Option<crate::models::MediaControl> {
        if !self.is_audio_player_surface(surface) || self.audio_snapshot.media_status.is_none() {
            return None;
        }
        crate::render::audio_player::control_at(
            x.round() as i32,
            y.round() as i32,
            self.audio_player_expanded_on_surface(surface),
        )
    }

    pub fn bluetooth_button_at_surface(&self, surface: &WlSurface, x: f64, y: f64) -> bool {
        self.widget_button_at_surface(surface, x, y, crate::render::widget_bar::BLUETOOTH_INDEX)
    }

    /// Check if the click is on the Bluetooth icon inside the dashed block widget.
    ///
    /// The dashed block is a separate layer surface, so this check is independent
    /// of the widget bar button hit test.
    pub fn bluetooth_button_at_dashed_block(&self, surface: &WlSurface, x: f64, y: f64) -> bool {
        let on_dashed_block = self.docks.iter().any(|dock| {
            dock.dashed_block_surface
                .as_ref()
                .is_some_and(|block| block.surface.wl_surface() == surface)
        });
        on_dashed_block && crate::render::dashed_block::bluetooth_icon_at(x, y, &self.config)
    }

    /// Calculate the screen position for the Bluetooth widget window.
    ///
    /// Returns (x, y) in logical screen coordinates, positioned to the right
    /// of the dashed block with a 4px gap. The widget uses these coordinates
    /// via environment variables passed to the spawned process.
    pub fn bluetooth_widget_position(&self) -> (i32, i32) {
        let config = &self.config;
        let gap = config.dashed_block_gap_from_dock as i32;
        let margin = config.margin;
        let db_width = config.dashed_block_width as i32;
        let db_height = config.dashed_block_height as i32;
        let bt_gap: i32 = 4; // gap between dashed block and BT widget

        let dock = self.docks.first();
        let Some(dock) = dock else {
            return (0, 0);
        };

        let output_size = self.output_state.info(&dock.output)
            .and_then(|info| info.logical_size)
            .unwrap_or((1920, 1080));
        let output_width = output_size.0 as i32;
        let output_height = output_size.1 as i32;
        let dock_width = dock.width as i32;
        let surface_height = dock.height as i32;

        // Calculate the dashed block's left edge (screen X coordinate)
        let db_left = match config.anchor {
            crate::config::DockAnchor::Bottom => {
                let dock_right_edge = (output_width / 2) + (dock_width / 2);
                dock_right_edge + gap
            }
            crate::config::DockAnchor::Top => {
                let dock_right_edge = (output_width / 2) + (dock_width / 2);
                dock_right_edge + gap
            }
            crate::config::DockAnchor::Left => {
                let dock_bottom_edge = (output_height / 2) + (surface_height / 2);
                dock_bottom_edge + gap
            }
            crate::config::DockAnchor::Right => {
                let dock_bottom_edge = (output_height / 2) + (surface_height / 2);
                dock_bottom_edge + gap
            }
        };

        // Calculate the BT widget's X: dashed block left + block width + gap
        let bt_x = db_left + db_width + bt_gap;

        // Calculate the BT widget's Y: aligned vertically with the dashed block
        let bt_y = match config.anchor {
            crate::config::DockAnchor::Bottom => {
                output_height - surface_height - margin - db_height
            }
            crate::config::DockAnchor::Top => {
                margin + surface_height
            }
            crate::config::DockAnchor::Left => {
                let dock_bottom_edge = (output_height / 2) + (surface_height / 2);
                let db_bottom = dock_bottom_edge + gap;
                output_height - db_bottom - margin - db_height
            }
            crate::config::DockAnchor::Right => {
                let dock_bottom_edge = (output_height / 2) + (surface_height / 2);
                let db_bottom = dock_bottom_edge + gap;
                output_height - db_bottom - margin - db_height
            }
        };

        (bt_x, bt_y)
    }

    pub fn send_audio_action(&self, action: crate::models::AudioAction) -> bool {
        self.audio_action_tx.send(action).is_ok()
    }

    pub fn send_bluetooth_action(&self, action: crate::models::BluetoothAction) -> bool {
        self.bluetooth_action_tx.send(action).is_ok()
    }

    pub fn rclone_button_at_surface(&self, surface: &WlSurface, x: f64, y: f64) -> bool {
        self.widget_button_at_surface(surface, x, y, crate::render::widget_bar::RCLONE_INDEX)
    }

    pub fn process_button_at_surface(&self, surface: &WlSurface, x: f64, y: f64) -> bool {
        self.widget_button_at_surface(surface, x, y, crate::render::widget_bar::PROCESS_INDEX)
    }

    pub fn set_rclone_monitor_enabled(&self, enabled: bool) {
        let _ = self.rclone_monitor_tx.send(enabled);
    }

    pub fn set_process_monitor_enabled(&self, enabled: bool) {
        let _ = self.process_monitor_tx.send(enabled);
    }
}

/// Generates a blank/generic 48x48 RGBA fallback icon when an icon cannot be found anywhere
pub(crate) fn load_generic_fallback_bytes() -> Option<(Vec<u8>, u32)> {
    let size = 48;
    let total_bytes = (size * size * 4) as usize;
    let mut rgba = vec![0u8; total_bytes];

    // Efficiently fill buffer using fixed 4-byte RGBA chunks (128, 128, 128, 180)
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[128, 128, 128, 180]);
    }

    Some((rgba, size))
}

impl AppState {
    pub fn window_id_for_object(&self, object_id: &ObjectId) -> Option<WindowId> {
        let id = WindowId(object_id.clone());
        self.windows.contains_key(&id).then_some(id)
    }

    pub fn window_state(&self, object_id: &ObjectId) -> Option<&WindowState> {
        self.window_id_for_object(object_id)
            .and_then(|id| self.windows.get(&id))
    }

    pub fn window_state_mut(&mut self, object_id: &ObjectId) -> Option<&mut WindowState> {
        let id = self.window_id_for_object(object_id)?;
        self.windows.get_mut(&id)
    }

    pub fn window_handle(&self, object_id: &ObjectId) -> Option<&wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1>{
        self.window_handles.get(object_id)
    }

    pub fn window_handle_mut(&mut self, object_id: &ObjectId) -> Option<&mut wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1>{
        self.window_handles.get_mut(object_id)
    }

    /// Projects the authoritative registry into the live state tree.
    pub fn sync_system_registry(&mut self) {
        self.sync_live_state_from_registry();
    }

    /// Mirrors the first discovered media-bearing window into the UI state.
    /// Rebuilds the live state projections from the authoritative registry.
    /// This is intentionally a projection step: resolvers and listeners own discovery,
    /// while state owns the fast, UI-facing representation.
    pub fn sync_live_state_from_registry(&mut self) {
        self.applications.clear();
        let mut old_windows = std::mem::take(&mut self.windows);
        self.processes.clear();
        self.outputs.clear();

        for (app_id, app) in &self.system_registry.applications {
            let mut live = ApplicationState::new(app_id.clone());
            live.identifier = app.identity.identifier.clone();
            live.name = app.identity.name.clone();
            live.version = app.identity.version.clone();
            live.icon = app.identity.icon.clone();
            live.category = app.identity.category.clone();
            live.binary = app.binary.clone();
            live.running = app.status.running;
            live.foreground = app.status.foreground;
            live.background = app.status.background;
            live.attention = app.status.attention;
            live.paused = app.status.paused || app.status.suspended;
            live.stuck = app.status.stuck;
            live.locked = app.status.locked;
            live.media_active = false;
            live.media = None;
            live.capabilities = app.capabilities.clone();
            live.platform = app.platform.clone();
            live.runtime = app.runtime.clone();
            live.packaging = app.packaging.clone();
            live.distribution = app.distribution.clone();
            live.resolution_confidence = app.resolution_confidence;
            live.resolution_trace = app.resolution_trace.clone();
            live.process_count = app.processes.len();
            live.window_count = app.windows.len();
            self.applications.insert(app_id.clone(), live);
        }

        for (id, window) in &self.system_registry.windows {
            let mut live = old_windows
                .remove(id)
                .unwrap_or_else(|| WindowState::new(id.clone()));
            live.id = id.clone();
            live.app_id = Some(window.app_id.clone());
            live.process_id = window.process_id;
            live.title = window.title.clone();
            live.geometry = window.geometry;
            live.capabilities = window.capabilities.clone();
            live.output_id = window.outputs.first().cloned();
            live.output_ids = window.outputs.clone();
            live.status = window.status;
            live.priority = window.priority;
            live.layer = window.layer;
            live.focused = window.is_activated;
            live.foreground = window.is_foreground;
            live.background = window.is_background;
            live.minimized = window.is_minimized;
            live.locked = window.is_locked;
            live.order_index = window.order_index;
            live.media = window.media.clone();
            live.resolution_confidence = window.resolution_confidence;
            live.resolution_trace = window.resolution_trace.clone();
            if window.geometry.width > 0 && window.geometry.height > 0 {
                live.areas.outer = Some(crate::geometry::Rect::new(
                    window.geometry.x as f64,
                    window.geometry.y as f64,
                    window.geometry.width as f64,
                    window.geometry.height as f64,
                ));
                live.areas.content = live.areas.outer;
                live.areas.main = live.areas.outer;
            }
            live.input.input_region = live.areas.input.or(live.areas.content);
            live.visible = !window.is_minimized;
            live.input.accepts_input = !live.locked && live.visible;
            self.windows.insert(id.clone(), live);
        }

        for (pid, process) in &self.system_registry.processes {
            let mut live = ProcessState::new(*pid);
            live.app_id = process.app_id.clone();
            live.parent_pid = process.ppid;
            live.binary = process.binary.clone();
            live.runtime = process.runtime.clone();
            live.packaging = process.packaging.clone();
            live.distribution = process.distribution.clone();
            live.platform = process.platform.clone();
            live.responding = process.status.responding;
            live.health = if !process.status.running {
                ProcessHealthState::Stopped
            } else if !process.status.responding {
                ProcessHealthState::Unresponsive
            } else {
                ProcessHealthState::Running
            };
            live.executable_path = process.binary.path.clone();
            live.resolution_confidence = process.resolution_confidence;
            live.resolution_trace = process.resolution_trace.clone();
            self.processes.insert(*pid, live);
        }

        for (id, output) in &self.system_registry.outputs {
            let mut live = LiveOutputState::default();
            live.output_id = Some(id.clone());
            live.connected = output.status.enabled;
            live.scale_factor = output.scale_factor;
            live.width = output.geometry.width;
            live.height = output.geometry.height;
            live.refresh_hz = output.refresh_rate_hz;
            self.outputs.insert(id.clone(), live);
        }

        self.system.outputs = self.outputs.keys().cloned().collect();
        self.system.active = !self.system.outputs.is_empty();
        self.system.touch();

        // Media is associated with windows; project it upward to the owning app.
        for window in self.windows.values() {
            if let (Some(app_id), Some(media)) = (&window.app_id, &window.media) {
                if let Some(app) = self.applications.get_mut(app_id) {
                    app.media_active = true;
                    app.media = Some(media.clone());
                }
            }
        }

        self.sync_media_state_from_registry();
    }

    pub fn sync_media_state_from_registry(&mut self) {
        let snapshot = self
            .system_registry
            .media_windows()
            .into_iter()
            .next()
            .map(|window| (window.id.0.clone(), window.media.clone()));

        if let Some((window_id, media)) = snapshot {
            self.media_state.target_window = Some(window_id.clone());
            self.media_state.media_link = media.clone();
            if let Some(link) = media.as_ref() {
                self.media_state.is_playing = link.is_playing;
                self.media_state.muted = link.muted;
                self.media_state.volume = link.volume;
                self.media_state.position_ms = link.position_ms;
                self.media_state.duration_ms = link.duration_ms;
            } else {
                self.media_state.is_playing = false;
                self.media_state.muted = false;
                self.media_state.volume = None;
                self.media_state.position_ms = None;
                self.media_state.duration_ms = None;
            }

            let target = crate::models::system::WindowId(window_id.clone());
            self.media.target_window = Some(target.clone());
            self.media.active_app = self
                .windows
                .get(&target)
                .and_then(|window| window.app_id.clone());
            self.media.media_link = media;
            self.media.is_playing = self
                .media
                .media_link
                .as_ref()
                .map(|link| link.is_playing)
                .unwrap_or(false);
        } else {
            self.media_state.target_window = None;
            self.media_state.media_link = None;
            self.media_state.is_playing = false;
            self.media.clear();
        }
    }
}
