//! Date/time block widget.
//!
//! Renders a configurable block displaying time, day+month, and year via wgpu GPU.

use crate::state::AppState;
use smithay_client_toolkit::{dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool};
use wayland_client::Proxy;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::types::DockInstance;
use std::sync::Mutex;
use wgpu_api::Renderer as WgpuRenderer;

#[derive(Clone, Default)]
struct CachedDateTime {
    minute_key: Option<(i32, i32, i32, i32, i32)>,
    day_key: Option<(i32, i32, i32)>,
    time: String,
    day_month: String,
    year: String,
}

static DATE_TIME_CACHE: Mutex<CachedDateTime> = Mutex::new(CachedDateTime {
    minute_key: None,
    day_key: None,
    time: String::new(),
    day_month: String::new(),
    year: String::new(),
});

fn format_from_tm(local: &libc::tm, fmt: &[u8], buf_len: usize) -> String {
    let mut buf = vec![0i8; buf_len];
    let n = unsafe { libc::strftime(buf.as_mut_ptr(), buf.len(), fmt.as_ptr() as *const _, local) };
    if n == 0 {
        return String::new();
    }
    let bytes = &buf[..n as usize];
    let bytes_u8: &[u8] =
        unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u8, bytes.len()) };
    String::from_utf8_lossy(bytes_u8).trim().to_owned()
}

fn current_local_tm() -> Option<libc::tm> {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    if now == libc::time_t::MAX {
        return None;
    }
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
        None
    } else {
        Some(local)
    }
}

fn cached_date_time_strings() -> (String, String, String) {
    let Some(local) = current_local_tm() else {
        return (String::new(), String::new(), String::new());
    };
    let minute_key = (local.tm_year, local.tm_mon, local.tm_mday, local.tm_hour, local.tm_min);
    let day_key = (local.tm_year, local.tm_mon, local.tm_mday);
    let mut cache = DATE_TIME_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if cache.minute_key != Some(minute_key) {
        cache.time = format_from_tm(&local, b"%H:%M\0", 16);
        cache.minute_key = Some(minute_key);
    }
    if cache.day_key != Some(day_key) {
        cache.day_month = format_from_tm(&local, b"%e %B\0", 32);
        cache.year = format_from_tm(&local, b"'%y\0", 16);
        cache.day_key = Some(day_key);
    }
    (cache.time.clone(), cache.day_month.clone(), cache.year.clone())
}

fn format_time() -> String {
    cached_date_time_strings().0
}

fn format_day_month() -> String {
    cached_date_time_strings().1
}

fn format_year() -> String {
    cached_date_time_strings().2
}

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
    _pool: &mut SlotPool,
    alpha: f32,
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

        // FIX (v1.5.5): Use pooled DMABUF texture instead of allocating a new one.
        // The pool reuses textures across frames, eliminating per-frame Vulkan allocations.
        if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
            if let Ok((texture, export_info)) = dmabuf_pool.acquire_texture(
                adapter, device, qh, dmabuf_state, phys_width as u32, phys_height as u32,
            ) {
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
        }
    }

    // GPU-only: no CPU fallback
    tracing::warn!("GPU rendering failed for date/time block; skipping frame");
}
