//! Rclone popup panel.
//!
//! Paginated list of rclone mounts with transfer rate information.
//! Renders via wgpu GPU using DrawCommands and DMA-BUF.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface,
};
use wayland_client::{Proxy, QueueHandle};

use super::draw::DrawFrameData;
use crate::models::{RcloneSnapshot, SystemBarSnapshot, TrayItem};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

// Layout constants for rclone popup
pub const RCLONE_POPUP_WIDTH: i32 = 200;
pub const RCLONE_POPUP_HEIGHT: i32 = 200;

// Widget bar layout constants
const BUTTON_SIZE: i32 = 28;
const BUTTON_XS: [i32; 4] = [7, 41, 7, 41];

fn widget_bar_position(dock_width: i32, tray_item_count: usize, _surface_height: i32) -> (i32, i32) {
    let tray_box_width = 76;
    let app_row_height = crate::state::dock::DOCK_HEIGHT as i32;
    let y = (app_row_height - 72) / 2;
    let tray_width = if tray_item_count == 0 { 0 } else { tray_box_width };
    (
        dock_width.max(0)
            + 8
            + tray_width
            + if tray_width > 0 { 8 } else { 0 },
        y,
    )
}

fn button_center_x(index: usize) -> i32 {
    BUTTON_XS.get(index).copied().unwrap_or(0) + BUTTON_SIZE / 2
}

pub fn draw_rclone_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    _pool: &mut smithay_client_toolkit::shm::slot::SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    is_open: bool,
    snapshot: &RcloneSnapshot,
    page: usize,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    if !is_open {
        if let Some(popup) = dock.rclone_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
        return;
    }
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (RCLONE_POPUP_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (RCLONE_POPUP_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let (bar_x, bar_y) = widget_bar_position(
        frame.dock_width as i32,
        _tray_items.len(),
        frame.surface_height as i32,
    );
    let anchor_x = ((bar_x + button_center_x(0)) as f64 * scale).round() as i32;
    let x = (anchor_x - phys_width / 2).max(0);
    let anchor_y = (bar_y as f64 * scale).round() as i32;
    let y = anchor_y - phys_height;
    let logical_x = (x as f64 / scale).round() as i32;
    let logical_y = (y as f64 / scale).round() as i32;

    if dock.rclone_popup.is_none() {
        let Some(sc) = subcompositor else {
            tracing::warn!("Subcompositor not available; rclone popup rendering degraded");
            return;
        };
        let surface = compositor_state.create_surface(qh);
        let subsurface = sc.get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
        subsurface.set_desync();
        dock.rclone_popup = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("rclone-panel".into()),
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
    let Some(popup) = dock.rclone_popup.as_mut() else {
        tracing::warn!("Rclone popup not initialized");
        return;
    };
    popup.position = (logical_x, logical_y);
    popup.subsurface.set_position(logical_x, logical_y);
    popup.width = RCLONE_POPUP_WIDTH as u32;
    popup.height = RCLONE_POPUP_HEIGHT as u32;

    // GPU rendering via wgpu-api using DrawCommands
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
    let scale_f32 = scale as f32;
    let width = phys_width as f32;
    let height = phys_height as f32;

    // Background panel
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(0.0, 0.0, width, height),
        color: wgpu_api::Color {
            r: 45.0 / 255.0,
            g: 45.0 / 255.0,
            b: 56.0 / 255.0,
            a: 1.0,
        },
        radius: 14.0 * scale_f32,
        stroke: None,
        stroke_width: 0.0,
    });

    // Header
    draw_commands.push(wgpu_api::DrawCommand::Text {
        position: wgpu_api::Point::new(8.0 * scale_f32, 16.0 * scale_f32),
        content: "Rclone".to_string(),
        font_size: 12.0 * scale_f32,
        color: wgpu_api::Color {
            r: 232.0 / 255.0,
            g: 229.0 / 255.0,
            b: 219.0 / 255.0,
            a: 1.0,
        },
        align: 0.0,
    });

    // Separator
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(0.0, 20.0 * scale_f32, width, 1.0 * scale_f32),
        color: wgpu_api::Color {
            r: 80.0 / 255.0,
            g: 80.0 / 255.0,
            b: 90.0 / 255.0,
            a: 1.0,
        },
        radius: 0.0,
        stroke: None,
        stroke_width: 0.0,
    });

    if snapshot.mounts.is_empty() {
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 50.0 * scale_f32),
            content: "No mounts".to_string(),
            font_size: 9.0 * scale_f32,
            color: wgpu_api::Color {
                r: 160.0 / 255.0,
                g: 160.0 / 255.0,
                b: 170.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
    } else {
        let rows_per_page = 8;
        let start = page * rows_per_page;
        let end = (start + rows_per_page).min(snapshot.mounts.len());
        for i in start..end {
            let mount = &snapshot.mounts[i];
            let row_y = (24.0 + (i - start) as f32 * 20.0) * scale_f32;

            // Mount name
            let name = mount.mount_point.chars().take(16).collect::<String>()
                + if mount.mount_point.chars().count() > 16 { "..." } else { "" };
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(8.0 * scale_f32, row_y),
                content: name,
                font_size: 9.0 * scale_f32,
                color: wgpu_api::Color {
                    r: 232.0 / 255.0,
                    g: 229.0 / 255.0,
                    b: 219.0 / 255.0,
                    a: 1.0,
                },
                align: 0.0,
            });
            // Source
            let source = mount.source.chars().take(10).collect::<String>()
                + if mount.source.chars().count() > 10 { "..." } else { "" };
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(100.0 * scale_f32, row_y),
                content: source,
                font_size: 7.0 * scale_f32,
                color: wgpu_api::Color {
                    r: 160.0 / 255.0,
                    g: 160.0 / 255.0,
                    b: 170.0 / 255.0,
                    a: 1.0,
                },
                align: 0.0,
            });
        }
    }

    // Render to GPU texture via DMA-BUF pool
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
                                popup.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                                popup.surface.attach(Some(&wl_buffer), 0, 0);
                                popup.surface.damage_buffer(0, 0, phys_width, phys_height);
                                let region = compositor_state.wl_compositor().create_region(qh, ());
                                region.add(0, 0, RCLONE_POPUP_WIDTH, RCLONE_POPUP_HEIGHT);
                                popup.surface.set_input_region(Some(&region));
                                region.destroy();
                                popup.surface.commit();
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
        tracing::warn!("GPU rendering failed for rclone popup; skipping frame");
    }
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (151..=173).contains(&x) && (4..=20).contains(&y)
}
pub fn page_delta_at(x: f64, y: f64) -> Option<i32> {
    let x = x.round() as i32;
    let y = y.round() as i32;
    if !(151..=173).contains(&x) || !(175..=191).contains(&y) {
        return None;
    }
    if (160..=173).contains(&x) {
        Some(1)
    } else {
        Some(-1)
    }
}
