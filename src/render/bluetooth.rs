//! Bluetooth widget renderer — uses the actual `bluetooth-widget-complete` Slint design.
//!
//! Renders the `BluetoothWidget` component from `bluetooth-widget.slint` using Slint's
//! software renderer (MinimalSoftwareWindow) to a BGRA8888 buffer, then commits it
//! to the Wayland layer surface popup.
//!
//! The Slint widget handles all layout, styling, animations, and state transitions
//! natively — no hand-coded pixel drawing needed.

use std::rc::Rc;

use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor};
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
pub struct SlintBluetoothRenderer {
    widget: BluetoothWidget,
    window: Rc<MinimalSoftwareWindow>,
    /// Reusable render buffer to avoid per-frame allocation
    render_buffer: Vec<PremultipliedRgbaColor>,
    /// Cache key for last rendered snapshot.
    last_snapshot: Option<BluetoothSnapshot>,
    /// Cached BGRA output for the last rendered snapshot/canvas size.
    last_bgra: Vec<u8>,
    last_size: (u32, u32),
}

impl SlintBluetoothRenderer {
    /// Initialize the Slint Bluetooth renderer.
    ///
    /// Reuses the Slint platform already set by the dock renderer
    /// (SlintDockRenderer). Does not call set_platform() to avoid
    /// overriding the existing platform.
    pub fn new() -> Result<Self, String> {
        // The dock renderer already set the Slint platform.
        // Create a window using the existing platform's window adapter.
        let window = MinimalSoftwareWindow::new(Default::default());

        let widget = BluetoothWidget::new()
            .map_err(|e| format!("Failed to create BluetoothWidget: {:?}", e))?;

        Ok(Self {
            widget,
            window,
            render_buffer: Vec::new(),
            last_snapshot: None,
            last_bgra: Vec::new(),
            last_size: (0, 0),
        })
    }

    /// Render the Bluetooth widget to a BGRA8888 canvas buffer.
    ///
    /// The canvas is in BGRA8888 format (4 bytes per pixel, stride = width * 4).
    /// Slint's software renderer outputs PremultipliedRgbaColor which is RGBA.
    /// We convert RGBA -> BGRA for the Wayland SHM buffer.
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

        if self.last_snapshot.as_ref() == Some(snapshot)
            && self.last_size == (phys_width, phys_height)
            && self.last_bgra.len() == required
        {
            canvas[..required].copy_from_slice(&self.last_bgra[..required]);
            return;
        }

        let slint_width = BLUETOOTH_POPUP_WIDTH as usize;
        let slint_height = BLUETOOTH_POPUP_HEIGHT as usize;
        let total_pixels = slint_width * slint_height;

        // Resize render buffer if needed
        if self.render_buffer.len() < total_pixels {
            self.render_buffer.resize(
                total_pixels,
                PremultipliedRgbaColor {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 0,
                },
            );
        }
        // Split borrows so we can mutate widget/window while borrowing render_buffer.
        let render_buffer = &mut self.render_buffer;
        let slint_buffer = &mut render_buffer[..total_pixels];

        // Set the Slint window size
        self.window.set_size(PhysicalSize::new(
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

        // Clear the Slint render buffer to transparent
        slint_buffer.fill(PremultipliedRgbaColor {
            red: 0,
            green: 0,
            blue: 0,
            alpha: 0,
        });

        // Render via Slint
        self.window.request_redraw();
        slint::platform::update_timers_and_animations();
        self.window.draw_if_needed(|renderer| {
            renderer.render(slint_buffer, slint_width);
        });

        // Convert Slint's RGBA buffer to BGRA for Wayland SHM
        canvas[..required].fill(0);
        let copy_width = slint_width.min(phys_width as usize);
        let copy_height = slint_height.min(phys_height as usize);
        let copy_pixels = copy_width * copy_height;
        let max_pixels = copy_pixels.min(slint_buffer.len()).min(canvas.len() / 4);

        for (idx, pixel) in slint_buffer.iter().take(max_pixels).enumerate() {
            let offset = idx * 4;
            canvas[offset] = pixel.blue;
            canvas[offset + 1] = pixel.green;
            canvas[offset + 2] = pixel.red;
            canvas[offset + 3] = pixel.alpha;
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
