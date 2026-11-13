//! Side bar components.
//!
//! Left tray (system tray icons) and bottom system bar (clock, keyboard, network).

use wayland_client::Proxy;

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::QueueHandle;

use crate::models::SystemBarSnapshot;
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

// Tray layout constants
pub const TRAY_ICON_SIZE: i32 = 24;
const TRAY_SPACING: i32 = 8;
const TRAY_PADDING_Y: i32 = 8;
const TRAY_BOX_WIDTH: i32 = 76;
const TRAY_HEIGHT: i32 = 72;
const TRAY_GAP: i32 = 6;
pub const SYSTEM_BAR_HEIGHT: i32 = 34;
const SYSTEM_BAR_MIN_WIDTH: i32 = 360;
const SIDE_BOX_GAP: i32 = 8;

fn tray_logical_size(item_count: usize) -> (i32, i32) {
    if item_count == 0 {
        return (0, TRAY_HEIGHT);
    }
    let rows = item_count.div_ceil(2);
    let content_height =
        rows as i32 * TRAY_ICON_SIZE + rows.saturating_sub(1) as i32 * TRAY_SPACING;
    let height = (content_height + TRAY_PADDING_Y * 2).max(TRAY_HEIGHT);
    (TRAY_BOX_WIDTH, height)
}

fn tray_icon_position(index: usize, item_count: usize) -> Option<(i32, i32)> {
    if index >= item_count || item_count == 0 {
        return None;
    }
    let (width, height) = tray_logical_size(item_count);
    let rows = item_count.div_ceil(2);
    let row = index / 2;
    let first_index_in_row = row * 2;
    let items_in_row = (item_count - first_index_in_row).min(2) as i32;
    let row_width = items_in_row * TRAY_ICON_SIZE + items_in_row.saturating_sub(1) * TRAY_SPACING;
    let column = (index % 2) as i32;
    let x = (width - row_width) / 2 + column * (TRAY_ICON_SIZE + TRAY_SPACING);
    let content_height =
        rows as i32 * TRAY_ICON_SIZE + rows.saturating_sub(1) as i32 * TRAY_SPACING;
    let top = (height - content_height) / 2;
    let y = top + row as i32 * (TRAY_ICON_SIZE + TRAY_SPACING);
    Some((x, y))
}

fn tray_bar_position(dock_width: i32, item_count: usize) -> (i32, i32) {
    let (_, height) = tray_logical_size(item_count);
    let app_row_height = crate::state::dock::DOCK_HEIGHT as i32;
    let base_y = (app_row_height - TRAY_HEIGHT) / 2;
    let bottom = base_y + TRAY_HEIGHT;
    (dock_width.max(0) + SIDE_BOX_GAP, bottom - height)
}

fn system_bar_logical_width(dock_width: i32) -> i32 {
    dock_width.max(SYSTEM_BAR_MIN_WIDTH)
}

fn system_bar_position(dock_width: i32) -> (i32, i32) {
    let strip_width = system_bar_logical_width(dock_width);
    (
        (dock_width - strip_width) / 2,
        crate::state::dock::DOCK_HEIGHT as i32 + TRAY_GAP,
    )
}

fn ensure_surface(
    slot: &mut Option<PopupSurface>,
    parent_surface: &wayland_client::protocol::wl_surface::WlSurface,
    qh: &QueueHandle<AppState>,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    app_id: &'static str,
) -> bool {
    if slot.is_some() {
        return true;
    }
    let Some(sc) = subcompositor else {
        tracing::warn!("Subcompositor not available; {} rendering degraded", app_id);
        return false;
    };
    let surface = compositor.create_surface(qh);
    let subsurface = sc.get_subsurface(&surface, parent_surface, qh, ());
    subsurface.set_desync();
    *slot = Some(PopupSurface {
        subsurface,
        surface,
        current_buffer: None,
        width: 0,
        height: 0,
        current_app_id: Some(app_id.into()),
        position: (0, 0),
        cached_content: Vec::new(),
        cached_phys_width: 0,
        cached_phys_height: 0,
        input_region_key: u64::MAX,
        // FIX (v1.5.5): Initialize last_content_hash to 0 for content-hash caching.
        last_content_hash: 0,
        last_texture_generation: 0,
    });
    true
}

fn set_interactive_region(
    bar: &mut PopupSurface,
    visible: bool,
    compositor: &CompositorState,
    qh: &QueueHandle<AppState>,
) {
    let key = if visible { 1 } else { 0 };
    if bar.input_region_key == key {
        return;
    }
    if visible {
        bar.surface.set_input_region(None);
    } else {
        let region = compositor.wl_compositor().create_region(qh, ());
        bar.surface.set_input_region(Some(&region));
        region.destroy();
    }
    bar.input_region_key = key;
}

/// StatusNotifierItem icons in an independent rounded box to the right of the
/// centered app row. It matches the utility box for up to four icons,
/// then grows vertically in two-column rows as more SNI items arrive.
pub fn draw_left_tray_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _pool: &mut SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    items: &[crate::models::TrayItem],
    visible: bool,
    alpha: f32,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    let parent_surface = dock.surface.wl_surface().clone();
    if !ensure_surface(
        &mut dock.tray_bar,
        &parent_surface,
        qh,
        compositor,
        subcompositor,
        "left-status-tray",
    ) {
        return;
    }
    let (logical_width, logical_height) = tray_logical_size(items.len());
    let buffer_width = logical_width.max(1) as u32;
    let (x, y) = tray_bar_position(dock.width as i32, items.len());
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (buffer_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (logical_height as f64 * scale).round().max(1.0) as i32;
    let expected = phys_width as usize * phys_height as usize * 4;

    let bar = match dock.tray_bar.as_mut() {
        Some(bar) => bar,
        None => {
            tracing::warn!("Tray bar not initialized");
            return;
        }
    };
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = buffer_width;
    bar.height = logical_height as u32;
    bar.cached_content.resize(expected, 0);
    bar.cached_content.fill(0);
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    // GPU rendering path via wgpu-api DMA-BUF
    let gpu_success = if !items.is_empty() {
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
            radius: 7.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });

        // Tray icons as textured quads
        for (index, item) in items.iter().enumerate() {
            let (logical_x, logical_y) = tray_icon_position(index, items.len()).unwrap_or((0, 0));
            let phys_x = (logical_x as f64 * scale).round() as f32;
            let phys_y = (logical_y as f64 * scale).round() as f32;
            let icon_size = (TRAY_ICON_SIZE as f64 * scale).round().max(1.0) as f32;

            if let Some(icon) = &item.icon {
                if let Some(ref mut renderer) = gpu_context {
                    let cache_key = format!("tray-icon:{}", item.key);
                    if let Some(texture_id) = renderer.upload_texture_from_pixels_cached(
                        &cache_key,
                        &icon.rgba,
                        icon.width,
                        icon.height,
                    ) {
                        draw_commands.push(wgpu_api::DrawCommand::Image {
                            dest_rect: wgpu_api::Rect {
                                x: phys_x,
                                y: phys_y,
                                width: icon_size,
                                height: icon_size,
                            },
                            texture: texture_id,
                            source_uv: wgpu_api::Rect {
                                x: 0.0,
                                y: 0.0,
                                width: 1.0,
                                height: 1.0,
                            },
                            tint: wgpu_api::Color {
                                r: 1.0,
                                g: 1.0,
                                b: 1.0,
                                a: alpha,
                            },
                        });
                    }
                }
            }
        }

        // FIX (v1.5.5): Use pooled DMABUF texture
        if let Some(ref mut renderer) = gpu_context {
            if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
                match dmabuf_pool.acquire_texture(
                    adapter, device, qh, dmabuf_state, phys_width as u32, phys_height as u32,
                ) {
                    Ok((texture, export_info, slot_handle)) => {
                        if renderer.render_to_texture(
                            &texture, phys_width as u32, phys_height as u32, 1.0, &draw_commands,
                        ).is_err() {
                            false
                        } else {
                            match crate::render::dmabuf_export::create_buffer_from_export(
                                dmabuf_state, qh, &export_info,
                            ) {
                                Ok(wl_buffer) => {
                                    match slot_handle {
                                        Some((k, idx)) => dmabuf_pool.set_buffer_id(k, idx, wl_buffer.id()),
                                        None => dmabuf_pool.set_fallback_buffer_id(
                                            wl_buffer.id(),
                                            (phys_width as u64) * (phys_height as u64) * 4,
                                        ),
                                    }
                                    let scale_i32 = scale.round().max(1.0) as i32;
                                    bar.surface.set_buffer_scale(scale_i32);
                                    bar.surface.attach(Some(&wl_buffer), 0, 0);
                                    bar.surface.damage_buffer(0, 0, phys_width, phys_height);
                                    set_interactive_region(bar, visible && !items.is_empty(), compositor, qh);
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
    } else {
        false
    };

    // GPU-only: no CPU fallback
    if !gpu_success {
        tracing::warn!("GPU rendering failed for left tray bar; skipping frame");
    }
}

/// Keyboard layout, network rates and clock in a passive, always-opaque strip
/// directly under the dock row. It deliberately does not follow the dock fade;
/// its empty input region lets pointer input pass through to the desktop.
pub fn draw_bottom_system_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _pool: &mut SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    snapshot: &SystemBarSnapshot,
    visible: bool,
    alpha: f32,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    let parent_surface = dock.surface.wl_surface().clone();
    if !ensure_surface(
        &mut dock.system_status_bar,
        &parent_surface,
        qh,
        compositor,
        subcompositor,
        "bottom-system-status",
    ) {
        return;
    }
    let scale = dock.scale_factor.max(0.25);
    let logical_width = system_bar_logical_width(dock.width as i32).max(1) as u32;
    let phys_width = (logical_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (SYSTEM_BAR_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let expected = phys_width as usize * phys_height as usize * 4;
    let (x, y) = system_bar_position(dock.width as i32);

    let bar = match dock.system_status_bar.as_mut() {
        Some(bar) => bar,
        None => {
            tracing::warn!("System status bar not initialized");
            return;
        }
    };
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = logical_width;
    bar.height = SYSTEM_BAR_HEIGHT as u32;
    bar.cached_content.resize(expected, 0);
    bar.cached_content.fill(0);
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    // GPU rendering path via wgpu-api DMA-BUF
    let gpu_success = if !visible {
        false
    } else {
        let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
        let scale_f32 = scale as f32;
        let width = phys_width as f32;
        let height = phys_height as f32;

        // Background panel
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
                a: alpha * 0.92,
            },
            radius: 10.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });

        // Keyboard layout badge
        let keyboard = if snapshot.keyboard_layout.is_empty() {
            "--"
        } else {
            &snapshot.keyboard_layout
        };
        let badge_x = 8.0 * scale_f32;
        let badge_y = (SYSTEM_BAR_HEIGHT as f32 - 20.0) / 2.0 * scale_f32;
        let badge_w = 34.0 * scale_f32;
        let badge_h = 20.0 * scale_f32;
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect {
                x: badge_x,
                y: badge_y,
                width: badge_w,
                height: badge_h,
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
        // Keyboard text
        let keyboard_text_x = badge_x + (badge_w - keyboard.len() as f32 * 6.0 * scale_f32) / 2.0;
        let baseline = (SYSTEM_BAR_HEIGHT as f32 / 2.0 + 4.0) * scale_f32;
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(keyboard_text_x, baseline),
            content: keyboard.to_string(),
            font_size: 11.0 * scale_f32,
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });

        // Network download text
        let download = if snapshot.download_rate.is_empty() {
            "0 B/s"
        } else {
            &snapshot.download_rate
        };
        let network_x = 58.0 * scale_f32;
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(network_x + 12.0 * scale_f32, baseline),
            content: download.to_string(),
            font_size: 11.0 * scale_f32,
            color: wgpu_api::Color {
                r: 224.0 / 255.0,
                g: 219.0 / 255.0,
                b: 201.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });

        // Network upload text
        let upload = if snapshot.upload_rate.is_empty() {
            "0 B/s"
        } else {
            &snapshot.upload_rate
        };
        let upload_x = (58.0 + 76.0) * scale_f32;
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(upload_x + 12.0 * scale_f32, baseline),
            content: upload.to_string(),
            font_size: 11.0 * scale_f32,
            color: wgpu_api::Color {
                r: 224.0 / 255.0,
                g: 219.0 / 255.0,
                b: 201.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });

        // Date and time text (right-aligned)
        let date = if snapshot.date.is_empty() {
            "--"
        } else {
            &snapshot.date
        };
        let time = if snapshot.time.is_empty() {
            "--:--"
        } else {
            &snapshot.time
        };
        let right_margin = 12.0 * scale_f32;
        let pair_gap = 8.0 * scale_f32;
        let time_est_width = time.len() as f32 * 8.0 * scale_f32;
        let date_est_width = date.len() as f32 * 7.0 * scale_f32;
        let time_x = width - right_margin - time_est_width;
        let date_x = time_x - pair_gap - date_est_width;

        // Date/time background panel
        let panel_x = (date_x - 7.0 * scale_f32).max(0.0);
        let panel_w = width - panel_x - right_margin;
        let panel_y = (SYSTEM_BAR_HEIGHT as f32 / 2.0 - 13.0) * scale_f32;
        let panel_h = 26.0 * scale_f32;
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect {
                x: panel_x,
                y: panel_y,
                width: panel_w,
                height: panel_h,
            },
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: alpha,
            },
            radius: 7.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });

        // Date text
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(date_x, baseline),
            content: date.to_string(),
            font_size: 12.0 * scale_f32,
            color: wgpu_api::Color {
                r: 77.0 / 255.0,
                g: 57.0 / 255.0,
                b: 17.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });

        // Time text
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(time_x, baseline),
            content: time.to_string(),
            font_size: 14.0 * scale_f32,
            color: wgpu_api::Color {
                r: 34.0 / 255.0,
                g: 30.0 / 255.0,
                b: 24.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });

        // FIX (v1.5.5): Use pooled DMABUF texture
        if let Some(ref mut renderer) = gpu_context {
            if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
                match dmabuf_pool.acquire_texture(
                    adapter, device, qh, dmabuf_state, phys_width as u32, phys_height as u32,
                ) {
                    Ok((texture, export_info, slot_handle)) => {
                        if renderer.render_to_texture(
                            &texture, phys_width as u32, phys_height as u32, 1.0, &draw_commands,
                        ).is_err() {
                            false
                        } else {
                            match crate::render::dmabuf_export::create_buffer_from_export(
                                dmabuf_state, qh, &export_info,
                            ) {
                                Ok(wl_buffer) => {
                                    match slot_handle {
                                        Some((k, idx)) => dmabuf_pool.set_buffer_id(k, idx, wl_buffer.id()),
                                        None => dmabuf_pool.set_fallback_buffer_id(
                                            wl_buffer.id(),
                                            (phys_width as u64) * (phys_height as u64) * 4,
                                        ),
                                    }
                                    let scale_i32 = scale.round().max(1.0) as i32;
                                    bar.surface.set_buffer_scale(scale_i32);
                                    bar.surface.attach(Some(&wl_buffer), 0, 0);
                                    bar.surface.damage_buffer(0, 0, phys_width, phys_height);
                                    set_interactive_region(bar, false, compositor, qh);
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
        tracing::warn!("GPU rendering failed for bottom system bar; skipping frame");
    }
}

