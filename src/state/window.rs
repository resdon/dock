//! Window state tracking.
//!
//! Tracks per-window state: title, app_id, focused, minimized.

use crate::geometry::{Point, Rect};
use crate::models::MediaWindowLink;

/// Tracks the last known state transition for a window.
///
/// Used to detect when a window has been focused and then lost focus,
/// so the dock can update its visual indicators correctly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LastState {
    #[default]
    /// No state transition has been recorded yet
    None,
    /// The window received focus (activated=true from Wayland)
    ReceivedFocus,
    /// The window was reported as inactive by the compositor
    ReportedInactive,
}
use crate::models::system::{
    AppId, OutputId, ProcessId, ResolutionConfidence, ResolutionTrace, WindowCapabilities,
    WindowGeometry, WindowId, WindowLayer, WindowPriority, WindowStatus,
};

/// Classifies the type of a Wayland window.
///
/// Used to determine how windows appear in the dock and popup lists.
/// Some types (tooltips, notifications) may be excluded from the dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[derive(Default)]
pub enum WindowType {
    /// A main application window
    Main,
    /// A secondary window of an application
    SubWindow,
    /// A dialog window (modal or modeless)
    Dialog,
    /// A popup window (dropdown, context menu, etc.)
    Popup,
    /// A menu window
    Menu,
    /// A tooltip window
    Tooltip,
    /// A system tray window
    Tray,
    /// A notification window
    Notification,
    /// A utility window (color picker, font picker, etc.)
    Utility,
    /// Window type could not be determined
    #[default]
    Unknown,
}


/// Tracks the spatial areas occupied by a window.
///
/// Used for hit testing and visual positioning of dock elements.
#[derive(Clone, Debug, Default)]
pub struct WindowAreas {
    /// Outer bounds including decorations
    pub outer: Option<Rect>,
    /// Content area excluding decorations
    pub content: Option<Rect>,
    /// Main window area
    pub main: Option<Rect>,
    /// Sub-window areas (child windows)
    pub sub: Vec<Rect>,
    /// Input focus area
    pub input: Option<Rect>,
}

/// Tracks input-related state for a window.
///
/// Used to determine if a window can receive pointer events and drag-and-drop.
#[derive(Clone, Debug, Default)]
pub struct WindowInputState {
    /// Whether the window accepts pointer input
    pub accepts_input: bool,
    /// Whether the pointer is currently inside the window
    pub pointer_inside: bool,
    /// Current pointer position relative to the window
    pub pointer_position: Option<Point>,
    /// Region where input is accepted
    pub input_region: Option<Rect>,
    /// Whether the window has captured the pointer
    pub capture_pointer: bool,
    /// Whether the window is a drag-and-drop target
    pub drop_target: bool,
}

/// Complete state for a single Wayland window.
///
/// Tracks all properties needed for dock display, focus management,
/// PipeWire stream matching, and animation triggers. Updated by
/// Wayland StateChanged events and the PipeWire capture service.
#[derive(Clone, Debug)]
pub struct WindowState {
    /// Stable Wayland window ID (from ObjectId::protocol_id())
    pub id: WindowId,
    /// Application ID (e.g., "org.wezfurlong.wezterm")
    pub app_id: Option<AppId>,
    /// Process ID from Wayland (if available)
    pub process_id: Option<ProcessId>,
    /// Window title from the compositor
    pub title: String,
    /// Human-readable application name
    pub app_name: String,
    /// Icon name for XDG icon lookup
    pub icon_name: String,
    /// Terminal icon code (for terminal emulators)
    pub terminal_icon_code: String,
    /// Pre-loaded icon RGBA data
    pub icon_rgba: Option<Vec<u8>>,
    /// Icon size in pixels
    pub icon_size: u32,
    /// Whether the icon has been resolved and loaded
    pub icon_resolved: bool,
    /// Whether the window is pending (not yet fully mapped)
    pub is_pending: bool,
    /// Last known state transition
    pub last_state: LastState,
    /// Window geometry (position and size)
    pub geometry: WindowGeometry,
    /// Window capabilities (resizable, maximizable, etc.)
    pub capabilities: WindowCapabilities,
    /// Primary output ID
    pub output_id: Option<OutputId>,
    /// All output IDs the window spans
    pub output_ids: Vec<OutputId>,
    /// Type classification of this window
    pub window_type: WindowType,
    /// Window status (foreground, background, etc.)
    pub status: WindowStatus,
    /// Window priority for ordering
    pub priority: WindowPriority,
    /// Window layer (normal, overlay, etc.)
    pub layer: WindowLayer,
    /// Whether the window is visible (not minimized)
    pub visible: bool,
    /// Whether the window is mapped to the screen
    pub mapped: bool,
    /// Whether the window has focus (set by StateChanged activated=true)
    pub focused: bool,
    /// Whether the window is in the foreground
    pub foreground: bool,
    /// Whether the window is in the background
    pub background: bool,
    /// Whether the window is minimized (set by StateChanged minimized=true)
    pub minimized: bool,
    /// Whether the window is maximized
    pub maximized: bool,
    /// Whether the window is fullscreen
    pub fullscreen: bool,
    /// Whether the window has urgent state (demands attention)
    pub urgent: bool,
    /// Whether the window is locked (cannot be closed)
    pub locked: bool,
    /// Whether the window is paused
    pub paused: bool,
    /// Whether the window is stuck (state not updating)
    pub stuck: bool,
    /// Whether the window is responding to events
    pub responding: bool,
    /// Order index for stable sorting
    pub order_index: u64,
    /// Spatial areas of the window
    pub areas: WindowAreas,
    /// Input-related state
    pub input: WindowInputState,
    /// Link to media player (if this window plays media)
    pub media: Option<MediaWindowLink>,
    /// Confidence in how the window was resolved
    pub resolution_confidence: ResolutionConfidence,
    /// Trace of how the window was resolved
    pub resolution_trace: ResolutionTrace,
}

impl WindowState {
    /// Creates a new window state with default values.
    ///
    /// Called when a new window appears via Wayland events.
    pub fn new(id: WindowId) -> Self {
        Self {
            id,
            app_id: None,
            process_id: None,
            title: String::new(),
            app_name: "Unknown".to_string(),
            icon_name: String::new(),
            terminal_icon_code: String::new(),
            icon_rgba: None,
            icon_size: 48,
            icon_resolved: false,
            is_pending: false,
            last_state: LastState::None,
            geometry: WindowGeometry::default(),
            capabilities: WindowCapabilities::default(),
            output_id: None,
            output_ids: Vec::new(),
            window_type: WindowType::Unknown,
            status: WindowStatus::Background,
            priority: WindowPriority::Normal,
            layer: WindowLayer::Normal,
            visible: true,
            mapped: true,
            focused: false,
            foreground: false,
            background: true,
            minimized: false,
            maximized: false,
            fullscreen: false,
            urgent: false,
            locked: false,
            paused: false,
            stuck: false,
            responding: true,
            order_index: 0,
            areas: WindowAreas::default(),
            input: WindowInputState::default(),
            media: None,
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
        }
    }

    /// Returns true if the window can receive user input.
    ///
    /// Used to filter out non-interactive windows from the dock.
    pub fn is_interactive(&self) -> bool {
        self.visible && self.mapped && !self.locked && self.input.accepts_input
    }
}
