//! Date/time block widget renderer.
//!
//! Draws a configurable block with dashed borders displaying time (HH:MM),
//! day+month (e.g., "8 November"), and year (e.g., "'26").
//!
//! # Layout
//!
//! The block is rectangular (default 104×62px) with dashed green borders on all four sides.
//! Content is arranged top-to-bottom:
//!
//! 1. **Time** (HH:MM) — centered horizontally, large font (32pt), blue color
//! 2. **Day + Month** (e.g., "8 November") — centered, half the time font size (16pt)
//! 3. **Year** (e.g., "'26") — centered, same font as day+month
//!
//! # Color Format
//!
//! The canvas uses BGRA8888 pixel format. All color writes use the `write_bgra_color` and
//! `write_bgra_pixel` helpers from `color_check` to ensure correct channel order.
//!
//! # Configuration
//!
//! All visual parameters are controlled by `DockConfig` fields prefixed with `date_time_block_`.
//! See `config.rs` for the full list of configurable options.

use crate::config::DockConfig;
use crate::render::color_check::write_bgra_color;
use crate::render::font::FontManager;
use crate::render::text::draw_text;

/// Logical width of the date/time block widget (from config, fallback default).
pub const DATE_TIME_BLOCK_WIDTH: i32 = 104;
/// Logical height of the date/time block widget (from config, fallback default).
pub const DATE_TIME_BLOCK_HEIGHT: i32 = 62;

/// Format the current time as "HH:MM" using libc::strftime.
fn format_time() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
        return String::new();
    }
    let mut buf = [0i8; 16];
    let n = unsafe { libc::strftime(buf.as_mut_ptr(), buf.len(), b"%H:%M\0".as_ptr() as *const _, &local) };
    if n == 0 { return String::new(); }
    String::from_utf8_lossy(&buf[..n as usize].iter().map(|c| *c as u8).collect::<Vec<u8>>()).into_owned()
}

/// Format the current day and month as "8 November" using libc::strftime.
fn format_day_month() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
        return String::new();
    }
    let mut buf = [0i8; 32];
    let n = unsafe { libc::strftime(buf.as_mut_ptr(), buf.len(), b"%e %B\0".as_ptr() as *const _, &local) };
    if n == 0 { return String::new(); }
    String::from_utf8_lossy(&buf[..n as usize].iter().map(|c| *c as u8).collect::<Vec<u8>>()).into_owned()
}

/// Format the current year as "'26" using libc::strftime.
fn format_year() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
        return String::new();
    }
    let mut buf = [0i8; 16];
    let n = unsafe { libc::strftime(buf.as_mut_ptr(), buf.len(), b"'%y\0".as_ptr() as *const _, &local) };
    if n == 0 { return String::new(); }
    String::from_utf8_lossy(&buf[..n as usize].iter().map(|c| *c as u8).collect::<Vec<u8>>()).into_owned()
}

/// Renders the date/time block into a BGRA8888 canvas.
///
/// Draws a dashed border around the block with a transparent interior.
/// The border color, dash pattern, and thickness are all configurable.
/// Inside the block, the current time, day+month, and year are drawn
/// using the configured colors, font sizes, and positions.
///
/// # Parameters
///
/// - `canvas` — mutable slice of BGRA8888 pixels (4 bytes per pixel)
/// - `width` — canvas width in pixels
/// - `height` — canvas height in pixels
/// - `_scale` — scale factor (unused, kept for API consistency)
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all date_time_block_* settings
pub fn render_date_time_block(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    _scale: f64,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
) {
    if canvas.is_empty() || width == 0 || height == 0 {
        return;
    }

    let w = width as usize;
    let h = height as usize;
    let a = (alpha * 255.0) as u8;

    // === Dashed border parameters (from config) ===
    let border_color = DockConfig::parse_hex_color(&config.date_time_block_border_color);
    let dash_len = config.date_time_block_dash_length as usize;
    let gap_len = config.date_time_block_gap_length as usize;
    let cycle = dash_len + gap_len;
    let thickness = config.date_time_block_border_thickness as usize;

    // Draw dashed border on all four sides
    for y in 0..h {
        for x in 0..w {
            let on_border = if y < thickness {
                x % cycle < dash_len
            } else if y >= h - thickness {
                x % cycle < dash_len
            } else if x < thickness {
                y % cycle < dash_len
            } else if x >= w - thickness {
                y % cycle < dash_len
            } else {
                false
            };

            if on_border {
                let idx = (y * w + x) * 4;
                write_bgra_color(canvas, idx, border_color, a);
            }
        }
    }

    // === Text color for time, date, and year (from config) ===
    let text_color = DockConfig::parse_hex_color(&config.date_time_block_text_color);

    // === Draw current time (HH:MM) centered near the top ===
    let time_str = format_time();
    let time_font_size = config.date_time_block_time_font_size as usize;
    let time_text_y = config.date_time_block_time_text_y as i32;
    let mut time_text_width = 0i32;
    for c in time_str.chars() {
        if c == '\u{200b}' || c.is_control() { continue; }
        let glyph = font_manager.get_glyph(c, time_font_size as f32);
        time_text_width += glyph.metrics.advance_width.round() as i32;
    }
    let time_text_x = (width as i32 - time_text_width) / 2;
    draw_text(canvas, width as i32, height as i32, font_manager, &time_str, time_font_size as i32, time_text_x, time_text_y, text_color);

    // === Draw current day+month (e.g., "8 November") centered below the time ===
    let day_month_str = format_day_month();
    let date_font_size = config.date_time_block_date_font_size as usize;
    let day_month_text_y = config.date_time_block_day_month_y as i32;
    let mut day_month_text_width = 0i32;
    for c in day_month_str.chars() {
        if c == '\u{200b}' || c.is_control() { continue; }
        let glyph = font_manager.get_glyph(c, date_font_size as f32);
        day_month_text_width += glyph.metrics.advance_width.round() as i32;
    }
    let day_month_text_x = (width as i32 - day_month_text_width) / 2;
    draw_text(canvas, width as i32, height as i32, font_manager, &day_month_str, date_font_size as i32, day_month_text_x, day_month_text_y, text_color);

    // === Draw current year (e.g., "'26") centered below the day+month ===
    let year_str = format_year();
    let year_text_y = config.date_time_block_year_y as i32;
    let mut year_text_width = 0i32;
    for c in year_str.chars() {
        if c == '\u{200b}' || c.is_control() { continue; }
        let glyph = font_manager.get_glyph(c, date_font_size as f32);
        year_text_width += glyph.metrics.advance_width.round() as i32;
    }
    let year_text_x = (width as i32 - year_text_width) / 2;
    draw_text(canvas, width as i32, height as i32, font_manager, &year_str, date_font_size as i32, year_text_x, year_text_y, text_color);
}
