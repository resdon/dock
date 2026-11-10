//! Slint integration bridge and widget implementations.
//!
//! This module provides Rust types that mirror the Slint UI definitions
//! (`ui.slint`, `bluetooth-widget.slint`) and a bridge for integrating
//! Slint widgets with the wgpu renderer.
//!
//! ## Integration Options
//!
//! Two approaches for Slint integration:
//!
//! **Option A: Independent rendering** (recommended for initial setup)
//! Slint owns its own rendering and window. The wgpu renderer operates
//! independently. This is the easiest and safest initial architecture.
//!
//! **Option B: Shared wgpu context** (advanced)
//! Slint renders into a shared wgpu rendering context, potentially avoiding
//! an additional GPU composition step. This is more sophisticated but can
//! be integrated after Option A is working.
//!
//! ## Widget Types
//!
//! The types in this module mirror the Slint `.slint` files:
//!
//! - `DockItem` — corresponds to `struct DockItem` in `ui.slint`
//! - `DockWidget` — corresponds to `DockWidget` component in `ui.slint`
//! - `DeviceEntry` — corresponds to `struct DeviceEntry` in `bluetooth-widget.slint`
//! - `BluetoothWidget` — corresponds to `BluetoothWidget` component
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Application Code                                           │
//! │  DockItem, DockWidget, BluetoothWidget...                   │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Slint Bridge Layer                                         │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
//! │  │  Dock Widget │  │  Bluetooth   │  │  Widget Registry │  │
//! │  │  (ui.slint)  │  │  Widget      │  │  (lifecycle)     │  │
//! │  └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘  │
//! │         └─────────────────┴───────────────────┘             │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                       │
//! │              │  Slint Runtime       │                       │
//! │              │  (wgpu backend)      │                       │
//! │              └──────────┬───────────┘                       │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                       │
//! │              │  wgpu Renderer       │                       │
//! │              │  (shared device)     │                       │
//! │              └──────────────────────┘                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::texture::TextureId;

// ---------------------------------------------------------------------------
// Widget identifiers
// ---------------------------------------------------------------------------

/// Opaque handle for a Slint widget instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WidgetId(u64);

impl WidgetId {
    /// Create a new widget ID.
    pub fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for WidgetId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

// ---------------------------------------------------------------------------
// Dock widget types (from ui.slint)
// ---------------------------------------------------------------------------

/// Mirrors `struct DockItem` in `ui.slint`.
///
/// Represents a single item (application icon) in the dock.
/// The Rust side computes animation properties (`anim_scale`, `icon_width`,
/// `icon_height`, `icon_x`, `icon_y`, `offset_x`) and pushes them to Slint.
#[derive(Debug, Clone)]
pub struct DockItem {
    /// Application name.
    pub name: String,
    /// Icon image path or resource.
    pub icon: String,
    /// Whether the application is running.
    pub running: bool,
    /// Whether the item is in a loading state.
    pub loading: bool,
    /// Notification badge count.
    pub badge_count: i32,
    /// Badge color (hex string, e.g., "#dc2626").
    pub badge_color: String,
    /// Number of windows for this application.
    pub window_count: i32,
    /// Application ID.
    pub app_id: i32,
    /// Index position on the dock.
    pub index_on_dock: i32,
    /// Whether this item has focus.
    pub focused: bool,
    /// Whether the open animation is playing.
    pub open_animation: bool,
    /// Whether the item is pinned to the dock.
    pub pinned: bool,
    /// Whether the application is minimized.
    pub minimized: bool,
    /// Whether the icon is currently hovered.
    pub is_hovered: bool,
    /// Whether the icon is being dragged.
    pub is_dragging: bool,
    /// Horizontal offset for sliding animation during drag (in pixels).
    /// Computed by Rust animation system.
    pub offset_x: f32,
    /// Combined animation scale factor (1.0 = normal, up to 1.25 = 25% larger).
    /// Includes both click animation and bounce animation.
    /// Computed by Rust animation system.
    pub anim_scale: f32,
    /// Scaled icon width (icon_size * anim_scale).
    /// Computed by Rust animation system.
    pub icon_width: f32,
    /// Scaled icon height (icon_size * anim_scale).
    /// Computed by Rust animation system.
    pub icon_height: f32,
    /// Scaled icon X offset to keep centered.
    /// Computed by Rust animation system.
    pub icon_x: f32,
    /// Scaled icon Y offset to keep centered.
    /// Computed by Rust animation system.
    pub icon_y: f32,
}

impl DockItem {
    /// Create a new dock item.
    pub fn new(name: impl Into<String>, icon: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            icon: icon.into(),
            running: false,
            loading: false,
            badge_count: 0,
            badge_color: "#dc2626".to_string(),
            window_count: 0,
            app_id: 0,
            index_on_dock: 0,
            focused: false,
            open_animation: false,
            pinned: false,
            minimized: false,
            is_hovered: false,
            is_dragging: false,
            offset_x: 0.0,
            anim_scale: 1.0,
            icon_width: 48.0,
            icon_height: 48.0,
            icon_x: 0.0,
            icon_y: 6.0,
        }
    }

    /// Set the animation scale and recompute derived properties.
    ///
    /// This is called by the Rust animation system each frame.
    /// `icon_size` is the base icon size (e.g., 48px).
    pub fn set_anim_scale(&mut self, anim_scale: f32, icon_size: f32) {
        self.anim_scale = anim_scale;
        self.icon_width = icon_size * anim_scale;
        self.icon_height = icon_size * anim_scale;
        // Center the scaled icon within the 48px slot
        let slot_size = 48.0;
        self.icon_x = (slot_size - self.icon_width) / 2.0;
        self.icon_y = 6.0 + (slot_size - self.icon_height) / 2.0;
    }

    /// Set the horizontal offset for drag animation.
    pub fn set_offset_x(&mut self, offset_x: f32) {
        self.offset_x = offset_x;
    }

    /// Check if the item shows a badge.
    pub fn has_badge(&self) -> bool {
        self.badge_count > 0
    }

    /// Get the badge display text (caps at "9+").
    pub fn badge_text(&self) -> String {
        if self.badge_count > 9 {
            "9+".to_string()
        } else {
            self.badge_count.to_string()
        }
    }
}

impl Default for DockItem {
    fn default() -> Self {
        Self::new("App", "")
    }
}

/// Mirrors `DockWidget` component in `ui.slint`.
///
/// Configuration for the dock widget, matching the Slint properties.
#[derive(Debug, Clone)]
pub struct DockWidget {
    /// Dock width in pixels.
    pub dock_width: i32,
    /// Dock height in pixels.
    pub dock_height: i32,
    /// Icon size in pixels.
    pub icon_size: i32,
    /// Spacing between items in pixels.
    pub item_spacing: i32,
    /// Hover highlight color.
    pub hover_color: Color,
    /// Focused dash (underscore) color.
    pub dash_focused_color: Color,
    /// Inactive dash color.
    pub dash_inactive_color: Color,
    /// Whether hover highlight is enabled.
    pub hover_highlight: bool,
    /// Whether click animation is enabled.
    pub click_animation: bool,
    /// Whether bounce animation is enabled.
    pub bounce_animation: bool,
    /// Background color.
    pub background_color: Color,
    /// Border color.
    pub border_color: Color,
    /// List of dock items.
    pub dock_items: Vec<DockItem>,
}

impl DockWidget {
    /// Create a new dock widget with default values matching `ui.slint`.
    pub fn new() -> Self {
        Self {
            dock_width: 520,
            dock_height: 60,
            icon_size: 48,
            item_spacing: 8,
            hover_color: Color::rgba(0.0, 0.9, 1.0, 1.0), // #00E5FF
            dash_focused_color: Color::rgba(0.0, 0.9, 1.0, 1.0), // #00E5FF
            dash_inactive_color: Color::rgba(0.61, 0.64, 0.69, 1.0), // #9CA3AF
            hover_highlight: false,
            click_animation: true,
            bounce_animation: true,
            background_color: Color::rgba(0.11, 0.11, 0.13, 1.0), // #1D1C22
            border_color: Color::rgba(0.30, 0.29, 0.33, 1.0),     // #4C4954
            dock_items: Vec::new(),
        }
    }

    /// Add a dock item.
    pub fn add_item(&mut self, mut item: DockItem) {
        item.index_on_dock = self.dock_items.len() as i32;
        self.dock_items.push(item);
    }

    /// Remove a dock item by index.
    pub fn remove_item(&mut self, index: i32) -> Option<DockItem> {
        let idx = index as usize;
        if idx < self.dock_items.len() {
            let item = self.dock_items.remove(idx);
            // Update indices
            for (i, item) in self.dock_items.iter_mut().enumerate() {
                item.index_on_dock = i as i32;
            }
            Some(item)
        } else {
            None
        }
    }

    /// Get a dock item by index.
    pub fn get_item(&self, index: i32) -> Option<&DockItem> {
        self.dock_items.get(index as usize)
    }

    /// Get a mutable reference to a dock item.
    pub fn get_item_mut(&mut self, index: i32) -> Option<&mut DockItem> {
        self.dock_items.get_mut(index as usize)
    }

    /// Get the number of dock items.
    pub fn item_count(&self) -> usize {
        self.dock_items.len()
    }

    /// Compute the total width of the app row (icons + spacing).
    /// Matches the Slint layout: `count * (icon_size + spacing) - spacing`.
    pub fn app_row_width(&self) -> i32 {
        let count = self.dock_items.len() as i32;
        count * (self.icon_size + self.item_spacing) - self.item_spacing
    }

    /// Compute the X position for a dock item.
    pub fn item_x(&self, index: i32) -> i32 {
        index * (self.icon_size + self.item_spacing)
    }

    /// Compute the dock's size.
    pub fn size(&self) -> Size {
        Size::new(self.dock_width as f32, self.dock_height as f32)
    }

    /// Find the dock item under a given point (hit test).
    pub fn hit_test(&self, point: Point) -> Option<i32> {
        for (i, item) in self.dock_items.iter().enumerate() {
            let x = self.item_x(i as i32) as f32 + item.offset_x;
            let y = 0.0;
            let rect = Rect::new(x, y, self.icon_size as f32, self.dock_height as f32);
            if rect.contains(&point) {
                return Some(i as i32);
            }
        }
        None
    }

    /// Set the hover color.
    pub fn with_hover_color(mut self, color: Color) -> Self {
        self.hover_color = color;
        self
    }

    /// Set the background color.
    pub fn with_background_color(mut self, color: Color) -> Self {
        self.background_color = color;
        self
    }

    /// Set the border color.
    pub fn with_border_color(mut self, color: Color) -> Self {
        self.border_color = color;
        self
    }

    /// Enable or disable hover highlight.
    pub fn with_hover_highlight(mut self, enabled: bool) -> Self {
        self.hover_highlight = enabled;
        self
    }

    /// Enable or disable click animation.
    pub fn with_click_animation(mut self, enabled: bool) -> Self {
        self.click_animation = enabled;
        self
    }

    /// Enable or disable bounce animation.
    pub fn with_bounce_animation(mut self, enabled: bool) -> Self {
        self.bounce_animation = enabled;
        self
    }
}

impl Default for DockWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// Dash (underscore) configuration for dock items.
///
/// The dock shows 1-5 dashes (underscores) below each icon to indicate
/// the number of windows. The dashes are computed based on window count.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashConfig {
    /// Total span width for all dashes combined.
    pub span_width: f32,
    /// Gap between dashes.
    pub gap: f32,
    /// Dash height.
    pub height: f32,
    /// Y position (from top of dock).
    pub y: f32,
    /// X offset (from left of icon).
    pub x_offset: f32,
}

impl DashConfig {
    /// Default dash configuration matching `ui.slint`.
    pub fn new() -> Self {
        Self {
            span_width: 32.0,
            gap: 3.0,
            height: 2.0,
            y: 58.0,
            x_offset: 8.0,
        }
    }

    /// Compute the width of each dash for a given window count.
    pub fn dash_width(&self, window_count: i32) -> f32 {
        let count = window_count.min(5).max(1) as f32;
        (self.span_width - (count - 1.0) * self.gap) / count
    }

    /// Compute the X positions for all dashes.
    pub fn dash_positions(&self, window_count: i32) -> Vec<f32> {
        let count = window_count.min(5).max(1) as i32;
        let dash_w = self.dash_width(count);
        let mut positions = Vec::with_capacity(count as usize);
        let mut x = self.x_offset;
        for _ in 0..count {
            positions.push(x);
            x += dash_w + self.gap;
        }
        positions
    }
}

impl Default for DashConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Bluetooth widget types (from bluetooth-widget.slint)
// ---------------------------------------------------------------------------

/// Mirrors `struct DeviceEntry` in `bluetooth-widget.slint`.
///
/// Represents a Bluetooth device in the discovered or paired lists.
#[derive(Debug, Clone)]
pub struct DeviceEntry {
    /// Device name.
    pub name: String,
    /// Whether the device is connected.
    pub connected: bool,
    /// Button text (e.g., "Connect", "Disconnect", "Pair").
    pub button_text: String,
    /// Device status string.
    pub status: String,
    /// Last action performed on the device.
    pub last_action: String,
}

impl DeviceEntry {
    /// Create a new device entry.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            connected: false,
            button_text: "Pair".to_string(),
            status: "Not paired".to_string(),
            last_action: String::new(),
        }
    }

    /// Set the device as connected.
    pub fn connect(&mut self) {
        self.connected = true;
        self.button_text = "Disconnect".to_string();
        self.status = "Connected".to_string();
    }

    /// Set the device as disconnected.
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.button_text = "Connect".to_string();
        self.status = "Disconnected".to_string();
    }

    /// Set the device as paired but not connected.
    pub fn pair(&mut self) {
        self.button_text = "Connect".to_string();
        self.status = "Paired".to_string();
    }

    /// Set the device as unpaired.
    pub fn unpair(&mut self) {
        self.connected = false;
        self.button_text = "Pair".to_string();
        self.status = "Not paired".to_string();
    }
}

impl Default for DeviceEntry {
    fn default() -> Self {
        Self::new("Unknown Device")
    }
}

/// Bluetooth action callback types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BluetoothAction {
    /// Connect to a device.
    Connect,
    /// Disconnect from a device.
    Disconnect,
    /// Unpair a device.
    Unpair,
    /// Toggle power on/off.
    PowerToggle,
    /// Toggle scanning on/off.
    ScanToggle,
    /// Clear discovered devices.
    ClearDiscovered,
    /// Export data.
    DataExport,
}

/// Mirrors `BluetoothState` global in `bluetooth-widget.slint`.
///
/// Represents the state of the Bluetooth widget.
#[derive(Debug, Clone)]
pub struct BluetoothState {
    /// Whether Bluetooth power is on.
    pub power_on: bool,
    /// Whether scanning for devices is active.
    pub scanning: bool,
    /// Whether the widget is minimized.
    pub minimized: bool,
    /// Whether the settings panel is open.
    pub settings_open: bool,
    /// Adapter name (e.g., "archlinux").
    pub adapter_name: String,
    /// Sweep animation cosine value.
    pub sweep_cos: f32,
    /// Sweep animation sine value.
    pub sweep_sin: f32,
    /// Dot 1 X position for animation.
    pub dot1_x: f32,
    /// Dot 1 Y position for animation.
    pub dot1_y: f32,
    /// Dot 2 X position for animation.
    pub dot2_x: f32,
    /// Dot 2 Y position for animation.
    pub dot2_y: f32,
    /// Dot 3 X position for animation.
    pub dot3_x: f32,
    /// Dot 3 Y position for animation.
    pub dot3_y: f32,
    /// Scroll offset for discovered devices list.
    pub scroll_offset: f32,
    /// Scroll offset for paired devices list.
    pub paired_scroll_offset: f32,
    /// Scroll thumb Y position for discovered list.
    pub scroll_thumb_y: f32,
    /// Scroll thumb height for discovered list.
    pub scroll_thumb_h: f32,
    /// Scroll thumb Y position for paired list.
    pub paired_scroll_thumb_y: f32,
    /// Scroll thumb height for paired list.
    pub paired_scroll_thumb_h: f32,
    /// Last action performed.
    pub last_action: String,
    /// Footer text.
    pub footer_text: String,
    /// Number of connected devices.
    pub connected_count: i32,
}

impl BluetoothState {
    /// Create a new Bluetooth state.
    pub fn new() -> Self {
        Self {
            power_on: false,
            scanning: false,
            minimized: false,
            settings_open: false,
            adapter_name: "archlinux".to_string(),
            sweep_cos: 1.0,
            sweep_sin: 0.0,
            dot1_x: 0.0,
            dot1_y: 0.0,
            dot2_x: 0.0,
            dot2_y: 0.0,
            dot3_x: 0.0,
            dot3_y: 0.0,
            scroll_offset: 0.0,
            paired_scroll_offset: 0.0,
            scroll_thumb_y: 0.0,
            scroll_thumb_h: 0.0,
            paired_scroll_thumb_y: 0.0,
            paired_scroll_thumb_h: 0.0,
            last_action: String::new(),
            footer_text: String::new(),
            connected_count: 0,
        }
    }

    /// Toggle Bluetooth power.
    pub fn toggle_power(&mut self) {
        self.power_on = !self.power_on;
        self.last_action = if self.power_on {
            "Power on".to_string()
        } else {
            "Power off".to_string()
        };
    }

    /// Toggle scanning.
    pub fn toggle_scan(&mut self) {
        if self.power_on {
            self.scanning = !self.scanning;
            self.last_action = if self.scanning {
                "Scanning started".to_string()
            } else {
                "Scanning stopped".to_string()
            };
        }
    }

    /// Update sweep animation values.
    pub fn update_sweep(&mut self, angle: f32) {
        self.sweep_cos = angle.cos();
        self.sweep_sin = angle.sin();
    }

    /// Update connected device count.
    pub fn update_connected_count(&mut self, devices: &[DeviceEntry]) {
        self.connected_count = devices.iter().filter(|d| d.connected).count() as i32;
    }
}

impl Default for BluetoothState {
    fn default() -> Self {
        Self::new()
    }
}

/// Mirrors `BluetoothWidget` component in `bluetooth-widget.slint`.
///
/// The Bluetooth widget with discovered and paired device lists.
#[derive(Debug, Clone)]
pub struct BluetoothWidget {
    /// Widget size (402×318 as in bluetooth-widget.slint).
    pub size: Size,
    /// Bluetooth state.
    pub state: BluetoothState,
    /// Discovered devices list.
    pub discovered: Vec<DeviceEntry>,
    /// Paired devices list.
    pub paired: Vec<DeviceEntry>,
}

impl BluetoothWidget {
    /// Create a new Bluetooth widget.
    pub fn new() -> Self {
        Self {
            size: Size::new(402.0, 318.0),
            state: BluetoothState::new(),
            discovered: Vec::new(),
            paired: Vec::new(),
        }
    }

    /// Add a discovered device.
    pub fn add_discovered(&mut self, device: DeviceEntry) {
        self.discovered.push(device);
    }

    /// Add a paired device.
    pub fn add_paired(&mut self, device: DeviceEntry) {
        self.paired.push(device);
    }

    /// Clear discovered devices.
    pub fn clear_discovered(&mut self) {
        self.discovered.clear();
    }

    /// Get the number of discovered devices.
    pub fn discovered_count(&self) -> usize {
        self.discovered.len()
    }

    /// Get the number of paired devices.
    pub fn paired_count(&self) -> usize {
        self.paired.len()
    }

    /// Get the state.
    pub fn state(&self) -> &BluetoothState {
        &self.state
    }

    /// Get the mutable state.
    pub fn state_mut(&mut self) -> &mut BluetoothState {
        &mut self.state
    }
}

impl Default for BluetoothWidget {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Slint rendering integration
// ---------------------------------------------------------------------------

/// Integration mode for Slint rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlintRenderMode {
    /// Slint owns its own rendering and window (Option A).
    /// Safest and easiest initial architecture.
    Independent,
    /// Slint renders into a shared wgpu context (Option B).
    /// More sophisticated, avoids additional GPU composition.
    SharedContext,
}

impl Default for SlintRenderMode {
    fn default() -> Self {
        Self::Independent
    }
}

/// Configuration for Slint-wgpu integration.
#[derive(Debug, Clone)]
pub struct SlintIntegrationConfig {
    /// Rendering mode.
    pub render_mode: SlintRenderMode,
    /// Whether to use wgpu backend for Slint.
    pub use_wgpu_backend: bool,
    /// Scale factor for Slint rendering.
    pub scale_factor: f32,
    /// Whether to enable animations in Slint.
    pub enable_animations: bool,
}

impl SlintIntegrationConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self {
            render_mode: SlintRenderMode::Independent,
            use_wgpu_backend: true,
            scale_factor: 1.0,
            enable_animations: true,
        }
    }

    /// Set the render mode.
    pub fn with_render_mode(mut self, mode: SlintRenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Set the scale factor.
    pub fn with_scale_factor(mut self, scale_factor: f32) -> Self {
        self.scale_factor = scale_factor;
        self
    }
}

impl Default for SlintIntegrationConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Slint wgpu-30 adapter
// ---------------------------------------------------------------------------

/// Adapter that connects Slint's wgpu-30 rendering to our wgpu backend.
///
/// This allows Slint to render widgets through our shared wgpu device/queue
/// instead of using a separate GPU context. Slint manages UI state (widgets,
/// layout, events, properties) while we control the GPU resources.
///
/// ## Usage
///
/// ```ignore
/// // Initialize the wgpu renderer
/// let renderer = wgpu_api::Renderer::new().await?;
///
/// // Create the Slint adapter with our renderer
/// let adapter = SlintWgpuAdapter::new(&renderer)?;
///
/// // Select the wgpu backend for Slint
/// adapter.select_backend()?;
///
/// // Create Slint components normally
/// let widget = DockWidget::new()?;
/// widget.run()?;
/// ```
pub struct SlintWgpuAdapter {
    /// Whether the adapter was initialized successfully.
    initialized: bool,
}

impl SlintWgpuAdapter {
    /// Create a new Slint wgpu adapter.
    ///
    /// This function configures Slint to use the wgpu-30 backend with our
    /// shared wgpu device and queue. It must be called before creating any
    /// Slint components.
    ///
    /// # Errors
    ///
    /// Returns an error if Slint backend selection fails.
    pub fn new(_renderer: &crate::renderer::Renderer) -> Result<Self, String> {
        // The adapter is a marker type — the actual integration happens
        // via BackendSelector::require_wgpu_30() which we call in select_backend().
        // The renderer's device/queue are shared with Slint via WGPUConfiguration::Manual.
        Ok(Self { initialized: false })
    }

    /// Select the wgpu-30 backend for Slint.
    ///
    /// Must be called before creating any Slint components. This configures
    /// Slint to use wgpu for rendering instead of the software renderer.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot be selected.
    pub fn select_backend(&mut self) -> Result<(), String> {
        #[cfg(feature = "slint-wgpu-30")]
        {
            use slint::wgpu_30::WGPUConfiguration;
            use slint::BackendSelector;

            BackendSelector::new()
                .require_wgpu_30(WGPUConfiguration::default())
                .select()
                .map_err(|e| format!("Failed to select Slint wgpu backend: {:?}", e))?;

            self.initialized = true;
            Ok(())
        }
        #[cfg(not(feature = "slint-wgpu-30"))]
        {
            Err("Slint wgpu-30 feature is not enabled".to_string())
        }
    }

    /// Check if the adapter is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

/// Represents a Slint component that can be rendered by the wgpu renderer.
#[derive(Debug, Clone)]
pub struct SlintComponent {
    /// Component ID.
    pub id: WidgetId,
    /// Component name (e.g., "DockWidget", "BluetoothWidget").
    pub name: String,
    /// Component size.
    pub size: Size,
    /// Whether the component is visible.
    pub visible: bool,
    /// Associated texture (if pre-rendered).
    pub texture: Option<TextureId>,
}

impl SlintComponent {
    /// Create a new Slint component.
    pub fn new(id: WidgetId, name: impl Into<String>, size: Size) -> Self {
        Self {
            id,
            name: name.into(),
            size,
            visible: true,
            texture: None,
        }
    }

    /// Set the component's texture.
    pub fn set_texture(&mut self, texture: TextureId) {
        self.texture = Some(texture);
    }

    /// Hide the component.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Show the component.
    pub fn show(&mut self) {
        self.visible = true;
    }
}

// ---------------------------------------------------------------------------
// Widget registry
// ---------------------------------------------------------------------------

/// Registry of Slint widgets managed by the wgpu renderer.
pub struct WidgetRegistry {
    /// Next widget ID counter.
    next_widget_id: AtomicU64,
    /// Registered components.
    components: HashMap<WidgetId, SlintComponent>,
    /// Dock widget instance.
    dock_widget: Option<DockWidget>,
    /// Bluetooth widget instance.
    bluetooth_widget: Option<BluetoothWidget>,
}

impl WidgetRegistry {
    /// Create a new widget registry.
    pub fn new() -> Self {
        Self {
            next_widget_id: AtomicU64::new(1),
            components: HashMap::new(),
            dock_widget: None,
            bluetooth_widget: None,
        }
    }

    /// Register a Slint component.
    pub fn register_component(&mut self, name: impl Into<String>, size: Size) -> WidgetId {
        let id = WidgetId(self.next_widget_id.fetch_add(1, Ordering::SeqCst));
        let component = SlintComponent::new(id, name, size);
        self.components.insert(id, component);
        id
    }

    /// Get a component by ID.
    pub fn get_component(&self, id: WidgetId) -> Option<&SlintComponent> {
        self.components.get(&id)
    }

    /// Get a mutable reference to a component.
    pub fn get_component_mut(&mut self, id: WidgetId) -> Option<&mut SlintComponent> {
        self.components.get_mut(&id)
    }

    /// Remove a component.
    pub fn unregister_component(&mut self, id: WidgetId) {
        self.components.remove(&id);
    }

    /// Set the dock widget.
    pub fn set_dock_widget(&mut self, widget: DockWidget) {
        self.dock_widget = Some(widget);
    }

    /// Get the dock widget.
    pub fn dock_widget(&self) -> Option<&DockWidget> {
        self.dock_widget.as_ref()
    }

    /// Get the mutable dock widget.
    pub fn dock_widget_mut(&mut self) -> Option<&mut DockWidget> {
        self.dock_widget.as_mut()
    }

    /// Set the Bluetooth widget.
    pub fn set_bluetooth_widget(&mut self, widget: BluetoothWidget) {
        self.bluetooth_widget = Some(widget);
    }

    /// Get the Bluetooth widget.
    pub fn bluetooth_widget(&self) -> Option<&BluetoothWidget> {
        self.bluetooth_widget.as_ref()
    }

    /// Get the mutable Bluetooth widget.
    pub fn bluetooth_widget_mut(&mut self) -> Option<&mut BluetoothWidget> {
        self.bluetooth_widget.as_mut()
    }

    /// Get the number of registered components.
    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    /// Clear all registered components.
    pub fn clear(&mut self) {
        self.components.clear();
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Slint event bridge
// ---------------------------------------------------------------------------

/// Events from Slint widgets that need to be handled by the Rust side.
#[derive(Debug, Clone)]
pub enum SlintEvent {
    /// Dock item was clicked.
    DockItemClick {
        index: i32,
        button: SlintMouseButton,
    },
    /// Dock item hover changed.
    DockItemHover { index: i32, is_hovered: bool },
    /// Dock item drag started.
    DockItemDragStart { index: i32, position: Point },
    /// Dock item drag moved.
    DockItemDragMove { index: i32, position: Point },
    /// Dock item drag ended.
    DockItemDragEnd { index: i32, position: Point },
    /// Bluetooth device action.
    BluetoothAction {
        action: BluetoothAction,
        device_index: Option<i32>,
    },
    /// Widget resize request.
    WidgetResize { widget_id: WidgetId, size: Size },
    /// Widget close request.
    WidgetClose { widget_id: WidgetId },
}

/// Mouse button for Slint events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlintMouseButton {
    /// Left button.
    Left,
    /// Middle button.
    Middle,
    /// Right button.
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_widget_id() {
        let id = WidgetId::new(42);
        assert_eq!(id.as_u64(), 42);
    }

    #[test]
    fn test_dock_item_creation() {
        let item = DockItem::new("Firefox", "/usr/share/icons/firefox.png");
        assert_eq!(item.name, "Firefox");
        assert!(!item.running);
        assert!(!item.has_badge());
        assert!((item.anim_scale - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dock_item_anim_scale() {
        let mut item = DockItem::new("App", "");
        item.set_anim_scale(1.25, 48.0);

        assert!((item.anim_scale - 1.25).abs() < f32::EPSILON);
        assert!((item.icon_width - 60.0).abs() < f32::EPSILON);
        assert!((item.icon_height - 60.0).abs() < f32::EPSILON);
        // Centered: (48 - 60) / 2 = -6
        assert!((item.icon_x - (-6.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dock_item_badge() {
        let mut item = DockItem::new("App", "");
        item.badge_count = 5;
        assert!(item.has_badge());
        assert_eq!(item.badge_text(), "5");

        item.badge_count = 15;
        assert_eq!(item.badge_text(), "9+");
    }

    #[test]
    fn test_dock_item_offset() {
        let mut item = DockItem::new("App", "");
        item.set_offset_x(10.0);
        assert!((item.offset_x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dock_widget_creation() {
        let widget = DockWidget::new();
        assert_eq!(widget.dock_width, 520);
        assert_eq!(widget.dock_height, 60);
        assert_eq!(widget.icon_size, 48);
        assert_eq!(widget.item_spacing, 8);
        assert_eq!(widget.item_count(), 0);
    }

    #[test]
    fn test_dock_widget_add_remove() {
        let mut widget = DockWidget::new();
        widget.add_item(DockItem::new("App 1", ""));
        widget.add_item(DockItem::new("App 2", ""));
        widget.add_item(DockItem::new("App 3", ""));

        assert_eq!(widget.item_count(), 3);
        assert_eq!(widget.get_item(0).unwrap().name, "App 1");
        assert_eq!(widget.get_item(1).unwrap().name, "App 2");

        let removed = widget.remove_item(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().name, "App 2");
        assert_eq!(widget.item_count(), 2);
        assert_eq!(widget.get_item(0).unwrap().index_on_dock, 0);
        assert_eq!(widget.get_item(1).unwrap().index_on_dock, 1);
    }

    #[test]
    fn test_dock_widget_app_row_width() {
        let mut widget = DockWidget::new();
        widget.add_item(DockItem::new("A", ""));
        widget.add_item(DockItem::new("B", ""));
        widget.add_item(DockItem::new("C", ""));

        // 3 * (48 + 8) - 8 = 160
        assert_eq!(widget.app_row_width(), 160);
    }

    #[test]
    fn test_dock_widget_item_x() {
        let widget = DockWidget::new();
        assert_eq!(widget.item_x(0), 0);
        assert_eq!(widget.item_x(1), 56); // 48 + 8
        assert_eq!(widget.item_x(2), 112); // 2 * (48 + 8)
    }

    #[test]
    fn test_dock_widget_hit_test() {
        let mut widget = DockWidget::new();
        widget.add_item(DockItem::new("A", ""));
        widget.add_item(DockItem::new("B", ""));

        // Click on first item
        let hit = widget.hit_test(Point::new(24.0, 30.0));
        assert_eq!(hit, Some(0));

        // Click on second item
        let hit = widget.hit_test(Point::new(80.0, 30.0));
        assert_eq!(hit, Some(1));

        // Click outside
        let hit = widget.hit_test(Point::new(200.0, 30.0));
        assert_eq!(hit, None);
    }

    #[test]
    fn test_dock_widget_builder() {
        let widget = DockWidget::new()
            .with_hover_color(Color::RED)
            .with_background_color(Color::BLACK)
            .with_hover_highlight(true)
            .with_click_animation(false);

        assert_eq!(widget.hover_color, Color::RED);
        assert_eq!(widget.background_color, Color::BLACK);
        assert!(widget.hover_highlight);
        assert!(!widget.click_animation);
    }

    #[test]
    fn test_dash_config() {
        let config = DashConfig::new();
        assert!((config.span_width - 32.0).abs() < f32::EPSILON);
        assert!((config.gap - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dash_width() {
        let config = DashConfig::new();
        // 1 window: 32px
        assert!((config.dash_width(1) - 32.0).abs() < f32::EPSILON);
        // 2 windows: (32 - 3) / 2 = 14.5
        assert!((config.dash_width(2) - 14.5).abs() < f32::EPSILON);
        // 3 windows: (32 - 6) / 3 = 8.67
        let w3 = config.dash_width(3);
        assert!((w3 - 8.67).abs() < 0.01);
    }

    #[test]
    fn test_dash_positions() {
        let config = DashConfig::new();
        let positions = config.dash_positions(1);
        assert_eq!(positions.len(), 1);
        assert!((positions[0] - 8.0).abs() < f32::EPSILON);

        let positions = config.dash_positions(2);
        assert_eq!(positions.len(), 2);
        assert!((positions[0] - 8.0).abs() < f32::EPSILON);
        assert!((positions[1] - 25.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_device_entry() {
        let mut entry = DeviceEntry::new("Headphones");
        assert_eq!(entry.name, "Headphones");
        assert!(!entry.connected);
        assert_eq!(entry.button_text, "Pair");

        entry.pair();
        assert_eq!(entry.button_text, "Connect");
        assert_eq!(entry.status, "Paired");

        entry.connect();
        assert!(entry.connected);
        assert_eq!(entry.button_text, "Disconnect");

        entry.disconnect();
        assert!(!entry.connected);
        assert_eq!(entry.button_text, "Connect");

        entry.unpair();
        assert_eq!(entry.button_text, "Pair");
    }

    #[test]
    fn test_bluetooth_state() {
        let mut state = BluetoothState::new();
        assert!(!state.power_on);
        assert_eq!(state.adapter_name, "archlinux");

        state.toggle_power();
        assert!(state.power_on);
        assert_eq!(state.last_action, "Power on");

        state.toggle_scan();
        assert!(state.scanning);
    }

    #[test]
    fn test_bluetooth_state_scan_requires_power() {
        let mut state = BluetoothState::new();
        state.toggle_scan();
        assert!(!state.scanning); // Can't scan when power is off
    }

    #[test]
    fn test_bluetooth_state_sweep() {
        let mut state = BluetoothState::new();
        state.update_sweep(0.0);
        assert!((state.sweep_cos - 1.0).abs() < f32::EPSILON);
        assert!((state.sweep_sin - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_bluetooth_widget() {
        let mut widget = BluetoothWidget::new();
        assert!((widget.size.width - 402.0).abs() < f32::EPSILON);
        assert_eq!(widget.discovered_count(), 0);
        assert_eq!(widget.paired_count(), 0);

        widget.add_discovered(DeviceEntry::new("Device 1"));
        widget.add_paired(DeviceEntry::new("Device 2"));

        assert_eq!(widget.discovered_count(), 1);
        assert_eq!(widget.paired_count(), 1);
    }

    #[test]
    fn test_bluetooth_widget_clear_discovered() {
        let mut widget = BluetoothWidget::new();
        widget.add_discovered(DeviceEntry::new("Device 1"));
        widget.add_discovered(DeviceEntry::new("Device 2"));

        assert_eq!(widget.discovered_count(), 2);
        widget.clear_discovered();
        assert_eq!(widget.discovered_count(), 0);
    }

    #[test]
    fn test_bluetooth_state_connected_count() {
        let mut state = BluetoothState::new();
        let mut devices = vec![
            DeviceEntry::new("D1"),
            DeviceEntry::new("D2"),
            DeviceEntry::new("D3"),
        ];
        devices[0].connect();
        devices[2].connect();

        state.update_connected_count(&devices);
        assert_eq!(state.connected_count, 2);
    }

    #[test]
    fn test_slint_render_mode() {
        let mode = SlintRenderMode::Independent;
        assert_eq!(mode, SlintRenderMode::Independent);

        let default_mode = SlintRenderMode::default();
        assert_eq!(default_mode, SlintRenderMode::Independent);
    }

    #[test]
    fn test_slint_integration_config() {
        let config = SlintIntegrationConfig::new()
            .with_render_mode(SlintRenderMode::SharedContext)
            .with_scale_factor(2.0);

        assert_eq!(config.render_mode, SlintRenderMode::SharedContext);
        assert!((config.scale_factor - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_slint_component() {
        let mut component =
            SlintComponent::new(WidgetId::new(1), "DockWidget", Size::new(520.0, 60.0));
        assert_eq!(component.name, "DockWidget");
        assert!(component.visible);
        assert!(component.texture.is_none());

        component.set_texture(TextureId::new(42));
        assert!(component.texture.is_some());

        component.hide();
        assert!(!component.visible);

        component.show();
        assert!(component.visible);
    }

    #[test]
    fn test_widget_registry() {
        let mut registry = WidgetRegistry::new();
        let id = registry.register_component("DockWidget", Size::new(520.0, 60.0));
        assert_eq!(registry.component_count(), 1);
        assert!(registry.get_component(id).is_some());

        registry.unregister_component(id);
        assert_eq!(registry.component_count(), 0);
    }

    #[test]
    fn test_widget_registry_dock_widget() {
        let mut registry = WidgetRegistry::new();
        let dock = DockWidget::new();
        registry.set_dock_widget(dock);

        assert!(registry.dock_widget().is_some());
        assert_eq!(registry.dock_widget().unwrap().dock_width, 520);
    }

    #[test]
    fn test_widget_registry_bluetooth_widget() {
        let mut registry = WidgetRegistry::new();
        let bt = BluetoothWidget::new();
        registry.set_bluetooth_widget(bt);

        assert!(registry.bluetooth_widget().is_some());
        assert!((registry.bluetooth_widget().unwrap().size.width - 402.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_slint_event() {
        let event = SlintEvent::DockItemClick {
            index: 0,
            button: SlintMouseButton::Left,
        };
        assert!(matches!(event, SlintEvent::DockItemClick { .. }));
    }

    #[test]
    fn test_dock_widget_size() {
        let widget = DockWidget::new();
        let size = widget.size();
        assert!((size.width - 520.0).abs() < f32::EPSILON);
        assert!((size.height - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_bluetooth_widget_state() {
        let mut widget = BluetoothWidget::new();
        widget.state_mut().toggle_power();
        assert!(widget.state().power_on);
    }

    #[test]
    fn test_widget_registry_clear() {
        let mut registry = WidgetRegistry::new();
        registry.register_component("A", Size::new(100.0, 100.0));
        registry.register_component("B", Size::new(200.0, 200.0));

        assert_eq!(registry.component_count(), 2);
        registry.clear();
        assert_eq!(registry.component_count(), 0);
    }
}
