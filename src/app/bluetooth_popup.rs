//! Bluetooth popup panel and hit-test helpers.
//!
//! The Bluetooth icon on the dashed block toggles a subsurface popup rendered
//! via wgpu GPU rendering. This module provides the draw function and hit-test
//! helpers for the popup buttons (minimize, power, scan, device actions).
//!
//! The popup is a subsurface of the dock surface, positioned relative to the
//! dock's coordinate space. Since the dashed block sits to the left of the dock,
//! the popup X coordinate is negative relative to the dock's left edge, but
//! subsurfaces support negative coordinates and will render correctly on-screen.

use wayland_client::Proxy;

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, output::OutputState, shell::WaylandSurface,
    shm::slot::SlotPool,
};
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::models::{BluetoothAction, BluetoothSnapshot};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

use super::draw::DrawFrameData;

/// Popup width matching the bluetooth-widget Slint design.
pub const BLUETOOTH_POPUP_WIDTH: i32 = 402;
/// Popup height matching the bluetooth-widget Slint design.
pub const BLUETOOTH_POPUP_HEIGHT: i32 = 318;

// ===== Hit-test constants (unchanged from Slint layout) =====

/// Minimize button in the header (top-right corner).
pub const HEADER_MINIMIZE_X: i32 = 358;
pub const HEADER_MINIMIZE_Y: i32 = 14;
pub const HEADER_MINIMIZE_W: i32 = 20;
pub const HEADER_MINIMIZE_H: i32 = 20;

/// Power toggle card (controls row, left).
pub const POWER_BUTTON_X: i32 = 12;
pub const POWER_BUTTON_Y: i32 = 85;
pub const POWER_BUTTON_W: i32 = 89;
pub const POWER_BUTTON_H: i32 = 40;

/// Scan toggle card (controls row, right).
pub const SCAN_BUTTON_X: i32 = 107;
pub const SCAN_BUTTON_Y: i32 = 85;
pub const SCAN_BUTTON_W: i32 = 89;
pub const SCAN_BUTTON_H: i32 = 40;

/// Device list section.
pub const DEVICE_LIST_Y: i32 = 154;
pub const DEVICE_ROW_HEIGHT: i32 = 39;
pub const DEVICE_ROW_COUNT: usize = 4;
pub const DEVICE_CARD_WIDTH: i32 = 174;
pub const DEVICE_CARD_HEIGHT: i32 = 39;

/// Action button within a device card.
pub const DEVICE_ACTION_X: i32 = 123;
pub const DEVICE_ACTION_W: i32 = 51;
pub const DEVICE_ACTION_Y_OFFSET: i32 = 0;
pub const DEVICE_ACTION_H: i32 = 23;

/// Paired list starts higher (y=111) to align with adapter label.
pub const PAIRED_LIST_X: i32 = 208;
pub const PAIRED_LIST_Y: i32 = 111;

/// Draw the Bluetooth popup as a subsurface of the dock.
///
/// Creates the subsurface on first open, positions it centered on the screen,
/// renders via wgpu GPU (DrawCommand list), and commits via DMA-BUF.
pub fn draw_bluetooth_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _frame: &DrawFrameData<'_>,
    _pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    output_state: &OutputState,
    _renderer: &(),
    is_open: bool,
    snapshot: &BluetoothSnapshot,
    _tray_item_count: usize,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
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

    // Screen-center positioning for the Bluetooth popup.
    let output_size = output_state
        .info(&dock.output)
        .and_then(|info| info.logical_size)
        .unwrap_or((1920, 1080));

    let actual_dock_width = dock.width as i32;
    let actual_dock_height = dock.height as i32;
    let dock_left_edge = (output_size.0 / 2) - (actual_dock_width / 2);
    let margin = config.margin;
    let dock_top_edge = output_size.1 - actual_dock_height - margin;
    let screen_center_x = output_size.0 / 2;
    let screen_center_y = output_size.1 / 2;
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

    // FIX (v1.5.5): Content-hash caching — skip rendering if Bluetooth state
    // hasn't changed. The popup is typically static once opened.
    let mut content_hash: u64 = if snapshot.available { 1 } else { 0 };
    content_hash = content_hash.wrapping_mul(31).wrapping_add(snapshot.powered as u64);
    content_hash = content_hash.wrapping_mul(31).wrapping_add(snapshot.discovering as u64);
    content_hash = content_hash.wrapping_mul(31).wrapping_add(snapshot.discovery_owned as u64);
    for b in snapshot.adapter_name.bytes() {
        content_hash = content_hash.wrapping_mul(37).wrapping_add(b as u64);
    }
    content_hash = content_hash.wrapping_mul(41).wrapping_add(snapshot.devices.len() as u64);
    for dev in &snapshot.devices {
        for b in dev.name.bytes() {
            content_hash = content_hash.wrapping_mul(37).wrapping_add(b as u64);
        }
        content_hash = content_hash.wrapping_mul(43)
            .wrapping_add(dev.paired as u64)
            .wrapping_add(dev.connected as u64);
    }
    if popup.last_content_hash == content_hash {
        return;
    }

    // GPU rendering path: build draw commands and render via DMA-BUF
    let scale_f32 = scale as f32;
    let width = phys_width as f32;
    let height = phys_height as f32;
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();

    // Parse colors from config
    let popup_bg = DockConfig::parse_hex_color(&config.background_color);
    let popup_border = DockConfig::parse_hex_color(&config.popup_border_color);
    let popup_text = DockConfig::parse_hex_color(&config.popup_text_color);

    // Background
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(0.0, 0.0, width, height),
        color: wgpu_api::Color {
            r: popup_bg.0 as f32 / 255.0,
            g: popup_bg.1 as f32 / 255.0,
            b: popup_bg.2 as f32 / 255.0,
            a: config.background_alpha,
        },
        radius: config.border_radius as f32 * scale_f32,
        stroke: Some(wgpu_api::Color {
            r: popup_border.0 as f32 / 255.0,
            g: popup_border.1 as f32 / 255.0,
            b: popup_border.2 as f32 / 255.0,
            a: config.border_alpha,
        }),
        stroke_width: 1.0 * scale_f32,
    });

    // Header: "Bluetooth" title
    draw_commands.push(wgpu_api::DrawCommand::Text {
        position: wgpu_api::Point::new(12.0 * scale_f32, 24.0 * scale_f32),
        content: "Bluetooth".to_string(),
        font_size: config.popup_font_size as f32 * 1.4 * scale_f32,
        color: wgpu_api::Color {
            r: popup_text.0 as f32 / 255.0,
            g: popup_text.1 as f32 / 255.0,
            b: popup_text.2 as f32 / 255.0,
            a: 1.0,
        },
        align: 0.0,
    });

    // Minimize button (X)
    draw_commands.push(wgpu_api::DrawCommand::Text {
        position: wgpu_api::Point::new(
            (HEADER_MINIMIZE_X + HEADER_MINIMIZE_W / 2) as f32 * scale_f32,
            ((HEADER_MINIMIZE_Y + HEADER_MINIMIZE_H / 2 + 6) as f32) * scale_f32,
        ),
        content: "✕".to_string(),
        font_size: 12.0 * scale_f32,
        color: wgpu_api::Color {
            r: popup_text.0 as f32 / 255.0,
            g: popup_text.1 as f32 / 255.0,
            b: popup_text.2 as f32 / 255.0,
            a: 0.7,
        },
        align: 0.5,
    });

    // Adapter label
    if snapshot.available {
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(12.0 * scale_f32, 55.0 * scale_f32),
            content: format!("Adapter: {}", snapshot.adapter_name),
            font_size: config.popup_font_size as f32 * 0.9 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 0.7,
            },
            align: 0.0,
        });

        // Power button
        let power_bg = if snapshot.powered {
            wgpu_api::Color { r: 0.2, g: 0.6, b: 0.2, a: 0.3 }
        } else {
            wgpu_api::Color { r: 0.6, g: 0.2, b: 0.2, a: 0.3 }
        };
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(
                POWER_BUTTON_X as f32 * scale_f32,
                POWER_BUTTON_Y as f32 * scale_f32,
                POWER_BUTTON_W as f32 * scale_f32,
                POWER_BUTTON_H as f32 * scale_f32,
            ),
            color: power_bg,
            radius: 6.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(
                (POWER_BUTTON_X + POWER_BUTTON_W / 2) as f32 * scale_f32,
                ((POWER_BUTTON_Y + POWER_BUTTON_H / 2 + 5) as f32) * scale_f32,
            ),
            content: if snapshot.powered { "On" } else { "Off" }.to_string(),
            font_size: config.popup_font_size as f32 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 1.0,
            },
            align: 0.5,
        });

        // Scan button
        let scan_bg = if snapshot.discovering {
            wgpu_api::Color { r: 0.2, g: 0.4, b: 0.8, a: 0.3 }
        } else {
            wgpu_api::Color { r: 0.3, g: 0.3, b: 0.3, a: 0.3 }
        };
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(
                SCAN_BUTTON_X as f32 * scale_f32,
                SCAN_BUTTON_Y as f32 * scale_f32,
                SCAN_BUTTON_W as f32 * scale_f32,
                SCAN_BUTTON_H as f32 * scale_f32,
            ),
            color: scan_bg,
            radius: 6.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(
                (SCAN_BUTTON_X + SCAN_BUTTON_W / 2) as f32 * scale_f32,
                ((SCAN_BUTTON_Y + SCAN_BUTTON_H / 2 + 5) as f32) * scale_f32,
            ),
            content: if snapshot.discovering { "Scanning" } else { "Scan" }.to_string(),
            font_size: config.popup_font_size as f32 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 1.0,
            },
            align: 0.5,
        });

        // Device lists
        let discovered: Vec<_> = snapshot.devices.iter().filter(|d| !d.paired).collect();
        let paired: Vec<_> = snapshot.devices.iter().filter(|d| d.paired).collect();

        // Discovered list header
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(22.0 * scale_f32, (DEVICE_LIST_Y - 12) as f32 * scale_f32),
            content: "Discovered".to_string(),
            font_size: config.popup_font_size as f32 * 0.85 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 0.6,
            },
            align: 0.0,
        });

        // Paired list header
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(
                PAIRED_LIST_X as f32 * scale_f32,
                (PAIRED_LIST_Y - 12) as f32 * scale_f32,
            ),
            content: "Paired".to_string(),
            font_size: config.popup_font_size as f32 * 0.85 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 0.6,
            },
            align: 0.0,
        });

        // Discovered devices (left column)
        for (idx, device) in discovered.iter().take(DEVICE_ROW_COUNT).enumerate() {
            let card_y = (DEVICE_LIST_Y + idx as i32 * 43) as f32 * scale_f32;
            let card_x = 22.0 * scale_f32;
            let card_w = DEVICE_CARD_WIDTH as f32 * scale_f32;
            let card_h = DEVICE_CARD_HEIGHT as f32 * scale_f32;

            // Device card background
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(card_x, card_y, card_w, card_h),
                color: wgpu_api::Color { r: 0.15, g: 0.15, b: 0.15, a: 0.5 },
                radius: 4.0 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });

            // Device name
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(card_x + 4.0 * scale_f32, card_y + 14.0 * scale_f32),
                content: device.name.clone(),
                font_size: config.popup_font_size as f32 * 0.85 * scale_f32,
                color: wgpu_api::Color {
                    r: popup_text.0 as f32 / 255.0,
                    g: popup_text.1 as f32 / 255.0,
                    b: popup_text.2 as f32 / 255.0,
                    a: 1.0,
                },
                align: 0.0,
            });

            // Action button
            let action_text = if device.connected { "Disconnect" } else { "Pair" };
            let action_x = card_x + DEVICE_ACTION_X as f32 * scale_f32;
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(
                    action_x + DEVICE_ACTION_W as f32 * scale_f32 / 2.0,
                    card_y + 14.0 * scale_f32,
                ),
                content: action_text.to_string(),
                font_size: config.popup_font_size as f32 * 0.8 * scale_f32,
                color: wgpu_api::Color {
                    r: popup_text.0 as f32 / 255.0,
                    g: popup_text.1 as f32 / 255.0,
                    b: popup_text.2 as f32 / 255.0,
                    a: 0.8,
                },
                align: 0.5,
            });
        }

        // Paired devices (right column)
        for (idx, device) in paired.iter().take(4).enumerate() {
            let card_y = (PAIRED_LIST_Y + idx as i32 * 43) as f32 * scale_f32;
            let card_x = PAIRED_LIST_X as f32 * scale_f32;
            let card_w = DEVICE_CARD_WIDTH as f32 * scale_f32;
            let card_h = DEVICE_CARD_HEIGHT as f32 * scale_f32;

            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(card_x, card_y, card_w, card_h),
                color: wgpu_api::Color { r: 0.15, g: 0.15, b: 0.15, a: 0.5 },
                radius: 4.0 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });

            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(card_x + 4.0 * scale_f32, card_y + 14.0 * scale_f32),
                content: device.name.clone(),
                font_size: config.popup_font_size as f32 * 0.85 * scale_f32,
                color: wgpu_api::Color {
                    r: popup_text.0 as f32 / 255.0,
                    g: popup_text.1 as f32 / 255.0,
                    b: popup_text.2 as f32 / 255.0,
                    a: 1.0,
                },
                align: 0.0,
            });

            let action_text = if device.connected { "Connected" } else { "Connect" };
            let action_x = card_x + DEVICE_ACTION_X as f32 * scale_f32;
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(
                    action_x + DEVICE_ACTION_W as f32 * scale_f32 / 2.0,
                    card_y + 14.0 * scale_f32,
                ),
                content: action_text.to_string(),
                font_size: config.popup_font_size as f32 * 0.8 * scale_f32,
                color: wgpu_api::Color {
                    r: popup_text.0 as f32 / 255.0,
                    g: popup_text.1 as f32 / 255.0,
                    b: popup_text.2 as f32 / 255.0,
                    a: 0.8,
                },
                align: 0.5,
            });
        }
    } else {
        // Bluetooth unavailable
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(width / 2.0, height / 2.0),
            content: "Bluetooth unavailable".to_string(),
            font_size: config.popup_font_size as f32 * 1.2 * scale_f32,
            color: wgpu_api::Color {
                r: popup_text.0 as f32 / 255.0,
                g: popup_text.1 as f32 / 255.0,
                b: popup_text.2 as f32 / 255.0,
                a: 0.6,
            },
            align: 0.5,
        });
    }

    // Render via GPU
    let gpu_success = if let Some(ref mut renderer) = gpu_context {
        if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
            match dmabuf_pool.acquire_texture(
                adapter, device, qh, dmabuf_state,
                phys_width as u32, phys_height as u32,
            ) {
                Ok((texture, export_info)) => {
                    if renderer.render_to_texture(
                        &texture, phys_width as u32, phys_height as u32,
                        1.0, &draw_commands,
                    ).is_err() {
                        false
                    } else {
                        match crate::render::dmabuf_export::create_buffer_from_export(
                            dmabuf_state, qh, &export_info,
                        ) {
                            Ok(wl_buffer) => {
                                dmabuf_pool.set_buffer_id(wl_buffer.id());
                                popup.surface.set_buffer_scale(scale_f32.round().max(1.0) as i32);
                                popup.surface.attach(Some(&wl_buffer), 0, 0);
                                popup.surface.damage_buffer(0, 0, phys_width, phys_height);
                                let region = compositor_state.wl_compositor().create_region(qh, ());
                                region.add(0, 0, BLUETOOTH_POPUP_WIDTH, BLUETOOTH_POPUP_HEIGHT);
                                popup.surface.set_input_region(Some(&region));
                                region.destroy();
                                popup.surface.commit();
                                // Commit hash only after successful GPU surface commit
                                popup.last_content_hash = content_hash;
                                true
                            }
                            Err(_) => false,
                        }
                    }
                }
                Err(_) => false,
            }
        } else {
            false
        }
    } else {
        false
    };

    if !gpu_success {
        tracing::warn!("Bluetooth popup GPU rendering failed, skipping frame");
    }
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
