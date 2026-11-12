//! Date/time block surface management.
//!
//! Allocates a shared memory buffer, renders the date/time block via the
//! `render_date_time_block` function, and commits the buffer to the layer surface.
//! The block is positioned to the right of the dock as a floating widget.

use crate::state::AppState;
use smithay_client_toolkit::{dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool};
use wayland_client::Proxy;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::render::date_time_block::{
    format_day_month, format_time, format_year, render_date_time_block,
};
use crate::render::font::FontManager;
use crate::types::DockInstance;
use wgpu_api::Renderer as WgpuRenderer;

/// Draws the date/time block widget and commits the buffer.
///
/// The layer surface is pre-created during init; this function renders the
/// dashed border pattern with the current time, day+month, and year,
/// then attaches the buffer each frame.
///
/// # Parameters
///
/// - `dock` — dock instance containing the date/time block layer surface
/// - `pool` — shared memory buffer pool for allocating frame buffers
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all date_time_block_* settings
pub fn draw_date_time_block(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
    mut gpu_context: Option<&mut WgpuRenderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    // Skip rendering if the date/time block is disabled in config
    if !config.date_time_block_enabled {
        return;
    }

    let block = match &mut dock.date_time_block_surface {
        Some(b) => b,
        None => return,
    };

    // Use configured dimensions, falling back to defaults for the surface size constants
    let block_width = config.date_time_block_width.max(1) as i32;
    let block_height = config.date_time_block_height.max(1) as i32;
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (block_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (block_height as f64 * scale).round().max(1.0) as i32;
    let a = alpha.clamp(0.0, 1.0);

    // Try GPU rendering via DMA-BUF first
    if let Some(ref mut renderer) = gpu_context {
        let mut draw_commands = Vec::new();

        // Text: time, day+month, year
        let text_color = DockConfig::parse_hex_color(&config.date_time_block_text_color);
        let tr = text_color.0 as f32 / 255.0;
        let tg = text_color.1 as f32 / 255.0;
        let tb = text_color.2 as f32 / 255.0;
        let text_col = wgpu_api::Color::rgba(tr, tg, tb, a);

        // Text coordinates use the widget's logical-pixel space.
        let time_str = format_time();
        if !time_str.is_empty() {
            let time_font_size = config.date_time_block_time_font_size as f32;
            let time_text_y = config.date_time_block_time_text_y as f32
                + config.date_time_block_time_offset_y as f32;
            draw_commands.push(wgpu_api::DrawCommand::Text {
                content: time_str,
                position: wgpu_api::Point::new(block_width as f32 / 2.0, time_text_y),
                font_size: time_font_size,
                color: text_col,
                align: 0.5,
            });
        }

        // Day+Month — centered below the time.
        let day_month_str = format_day_month();
        if !day_month_str.is_empty() {
            let date_font_size = config.date_time_block_date_font_size as f32;
            let day_month_text_y = config.date_time_block_day_month_y as f32;
            draw_commands.push(wgpu_api::DrawCommand::Text {
                content: day_month_str,
                position: wgpu_api::Point::new(block_width as f32 / 2.0, day_month_text_y),
                font_size: date_font_size,
                color: text_col,
                align: 0.5,
            });
        }

        // Year — centered below the day and month.
        let year_str = format_year();
        if !year_str.is_empty() {
            let date_font_size = config.date_time_block_date_font_size as f32;
            let year_text_y = config.date_time_block_year_y as f32;
            draw_commands.push(wgpu_api::DrawCommand::Text {
                content: year_str,
                position: wgpu_api::Point::new(block_width as f32 / 2.0, year_text_y),
                font_size: date_font_size,
                color: text_col,
                align: 0.5,
            });
        }

        if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
            match dmabuf_pool.acquire_texture(
                adapter, device, qh, dmabuf_state, phys_width as u32, phys_height as u32,
            ) {
                Ok((texture, export_info)) => {
                    if renderer.render_to_texture(
                        &texture, phys_width as u32, phys_height as u32,
                        scale as f32, &draw_commands,
                    ).is_ok() {
                        if let Ok(wl_buffer) = crate::render::dmabuf_export::create_buffer_from_export(
                            dmabuf_state, qh, &export_info,
                        ) {
                            dmabuf_pool.set_buffer_id(wl_buffer.id());
                            block
                                .surface
                                .wl_surface()
                                .set_buffer_scale(scale.round().max(1.0) as i32);
                            block.surface.wl_surface().attach(Some(&wl_buffer), 0, 0);
                            block
                                .surface
                                .wl_surface()
                                .damage_buffer(0, 0, phys_width, phys_height);
                            block.surface.wl_surface().commit();
                            return;
                        }
                    }
                }
                Err(_) => {}
            }
        }
    }

    // CPU fallback rendering
    fallback_date_time_block_cpu(
        dock,
        pool,
        phys_width,
        phys_height,
        scale,
        a,
        font_manager,
        config,
    );
}

/// CPU fallback rendering for the date/time block
fn fallback_date_time_block_cpu(
    dock: &mut DockInstance,
    pool: &mut SlotPool,
    phys_width: i32,
    phys_height: i32,
    scale: f64,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
) {
    let block = match &mut dock.date_time_block_surface {
        Some(b) => b,
        None => return,
    };
    let expected_len = phys_width as usize * phys_height as usize * 4;
    block.cached_content.resize(expected_len, 0);
    block.cached_content.fill(0);

    render_date_time_block(
        &mut block.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale,
        alpha,
        font_manager,
        config,
    );
    block.cached_phys_width = phys_width;
    block.cached_phys_height = phys_height;

    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(b) => b,
        Err(_) => return,
    };

    if !crate::app::copy_popup_content(canvas, &block.cached_content) {
        return;
    }

    block
        .surface
        .wl_surface()
        .set_buffer_scale(scale.round().max(1.0) as i32);
    if buffer.attach_to(block.surface.wl_surface()).is_err() {
        return;
    }
    block
        .surface
        .wl_surface()
        .damage_buffer(0, 0, phys_width, phys_height);
    block.surface.wl_surface().commit();
    block.current_buffer = Some(buffer);
}
