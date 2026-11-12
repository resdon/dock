//! Widget bar app component.
//!
//! Right-side utility widget cluster: rclone, process monitor, audio, Bluetooth.

use std::os::fd::AsFd;

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use crate::models::BluetoothSnapshot;
use crate::render::widget_bar::{
    render_status_widget_bar, widget_bar_position, WIDGET_BAR_HEIGHT, WIDGET_BAR_WIDTH,
};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

/// Draws the four utility controls in a persistent subsurface to the right of
/// the tray app box.
pub fn draw_status_widget_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    bluetooth: &BluetoothSnapshot,
    tray_item_count: usize,
    visible: bool,
    alpha: f32,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
) {
    if dock.status_widget_bar.is_none() {
        let surface = compositor_state.create_surface(qh);
        let parent_surface = dock.surface.wl_surface().clone();
        let subsurface = match subcompositor {
            Some(sc) => sc.get_subsurface(&surface, &parent_surface, qh, ()),
            None => {
                tracing::warn!("Subcompositor not available; widget bar rendering degraded");
                return;
            }
        };
        subsurface.set_desync();
        dock.status_widget_bar = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("status-widget-bar".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: u64::MAX,
            // FIX (v1.5.5): Initialize last_content_hash to 0 for content-hash caching.
            last_content_hash: 0,
        });
    }

    let (x, y) = widget_bar_position(dock.width as i32, tray_item_count, dock.height as i32);
    let bar = match dock.status_widget_bar.as_mut() {
        Some(bar) => bar,
        None => {
            tracing::warn!("Widget bar not initialized");
            return;
        }
    };
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = WIDGET_BAR_WIDTH as u32;
    bar.height = WIDGET_BAR_HEIGHT as u32;

    let scale = dock.scale_factor.max(0.25);
    let phys_width = (WIDGET_BAR_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (WIDGET_BAR_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let alpha = alpha.clamp(0.0, 1.0);

    let expected_len = phys_width as usize * phys_height as usize * 4;
    bar.cached_content.resize(expected_len, 0);
    bar.cached_content.fill(0);
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    // GPU rendering path via wgpu-api DMA-BUF
    let gpu_success = {
        let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
        let scale_f32 = scale as f32;
        let width = phys_width as f32;
        let height = phys_height as f32;

        // Background rounded panel
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect {
                x: 0.0,
                y: 0.0,
                width,
                height,
            },
            color: wgpu_api::Color {
                r: 28.0 / 255.0,
                g: 27.0 / 255.0,
                b: 33.0 / 255.0,
                a: alpha * 0.96,
            },
            radius: 10.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });

        // 4 button backgrounds
        let button_size = 28.0 * scale_f32;
        let button_positions = [(10.0, 10.0), (42.0, 10.0), (10.0, 42.0), (42.0, 42.0)];
        for (bx, by) in &button_positions {
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect {
                    x: bx * scale_f32,
                    y: by * scale_f32,
                    width: button_size,
                    height: button_size,
                },
                color: wgpu_api::Color {
                    r: 48.0 / 255.0,
                    g: 46.0 / 255.0,
                    b: 55.0 / 255.0,
                    a: alpha,
                },
                radius: 4.0 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // Bluetooth icon indicator (filled if powered)
        if bluetooth.powered {
            let (bx, by) = button_positions[3]; // Bluetooth is at index 3
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect {
                    x: bx * scale_f32 + 7.0 * scale_f32,
                    y: by * scale_f32 + 8.0 * scale_f32,
                    width: 14.0 * scale_f32,
                    height: 2.0 * scale_f32,
                },
                color: wgpu_api::Color {
                    r: 250.0 / 255.0,
                    g: 204.0 / 255.0,
                    b: 21.0 / 255.0,
                    a: alpha,
                },
                radius: 1.0 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // Render via DMA-BUF
        if let Some(ref mut renderer) = gpu_context {
            match renderer.render_to_dmabuf(phys_width as u32, phys_height as u32, &draw_commands) {
                Ok(export_info) => match dmabuf_state.create_params(qh) {
                    Ok(params) => {
                        for (i, plane) in export_info.planes.iter().enumerate() {
                            params.add(
                                plane.fd.as_fd(),
                                i as u32,
                                plane.offset,
                                plane.stride,
                                export_info.modifier,
                            );
                        }
                        let (wl_buffer, _params) = params.create_immed(
                                phys_width,
                                phys_height,
                                export_info.fourcc as u32,
                                wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags::empty(),
                                qh,
                            );
                        let scale_i32 = scale.round().max(1.0) as i32;
                        bar.surface.set_buffer_scale(scale_i32);
                        bar.surface.attach(Some(&wl_buffer), 0, 0);
                        bar.surface.damage_buffer(0, 0, phys_width, phys_height);

                        let region_key = if visible { 1 } else { 0 };
                        if bar.input_region_key != region_key {
                            if visible {
                                bar.surface.set_input_region(None);
                            } else {
                                let region = compositor_state.wl_compositor().create_region(qh, ());
                                bar.surface.set_input_region(Some(&region));
                                region.destroy();
                            }
                            bar.input_region_key = region_key;
                        }

                        bar.surface.commit();
                        bar.current_buffer = None;
                        true
                    }
                    Err(_) => false,
                },
                Err(_) => false,
            }
        } else {
            false
        }
    };

    // Fallback: CPU rendering if GPU failed
    if !gpu_success {
        render_status_widget_bar(
            &mut bar.cached_content,
            phys_width as u32,
            phys_height as u32,
            scale,
            alpha,
            bluetooth,
        );

        let (buffer, canvas) = match pool.create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        ) {
            Ok(bc) => bc,
            Err(e) => {
                tracing::warn!("Failed to allocate status widget bar buffer: {}", e);
                return;
            }
        };
        if !super::copy_popup_content(canvas, &bar.cached_content) {
            return;
        }

        bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
        if let Err(e) = buffer.attach_to(&bar.surface) {
            tracing::warn!("Failed to attach status widget bar buffer: {}", e);
            return;
        }
        bar.surface.damage_buffer(0, 0, phys_width, phys_height);

        let region_key = if visible { 1 } else { 0 };
        if bar.input_region_key != region_key {
            if visible {
                bar.surface.set_input_region(None);
            } else {
                let region = compositor_state.wl_compositor().create_region(qh, ());
                bar.surface.set_input_region(Some(&region));
                region.destroy();
            }
            bar.input_region_key = region_key;
        }

        bar.surface.commit();
        bar.current_buffer = Some(buffer);
    }
}
