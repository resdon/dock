//! Bluetooth popup panel.
//!
//! Device list popup showing paired and available Bluetooth devices.

use crate::render::femtovg_wgpu::GpuContext;

use smithay_client_toolkit::{
    dmabuf::DmabufState,
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::models::{BluetoothAction, BluetoothSnapshot, SystemBarSnapshot, TrayItem};
use crate::render::bluetooth::{
    render_bluetooth_surface, BLUETOOTH_POPUP_HEIGHT, BLUETOOTH_POPUP_WIDTH, DEVICE_ACTION_H,
    DEVICE_ACTION_W, DEVICE_ACTION_X, DEVICE_ACTION_Y_OFFSET, DEVICE_LIST_Y, DEVICE_ROW_COUNT,
    DEVICE_ROW_HEIGHT, HEADER_MINIMIZE_H, HEADER_MINIMIZE_W, HEADER_MINIMIZE_X, HEADER_MINIMIZE_Y,
    POWER_BUTTON_H, POWER_BUTTON_W, POWER_BUTTON_X, POWER_BUTTON_Y, SCAN_BUTTON_H, SCAN_BUTTON_W,
    SCAN_BUTTON_X, SCAN_BUTTON_Y,
};
use crate::render::font::FontManager;
use crate::render::widget_bar::{button_center_x, widget_bar_position, BLUETOOTH_INDEX};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

use super::draw::DrawFrameData;

pub fn draw_bluetooth_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    is_open: bool,
    snapshot: &BluetoothSnapshot,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    gpu_context: Option<&mut GpuContext>,
    dmabuf_state: &DmabufState,
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
    let (bar_x, bar_y) = widget_bar_position(
        frame.dock_width as i32,
        _tray_items.len(),
        frame.surface_height as i32,
    );
    let anchor_x = ((bar_x + button_center_x(BLUETOOTH_INDEX)) as f64 * scale).round() as i32;
    let x = (anchor_x - phys_width / 2).max(0);
    let anchor_y = (bar_y as f64 * scale).round() as i32;
    let y = anchor_y - phys_height;
    let logical_x = (x as f64 / scale).round() as i32;
    let logical_y = (y as f64 / scale).round() as i32;

    if dock.bluetooth_popup.is_none() {
        let surface = compositor_state.create_surface(qh);
        let subsurface = subcompositor
            .expect("Subcompositor global not bound")
            .get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
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
        });
    }

    let popup = dock
        .bluetooth_popup
        .as_mut()
        .expect("Bluetooth popup was just created");
    popup.position = (logical_x, logical_y);
    popup.subsurface.set_position(logical_x, logical_y);
    popup.width = BLUETOOTH_POPUP_WIDTH as u32;
    popup.height = BLUETOOTH_POPUP_HEIGHT as u32;

    let expected_len = phys_width as usize * phys_height as usize * 4;

    // GPU rendering path
    if let Some(gpu) = gpu_context {
        let alpha = dock.hover_fade.current_alpha;
        let devices: Vec<(&str, &str, bool, bool)> = snapshot
            .devices
            .iter()
            .map(|d| (d.name.as_str(), "Paired", d.connected, d.paired))
            .collect();
        match gpu.dmabuf_pool.get_or_create(
            &gpu.adapter,
            &gpu.device,
            qh,
            dmabuf_state,
            phys_width as u32,
            phys_height as u32,
        ) {
            Ok(target) => {
                gpu.resize(phys_width as u32, phys_height as u32, (scale * 96.0) as f32);
                crate::render::femtovg_wgpu::render_bluetooth_popup_femtovg(
                    &mut gpu.canvas,
                    phys_width as f32,
                    phys_height as f32,
                    scale as f32,
                    snapshot.powered,
                    snapshot.discovering,
                    snapshot.adapter_name.as_str(),
                    &devices,
                    alpha,
                );
                let output = femtovg::renderer::WGPURenderOutput::from(&target.texture);
                if let Some(cmd_buf) = gpu.canvas.flush_to_output(output) {
                    gpu.queue.submit(std::iter::once(cmd_buf));
                }
                popup.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                crate::render::dmabuf_export::attach_and_commit(&popup.surface, &target.buffer);
                popup.surface.damage_buffer(0, 0, phys_width, phys_height);
                let region = compositor_state.wl_compositor().create_region(qh, ());
                region.add(0, 0, BLUETOOTH_POPUP_WIDTH, BLUETOOTH_POPUP_HEIGHT);
                popup.surface.set_input_region(Some(&region));
                region.destroy();
                popup.surface.commit();
                popup.current_buffer = None;
                return;
            }
            Err(_) => {
                // Fall through to CPU path
            }
        }
    }

    // CPU rendering path — always render every frame
    popup.cached_content.resize(expected_len, 0);
    render_bluetooth_surface(
        &mut popup.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale as f32,
        snapshot,
        font_manager,
    );
    popup.cached_phys_width = phys_width;
    popup.cached_phys_height = phys_height;

    let (buffer, canvas) = pool
        .create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate Bluetooth popup buffer");
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup
        .surface
        .set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&popup.surface)
        .expect("Failed to attach Bluetooth popup buffer");
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

/// Hit-test the compact power/scan buttons and the per-device Pair/Connect/
/// Disconnect actions. Each action contains the exact BlueZ device object path
/// from the latest ObjectManager snapshot.
pub fn action_for_click(snapshot: &BluetoothSnapshot, x: f64, y: f64) -> Option<BluetoothAction> {
    if !snapshot.available || snapshot.adapter_path.is_none() {
        return None;
    }
    let x = x.round() as i32;
    let y = y.round() as i32;

    if (POWER_BUTTON_Y..POWER_BUTTON_Y + POWER_BUTTON_H).contains(&y)
        && (POWER_BUTTON_X..POWER_BUTTON_X + POWER_BUTTON_W).contains(&x)
    {
        return Some(BluetoothAction::SetPowered(!snapshot.powered));
    }
    if (SCAN_BUTTON_Y..SCAN_BUTTON_Y + SCAN_BUTTON_H).contains(&y)
        && (SCAN_BUTTON_X..SCAN_BUTTON_X + SCAN_BUTTON_W).contains(&x)
        && snapshot.powered
    {
        return Some(BluetoothAction::SetDiscovering(!snapshot.discovery_owned));
    }

    for (index, device) in snapshot.devices.iter().take(DEVICE_ROW_COUNT).enumerate() {
        let row_y = DEVICE_LIST_Y + index as i32 * DEVICE_ROW_HEIGHT;
        let action_y = row_y + DEVICE_ACTION_Y_OFFSET;
        if (action_y..action_y + DEVICE_ACTION_H).contains(&y)
            && (DEVICE_ACTION_X..DEVICE_ACTION_X + DEVICE_ACTION_W).contains(&x)
        {
            return Some(if device.connected {
                BluetoothAction::Disconnect(device.path.clone())
            } else if device.paired {
                BluetoothAction::Connect(device.path.clone())
            } else {
                BluetoothAction::Pair(device.path.clone())
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
