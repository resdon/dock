//! Bluetooth popup panel and hit-test helpers.
//!
//! The Bluetooth icon on the dashed block toggles a subsurface popup rendered
//! by Slint. This module provides the draw function and hit-test helpers for
//! the popup buttons (minimize, power, scan, device actions).
//!
//! The popup is a subsurface of the dock surface, positioned relative to the
//! dock's coordinate space. Since the dashed block sits to the left of the dock,
//! the popup X coordinate is negative relative to the dock's left edge, but
//! subsurfaces support negative coordinates and will render correctly on-screen.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, output::OutputState, shell::WaylandSurface,
    shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::models::{BluetoothAction, BluetoothSnapshot};
use crate::render::bluetooth::{
    render_bluetooth_surface, SlintBluetoothRenderer, BLUETOOTH_POPUP_HEIGHT,
    BLUETOOTH_POPUP_WIDTH, DEVICE_ACTION_H, DEVICE_ACTION_W, DEVICE_ACTION_X,
    DEVICE_ACTION_Y_OFFSET, DEVICE_LIST_Y, DEVICE_ROW_COUNT, HEADER_MINIMIZE_H, HEADER_MINIMIZE_W,
    HEADER_MINIMIZE_X, HEADER_MINIMIZE_Y, PAIRED_LIST_X, PAIRED_LIST_Y, POWER_BUTTON_H,
    POWER_BUTTON_W, POWER_BUTTON_X, POWER_BUTTON_Y, SCAN_BUTTON_H, SCAN_BUTTON_W, SCAN_BUTTON_X,
    SCAN_BUTTON_Y,
};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

use super::draw::DrawFrameData;

/// Draw the Bluetooth popup as a subsurface of the dock.
///
/// Creates the subsurface on first open, positions it centered on the screen,
/// renders the Slint BluetoothWidget, and commits the buffer.
pub fn draw_bluetooth_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    output_state: &OutputState,
    renderer: &mut SlintBluetoothRenderer,
    is_open: bool,
    snapshot: &BluetoothSnapshot,
    _tray_item_count: usize,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
    config: &DockConfig,
) {
    if !is_open {
        if let Some(popup) = dock.bluetooth_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
        return;
    }

    let scale = dock.scale_factor.max(0.25);
    let phys_width = (BLUETOOTH_POPUP_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (BLUETOOTH_POPUP_HEIGHT as f64 * scale).round().max(1.0) as i32;

    // **FIX (v1.5.5)**: Screen-center positioning for the Bluetooth popup.
    // The popup is a subsurface of the dock, so its position is relative to the
    // dock's top-left corner. To center it on screen, we compute the offset from
    // the dock's position to the screen center. The dock is a layer surface
    // anchored to the bottom, centered horizontally.
    //
    // Previously, the popup was positioned relative to the Bluetooth icon on the
    // dashed block, which placed it off-screen or at wrong coordinates.
    let output_size = output_state
        .info(&dock.output)
        .and_then(|info| info.logical_size)
        .unwrap_or((1920, 1080));
    
    // Use the actual dock surface dimensions, not the popup dimensions
    let actual_dock_width = dock.width as i32;
    let actual_dock_height = dock.height as i32;
    
    // Dock is centered horizontally at the bottom of the screen
    let dock_left_edge = (output_size.0 as i32 / 2) - (actual_dock_width / 2);
    let margin = config.margin;
    let dock_top_edge = output_size.1 as i32 - actual_dock_height - margin;
    
    // Screen center
    let screen_center_x = output_size.0 as i32 / 2;
    let screen_center_y = output_size.1 as i32 / 2;
    
    // Popup position relative to dock's top-left corner
    let popup_x = screen_center_x - dock_left_edge - BLUETOOTH_POPUP_WIDTH / 2;
    let popup_y = screen_center_y - dock_top_edge - BLUETOOTH_POPUP_HEIGHT / 2;

    if dock.bluetooth_popup.is_none() {
        let surface = compositor_state.create_surface(qh);
        let Some(sc) = subcompositor else {
            tracing::warn!("Subcompositor not available; bluetooth popup rendering degraded");
            return;
        };
        let parent_surface = dock.surface.wl_surface();
        let subsurface = sc.get_subsurface(&surface, parent_surface, qh, ());
        subsurface.set_desync();
        dock.bluetooth_popup = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("bluetooth-panel".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: 0,
            // FIX (v1.5.5): Initialize last_content_hash to 0 for content-hash caching.
            last_content_hash: 0,
            last_texture_generation: 0,
        });
    }
    let Some(popup) = dock.bluetooth_popup.as_mut() else {
        return;
    };

    popup.position = (popup_x, popup_y);
    popup.subsurface.set_position(popup_x, popup_y);
    popup.width = BLUETOOTH_POPUP_WIDTH as u32;
    popup.height = BLUETOOTH_POPUP_HEIGHT as u32;

    let expected_len = phys_width as usize * phys_height as usize * 4;
    popup.cached_content.resize(expected_len, 0);
    render_bluetooth_surface(
        renderer,
        &mut popup.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale as f32,
        snapshot,
    );
    popup.cached_phys_width = phys_width;
    popup.cached_phys_height = phys_height;

    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(bc) => bc,
        Err(e) => {
            tracing::warn!("Failed to allocate bluetooth popup buffer: {}", e);
            return;
        }
    };
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup
        .surface
        .set_buffer_scale(scale.round().max(1.0) as i32);
    if let Err(e) = buffer.attach_to(&popup.surface) {
        tracing::warn!("Failed to attach bluetooth popup buffer: {}", e);
        return;
    }
    popup.surface.damage_buffer(0, 0, phys_width, phys_height);

    let region = compositor_state.wl_compositor().create_region(qh, ());
    region.add(0, 0, BLUETOOTH_POPUP_WIDTH, BLUETOOTH_POPUP_HEIGHT);
    popup.surface.set_input_region(Some(&region));
    region.destroy();
    popup.surface.commit();
    popup.current_buffer = Some(buffer);
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (HEADER_MINIMIZE_X..HEADER_MINIMIZE_X + HEADER_MINIMIZE_W).contains(&x)
        && (HEADER_MINIMIZE_Y..HEADER_MINIMIZE_Y + HEADER_MINIMIZE_H).contains(&y)
}

/// Hit-test the power/scan buttons and the per-device Pair/Connect/
/// Disconnect actions in the two-column layout (DISCOVERED left, PAIRED right).
/// Each action contains the exact BlueZ device object path from the latest
/// ObjectManager snapshot.
pub fn action_for_click(snapshot: &BluetoothSnapshot, x: f64, y: f64) -> Option<BluetoothAction> {
    if !snapshot.available || snapshot.adapter_path.is_none() {
        return None;
    }
    let x = x.round() as i32;
    let y = y.round() as i32;

    // Power toggle card
    if (POWER_BUTTON_Y..POWER_BUTTON_Y + POWER_BUTTON_H).contains(&y)
        && (POWER_BUTTON_X..POWER_BUTTON_X + POWER_BUTTON_W).contains(&x)
    {
        return Some(BluetoothAction::SetPowered(!snapshot.powered));
    }
    // Scan toggle card
    if (SCAN_BUTTON_Y..SCAN_BUTTON_Y + SCAN_BUTTON_H).contains(&y)
        && (SCAN_BUTTON_X..SCAN_BUTTON_X + SCAN_BUTTON_W).contains(&x)
        && snapshot.powered
    {
        return Some(BluetoothAction::SetDiscovering(!snapshot.discovery_owned));
    }

    // Separate devices into discovered (not paired) and paired
    let discovered: Vec<_> = snapshot.devices.iter().filter(|d| !d.paired).collect();
    let paired: Vec<_> = snapshot.devices.iter().filter(|d| d.paired).collect();

    // Hit-test DISCOVERED list (left column, x=22, y=154)
    for (idx, device) in discovered.iter().take(DEVICE_ROW_COUNT).enumerate() {
        let card_y = DEVICE_LIST_Y + idx as i32 * 43;
        let action_x = 22 + DEVICE_ACTION_X; // card x + action offset
        let action_y = card_y + DEVICE_ACTION_Y_OFFSET;
        if (action_y..action_y + DEVICE_ACTION_H).contains(&y)
            && (action_x..action_x + DEVICE_ACTION_W).contains(&x)
        {
            return Some(if device.connected {
                BluetoothAction::Disconnect(device.path.clone())
            } else {
                BluetoothAction::Pair(device.path.clone())
            });
        }
    }

    // Hit-test PAIRED list (right column, x=208, y=111)
    for (idx, device) in paired.iter().take(4).enumerate() {
        let card_y = PAIRED_LIST_Y + idx as i32 * 43;
        let action_x = PAIRED_LIST_X + DEVICE_ACTION_X; // card x + action offset
        let action_y = card_y + DEVICE_ACTION_Y_OFFSET;
        if (action_y..action_y + DEVICE_ACTION_H).contains(&y)
            && (action_x..action_x + DEVICE_ACTION_W).contains(&x)
        {
            return Some(if device.connected {
                BluetoothAction::Disconnect(device.path.clone())
            } else {
                BluetoothAction::Connect(device.path.clone())
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BluetoothDevice;

    fn sample() -> BluetoothSnapshot {
        BluetoothSnapshot {
            available: true,
            adapter_path: Some("/org/bluez/hci0".into()),
            adapter_name: "Arch Linux".into(),
            powered: true,
            devices: vec![
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF".into(),
                    address: "AA:BB:CC:DD:EE:FF".into(),
                    name: "Headphones".into(),
                    paired: true,
                    connected: true,
                    trusted: true,
                },
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_11_22_33_44_55_66".into(),
                    address: "11:22:33:44:55:66".into(),
                    name: "Keyboard".into(),
                    paired: true,
                    connected: false,
                    trusted: true,
                },
                BluetoothDevice {
                    path: "/org/bluez/hci0/dev_77_88_99_AA_BB_CC".into(),
                    address: "77:88:99:AA:BB:CC".into(),
                    name: "New device".into(),
                    paired: false,
                    connected: false,
                    trusted: false,
                },
            ],
            ..BluetoothSnapshot::default()
        }
    }

    #[test]
    fn power_button_toggles_power_state() {
        assert!(matches!(
            action_for_click(&sample(), 20.0, 35.0),
            Some(BluetoothAction::SetPowered(false))
        ));
    }

    #[test]
    fn scan_button_starts_discovery_when_powered() {
        assert!(matches!(
            action_for_click(&sample(), 150.0, 35.0),
            Some(BluetoothAction::SetDiscovering(true))
        ));
    }

    #[test]
    fn device_rows_offer_disconnect_connect_and_pair() {
        let snapshot = sample();
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 80.0),
            Some(BluetoothAction::Disconnect(_))
        ));
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 118.0),
            Some(BluetoothAction::Connect(_))
        ));
        assert!(matches!(
            action_for_click(&snapshot, 180.0, 156.0),
            Some(BluetoothAction::Pair(_))
        ));
    }

    #[test]
    fn minimize_button_is_only_header_hitbox() {
        assert!(minimize_button_at(200.0, 10.0));
        assert!(!minimize_button_at(150.0, 10.0));
    }
}
