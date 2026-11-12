//! Bluetooth widget renderer — uses the actual `bluetooth-widget-complete` Slint design.
//!
//! Renders the `BluetoothWidget` component from `bluetooth-widget.slint` using Slint's
//! software renderer (MinimalSoftwareWindow) to a BGRA8888 buffer, then commits it
//! to the Wayland layer surface popup.
//!
//! The Slint widget handles all layout, styling, animations, and state transitions
//! natively — no hand-coded pixel drawing needed.

use std::time::Instant;

use slint::{ModelRc, PhysicalSize, SharedString};

use crate::models::BluetoothSnapshot;

// Include the Slint-generated Bluetooth widget module.
include!(env!("SLINT_INCLUDE_BLUETOOTH"));

/// Popup width matching the bluetooth-widget-complete Slint design.
pub const BLUETOOTH_POPUP_WIDTH: i32 = 402;
/// Popup height matching the bluetooth-widget-complete Slint design.
pub const BLUETOOTH_POPUP_HEIGHT: i32 = 318;

// ===== Hit-test constants for the Slint widget layout =====

/// Minimize button in the header (top-right corner).
pub const HEADER_MINIMIZE_X: i32 = 358;
pub const HEADER_MINIMIZE_Y: i32 = 14;
pub const HEADER_MINIMIZE_W: i32 = 20;
pub const HEADER_MINIMIZE_H: i32 = 20;

/// Power toggle card (controls row, left).
pub const POWER_BUTTON_X: i32 = 12;
pub const POWER_BUTTON_Y: i32 = 85; // 75 (row y) + 10 (card y)
pub const POWER_BUTTON_W: i32 = 89;
pub const POWER_BUTTON_H: i32 = 40;

/// Scan toggle card (controls row, right).
pub const SCAN_BUTTON_X: i32 = 107;
pub const SCAN_BUTTON_Y: i32 = 85; // 75 (row y) + 10 (card y)
pub const SCAN_BUTTON_W: i32 = 89;
pub const SCAN_BUTTON_H: i32 = 40;

/// Device list section.
pub const DEVICE_LIST_Y: i32 = 154; // y where discovered list starts
pub const DEVICE_ROW_HEIGHT: i32 = 39; // card height
pub const DEVICE_ROW_COUNT: usize = 4; // max rows per column
pub const DEVICE_CARD_WIDTH: i32 = 174;
pub const DEVICE_CARD_HEIGHT: i32 = 39;

/// Action button within a device card.
pub const DEVICE_ACTION_X: i32 = 123; // x relative to card
pub const DEVICE_ACTION_W: i32 = 51;
pub const DEVICE_ACTION_Y_OFFSET: i32 = 0;
pub const DEVICE_ACTION_H: i32 = 23;

/// Paired list starts higher (y=111) to align with adapter label.
pub const PAIRED_LIST_X: i32 = 208;
pub const PAIRED_LIST_Y: i32 = 111;

/// Renderer that uses the actual Slint BluetoothWidget component.
///
/// The widget is associated with the Slint platform's window adapter (the dock's
/// MinimalSoftwareWindow), not a separate window. Slint's platform can only provide
/// one window adapter, so all Slint widgets share the same underlying window.
/// Rendering uses Window::take_snapshot() to get pixel data.
pub struct SlintBluetoothRenderer {
    widget: BluetoothWidget,
    /// Cache key for last rendered snapshot.
    last_snapshot: Option<BluetoothSnapshot>,
    /// Cached BGRA output for the last rendered snapshot/canvas size.
    last_bgra: Vec<u8>,
    last_size: (u32, u32),
    /// **FIX (v1.5.5)**: Timer for the radar sweep animation when scanning is active.
    /// Drives the sweepCos, sweepSin, and dot1/2/3 X/Y properties on the Slint widget.
    radar_timer: Instant,
}

impl SlintBluetoothRenderer {
    /// Initialize the Slint Bluetooth renderer.
    ///
    /// Reuses the Slint platform already set by the dock renderer
    /// (SlintDockRenderer). Does not call set_platform() to avoid
    /// overriding the existing platform. The BluetoothWidget is associated
    /// with the platform's window adapter (the dock's MinimalSoftwareWindow),
    /// so rendering must use the platform's window, not a separate one.
    pub fn new() -> Result<Self, String> {
        // The dock renderer already set the Slint platform.
        // The BluetoothWidget will be associated with the platform's window adapter.
        let widget = BluetoothWidget::new()
            .map_err(|e| format!("Failed to create BluetoothWidget: {:?}", e))?;

        Ok(Self {
            widget,
            last_snapshot: None,
            last_bgra: Vec::new(),
            last_size: (0, 0),
            radar_timer: Instant::now(),
        })
    }

    /// Render the Bluetooth widget to a BGRA8888 canvas buffer.
    ///
    /// The canvas is in BGRA8888 format (4 bytes per pixel, stride = width * 4).
    /// Uses Window::take_snapshot() to render the Slint scene and get RGBA8 pixels,
    /// then converts to BGRA for the Wayland SHM buffer.
    pub fn render(
        &mut self,
        canvas: &mut [u8],
        phys_width: u32,
        phys_height: u32,
        snapshot: &BluetoothSnapshot,
    ) {
        let required = (phys_width as usize) * (phys_height as usize) * 4;
        if canvas.len() < required {
            return;
        }

        // **FIX (v1.5.5)**: Cache hit: reuse last rendered output, but skip cache
        // when scanning is active because the radar animation properties change
        // every frame. Without this, the radar animation would freeze on the first
        // frame because the cached output would be reused indefinitely.
        if !snapshot.discovering
            && self.last_snapshot.as_ref() == Some(snapshot)
            && self.last_size == (phys_width, phys_height)
            && self.last_bgra.len() == required
        {
            canvas[..required].copy_from_slice(&self.last_bgra[..required]);
            return;
        }

        // Set the Slint window size
        let window = self.widget.window();
        window.set_size(PhysicalSize::new(
            BLUETOOTH_POPUP_WIDTH as u32,
            BLUETOOTH_POPUP_HEIGHT as u32,
        ));

        // Map BluetoothSnapshot to Slint global state properties
        let global = BluetoothState::get(&self.widget);
        global.set_powerOn(snapshot.powered);
        global.set_scanning(snapshot.discovering);
        global.set_adapterName(SharedString::from(&snapshot.adapter_name));

        // Count connected devices
        let connected_count = snapshot.devices.iter().filter(|d| d.connected).count();
        global.set_connectedCount(connected_count as i32);

        // Separate devices into discovered (not paired) and paired
        let discovered: Vec<DeviceEntry> = snapshot
            .devices
            .iter()
            .filter(|d| !d.paired)
            .map(|d| DeviceEntry {
                name: SharedString::from(&d.name),
                connected: d.connected,
                buttonText: if d.connected {
                    SharedString::from("Disconnect")
                } else {
                    SharedString::from("Pair")
                },
                status: if d.connected {
                    SharedString::from("Connected")
                } else {
                    SharedString::from("New device")
                },
                lastAction: SharedString::from(""),
            })
            .collect();

        let paired: Vec<DeviceEntry> = snapshot
            .devices
            .iter()
            .filter(|d| d.paired)
            .map(|d| DeviceEntry {
                name: SharedString::from(&d.name),
                connected: d.connected,
                buttonText: if d.connected {
                    SharedString::from("Connected")
                } else {
                    SharedString::from("Connect")
                },
                status: if d.connected {
                    SharedString::from("Connected")
                } else {
                    SharedString::from("Paired")
                },
                lastAction: SharedString::from(""),
            })
            .collect();

        // Set device models
        self.widget
            .set_discoveredModel(ModelRc::from(discovered.as_slice()));
        self.widget
            .set_pairedModel(ModelRc::from(paired.as_slice()));

        // **FIX (v1.5.5)**: Radar sweep animation for Bluetooth scanning.
        // Update radar animation properties when scanning is active. The radar consists
        // of a sweeping dot orbiting the center and three trailing dots at fixed angles.
        // Previously, the radar properties (sweepCos, sweepSin, dot1X/Y, etc.) were
        // defined in the Slint widget but never set from Rust, so they stayed at 0.0.
        if snapshot.discovering {
            let elapsed = self.radar_timer.elapsed().as_secs_f64();
            // Sweep period: 2 seconds per full rotation
            let angle = elapsed * 2.0 * std::f64::consts::PI / 2.0;
            let sweep_cos = angle.cos();
            let sweep_sin = angle.sin();
            
            // Three trailing dots at 120° intervals behind the sweep
            let dot1_angle = angle - std::f64::consts::PI * 2.0 / 3.0;
            let dot2_angle = angle - std::f64::consts::PI * 4.0 / 3.0;
            let dot3_angle = angle - std::f64::consts::PI * 5.0 / 3.0;
            
            // Dot radius: 6 (smaller orbit than sweep which uses radius 8)
            let dot_radius = 6.0;
            
            global.set_sweepCos(sweep_cos as f32);
            global.set_sweepSin(sweep_sin as f32);
            global.set_dot1X((dot1_angle.cos() * dot_radius) as f32);
            global.set_dot1Y((dot1_angle.sin() * dot_radius) as f32);
            global.set_dot2X((dot2_angle.cos() * dot_radius) as f32);
            global.set_dot2Y((dot2_angle.sin() * dot_radius) as f32);
            global.set_dot3X((dot3_angle.cos() * dot_radius) as f32);
            global.set_dot3Y((dot3_angle.sin() * dot_radius) as f32);
        } else {
            // **FIX (v1.5.5)**: Reset radar timer when scanning stops so the
            // animation starts fresh next time scanning is enabled.
            self.radar_timer = Instant::now();
        }

        // **FIX (v1.5.5)**: Force Slint to re-render the scene with updated state
        // before taking a snapshot. Without request_redraw(), take_snapshot() may
        // return a cached render from before the global state (powered, discovering)
        // was updated, so clicking buttons had no visual effect.
        window.request_redraw();
        slint::platform::update_timers_and_animations();

        // Render via Slint using Window::take_snapshot() which re-renders the scene
        // and returns RGBA8 pixels. This works regardless of which window adapter
        // the widget is associated with.
        canvas[..required].fill(0);
        if let Ok(snap) = window.take_snapshot() {
            let snap_bytes = snap.as_bytes();
            let copy_pixels = snap_bytes.len() / 4;
            for (idx, chunk) in snap_bytes.chunks_exact(4).take(copy_pixels.min(required / 4)).enumerate() {
                let offset = idx * 4;
                // Convert RGBA -> BGRA for Wayland SHM
                canvas[offset] = chunk[2];   // B
                canvas[offset + 1] = chunk[1]; // G
                canvas[offset + 2] = chunk[0]; // R
                canvas[offset + 3] = chunk[3]; // A
            }
        } else {
            tracing::warn!("Failed to take Bluetooth widget snapshot");
        }

        self.last_snapshot = Some(snapshot.clone());
        self.last_size = (phys_width, phys_height);
        self.last_bgra.clear();
        self.last_bgra.extend_from_slice(&canvas[..required]);
    }
}

/// Main entry point: render the Bluetooth widget UI using Slint.
///
/// Renders the `BluetoothWidget` Slint component to a BGRA8888 canvas buffer.
/// The renderer is passed by mutable reference (stored in AppState, not static).
pub fn render_bluetooth_surface(
    renderer: &mut SlintBluetoothRenderer,
    canvas: &mut [u8],
    width: u32,
    height: u32,
    _scale: f32,
    snapshot: &BluetoothSnapshot,
) {
    let w = width as usize;
    let h = height as usize;
    if w == 0 || h == 0 || canvas.len() < w * h * 4 {
        return;
    }

    canvas.fill(0);
    renderer.render(canvas, width, height, snapshot);
}
