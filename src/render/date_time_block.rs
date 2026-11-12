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
use crate::render::color_check::draw_dashed_border;
use crate::render::font::FontManager;
use crate::render::text::draw_text;
use std::sync::Mutex;

/// Logical width of the date/time block widget (from config, fallback default).
pub const DATE_TIME_BLOCK_WIDTH: i32 = 104;
/// Logical height of the date/time block widget (from config, fallback default).
pub const DATE_TIME_BLOCK_HEIGHT: i32 = 62;

#[derive(Clone, Default)]
struct CachedDateTime {
    minute_key: Option<(i32, i32, i32, i32, i32)>, // year, mon, mday, hour, min
    day_key: Option<(i32, i32, i32)>,              // year, mon, mday
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

/// Format a libc::tm using strftime.
///
/// # Safety
/// Calls libc::strftime which requires a valid tm struct and format string.
fn format_from_tm(local: &libc::tm, fmt: &[u8], buf_len: usize) -> String {
    let mut buf = vec![0i8; buf_len];
    let n = unsafe { libc::strftime(buf.as_mut_ptr(), buf.len(), fmt.as_ptr() as *const _, local) };
    if n == 0 {
        return String::new();
    }
    // strftime writes bytes; interpret as UTF-8 lossily.
    // buf[..n] is already a valid &[i8] slice — no unsafe cast needed.
    let bytes = &buf[..n as usize];
    // SAFETY: i8 and u8 have identical memory layouts; this is a safe reinterpretation.
    let bytes_u8: &[u8] =
        unsafe { std::slice::from_raw_parts(bytes.as_ptr() as *const u8, bytes.len()) };
    String::from_utf8_lossy(bytes_u8).trim().to_owned()
}

/// Get the current local time as a libc::tm struct.
///
/// # Safety
/// Calls libc::time and libc::localtime_r which require proper initialization.
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

    let minute_key = (
        local.tm_year,
        local.tm_mon,
        local.tm_mday,
        local.tm_hour,
        local.tm_min,
    );
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

    (
        cache.time.clone(),
        cache.day_month.clone(),
        cache.year.clone(),
    )
}

/// Format the current time as "HH:MM" using libc::strftime.
pub fn format_time() -> String {
    cached_date_time_strings().0
}

/// Format the current day and month as "8 November" using libc::strftime.
pub fn format_day_month() -> String {
    cached_date_time_strings().1
}

/// Format the current year as "'26" using libc::strftime.
pub fn format_year() -> String {
    cached_date_time_strings().2
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
/// - `scale` — scale factor used to scale the time text offset
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all date_time_block_* settings
pub fn render_date_time_block(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f64,
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
    let thickness = config.date_time_block_border_thickness as usize;

    draw_dashed_border(canvas, w, h, dash_len, gap_len, thickness, border_color, a);

    // === Text color for time, date, and year (from config) ===
    let text_color = DockConfig::parse_hex_color(&config.date_time_block_text_color);

    // === Fetch formatted time/date/year once (cached by minute/day) ===
    let (time_str, day_month_str, year_str) = cached_date_time_strings();

    // === Draw current time (HH:MM) centered near the top ===
    let time_font_size = config.date_time_block_time_font_size as usize;
    let time_text_y = config.date_time_block_time_text_y as i32
        + (config.date_time_block_time_offset_y as f64 * scale).round() as i32;
    let mut time_text_width = 0i32;
    for c in time_str.chars() {
        if c == '\u{200b}' || c.is_control() {
            continue;
        }
        let glyph = font_manager.get_glyph(c, time_font_size as f32);
        time_text_width += glyph.advance_width.round() as i32;
    }
    let time_text_x = (width as i32 - time_text_width) / 2;
    draw_text(
        canvas,
        width as i32,
        height as i32,
        font_manager,
        &time_str,
        time_font_size as i32,
        time_text_x,
        time_text_y,
        text_color,
    );

    // === Draw current day+month (e.g., "8 November") centered below the time ===
    let date_font_size = config.date_time_block_date_font_size as usize;
    let day_month_text_y = config.date_time_block_day_month_y as i32;
    let mut day_month_text_width = 0i32;
    for c in day_month_str.chars() {
        if c == '\u{200b}' || c.is_control() {
            continue;
        }
        let glyph = font_manager.get_glyph(c, date_font_size as f32);
        day_month_text_width += glyph.advance_width.round() as i32;
    }
    let day_month_text_x = (width as i32 - day_month_text_width) / 2;
    draw_text(
        canvas,
        width as i32,
        height as i32,
        font_manager,
        &day_month_str,
        date_font_size as i32,
        day_month_text_x,
        day_month_text_y,
        text_color,
    );

    // === Draw current year (e.g., "'26") centered below the day+month ===
    let year_text_y = config.date_time_block_year_y as i32;
    let mut year_text_width = 0i32;
    for c in year_str.chars() {
        if c == '\u{200b}' || c.is_control() {
            continue;
        }
        let glyph = font_manager.get_glyph(c, date_font_size as f32);
        year_text_width += glyph.advance_width.round() as i32;
    }
    let year_text_x = (width as i32 - year_text_width) / 2;
    draw_text(
        canvas,
        width as i32,
        height as i32,
        font_manager,
        &year_str,
        date_font_size as i32,
        year_text_x,
        year_text_y,
        text_color,
    );
}
