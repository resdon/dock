//! Wayland client integration for window and surface management.
//!
//! This module provides a high-level abstraction over Wayland client protocols
//! for creating windows, popups, and handling input events. It is designed to
//! work with `labwc` (and other Wayland compositors) without implementing a
//! compositor itself.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Application Code (wgpu-api public types only)              │
//! │  WaylandWindow, WaylandPopup, InputEvent...                 │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Wayland Client Layer                                       │
//! │  ┌──────────────┐  ┌──────────┐  ┌──────────────────────┐  │
//! │  │  Window      │  │  Popup   │  │  Input Handling      │  │
//! │  │  (toplevel)  │  │  (popup) │  │  (keyboard/pointer)  │  │
//! │  └──────┬───────┘  └────┬─────┘  └──────────┬───────────┘  │
//! │         └───────────────┴───────────────────┘              │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │  Wayland Client      │                       │
//! │              │  (wayland-client)    │                       │
//! │              └──────────┬───────────┘                       │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │  labwc Compositor    │                       │
//! │              │  (window management) │                       │
//! │              └──────────────────────┘                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Responsibilities
//!
//! - **labwc** handles: window positioning, stacking, decorations, input
//!   routing, toplevel management, popup placement.
//! - **wgpu-api** handles: rendering pixels inside windows, scene graph,
//!   animations, video.
//!
//! ## Popup Surfaces vs Visual Popups
//!
//! Two distinct concepts:
//!
//! 1. **Visual popup**: A group of nodes drawn inside an existing window.
//!    No separate Wayland surface. Used for tooltips, dropdowns within
//!    a window.
//! 2. **Wayland popup**: A separate Wayland surface managed by the
//!    compositor, with popup positioning, dismissal, and parent
//!    relationships. Used for contextual menus extending outside the
//!    parent window.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::geometry::{Size, Point, Rect};
use crate::color::Color;
use crate::viewport::Viewport;

// ---------------------------------------------------------------------------
// Window identifiers
// ---------------------------------------------------------------------------

/// Opaque handle for a Wayland window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WaylandWindowId(u64);

impl WaylandWindowId {
    /// Create a new window ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for WaylandWindowId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// Opaque handle for a Wayland popup surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WaylandPopupId(u64);

impl WaylandPopupId {
    /// Create a new popup ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for WaylandPopupId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

// ---------------------------------------------------------------------------
// Window state
// ---------------------------------------------------------------------------

/// Lifecycle state of a Wayland window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowState {
    /// Window is being created.
    Creating,
    /// Window is mapped and visible.
    Mapped,
    /// Window is minimized/hidden.
    Minimized,
    /// Window is maximized.
    Maximized,
    /// Window is fullscreen.
    Fullscreen,
    /// Window is being closed.
    Closing,
    /// Window has been closed.
    Closed,
}

impl Default for WindowState {
    fn default() -> Self {
        Self::Creating
    }
}

/// Window type hint for the compositor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowType {
    /// Standard application window.
    Toplevel,
    /// Popup menu or tooltip.
    Popup,
    /// Dialog window.
    Dialog,
    /// Utility window (toolbar, palette).
    Utility,
    /// Splash screen.
    Splash,
    /// Notification.
    Notification,
    /// Dock/panel.
    Dock,
}

impl Default for WindowType {
    fn default() -> Self {
        Self::Toplevel
    }
}

/// Configuration for creating a Wayland window.
#[derive(Debug, Clone)]
pub struct WaylandWindowConfig {
    /// Window title.
    pub title: String,
    /// Initial logical size.
    pub size: Size,
    /// Window type hint.
    pub window_type: WindowType,
    /// Whether the window is resizable.
    pub resizable: bool,
    /// Whether the window should be decorated.
    pub decorated: bool,
    /// Optional parent window ID.
    pub parent: Option<WaylandWindowId>,
    /// Background color (transparent if None).
    pub background: Option<Color>,
    /// Whether the window is visible on creation.
    pub visible: bool,
    /// Optional initial position (compositor may override).
    pub position: Option<Point>,
}

impl WaylandWindowConfig {
    /// Create a new window config with default values.
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            size: Size::new(800.0, 600.0),
            window_type: WindowType::Toplevel,
            resizable: true,
            decorated: true,
            parent: None,
            background: None,
            visible: true,
            position: None,
        }
    }

    /// Set the initial size.
    pub fn with_size(mut self, size: Size) -> Self {
        self.size = size;
        self
    }

    /// Set the window type.
    pub fn with_window_type(mut self, window_type: WindowType) -> Self {
        self.window_type = window_type;
        self
    }

    /// Set whether the window is resizable.
    pub fn with_resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Set whether the window is decorated.
    pub fn with_decorated(mut self, decorated: bool) -> Self {
        self.decorated = decorated;
        self
    }

    /// Set the parent window.
    pub fn with_parent(mut self, parent: WaylandWindowId) -> Self {
        self.parent = Some(parent);
        self
    }

    /// Set the background color.
    pub fn with_background(mut self, background: Color) -> Self {
        self.background = Some(background);
        self
    }

    /// Set the initial visibility.
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the initial position.
    pub fn with_position(mut self, position: Point) -> Self {
        self.position = Some(position);
        self
    }
}

impl Default for WaylandWindowConfig {
    fn default() -> Self {
        Self::new("Window")
    }
}

/// Represents a Wayland window managed by the compositor.
#[derive(Debug, Clone)]
pub struct WaylandWindow {
    /// Unique window identifier.
    pub id: WaylandWindowId,
    /// Window configuration.
    pub config: WaylandWindowConfig,
    /// Current lifecycle state.
    pub state: WindowState,
    /// Current logical size (may differ from config on resize).
    pub size: Size,
    /// Current scale factor from the compositor.
    pub scale_factor: f32,
    /// Associated viewport for coordinate conversion.
    pub viewport: Viewport,
    /// Whether the window has keyboard focus.
    pub focused: bool,
    /// Whether the window is currently visible.
    pub visible: bool,
}

impl WaylandWindow {
    /// Create a new window from configuration.
    pub fn new(id: WaylandWindowId, config: WaylandWindowConfig) -> Self {
        let viewport = Viewport::new(config.size, 1.0);
        Self {
            id,
            state: WindowState::Creating,
            size: config.size,
            scale_factor: 1.0,
            viewport,
            focused: false,
            visible: config.visible,
            config,
        }
    }

    /// Map the window (make it visible to the compositor).
    pub fn map(&mut self) {
        self.state = WindowState::Mapped;
        self.visible = true;
    }

    /// Unmap the window (hide it from the compositor).
    pub fn unmap(&mut self) {
        self.visible = false;
    }

    /// Close the window.
    pub fn close(&mut self) {
        self.state = WindowState::Closing;
        self.visible = false;
    }

    /// Handle a resize event from the compositor.
    pub fn on_resize(&mut self, new_size: Size, scale_factor: f32) {
        self.size = new_size;
        self.scale_factor = scale_factor;
        self.viewport = Viewport::new(new_size, scale_factor);
    }

    /// Handle a focus change event.
    pub fn on_focus_changed(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Handle a minimize event.
    pub fn on_minimize(&mut self) {
        self.state = WindowState::Minimized;
    }

    /// Handle a maximize event.
    pub fn on_maximize(&mut self) {
        self.state = WindowState::Maximized;
    }

    /// Handle a fullscreen event.
    pub fn on_fullscreen(&mut self) {
        self.state = WindowState::Fullscreen;
    }

    /// Get the viewport for coordinate conversion.
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Check if the window is in a visible state.
    pub fn is_visible(&self) -> bool {
        self.visible && self.state != WindowState::Closed && self.state != WindowState::Closing
    }
}

// ---------------------------------------------------------------------------
// Popup surfaces
// ---------------------------------------------------------------------------

/// Positioning rectangle for Wayland popups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupAnchor {
    /// Anchor rectangle in parent surface coordinates.
    pub rect: Rect,
    /// Gravity (which side of the anchor to attach to).
    pub gravity: PopupGravity,
    /// Offset from the anchor point.
    pub offset: Point,
}

impl PopupAnchor {
    /// Create a new anchor at the given point.
    pub fn new(rect: Rect, gravity: PopupGravity, offset: Point) -> Self {
        Self { rect, gravity, offset }
    }
}

/// Gravity for popup positioning relative to the anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupGravity {
    /// Attach to the top-left corner.
    TopLeft,
    /// Attach to the top-right corner.
    TopRight,
    /// Attach to the bottom-left corner.
    BottomLeft,
    /// Attach to the bottom-right corner.
    BottomRight,
    /// Attach to the center-top edge.
    CenterTop,
    /// Attach to the center-bottom edge.
    CenterBottom,
    /// Attach to the center-left edge.
    CenterLeft,
    /// Attach to the center-right edge.
    CenterRight,
}

impl Default for PopupGravity {
    fn default() -> Self {
        Self::BottomLeft
    }
}

/// Configuration for creating a Wayland popup surface.
#[derive(Debug, Clone)]
pub struct WaylandPopupConfig {
    /// Size of the popup surface.
    pub size: Size,
    /// Anchor positioning relative to the parent.
    pub anchor: PopupAnchor,
    /// Whether the popup should be reactive (receive input events).
    pub reactive: bool,
    /// Whether the popup should be hidden when the parent is hidden.
    pub hide_with_parent: bool,
}

impl WaylandPopupConfig {
    /// Create a new popup config.
    pub fn new(size: Size) -> Self {
        Self {
            size,
            anchor: PopupAnchor::new(Rect::new(0.0, 0.0, 0.0, 0.0), PopupGravity::default(), Point::new(0.0, 0.0)),
            reactive: true,
            hide_with_parent: true,
        }
    }

    /// Set the anchor positioning.
    pub fn with_anchor(mut self, anchor: PopupAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    /// Set whether the popup is reactive.
    pub fn with_reactive(mut self, reactive: bool) -> Self {
        self.reactive = reactive;
        self
    }
}

impl Default for WaylandPopupConfig {
    fn default() -> Self {
        Self::new(Size::new(200.0, 150.0))
    }
}

/// Represents a Wayland popup surface managed by the compositor.
#[derive(Debug, Clone)]
pub struct WaylandPopup {
    /// Unique popup identifier.
    pub id: WaylandPopupId,
    /// Parent window ID.
    pub parent: WaylandWindowId,
    /// Popup configuration.
    pub config: WaylandPopupConfig,
    /// Whether the popup is currently visible.
    pub visible: bool,
    /// Whether the popup has been dismissed.
    pub dismissed: bool,
}

impl WaylandPopup {
    /// Create a new popup.
    pub fn new(id: WaylandPopupId, parent: WaylandWindowId, config: WaylandPopupConfig) -> Self {
        Self {
            id,
            parent,
            config,
            visible: true,
            dismissed: false,
        }
    }

    /// Dismiss the popup.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.dismissed = true;
    }

    /// Hide the popup without dismissing (can be shown again).
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Show the popup.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Check if the popup is active.
    pub fn is_active(&self) -> bool {
        self.visible && !self.dismissed
    }
}

// ---------------------------------------------------------------------------
// Input events
// ---------------------------------------------------------------------------

/// Key state for keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    /// Key was pressed.
    Pressed,
    /// Key was released.
    Released,
}

/// Keyboard event from the Wayland compositor.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// Timestamp in milliseconds.
    pub time: u32,
    /// Key code (Linux key code).
    pub key_code: u32,
    /// Key state (pressed/released).
    pub state: KeyState,
    /// Modifiers state.
    pub modifiers: ModifiersState,
}

/// Pointer button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButtonState {
    /// Button was pressed.
    Pressed,
    /// Button was released.
    Released,
}

/// Pointer button code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
    /// Scroll up.
    ScrollUp,
    /// Scroll down.
    ScrollDown,
    /// Scroll left.
    ScrollLeft,
    /// Scroll right.
    ScrollRight,
    /// Other button.
    Other(u32),
}

/// Pointer event from the Wayland compositor.
#[derive(Debug, Clone)]
pub struct PointerEvent {
    /// Timestamp in milliseconds.
    pub time: u32,
    /// Event type.
    pub event_type: PointerEventType,
    /// Position in surface coordinates.
    pub position: Point,
    /// Delta for motion events.
    pub delta: Point,
}

/// Type of pointer event.
#[derive(Debug, Clone, PartialEq)]
pub enum PointerEventType {
    /// Pointer entered the surface.
    Enter,
    /// Pointer left the surface.
    Leave,
    /// Pointer motion.
    Motion,
    /// Button press/release.
    Button { button: PointerButton, state: PointerButtonState },
    /// Axis scroll.
    Axis { axis: AxisType, value: f64 },
}

/// Axis type for scroll events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisType {
    /// Vertical scroll.
    Vertical,
    /// Horizontal scroll.
    Horizontal,
}

/// Modifier keys state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ModifiersState {
    /// Control key.
    pub ctrl: bool,
    /// Alt key.
    pub alt: bool,
    /// Shift key.
    pub shift: bool,
    /// Super/Meta key.
    pub super_key: bool,
}

impl ModifiersState {
    /// Check if any modifier is active.
    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.super_key
    }

    /// Check if Ctrl is held.
    pub fn ctrl_held(&self) -> bool {
        self.ctrl
    }

    /// Check if Alt is held.
    pub fn alt_held(&self) -> bool {
        self.alt
    }

    /// Check if Shift is held.
    pub fn shift_held(&self) -> bool {
        self.shift
    }

    /// Check if Super is held.
    pub fn super_held(&self) -> bool {
        self.super_key
    }
}

/// Touch point identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TouchId(i32);

impl TouchId {
    /// Create a new touch ID.
    pub fn new(id: i32) -> Self {
        Self(id)
    }
}

/// Touch event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchEventType {
    /// Touch point down.
    Down,
    /// Touch point up.
    Up,
    /// Touch point motion.
    Motion,
    /// Touch frame ended.
    Frame,
    /// Touch cancelled.
    Cancel,
}

/// Touch event from the Wayland compositor.
#[derive(Debug, Clone)]
pub struct TouchEvent {
    /// Timestamp in milliseconds.
    pub time: u32,
    /// Touch point ID.
    pub touch_id: TouchId,
    /// Event type.
    pub event_type: TouchEventType,
    /// Position in surface coordinates.
    pub position: Point,
}

/// Unified input event from the Wayland compositor.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Keyboard event.
    Key(KeyEvent),
    /// Pointer event.
    Pointer(PointerEvent),
    /// Touch event.
    Touch(TouchEvent),
    /// Window close request from the compositor.
    CloseRequest,
    /// Window configure event (size/position change).
    Configure { size: Size, scale_factor: f32 },
    /// Window focus change.
    FocusChange(bool),
}

// ---------------------------------------------------------------------------
// Wayland connection
// ---------------------------------------------------------------------------

/// Represents a connection to the Wayland display server.
pub struct WaylandConnection {
    /// Next window ID counter.
    next_window_id: AtomicU64,
    /// Next popup ID counter.
    next_popup_id: AtomicU64,
    /// Registered windows.
    windows: HashMap<WaylandWindowId, WaylandWindow>,
    /// Registered popups.
    popups: HashMap<WaylandPopupId, WaylandPopup>,
    /// Active modifiers state.
    modifiers: ModifiersState,
}

impl WaylandConnection {
    /// Create a new Wayland connection.
    pub fn new() -> Self {
        Self {
            next_window_id: AtomicU64::new(1),
            next_popup_id: AtomicU64::new(1),
            windows: HashMap::new(),
            popups: HashMap::new(),
            modifiers: ModifiersState::default(),
        }
    }

    /// Create a new window.
    pub fn create_window(&mut self, config: WaylandWindowConfig) -> WaylandWindowId {
        let id = WaylandWindowId(self.next_window_id.fetch_add(1, Ordering::SeqCst));
        let window = WaylandWindow::new(id, config);
        self.windows.insert(id, window);
        id
    }

    /// Get a window by ID.
    pub fn get_window(&self, id: WaylandWindowId) -> Option<&WaylandWindow> {
        self.windows.get(&id)
    }

    /// Get a mutable reference to a window.
    pub fn get_window_mut(&mut self, id: WaylandWindowId) -> Option<&mut WaylandWindow> {
        self.windows.get_mut(&id)
    }

    /// Remove and destroy a window.
    pub fn destroy_window(&mut self, id: WaylandWindowId) {
        self.windows.remove(&id);
        // Also remove any popups associated with this window
        self.popups.retain(|_, popup| popup.parent != id);
    }

    /// Create a new popup surface.
    pub fn create_popup(
        &mut self,
        parent: WaylandWindowId,
        config: WaylandPopupConfig,
    ) -> Option<WaylandPopupId> {
        // Verify parent exists
        if !self.windows.contains_key(&parent) {
            return None;
        }

        let id = WaylandPopupId(self.next_popup_id.fetch_add(1, Ordering::SeqCst));
        let popup = WaylandPopup::new(id, parent, config);
        self.popups.insert(id, popup);
        Some(id)
    }

    /// Get a popup by ID.
    pub fn get_popup(&self, id: WaylandPopupId) -> Option<&WaylandPopup> {
        self.popups.get(&id)
    }

    /// Get a mutable reference to a popup.
    pub fn get_popup_mut(&mut self, id: WaylandPopupId) -> Option<&mut WaylandPopup> {
        self.popups.get_mut(&id)
    }

    /// Dismiss a popup.
    pub fn dismiss_popup(&mut self, id: WaylandPopupId) {
        if let Some(popup) = self.popups.get_mut(&id) {
            popup.dismiss();
        }
    }

    /// Dismiss all popups for a specific window.
    pub fn dismiss_popups_for_window(&mut self, window_id: WaylandWindowId) {
        self.popups.retain(|_, popup| {
            if popup.parent == window_id {
                popup.dismiss();
                false
            } else {
                true
            }
        });
    }

    /// Process an input event and route it to the appropriate window.
    pub fn process_input_event(&mut self, event: &InputEvent) {
        match event {
            InputEvent::Configure { size, scale_factor } => {
                // Resize event — find the focused window
                for window in self.windows.values_mut() {
                    if window.focused {
                        window.on_resize(*size, *scale_factor);
                    }
                }
            }
            InputEvent::FocusChange(focused) => {
                // Update focus state for all windows
                for window in self.windows.values_mut() {
                    window.on_focus_changed(*focused && window.id == window.id);
                }
            }
            InputEvent::CloseRequest => {
                // Close the focused window
                for window in self.windows.values_mut() {
                    if window.focused {
                        window.close();
                    }
                }
            }
            InputEvent::Key(key_event) => {
                self.modifiers = key_event.modifiers;
            }
            _ => {}
        }
    }

    /// Get the current modifiers state.
    pub fn modifiers(&self) -> ModifiersState {
        self.modifiers
    }

    /// Get all windows.
    pub fn windows(&self) -> &HashMap<WaylandWindowId, WaylandWindow> {
        &self.windows
    }

    /// Get all popups.
    pub fn popups(&self) -> &HashMap<WaylandPopupId, WaylandPopup> {
        &self.popups
    }

    /// Get the number of active windows.
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get the number of active popups.
    pub fn popup_count(&self) -> usize {
        self.popups.values().filter(|p| p.is_active()).count()
    }
}

impl Default for WaylandConnection {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Visual popup (in-window overlay)
// ---------------------------------------------------------------------------

/// A visual popup is drawn inside an existing window, not as a separate
/// Wayland surface. Used for tooltips, dropdowns, and overlays that stay
/// within the parent window's bounds.
#[derive(Debug, Clone)]
pub struct VisualPopup {
    /// Popup position relative to the parent window.
    pub position: Point,
    /// Popup size.
    pub size: Size,
    /// Background color.
    pub background: Color,
    /// Border color.
    pub border_color: Color,
    /// Border width.
    pub border_width: f32,
    /// Corner radius.
    pub corner_radius: f32,
    /// Whether the popup is visible.
    pub visible: bool,
    /// Optional shadow.
    pub shadow: Option<PopupShadow>,
}

impl VisualPopup {
    /// Create a new visual popup.
    pub fn new(position: Point, size: Size) -> Self {
        Self {
            position,
            size,
            background: Color::rgb(0.1, 0.1, 0.12),
            border_color: Color::rgb(0.3, 0.3, 0.35),
            border_width: 1.0,
            corner_radius: 4.0,
            visible: true,
            shadow: None,
        }
    }

    /// Set the background color.
    pub fn with_background(mut self, background: Color) -> Self {
        self.background = background;
        self
    }

    /// Set the border color.
    pub fn with_border_color(mut self, border_color: Color) -> Self {
        self.border_color = border_color;
        self
    }

    /// Set the corner radius.
    pub fn with_corner_radius(mut self, corner_radius: f32) -> Self {
        self.corner_radius = corner_radius;
        self
    }

    /// Set the shadow.
    pub fn with_shadow(mut self, shadow: PopupShadow) -> Self {
        self.shadow = Some(shadow);
        self
    }

    /// Hide the popup.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Show the popup.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Get the bounding rectangle.
    pub fn bounding_rect(&self) -> Rect {
        Rect::from_origin_size(&self.position, &self.size)
    }
}

/// Shadow configuration for visual popups.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PopupShadow {
    /// Horizontal offset.
    pub offset_x: f32,
    /// Vertical offset.
    pub offset_y: f32,
    /// Blur radius.
    pub blur: f32,
    /// Shadow color.
    pub color: Color,
}

impl Default for PopupShadow {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 2.0,
            blur: 8.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.3),
        }
    }
}

// ---------------------------------------------------------------------------
// Output information
// ---------------------------------------------------------------------------

/// Information about a Wayland output (monitor).
#[derive(Debug, Clone)]
pub struct OutputInfo {
    /// Output name (e.g., "HDMI-A-1").
    pub name: String,
    /// Description.
    pub description: String,
    /// Physical size in millimeters.
    pub physical_size: Size,
    /// Logical position on the virtual desktop.
    pub position: Point,
    /// Logical size.
    pub size: Size,
    /// Scale factor.
    pub scale_factor: f32,
    /// Transform (rotation/flipping).
    pub transform: OutputTransform,
    /// Whether the output is connected.
    pub connected: bool,
    /// Whether the output is the primary.
    pub primary: bool,
}

/// Output transform (rotation/flipping).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    /// No transformation.
    Normal,
    /// 90 degrees clockwise.
    Rotate90,
    /// 180 degrees.
    Rotate180,
    /// 270 degrees clockwise.
    Rotate270,
    /// Flipped horizontally.
    Flipped,
    /// Flipped and rotated 90 degrees.
    FlippedRotate90,
    /// Flipped and rotated 180 degrees.
    FlippedRotate180,
    /// Flipped and rotated 270 degrees.
    FlippedRotate270,
}

impl Default for OutputTransform {
    fn default() -> Self {
        Self::Normal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_id() {
        let id = WaylandWindowId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_popup_id() {
        let id = WaylandPopupId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_window_config_builder() {
        let config = WaylandWindowConfig::new("Test")
            .with_size(Size::new(400.0, 300.0))
            .with_window_type(WindowType::Dialog)
            .with_resizable(false)
            .with_decorated(false)
            .with_background(Color::BLACK)
            .with_visible(false)
            .with_position(Point::new(100.0, 100.0));

        assert_eq!(config.title, "Test");
        assert!((config.size.width - 400.0).abs() < f32::EPSILON);
        assert_eq!(config.window_type, WindowType::Dialog);
        assert!(!config.resizable);
        assert!(!config.decorated);
        assert!(config.background.is_some());
        assert!(!config.visible);
        assert!(config.position.is_some());
    }

    #[test]
    fn test_window_creation() {
        let config = WaylandWindowConfig::new("Test")
            .with_size(Size::new(800.0, 600.0));
        let window = WaylandWindow::new(WaylandWindowId::new(1), config);

        assert_eq!(window.id.as_u64(), 1);
        assert!((window.size.width - 800.0).abs() < f32::EPSILON);
        assert_eq!(window.state, WindowState::Creating);
        assert!(!window.focused);
    }

    #[test]
    fn test_window_lifecycle() {
        let config = WaylandWindowConfig::new("Test");
        let mut window = WaylandWindow::new(WaylandWindowId::new(1), config);

        assert_eq!(window.state, WindowState::Creating);
        assert!(window.is_visible());

        window.map();
        assert_eq!(window.state, WindowState::Mapped);

        window.on_minimize();
        assert_eq!(window.state, WindowState::Minimized);

        window.on_maximize();
        assert_eq!(window.state, WindowState::Maximized);

        window.on_fullscreen();
        assert_eq!(window.state, WindowState::Fullscreen);

        window.close();
        assert_eq!(window.state, WindowState::Closing);
        assert!(!window.is_visible());
    }

    #[test]
    fn test_window_resize() {
        let config = WaylandWindowConfig::new("Test")
            .with_size(Size::new(800.0, 600.0));
        let mut window = WaylandWindow::new(WaylandWindowId::new(1), config);

        window.on_resize(Size::new(1024.0, 768.0), 2.0);
        assert!((window.size.width - 1024.0).abs() < f32::EPSILON);
        assert!((window.scale_factor - 2.0).abs() < f32::EPSILON);
        assert!((window.viewport.physical_size().width - 2048.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_popup_creation() {
        let config = WaylandPopupConfig::new(Size::new(200.0, 150.0))
            .with_reactive(false);
        let popup = WaylandPopup::new(
            WaylandPopupId::new(1),
            WaylandWindowId::new(1),
            config,
        );

        assert_eq!(popup.parent.as_u64(), 1);
        assert!(popup.visible);
        assert!(popup.is_active());
    }

    #[test]
    fn test_popup_dismiss() {
        let config = WaylandPopupConfig::new(Size::new(200.0, 150.0));
        let mut popup = WaylandPopup::new(
            WaylandPopupId::new(1),
            WaylandWindowId::new(1),
            config,
        );

        assert!(popup.is_active());
        popup.dismiss();
        assert!(!popup.is_active());
        assert!(popup.dismissed);
    }

    #[test]
    fn test_popup_hide_show() {
        let config = WaylandPopupConfig::new(Size::new(200.0, 150.0));
        let mut popup = WaylandPopup::new(
            WaylandPopupId::new(1),
            WaylandWindowId::new(1),
            config,
        );

        popup.hide();
        assert!(!popup.visible);
        assert!(!popup.dismissed);

        popup.show();
        assert!(popup.visible);
        assert!(popup.is_active());
    }

    #[test]
    fn test_visual_popup() {
        let popup = VisualPopup::new(Point::new(10.0, 20.0), Size::new(100.0, 50.0))
            .with_background(Color::WHITE)
            .with_corner_radius(8.0);

        assert!((popup.position.x - 10.0).abs() < f32::EPSILON);
        assert!((popup.corner_radius - 8.0).abs() < f32::EPSILON);
        assert!(popup.visible);

        let rect = popup.bounding_rect();
        assert!((rect.x - 10.0).abs() < f32::EPSILON);
        assert!((rect.width - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_visual_popup_hide_show() {
        let mut popup = VisualPopup::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0));
        assert!(popup.visible);

        popup.hide();
        assert!(!popup.visible);

        popup.show();
        assert!(popup.visible);
    }

    #[test]
    fn test_popup_gravity() {
        let anchor = PopupAnchor::new(
            Rect::new(100.0, 200.0, 10.0, 10.0),
            PopupGravity::BottomRight,
            Point::new(5.0, 5.0),
        );

        assert_eq!(anchor.gravity, PopupGravity::BottomRight);
        assert!((anchor.offset.x - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_modifiers_state() {
        let mut mods = ModifiersState::default();
        assert!(mods.is_empty());

        mods.ctrl = true;
        assert!(mods.ctrl_held());
        assert!(!mods.is_empty());

        mods.shift = true;
        assert!(mods.shift_held());
    }

    #[test]
    fn test_input_event_configure() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));
        conn.get_window_mut(window_id).unwrap().focused = true;

        let event = InputEvent::Configure {
            size: Size::new(1024.0, 768.0),
            scale_factor: 2.0,
        };
        conn.process_input_event(&event);

        let window = conn.get_window(window_id).unwrap();
        assert!((window.size.width - 1024.0).abs() < f32::EPSILON);
        assert!((window.scale_factor - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_input_event_close_request() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));
        conn.get_window_mut(window_id).unwrap().focused = true;

        conn.process_input_event(&InputEvent::CloseRequest);

        let window = conn.get_window(window_id).unwrap();
        assert_eq!(window.state, WindowState::Closing);
    }

    #[test]
    fn test_wayland_connection_create_window() {
        let mut conn = WaylandConnection::new();
        let id1 = conn.create_window(WaylandWindowConfig::new("Window 1"));
        let id2 = conn.create_window(WaylandWindowConfig::new("Window 2"));

        assert_ne!(id1, id2);
        assert_eq!(conn.window_count(), 2);
        assert!(conn.get_window(id1).is_some());
    }

    #[test]
    fn test_wayland_connection_destroy_window() {
        let mut conn = WaylandConnection::new();
        let id = conn.create_window(WaylandWindowConfig::new("Test"));

        conn.destroy_window(id);
        assert_eq!(conn.window_count(), 0);
        assert!(conn.get_window(id).is_none());
    }

    #[test]
    fn test_wayland_connection_create_popup() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));

        let popup_id = conn.create_popup(window_id, WaylandPopupConfig::default());
        assert!(popup_id.is_some());
        assert_eq!(conn.popup_count(), 1);
    }

    #[test]
    fn test_wayland_connection_popup_without_parent() {
        let mut conn = WaylandConnection::new();
        let popup_id = conn.create_popup(WaylandWindowId::new(999), WaylandPopupConfig::default());
        assert!(popup_id.is_none());
    }

    #[test]
    fn test_wayland_connection_dismiss_popup() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));
        let popup_id = conn.create_popup(window_id, WaylandPopupConfig::default()).unwrap();

        conn.dismiss_popup(popup_id);
        assert_eq!(conn.popup_count(), 0);
    }

    #[test]
    fn test_wayland_connection_dismiss_popups_for_window() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));
        conn.create_popup(window_id, WaylandPopupConfig::default());
        conn.create_popup(window_id, WaylandPopupConfig::default());

        assert_eq!(conn.popup_count(), 2);
        conn.dismiss_popups_for_window(window_id);
        assert_eq!(conn.popup_count(), 0);
    }

    #[test]
    fn test_destroy_window_cleans_popups() {
        let mut conn = WaylandConnection::new();
        let window_id = conn.create_window(WaylandWindowConfig::new("Test"));
        conn.create_popup(window_id, WaylandPopupConfig::default());

        assert_eq!(conn.popup_count(), 1);
        conn.destroy_window(window_id);
        assert_eq!(conn.popup_count(), 0);
    }

    #[test]
    fn test_output_info() {
        let output = OutputInfo {
            name: "HDMI-A-1".to_string(),
            description: "Monitor".to_string(),
            physical_size: Size::new(527.0, 296.0),
            position: Point::new(0.0, 0.0),
            size: Size::new(1920.0, 1080.0),
            scale_factor: 1.0,
            transform: OutputTransform::Normal,
            connected: true,
            primary: true,
        };

        assert!(output.connected);
        assert!(output.primary);
        assert_eq!(output.transform, OutputTransform::Normal);
    }

    #[test]
    fn test_pointer_event() {
        let event = PointerEvent {
            time: 1000,
            event_type: PointerEventType::Button {
                button: PointerButton::Left,
                state: PointerButtonState::Pressed,
            },
            position: Point::new(100.0, 200.0),
            delta: Point::new(0.0, 0.0),
        };

        assert_eq!(event.time, 1000);
        assert!((event.position.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_key_event() {
        let event = KeyEvent {
            time: 1000,
            key_code: 65,
            state: KeyState::Pressed,
            modifiers: ModifiersState::default(),
        };

        assert_eq!(event.key_code, 65);
        assert_eq!(event.state, KeyState::Pressed);
    }

    #[test]
    fn test_touch_event() {
        let event = TouchEvent {
            time: 1000,
            touch_id: TouchId::new(1),
            event_type: TouchEventType::Down,
            position: Point::new(100.0, 200.0),
        };

        assert_eq!(event.event_type, TouchEventType::Down);
    }

    #[test]
    fn test_popup_shadow_default() {
        let shadow = PopupShadow::default();
        assert!((shadow.offset_y - 2.0).abs() < f32::EPSILON);
        assert!((shadow.blur - 8.0).abs() < f32::EPSILON);
        assert!((shadow.color.a - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_window_unmap() {
        let config = WaylandWindowConfig::new("Test");
        let mut window = WaylandWindow::new(WaylandWindowId::new(1), config);
        window.map();

        assert!(window.is_visible());
        window.unmap();
        assert!(!window.visible);
    }

    #[test]
    fn test_window_focus_change() {
        let config = WaylandWindowConfig::new("Test");
        let mut window = WaylandWindow::new(WaylandWindowId::new(1), config);

        assert!(!window.focused);
        window.on_focus_changed(true);
        assert!(window.focused);

        window.on_focus_changed(false);
        assert!(!window.focused);
    }

    #[test]
    fn test_visual_popup_with_shadow() {
        let shadow = PopupShadow {
            offset_x: 0.0,
            offset_y: 4.0,
            blur: 12.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.5),
        };

        let popup = VisualPopup::new(Point::new(0.0, 0.0), Size::new(100.0, 50.0))
            .with_shadow(shadow);

        assert!(popup.shadow.is_some());
        let shadow = popup.shadow.unwrap();
        assert!((shadow.blur - 12.0).abs() < f32::EPSILON);
    }
}
