//! Widget bar app component.
//!
//! Right-side utility widget cluster: rclone, process monitor, audio, Bluetooth.

use wayland_client::Proxy;

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::QueueHandle;

use crate::models::BluetoothSnapshot;
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

// Widget bar layout constants
const WIDGET_BAR_WIDTH: i32 = 76;
const WIDGET_BAR_HEIGHT: i32 = 72;
const WIDGET_BAR_GAP: i32 = 8;

fn widget_bar_position(dock_width: i32, tray_item_count: usize, _surface_height: i32) -> (i32, i32) {
    let tray_box_width = 76;
    let app_row_height = crate::state::dock::DOCK_HEIGHT as i32;
    let y = (app_row_height - WIDGET_BAR_HEIGHT) / 2;
    let tray_width = if tray_item_count == 0 { 0 } else { tray_box_width };
    (
        dock_width.max(0)
            + WIDGET_BAR_GAP
            + tray_width
            + if tray_width > 0 { WIDGET_BAR_GAP } else { 0 },
        y,
    )
}

/// Draws the four utility controls in a persistent subsurface to the right of
/// the tray app box.
pub fn draw_status_widget_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    bluetooth: &BluetoothSnapshot,
    tray_item_count: usize,
    visible: bool,
    alpha: f32,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
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
            last_texture_generation: 0,
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

        // FIX (v1.5.5): Use pooled DMABUF texture
        if let Some(ref mut renderer) = gpu_context {
            if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
                match dmabuf_pool.acquire_texture(
                    adapter, device, qh, dmabuf_state, phys_width as u32, phys_height as u32,
                ) {
                    Ok((texture, export_info)) => {
                        if renderer.render_to_texture(
                            &texture, phys_width as u32, phys_height as u32, 1.0, &draw_commands,
                        ).is_err() {
                            false
                        } else {
                            match crate::render::dmabuf_export::create_buffer_from_export(
                                dmabuf_state, qh, &export_info,
                            ) {
                                Ok(wl_buffer) => {
                                    dmabuf_pool.set_buffer_id(wl_buffer.id());
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
        }
    };

    // GPU-only: no CPU fallback
    if !gpu_success {
        tracing::warn!("GPU rendering failed for widget bar; skipping frame");
    }
}
