use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_data_offer::WlDataOffer, wl_output::WlOutput, wl_subsurface::WlSubsurface,
    wl_surface::WlSurface,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
// SCTK Types
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::Buffer;

// Wayland Protocols
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;

use std::time::Instant;

// Project Types
use crate::render::font::FontManager;
use crate::graphics::fade::FadeAnimation;
use crate::geometry::{Point, Rect};
use crate::state::AppState;
use crate::DesktopAction;

// --- Unified Interaction State ---
#[derive(Clone, Debug)] // <-- Add Clone here
pub struct DockState {
    pub pins: Vec<PinItem>,
    pub hovered_index: Option<usize>,
    pub current_visibility_alpha: f32,
    pub panel_rect: Rect,
    pub fade: FadeAnimation,
}

#[derive(Clone, Debug)]
pub struct PinItem {
    pub app_id: String,
    pub x: i32,
    pub y: i32,
    pub size: usize,
    pub scale_factor: f32,
    pub is_running: bool,
    pub is_activated: bool,
    pub running_count: usize,
    pub badge_count: Option<u32>,
}

pub struct DockRenderState<'a> {
    pub phys_width: u32,
    pub phys_height: u32,
    pub scale_factor: f64,
    pub dock_height: usize,
    pub alpha: f32,
    pub apps_in_dock: &'a [String],
    pub running_by_app:
        &'a std::collections::HashMap<String, Vec<&'a crate::models::WindowDiagnostics>>,
    pub open_windows: &'a std::collections::HashMap<ObjectId, crate::models::WindowDiagnostics>,
    pub icon_cache: &'a std::collections::HashMap<String, (Vec<u8>, u32)>,
    pub badges: &'a std::collections::HashMap<String, crate::models::BadgeUpdate>,
    pub font_manager: &'a mut FontManager,
    pub fallback_anim: &'a crate::animations::IconAnimation,
    pub is_dragging: bool,
    pub dragged_app_id: Option<&'a String>,
    pub pointer_position: (i32, i32),
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
    pub is_pointer_near: bool,
    pub focused_surface: Option<WlSurface>,
}

impl InteractionState {
    pub fn new() -> Self {
        Self {
            hovered_icon: None,
            hovered_menu: None,
            dragging_icon: None,
            pointer_position: Point { x: 0.0, y: 0.0 },
            pointer_inside: false,
            is_pointer_near: false,
            focused_surface: None,
        }
    }
}

impl Default for InteractionState {
    fn default() -> Self {
        Self::new()
    }
}

// --- Surfaces & Layouts ---

pub struct PopupSurface {
    pub subsurface: WlSubsurface,
    pub surface: WlSurface,
    pub current_buffer: Option<Buffer>,
    pub width: u32,
    pub height: u32,
    pub current_app_id: Option<String>,
    /// Position of the popup subsurface in dock-surface logical coordinates.
    /// This is the authoritative coordinate used to map pointer events back
    /// into dock coordinates.
    pub position: (i32, i32),
}

impl PopupSurface {
    pub fn destroy(self) {
        // Unmap the surface first
        self.surface.attach(None, 0, 0);
        self.surface.commit();
        
        // Destroy Wayland protocol objects on the server
        self.subsurface.destroy();
        self.surface.destroy();
    }
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
    pub configured: bool,
    pub context_menu_popup: Option<PopupSurface>,
    pub window_list_popup: Option<PopupSurface>,
    pub dock_state: Option<DockState>,
    pub hover_fade: FadeAnimation,
    pub menu_fade: FadeAnimation,
    pub window_list_fade: FadeAnimation,
    pub active_hovered_app: Option<(usize, String)>,
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
            let trigger_h = 3;
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
    FocusWindow(ZwlrForeignToplevelHandleV1),
}

#[derive(Clone, Debug)]
pub struct ContextMenuItem {
    pub label: String,
    pub item_type: MenuItemType,
}

pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}

// --- Unified Popup State ---
pub struct PopupState<T> {
    pub target_app_id: Option<String>,
    pub target_window: Option<ObjectId>,
    pub is_open: bool,
    pub items: T,
    pub waiting_for_initial_release: bool,
    pub just_opened: bool,
    pub cursor_moved: bool,
    pub fade: FadeAnimation,
    pub last_debug_print: Instant,
    pub no_motion_timer: Option<Instant>,
    pub dock_timer: Option<Instant>,
    pub popup_timer: Option<Instant>,
    
    // Compatibility & Interaction fields
    pub pointer_inside_popup: bool,
    pub x: i32,
    pub y: i32,
    pub opened_by_button: Option<u32>,
    pub consecutive_false_count: u32,
    pub last_pointer_x: f64,
    pub last_pointer_y: f64,
    pub consecutive_no_motion_count: u32,
    pub consecutive_on_dock_count: u32,
    pub consecutive_on_context_menu_count: u32,
    pub consecutive_on_window_list_count: u32,
    pub context_menu_timer: Option<Instant>,
    pub window_list_timer: Option<Instant>,
}

impl<T: Default> Default for PopupState<T> {
    fn default() -> Self {
        Self {
            target_app_id: None,
            target_window: None,
            is_open: false,
            items: T::default(),
            waiting_for_initial_release: false,
            just_opened: false,
            cursor_moved: false,
            fade: FadeAnimation::new(0.0, 1.0, 0.25),
            last_debug_print: Instant::now(),
            no_motion_timer: None,
            dock_timer: None,
            popup_timer: None,
            pointer_inside_popup: false,
            x: 0,
            y: 0,
            opened_by_button: None,
            consecutive_false_count: 0,
            last_pointer_x: 0.0,
            last_pointer_y: 0.0,
            consecutive_no_motion_count: 0,
            consecutive_on_dock_count: 0,
            consecutive_on_context_menu_count: 0,
            consecutive_on_window_list_count: 0,
            context_menu_timer: None,
            window_list_timer: None,
        }
    }
}

// Type aliases for clarity and backward compatibility
pub type MenuState = PopupState<Vec<ContextMenuItem>>;
pub type WindowListState = PopupState<()>;