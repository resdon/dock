//! Core data types for the dock application.
//!
//! Defines all major structs and enums used across the codebase:
//! - `DockInstance` — per-output dock surface and its popup subsurfaces
//! - `PopupSurface` — a Wayland subsurface for popups (window list, context menu, etc.)
//! - `InteractionState` — pointer hover, drag, and menu state
//! - `MenuState` / `WindowListState` — typed popup states via `PopupState<T>`
//! - `DockRenderState` — snapshot passed to renderers each frame
//! - `PinItem` — a single dock icon with position and running state
//! - `DndState` — drag-and-drop application state
//! - `MediaWindowState` — media player link to a window
//! - `ContextMenuItem` / `MenuItemType` — right-click context menu items

use wayland_client::backend::ObjectId;
use wayland_client::protocol::{
    wl_output::WlOutput, wl_subsurface::WlSubsurface, wl_surface::WlSurface,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;
// SCTK Types
use smithay_client_toolkit::shell::wlr_layer::LayerSurface;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shm::slot::Buffer;

// Wayland Protocols
use wayland_protocols::wp::fractional_scale::v1::client::wp_fractional_scale_v1::WpFractionalScaleV1;

use std::collections::{HashMap, HashSet};
use std::time::Instant;

// Project Types
use crate::geometry::{Point, Rect};
use crate::graphics::fade::FadeAnimation;
use crate::models::system::OfferId;
use crate::models::MediaWindowLink;
use crate::render::font::FontManager;
use crate::state::AppState;
use crate::DesktopAction;

// --- Unified Interaction State ---

/// Internal dock state used by the Slint renderer for icon layout and animations.
#[derive(Clone, Debug)]
pub struct DockState {
    /// Pinned apps with their positions and running state
    pub pins: Vec<PinItem>,
    /// Index of the currently hovered icon (for highlight rendering)
    pub hovered_index: Option<usize>,
    /// Current dock visibility alpha (0.0 = hidden, 1.0 = fully visible)
    pub current_visibility_alpha: f32,
    /// Dock panel bounding rectangle in screen coordinates
    pub panel_rect: Rect,
    /// Fade animation for dock show/hide transitions
    pub fade: FadeAnimation,
}

/// A single pinned application icon in the dock.
#[derive(Clone, Debug)]
pub struct PinItem {
    /// Application ID (e.g., "org.wezfurlong.wezterm")
    pub app_id: String,
    /// X position in the dock row (in pixels)
    pub x: i32,
    /// Y position (usually 0 for horizontal dock)
    pub y: i32,
    /// Icon size in pixels
    pub size: usize,
    /// HiDPI scale factor for this icon
    pub scale_factor: f32,
    /// Whether the app has running windows
    pub is_running: bool,
    /// Whether the app has the focused window
    pub is_activated: bool,
    /// Number of running windows for this app
    pub running_count: usize,
    /// Unity launcher badge count (e.g., notification count)
    pub badge_count: Option<u32>,
}

/// Snapshot of dock state passed to the renderer each frame.
///
/// Contains all data needed to render one frame of the dock: icon positions,
/// running state, icon images, badges, and pointer position.
pub struct DockRenderState<'a> {
    /// Physical width of the dock buffer (scaled by HiDPI)
    pub phys_width: u32,
    /// Physical height of the dock buffer
    pub phys_height: u32,
    /// HiDPI scale factor (e.g., 1.0 for standard, 2.0 for HiDPI)
    pub scale_factor: f64,
    /// Logical dock height in pixels
    pub dock_height: usize,
    /// Dock alpha for fade-in/out animations
    pub alpha: f32,
    /// Ordered list of pinned app IDs in the dock
    pub apps_in_dock: &'a [String],
    /// Map from app_id to its running window states
    pub running_by_app: &'a std::collections::HashMap<String, Vec<&'a crate::state::WindowState>>,
    /// Cache of loaded icon images (RGBA bytes + width)
    pub icon_cache: &'a std::collections::HashMap<String, (Vec<u8>, u32)>,
    /// Unity badge updates per app
    pub badges: &'a std::collections::HashMap<String, crate::models::BadgeUpdate>,
    /// Font manager for text rendering
    pub font_manager: &'a mut FontManager,
    /// Whether a drag operation is in progress
    pub is_dragging: bool,
    /// App ID being dragged (for reordering)
    pub dragged_app_id: Option<&'a String>,
    /// Current pointer position in dock coordinates
    pub pointer_position: (i32, i32),
}

/// Identifier for which popup menu is currently active.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuId {
    /// Right-click context menu (Launch, Focus, Unpin, Quit)
    ContextMenu,
    /// App-specific menu (e.g., tray item menu)
    AppMenu,
}

/// Tracks pointer interaction state: which icon is hovered, which menu is open,
/// and whether a drag operation is in progress.
#[derive(Clone, Debug)]
pub struct InteractionState {
    /// Index of the currently hovered dock icon
    pub hovered_icon: Option<usize>,
    /// Which popup menu is currently hovered (context menu or app menu)
    pub hovered_menu: Option<MenuId>,
    /// Index of the icon being dragged (for reordering)
    pub dragging_icon: Option<usize>,
    /// Current pointer position in screen coordinates
    pub pointer_position: Point,
    /// Whether the pointer is inside the dock area
    pub pointer_inside: bool,
    /// Whether the pointer is near the dock (for proximity-based show)
    pub is_pointer_near: bool,
    /// The Wayland surface that currently has keyboard focus
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

/// A Wayland subsurface used for popups (window list, context menu, audio panel, etc.).
///
/// Each popup is a separate subsurface attached to the main dock surface,
/// allowing independent rendering and fade animations.
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
    /// Cached opaque popup pixels. Fade frames reuse this content instead of
    /// rerasterizing backgrounds, separators, and glyphs.
    pub cached_content: Vec<u8>,
    pub cached_phys_width: i32,
    pub cached_phys_height: i32,
    /// Tracks when the input region geometry changes so we avoid redundant
    /// WlRegion recreation on every frame.
    pub input_region_key: u64,
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

/// One dock instance per Wayland output (monitor).
///
/// Contains the main layer surface, all popup subsurfaces, fade animations,
/// and rendering state for a single output.
pub struct DockInstance {
    pub surface: LayerSurface,
    pub output: WlOutput,
    pub width: u32,
    /// Most recent logical width requested from the compositor.
    pub requested_width: u32,
    pub height: u32,
    pub current_buffer: Option<Buffer>,
    pub scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,
    pub hover_popup: Option<PopupSurface>,
    pub menu_popup: Option<PopupSurface>,
    pub configured: bool,
    /// True only after a frame containing the tray/system status strip has
    /// been attached. This forces the second startup frame even if popup-only
    /// animation would otherwise skip the main surface.
    pub status_strip_rendered: bool,
    pub context_menu_popup: Option<PopupSurface>,
    pub window_list_popup: Option<PopupSurface>,
    pub audio_popup: Option<PopupSurface>,
    pub bluetooth_popup: Option<PopupSurface>,
    pub rclone_popup: Option<PopupSurface>,
    pub process_popup: Option<PopupSurface>,
    /// Native SNI icons in a separate rounded box to the left of the app dock.
    pub tray_bar: Option<PopupSurface>,
    /// Persistent utility widget cluster in a separate rounded box to the right.
    pub status_widget_bar: Option<PopupSurface>,
    /// Always-visible media player card in its own left-side subsurface.
    pub audio_player_bar: Option<PopupSurface>,
    /// Expands the compact media player while the pointer is over its surface.
    pub audio_player_expanded: bool,
    /// Keyboard, network and clock information in a slim bar beneath the dock.
    pub system_status_bar: Option<PopupSurface>,
    pub dock_state: Option<DockState>,
    pub hover_fade: FadeAnimation,
    pub menu_fade: FadeAnimation,
    pub window_list_fade: FadeAnimation,
    pub active_hovered_app: Option<(usize, String)>,
}

impl DockInstance {
    /// Update the clickable input region on the layer surface.
    ///
    /// When hidden, only a thin trigger strip at the edge is clickable.
    /// When visible, the full app row area is clickable.
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
            // The top-level layer surface contains the dock row and the lower
            // status strip. Restrict its input region to the app row so the
            // passive bottom status strip remains click-through to windows.
            let region = compositor_state.wl_compositor().create_region(qh, ());
            region.add(
                container_start_x.max(0),
                0,
                container_width.max(1),
                crate::state::dock::DOCK_HEIGHT as i32,
            );
            surface.set_input_region(Some(&region));
            region.destroy();
        }

        surface.commit();
    }
}

/// Wayland-only protocol negotiation state. This is deliberately separate from
/// `DndState`, which is the application/UI representation consumed by interaction.
#[derive(Default, Debug)]
pub struct WaylandDndProtocolState {
    pub current_offer: Option<OfferId>,
    pub mime_types: HashMap<OfferId, HashSet<String>>,
    pub drag_x: f64,
    pub drag_y: f64,
    pub current_serial: Option<u32>,
}

impl WaylandDndProtocolState {
    pub fn clear_offer(&mut self, offer_id: &OfferId) {
        self.mime_types.remove(offer_id);
        if self.current_offer.as_ref() == Some(offer_id) {
            self.current_offer = None;
            self.current_serial = None;
        }
    }
}

// --- Drag and Drop ---

/// Application-level drag-and-drop state (UI representation).
#[derive(Default)]
pub struct DndState {
    pub current_offer: Option<ObjectId>,
    pub mime_types: Vec<String>,
    pub drag_x: f64,
    pub drag_y: f64,
    pub current_serial: Option<u32>,
    pub hovered_dock_index: Option<usize>,
}

/// Links a media player (PipeWire) to a specific Wayland window.
#[derive(Clone, Debug, Default)]
pub struct MediaWindowState {
    pub target_window: Option<ObjectId>,
    pub media_link: Option<MediaWindowLink>,
    pub is_playing: bool,
    pub muted: bool,
    pub volume: Option<f32>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

// --- Context Menu ---

/// Action type for a context menu item (right-click on dock icon).
#[derive(Clone, Debug, PartialEq)]
pub enum MenuItemType {
    Focus,
    LaunchNew,
    Minimize,
    Action(DesktopAction),
    TogglePin,
    CloseApp,
    FocusWindow(ZwlrForeignToplevelHandleV1),
    TrayActivate(String),
    TraySecondaryActivate(String),
    TrayContextMenu(String),
}

/// A single row in the right-click context menu.
#[derive(Clone, Debug)]
pub struct ContextMenuItem {
    /// Display text (e.g., "Launch", "Quit Application")
    pub label: String,
    /// Action performed when clicked
    pub item_type: MenuItemType,
}

/// Tracks which dock icon is currently hovered by the pointer.
pub struct HoverState {
    /// X position of the hover highlight
    pub x: usize,
    /// App ID of the hovered icon
    pub app_id: Option<String>,
    /// Whether the hover highlight is visible
    pub is_visible: bool,
    /// When the pointer last left the dock area (for delayed hide)
    pub last_leave_time: Option<std::time::Instant>,
}

// --- Unified Popup State ---

/// Generic popup state used for both the context menu and window list popup.
///
/// `T` is the items type: `Vec<ContextMenuItem>` for context menu, `()` for window list.
pub struct PopupState<T> {
    pub target_app_id: Option<String>,
    pub target_window: Option<ObjectId>,
    /// Tray menu target, used only by the shared context-menu popup.
    pub target_tray_item: Option<String>,
    /// Optional anchor in parent dock-surface logical coordinates.
    pub anchor_override: Option<(i32, i32)>,
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
            target_tray_item: None,
            anchor_override: None,
            is_open: false,
            items: T::default(),
            waiting_for_initial_release: false,
            just_opened: false,
            cursor_moved: false,
            fade: FadeAnimation::new(0.0, 1.0, 0.25), // context_menu_fade_in from config
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
