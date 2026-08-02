use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_offer::WlDataOffer,
    wl_output::WlOutput,
};

// SCTK Types
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shm::slot::Buffer; // Or wayland_client::protocol::wl_buffer::WlBuffer if using raw wayland buffers

// Wayland Protocols
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;

// Project Types
use crate::DesktopAction;

// Store notifiers per-dock in DockInstance
pub struct DockInstance {
    pub surface: LayerSurface,
    pub output: WlOutput,
    pub width: u32,
    pub height: u32,
    pub current_buffer: Option<Buffer>,
    pub scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,
}
// -----

// Drag and drop
#[derive(Default)]
pub struct DndState {
    /// Currently active data offer being dragged over our surface
    pub current_offer: Option<WlDataOffer>,
    /// MIME types supported by the current offer
    pub mime_types: Vec<String>,
    /// Current pointer position during drag
    pub drag_x: f64,
    pub drag_y: f64,
    /// Index of dock item currently hovered during drag
    pub hovered_dock_index: Option<usize>,
}
// Context Menu
#[derive(Clone, Debug, PartialEq)]
pub enum MenuItemType {
    Focus,
    LaunchNew,
    Minimize,
    Action(DesktopAction),
    TogglePin,
    CloseApp,
}

#[derive(Clone, Debug)]
pub struct ContextMenuItem {
    pub label: String,
    pub item_type: MenuItemType,
}

pub struct MenuState {
    pub x: usize,
    pub y: usize,
    pub target_window: Option<ObjectId>,
    pub target_app_id: Option<String>,
    pub is_open: bool,
    pub items: Vec<ContextMenuItem>,
}

// ------
pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}
