use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_offer::WlDataOffer,
    wl_output::WlOutput,
};

// SCTK Types
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shm::slot::Buffer;
use smithay_client_toolkit::shell::WaylandSurface;

// Wayland Protocols
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;
use wayland_client::protocol::wl_subsurface::WlSubsurface;

// Project Types
use crate::DesktopAction;
use crate::app::state::AppState;

// --- Unified Interaction State ---

#[derive(Clone, Debug)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuId {
    ContextMenu,
    AppMenu,
}

#[derive(Clone, Debug)]
pub struct InteractionState {
    pub hovered_icon: Option<usize>,
    pub hovered_menu: Option<MenuId>,
    pub dragging_icon: Option<usize>,
    pub pointer_position: Point,
    pub pointer_inside: bool,
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            hovered_icon: None,
            hovered_menu: None,
            dragging_icon: None,
            pointer_position: Point { x: 0.0, y: 0.0 },
            pointer_inside: false,
        }
    }
}

// --- Surfaces & Layouts ---

pub struct PopupSurface {
    pub subsurface: WlSubsurface,
    pub surface: wayland_client::protocol::wl_surface::WlSurface,
    pub current_buffer: Option<Buffer>,
    pub width: u32,
    pub height: u32,
}

pub struct DockInstance {
    pub surface: LayerSurface,
    pub output: WlOutput,
    pub width: u32,
    pub height: u32,
    pub current_buffer: Option<Buffer>,
    pub scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,
    pub hover_popup: Option<PopupSurface>,
    pub menu_popup: Option<PopupSurface>,
}

impl DockInstance {
    pub fn update_input_region(
        &self,
        compositor_state: &smithay_client_toolkit::compositor::CompositorState,
        is_hidden: bool,
        container_start_x: i32,
        container_width: i32,
        qh: &wayland_client::QueueHandle<AppState>,
    ) {
        let surface = self.surface.wl_surface();

        if is_hidden {
            let region = compositor_state.wl_compositor().create_region(qh, ());
            
            let margin = 5;
            let trigger_x = (container_start_x - margin).max(0);
            let trigger_w = container_width + (margin * 2);

            // Anchor trigger region to the bottom of the dock surface
            let trigger_h = 5;
            let surface_h = self.height as i32;
            let trigger_y = (surface_h - trigger_h).max(0);

            region.add(trigger_x, trigger_y, trigger_w, trigger_h);

            surface.set_input_region(Some(&region));
            region.destroy();
        } else {
            // Restore full surface input region when visible
            surface.set_input_region(None);
        }

        surface.commit();
    }
}

// Drag and drop
#[derive(Default)]
pub struct DndState {
    pub current_offer: Option<WlDataOffer>,
    pub mime_types: Vec<String>,
    pub drag_x: f64,
    pub drag_y: f64,
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

pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}
