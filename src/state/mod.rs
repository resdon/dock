use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Instant;

use dockman_lib::animations::IconAnimation;

use smithay_client_toolkit::{
    compositor::CompositorState,
    output::OutputState,
    registry::RegistryState,
    seat::SeatState,
    shell::wlr_layer::LayerShell,
    shm::slot::{Buffer, SlotPool},
    shm::Shm,
};
use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_device::WlDataDevice,
    wl_data_device_manager::WlDataDeviceManager,
    wl_output::WlOutput,
    wl_pointer::WlPointer,
    wl_seat::WlSeat,
    wl_subcompositor::WlSubcompositor,
};
use wayland_client::Connection;

use wayland_protocols::wp::fractional_scale::v1::client::{
    wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1,
    wp_fractional_scale_v1::WpFractionalScaleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::ZwlrForeignToplevelManagerV1;

use crate::graphics::hide::AutoHideState;
use crate::models::{BadgeUpdate, WindowDiagnostics};
use crate::render::font::FontManager;

use crate::app::icon_load::IconLoader;
use crate::app::types::{DndState, DockInstance, HoverState, InteractionState, MenuState};


pub(crate) mod dock;
mod interaction;
mod window;

pub struct IconLoadResult {
    pub app_id: String,
    pub rgba: Vec<u8>,
    pub size: u32,
    pub animation: Option<IconAnimation>,
}

pub struct AppState {
    // Wayland Protocol & Compositor Core
    pub connection: Connection,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub output_state: OutputState,
    pub layer_shell: LayerShell,
    pub shm_state: Shm,
    pub pool: SlotPool,
    pub seat_state: SeatState,
    pub subcompositor: Option<WlSubcompositor>, // Updated: wrapped in Option
    pub layer_surface: Option<smithay_client_toolkit::shell::wlr_layer::LayerSurface>,
    // Removed: context_menu_layer_surface
    pub current_buffer: Option<Buffer>,
    pub width: i32,
    pub height: i32,
    pub toplevel_manager: Option<ZwlrForeignToplevelManagerV1>,
    pub font_manager: FontManager,
    pub wl_seat: Option<WlSeat>,
    pub wl_pointer: Option<WlPointer>,

    // Application & Dock State Management
    pub interaction: InteractionState,
    pub open_windows: HashMap<ObjectId, WindowDiagnostics>,
    pub pinned_apps: Vec<String>,
    pub menu_state: MenuState,
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
    pub current_output: Option<WlOutput>,
    pub docks: Vec<DockInstance>,

    // Fractional Scaling
    pub fractional_scale_manager: Option<WpFractionalScaleManagerV1>,
    pub fractional_scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,

    // Drag-and-Drop & Animations
    pub fallback_anim: IconAnimation,
    pub dnd_state: DndState,
    pub data_device_manager: Option<WlDataDeviceManager>,
    pub data_device: Option<WlDataDevice>,

    // App Integrations & DBus Badges
    pub badges: HashMap<String, BadgeUpdate>,

    // Async Icon Loading & Caching
    pub icon_cache: HashMap<String, (Vec<u8>, u32)>,
    pub pending_icon_searches: HashSet<String>,
    pub icon_rx: Receiver<IconLoadResult>,
    pub icon_tx: Sender<IconLoadResult>,
    pub icon_load: IconLoader,
    pub animations: HashMap<String, IconAnimation>,

    // Autohide System State
    pub hide_state: AutoHideState,

    // Focus check
    pub focus_action_performed: bool,
    pub focus_action_time: Option<Instant>,
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
