//! System tray renderer.
//!
//! Renders StatusNotifierItem icons in the left tray bar.

use crate::models::{BluetoothSnapshot, SystemBarSnapshot, TrayItem, TrayStatus};
use crate::render::font::FontManager;
use crate::render::text::draw_text;

pub const TRAY_ICON_SIZE: i32 = 24;
pub const TRAY_SPACING: i32 = 8;
pub const TRAY_PADDING_X: i32 = 7;
pub const TRAY_PADDING_Y: i32 = 8;
/// Match the compact rounded utility-widget box on the right.
pub const TRAY_BOX_WIDTH: i32 = 76;
pub const TRAY_HEIGHT: i32 = 72;
pub const TRAY_GAP: i32 = 6;
pub const SYSTEM_BAR_HEIGHT: i32 = 34;
pub const SYSTEM_BAR_MIN_WIDTH: i32 = 360;
pub const SIDE_BOX_GAP: i32 = 8;
pub const SIDE_BOX_MIN_WIDTH: i32 = TRAY_BOX_WIDTH;

const KEYBOARD_BADGE_WIDTH: i32 = 34;
const KEYBOARD_BADGE_HEIGHT: i32 = 20;
const GROUP_GAP: i32 = 10;
const AUDIO_BUTTON_WIDTH: i32 = 28;
const BLUETOOTH_BUTTON_WIDTH: i32 = 28;
const RCLONE_BUTTON_WIDTH: i32 = 28;
const PROCESS_BUTTON_WIDTH: i32 = 28;
const BAR_PANEL_HEIGHT: i32 = 34;
const NETWORK_LABEL_WIDTH: i32 = 140;
const DOCK_HORIZONTAL_PADDING: i32 = 24;

/// Computes the main layer-surface width from the application row only.
/// Side boxes are sibling subsurfaces attached outside this width, so the
/// application surface remains compact even when the tray contains many items.
pub fn preferred_surface_width(app_count: usize) -> u32 {
    preferred_surface_width_with_tray(app_count, 0)
}

/// Tray icons and utility widgets are independent side boxes. Keep a 12px
/// margin on both sides of the app row rather than reserving a fixed 480px.
pub fn preferred_surface_width_with_tray(app_count: usize, _tray_item_count: usize) -> u32 {
    let app_count = app_count.max(1) as i32;
    let app_row_width = app_count * (48 + 8) - 8;
    (app_row_width + DOCK_HORIZONTAL_PADDING).max(72) as u32
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrayLayout {
    /// Left edge of the tray panel. Hit-testing uses this same origin.
    pub panel_x: i32,
    pub panel_width: i32,
    pub rclone_x: i32,
    pub process_x: i32,
    pub audio_x: i32,
    pub bluetooth_x: i32,
    pub keyboard_x: i32,
    pub network_x: i32,
    pub left_group_width: i32,
}

pub fn tray_logical_size(item_count: usize) -> (i32, i32) {
    if item_count == 0 {
        return (0, TRAY_HEIGHT);
    }

    // Match the right 76x72 utility box. SNI icons are arranged in two columns
    // and wrap to additional rows only when more than four are registered.
    let rows = (item_count + 1) / 2;
    let content_height =
        rows as i32 * TRAY_ICON_SIZE + rows.saturating_sub(1) as i32 * TRAY_SPACING;
    let height = (content_height + TRAY_PADDING_Y * 2).max(TRAY_HEIGHT);
    (TRAY_BOX_WIDTH, height)
}

/// Returns the icon's logical top-left position in the tray box. Rows are
/// centered vertically; a final single icon is centered horizontally.
pub fn tray_icon_position(index: usize, item_count: usize) -> Option<(i32, i32)> {
    if index >= item_count || item_count == 0 {
        return None;
    }
    let (width, height) = tray_logical_size(item_count);
    let rows = (item_count + 1) / 2;
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

/// StatusNotifierItem icons live in a rounded sibling box to the right of the
/// centered application row. Its vertical center follows the app row.
pub fn tray_bar_position(dock_width: i32, item_count: usize) -> (i32, i32) {
    let (_, height) = tray_logical_size(item_count);
    let app_row_height = crate::state::dock::DOCK_HEIGHT as i32;
    let base_y = (app_row_height - TRAY_HEIGHT) / 2;
    // Keep the bottom edge aligned with the right utility box and above the
    // always-visible system strip. Extra SNI rows expand upward, never downward
    // into the keyboard/network/clock panel.
    let bottom = base_y + TRAY_HEIGHT;
    (dock_width.max(0) + SIDE_BOX_GAP, bottom - height)
}

fn estimate_text_width(text: &str, font_size: f32) -> i32 {
    (text.chars().count() as f32 * font_size * 0.62).ceil() as i32
}

fn network_label_parts(bar: &SystemBarSnapshot) -> (&str, &str) {
    // Keep status-bar text ASCII-only. The old Unicode arrows forced FontManager
    // into runtime Fontconfig/font-collection fallback during the first frame;
    // that synchronous work could stall the only Wayland thread before the
    // initial buffer was committed. Direction is drawn with tiny vector arrows.
    (&bar.download_rate, &bar.upload_rate)
}

fn blend_pixel(canvas: &mut [u8], index: usize, color: [u8; 3], alpha: f32) {
    if index + 3 >= canvas.len() {
        return;
    }
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    // wl_shm ARGB8888 on little-endian is BGRA in memory.
    canvas[index] = (color[2] as f32 * a + canvas[index] as f32 * inv).round() as u8;
    canvas[index + 1] = (color[1] as f32 * a + canvas[index + 1] as f32 * inv).round() as u8;
    canvas[index + 2] = (color[0] as f32 * a + canvas[index + 2] as f32 * inv).round() as u8;
    canvas[index + 3] = 255;
}

fn draw_status_arrow(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    scale: f64,
    down: bool,
    color: [u8; 3],
) {
    let unit = scale.round().max(1.0) as i32;
    let stem_w = 2 * unit;
    let stem_h = 7 * unit;
    let cx = x + 4 * unit;
    let shaft_y = if down { y } else { y + 4 * unit };
    for py in shaft_y..shaft_y + stem_h {
        for px in cx - stem_w / 2..cx + (stem_w + 1) / 2 {
            if px >= 0 && py >= 0 && px < width && py < height {
                blend_pixel(canvas, ((py * width + px) * 4) as usize, color, 1.0);
            }
        }
    }

    let tip_y = if down { y + stem_h + 4 * unit } else { y };
    for row in 0..(4 * unit) {
        let half = (row + unit) / unit;
        let py = if down {
            tip_y - 4 * unit + row
        } else {
            tip_y + row
        };
        for px in cx - half * unit..=cx + half * unit {
            if px >= 0 && py >= 0 && px < width && py < height {
                blend_pixel(canvas, ((py * width + px) * 4) as usize, color, 1.0);
            }
        }
    }
}

pub fn tray_layout(width: i32, item_count: usize, _bar: &SystemBarSnapshot) -> TrayLayout {
    // Keep SNI icons at the left edge of the status strip. System indicators
    // (keyboard, network and clock) form one compact group aligned to the
    // right; they no longer force a huge left-of-centre panel.
    let (tray_width, _) = tray_logical_size(item_count);
    let panel_x = 8;
    let panel_width = if tray_width > 0 { tray_width } else { 0 };

    let clock_width = estimate_text_width("30 Oct", 12.0) + 8 + estimate_text_width("23:59", 14.0);
    let right_margin = 14;
    let clock_left = width - right_margin - clock_width;
    let network_width = NETWORK_LABEL_WIDTH;
    let network_x = clock_left - GROUP_GAP - network_width;
    let keyboard_x = network_x - GROUP_GAP - KEYBOARD_BADGE_WIDTH;
    // The four interactive widgets are no longer placed in this parent
    // surface. They live in a dedicated subsurface to the right of the dock.
    TrayLayout {
        panel_x,
        panel_width,
        rclone_x: 0,
        process_x: 0,
        audio_x: 0,
        bluetooth_x: 0,
        keyboard_x,
        network_x,
        left_group_width: tray_width,
    }
}

/// Logical width for the passive system strip below the app row. Keep enough
/// room for keyboard layout, network rates and the clock even when the app row
/// is narrower than the strip's preferred content width.
pub fn system_bar_logical_width(dock_width: i32) -> i32 {
    dock_width.max(SYSTEM_BAR_MIN_WIDTH)
}

/// Position the system strip centered beneath the app row. Subsurface
/// coordinates are relative to the main dock surface; the strip begins below
/// the 60px app row with the configured vertical gap.
pub fn system_bar_position(dock_width: i32) -> (i32, i32) {
    let strip_width = system_bar_logical_width(dock_width);
    (
        (dock_width - strip_width) / 2,
        crate::state::dock::DOCK_HEIGHT as i32 + TRAY_GAP,
    )
}

pub fn system_bar_minimum_width(_bar: &SystemBarSnapshot, item_count: usize) -> i32 {
    // Minimum needed to fit the tray at the left and the complete status group
    // at the right without overlap. The dock app row normally determines the
    // final width; the status strip is a top-band overlay, not a second dock.
    let (tray_width, _) = tray_logical_size(item_count);
    let clock_width = estimate_text_width("30 Oct", 12.0) + 8 + estimate_text_width("23:59", 14.0);
    let right_group_width =
        KEYBOARD_BADGE_WIDTH + GROUP_GAP + NETWORK_LABEL_WIDTH + GROUP_GAP + clock_width + 14 + 8;
    tray_width + right_group_width
}

pub fn tray_item_at(x: i32, y: i32, item_count: usize) -> Option<usize> {
    let (_, panel_height) = tray_logical_size(item_count);
    if item_count == 0 || x < 0 || y < 0 || y >= panel_height {
        return None;
    }

    (0..item_count).find(|index| {
        tray_icon_position(*index, item_count).is_some_and(|(icon_x, icon_y)| {
            (icon_x..icon_x + TRAY_ICON_SIZE).contains(&x)
                && (icon_y..icon_y + TRAY_ICON_SIZE).contains(&y)
        })
    })
}

pub fn audio_button_at(
    x: i32,
    y: i32,
    width: i32,
    item_count: usize,
    bar: &SystemBarSnapshot,
) -> bool {
    let layout = tray_layout(width, item_count, bar);
    (layout.audio_x..layout.audio_x + AUDIO_BUTTON_WIDTH).contains(&x)
        && (3..TRAY_HEIGHT - 3).contains(&y)
}

pub fn bluetooth_button_at(
    x: i32,
    y: i32,
    width: i32,
    item_count: usize,
    bar: &SystemBarSnapshot,
) -> bool {
    let layout = tray_layout(width, item_count, bar);
    (layout.bluetooth_x..layout.bluetooth_x + BLUETOOTH_BUTTON_WIDTH).contains(&x)
        && (3..TRAY_HEIGHT - 3).contains(&y)
}

pub fn rclone_button_at(
    x: i32,
    y: i32,
    width: i32,
    item_count: usize,
    bar: &SystemBarSnapshot,
) -> bool {
    let layout = tray_layout(width, item_count, bar);
    (layout.rclone_x..layout.rclone_x + RCLONE_BUTTON_WIDTH).contains(&x)
        && (3..TRAY_HEIGHT - 3).contains(&y)
}

pub fn process_button_at(
    x: i32,
    y: i32,
    width: i32,
    item_count: usize,
    bar: &SystemBarSnapshot,
) -> bool {
    let layout = tray_layout(width, item_count, bar);
    (layout.process_x..layout.process_x + PROCESS_BUTTON_WIDTH).contains(&x)
        && (3..TRAY_HEIGHT - 3).contains(&y)
}

fn draw_rounded_panel(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    panel_width: i32,
    panel_height: i32,
    scale: f64,
    alpha: f32,
    color: [u8; 3],
) {
    draw_rounded_panel_with_radius(
        canvas,
        width,
        height,
        x,
        y,
        panel_width,
        panel_height,
        scale,
        5.0,
        alpha,
        color,
    );
}

fn draw_rounded_panel_with_radius(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    panel_width: i32,
    panel_height: i32,
    scale: f64,
    radius_logical: f64,
    alpha: f32,
    color: [u8; 3],
) {
    if panel_width <= 0 || panel_height <= 0 || width <= 0 || height <= 0 {
        return;
    }
    let x = (x as f64 * scale).round() as i32;
    let y = (y as f64 * scale).round() as i32;
    let panel_width = (panel_width as f64 * scale).round().max(1.0) as i32;
    let panel_height = (panel_height as f64 * scale).round().max(1.0) as i32;
    let radius = (radius_logical * scale).round().max(1.0) as i32;
    let alpha = alpha.clamp(0.0, 1.0);

    for py in 0..panel_height {
        let dy = if py < radius {
            radius - py
        } else if py >= panel_height - radius {
            py - (panel_height - radius - 1)
        } else {
            0
        };
        for px in 0..panel_width {
            let dx = if px < radius {
                radius - px
            } else if px >= panel_width - radius {
                px - (panel_width - radius - 1)
            } else {
                0
            };
            if dx > 0 && dy > 0 && dx * dx + dy * dy > radius * radius {
                continue;
            }
            let dst_x = x + px;
            let dst_y = y + py;
            if dst_x < 0 || dst_y < 0 || dst_x >= width || dst_y >= height {
                continue;
            }
            let idx = ((dst_y * width + dst_x) * 4) as usize;
            if idx + 3 >= canvas.len() {
                continue;
            }
            let inv = 1.0 - alpha;
            canvas[idx] = (color[2] as f32 * alpha + canvas[idx] as f32 * inv) as u8;
            canvas[idx + 1] = (color[1] as f32 * alpha + canvas[idx + 1] as f32 * inv) as u8;
            canvas[idx + 2] = (color[0] as f32 * alpha + canvas[idx + 2] as f32 * inv) as u8;
            canvas[idx + 3] = 255;
        }
    }
}

fn measure_text(font_manager: &mut FontManager, text: &str, size: f32) -> i32 {
    text.chars()
        .map(|character| {
            font_manager
                .get_glyph(character, size)
                .advance_width
        })
        .sum::<f32>()
        .round() as i32
}

fn draw_missing_tray_icon(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    size: i32,
    scale: f64,
    alpha: f32,
    _label: &str,
    _font_manager: &mut FontManager,
) {
    draw_rounded_panel(
        canvas,
        width,
        height,
        x,
        y,
        size,
        size,
        scale,
        alpha,
        [48, 46, 55],
    );
    // The reference's small loader treatment is used for unresolved tray icons.
    // Keep this static so a missing desktop icon cannot keep the render loop hot.
    let cx = ((x as f64 + size as f64 / 2.0) * scale).round() as i32;
    let cy = ((y as f64 + size as f64 / 2.0) * scale).round() as i32;
    let radius = (size as f64 * scale / 3.2).round().max(3.0) as i32;
    let dot_r = (1.5 * scale).round().max(1.0) as i32;
    let dots = [
        (0, -radius, 1.0),
        (radius, 0, 0.78),
        (0, radius, 0.56),
        (-radius, 0, 0.36),
        (radius / 2, -radius / 2, 0.9),
        (radius / 2, radius / 2, 0.68),
        (-radius / 2, radius / 2, 0.46),
        (-radius / 2, -radius / 2, 0.30),
    ];
    for (dx, dy, strength) in dots {
        for oy in -dot_r..=dot_r {
            for ox in -dot_r..=dot_r {
                if ox * ox + oy * oy > dot_r * dot_r {
                    continue;
                }
                let px = cx + dx + ox;
                let py = cy + dy + oy;
                if px < 0 || py < 0 || px >= width || py >= height {
                    continue;
                }
                blend_pixel(
                    canvas,
                    ((py * width + px) * 4) as usize,
                    [250, 204, 21],
                    alpha * strength,
                );
            }
        }
    }
}

/// Renders native SNI icons plus keyboard, network and clock indicators in
/// the main dock's top band. Audio/Bluetooth/rclone/process controls are kept
/// in a separate right-hand subsurface.
pub fn render_tray_into_canvas(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale: f64,
    items: &[TrayItem],
    bar: &SystemBarSnapshot,
    _bluetooth: &BluetoothSnapshot,
    tray_y: i32,
    alpha: f32,
    font_manager: &mut FontManager,
    debug: bool,
) {
    if phys_width == 0 || phys_height == 0 {
        return;
    }

    let scale = scale.max(0.25);
    let width = phys_width as i32;
    let height = phys_height as i32;
    let layout = tray_layout((width as f64 / scale).round() as i32, items.len(), bar);
    if debug {}
    let left_x = layout.panel_x;
    let panel_y = tray_y + 2;
    draw_rounded_panel(
        canvas,
        width,
        height,
        left_x,
        panel_y,
        layout.panel_width,
        BAR_PANEL_HEIGHT,
        scale,
        alpha * 0.92,
        [28, 27, 33],
    );
    if debug {}

    // Keyboard layout badge.
    let keyboard_x = layout.keyboard_x;
    let keyboard_y = tray_y + (TRAY_HEIGHT - KEYBOARD_BADGE_HEIGHT) / 2;
    draw_rounded_panel(
        canvas,
        width,
        height,
        keyboard_x,
        keyboard_y,
        KEYBOARD_BADGE_WIDTH,
        KEYBOARD_BADGE_HEIGHT,
        scale,
        alpha,
        [48, 46, 55],
    );
    let keyboard = if bar.keyboard_layout.is_empty() {
        "--"
    } else {
        &bar.keyboard_layout
    };
    let keyboard_size = (11.0 * scale) as i32;
    let keyboard_text_width = measure_text(font_manager, keyboard, keyboard_size as f32);
    let keyboard_text_x = (keyboard_x as f64 * scale).round() as i32
        + (((KEYBOARD_BADGE_WIDTH as f64 * scale).round() as i32 - keyboard_text_width) / 2);
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        keyboard,
        keyboard_size,
        keyboard_text_x,
        ((tray_y + 24) as f64 * scale).round() as i32,
        (250, 204, 21),
    );
    if debug {}

    // Network rates immediately follow the keyboard indicator. The arrows are
    // drawn as vector pixels, not font glyphs, so no fc-match or font parsing
    // can happen on the Wayland/render thread.
    let (download, upload) = network_label_parts(bar);
    let network_x = (layout.network_x as f64 * scale).round() as i32;
    let network_y = ((tray_y + 12) as f64 * scale).round() as i32;
    let network_size = (11.0 * scale) as i32;
    let network_baseline = ((tray_y + 24) as f64 * scale).round() as i32;
    draw_status_arrow(
        canvas,
        width,
        height,
        network_x,
        network_y,
        scale,
        true,
        [250, 204, 21],
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        download,
        network_size,
        network_x + (12.0 * scale) as i32,
        network_baseline,
        (224, 219, 201),
    );
    let upload_x = network_x + (76.0 * scale) as i32;
    draw_status_arrow(
        canvas,
        width,
        height,
        upload_x,
        network_y,
        scale,
        false,
        [250, 204, 21],
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        upload,
        network_size,
        upload_x + (12.0 * scale) as i32,
        network_baseline,
        (224, 219, 201),
    );
    if debug {}

    // Date is placed to the left of the time; the pair is right-aligned to the
    // dock surface instead of depending on the current tray item count.
    let date = if bar.date.is_empty() { "--" } else { &bar.date };
    let time = if bar.time.is_empty() {
        "--:--"
    } else {
        &bar.time
    };
    let date_size = (12.0 * scale) as i32;
    let time_size = (14.0 * scale) as i32;
    let date_width = measure_text(font_manager, date, date_size as f32);
    let time_width = measure_text(font_manager, time, time_size as f32);
    let weather = "--°"; // No weather provider is configured in this dock.
    let weather_size = (10.0 * scale).round().max(8.0) as i32;
    let weather_width = measure_text(font_manager, weather, weather_size as f32);
    let right_margin = (14.0 * scale).round() as i32;
    let pair_gap = (8.0 * scale).round() as i32;
    let time_x = width - right_margin - time_width;
    let date_x = time_x - pair_gap - date_width;
    let weather_x = date_x - pair_gap - weather_width;
    let clock_panel_x = ((weather_x - (8.0 * scale).round() as i32) as f64 / scale).round() as i32;
    let clock_panel_width =
        ((width - clock_panel_x - (6.0 * scale).round() as i32) as f64 / scale).round() as i32;
    draw_rounded_panel(
        canvas,
        width,
        height,
        clock_panel_x,
        panel_y,
        clock_panel_width,
        BAR_PANEL_HEIGHT,
        scale,
        alpha * 0.92,
        [250, 204, 21],
    );
    let baseline = ((tray_y + 24) as f64 * scale).round() as i32;
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        weather,
        weather_size,
        weather_x,
        baseline,
        (93, 49, 20),
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        date,
        date_size,
        date_x,
        baseline,
        (77, 57, 17),
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        time,
        time_size,
        time_x,
        baseline,
        (34, 30, 24),
    );
    if debug {}

    // Tray item icons occupy the leading portion of the left status group.
    for (index, item) in items.iter().enumerate() {
        let logical_x = left_x + TRAY_PADDING_X + index as i32 * (TRAY_ICON_SIZE + TRAY_SPACING);
        let logical_icon_y = tray_y + 7;
        let phys_x = (logical_x as f64 * scale).round() as i32;
        let phys_y = (logical_icon_y as f64 * scale).round() as i32;
        let icon_size = (TRAY_ICON_SIZE as f64 * scale).round().max(1.0) as i32;

        let mut drew_icon_pixel = false;
        if let Some(icon) = &item.icon {
            let src_w = icon.width.max(1) as usize;
            let src_h = icon.height.max(1) as usize;
            if icon.rgba.len() >= src_w.saturating_mul(src_h).saturating_mul(4) {
                for y in 0..icon_size {
                    let src_y = (((y as f32 + 0.5) * src_h as f32 / icon_size as f32) - 0.5)
                        .clamp(0.0, src_h.saturating_sub(1) as f32);
                    let y0 = src_y.floor() as usize;
                    let y1 = (y0 + 1).min(src_h.saturating_sub(1));
                    let ty = src_y - y0 as f32;
                    for x in 0..icon_size {
                        let dst_x = phys_x + x;
                        let dst_y = phys_y + y;
                        if dst_x < 0 || dst_y < 0 || dst_x >= width || dst_y >= height {
                            continue;
                        }
                        let src_x = (((x as f32 + 0.5) * src_w as f32 / icon_size as f32) - 0.5)
                            .clamp(0.0, src_w.saturating_sub(1) as f32);
                        let x0 = src_x.floor() as usize;
                        let x1 = (x0 + 1).min(src_w.saturating_sub(1));
                        let tx = src_x - x0 as f32;
                        let idx00 = (y0 * src_w + x0) * 4;
                        let idx10 = (y0 * src_w + x1) * 4;
                        let idx01 = (y1 * src_w + x0) * 4;
                        let idx11 = (y1 * src_w + x1) * 4;
                        if [idx00, idx10, idx01, idx11]
                            .iter()
                            .any(|idx| idx + 3 >= icon.rgba.len())
                        {
                            continue;
                        }
                        let mut sample = [0.0f32; 4];
                        for channel in 0..4 {
                            let top = icon.rgba[idx00 + channel] as f32 * (1.0 - tx)
                                + icon.rgba[idx10 + channel] as f32 * tx;
                            let bottom = icon.rgba[idx01 + channel] as f32 * (1.0 - tx)
                                + icon.rgba[idx11 + channel] as f32 * tx;
                            sample[channel] = top * (1.0 - ty) + bottom * ty;
                        }
                        let dst_idx = ((dst_y * width + dst_x) * 4) as usize;
                        if dst_idx + 3 >= canvas.len() {
                            continue;
                        }
                        let src_alpha = sample[3] / 255.0 * alpha.clamp(0.0, 1.0);
                        if src_alpha > 0.01 {
                            drew_icon_pixel = true;
                        }
                        let inv = 1.0 - src_alpha;
                        // Canonical source is RGBA; wl_shm ARGB8888 on little-endian
                        // systems is represented in memory as BGRA.
                        canvas[dst_idx] =
                            (sample[2] * src_alpha + canvas[dst_idx] as f32 * inv) as u8;
                        canvas[dst_idx + 1] =
                            (sample[1] * src_alpha + canvas[dst_idx + 1] as f32 * inv) as u8;
                        canvas[dst_idx + 2] =
                            (sample[0] * src_alpha + canvas[dst_idx + 2] as f32 * inv) as u8;
                        canvas[dst_idx + 3] = 255;
                    }
                }
            }
        }

        // Do not silently leave a registered tray item invisible when an SNI
        // implementation supplies neither a usable pixmap nor a resolvable
        // theme icon. Render a compact identifier tile as a last-resort glyph.
        if !drew_icon_pixel {
            let label = if !item.id.is_empty() {
                item.id.as_str()
            } else if !item.title.is_empty() {
                item.title.as_str()
            } else {
                item.service_name.as_str()
            };
            draw_missing_tray_icon(
                canvas,
                width,
                height,
                logical_x,
                logical_icon_y,
                TRAY_ICON_SIZE,
                scale,
                alpha,
                label,
                font_manager,
            );
        }

        if item.status == TrayStatus::NeedsAttention {
            let dot_r = (2.5 * scale).round().max(1.0) as i32;
            let cx = phys_x + icon_size - dot_r - (1.0 * scale).round() as i32;
            let cy = phys_y + dot_r + (1.0 * scale).round() as i32;
            for dy in -dot_r..=dot_r {
                for dx in -dot_r..=dot_r {
                    if dx * dx + dy * dy > dot_r * dot_r {
                        continue;
                    }
                    let px = cx + dx;
                    let py = cy + dy;
                    if px < 0 || py < 0 || px >= width || py >= height {
                        continue;
                    }
                    let idx = ((py * width + px) * 4) as usize;
                    if idx + 3 < canvas.len() {
                        // Signal red attention dot, in BGRA byte order.
                        canvas[idx] = 0x26;
                        canvas[idx + 1] = 0x26;
                        canvas[idx + 2] = 0xDC;
                        canvas[idx + 3] = 255;
                    }
                }
            }
        }
    }
    if debug {}
}

/// Draws only StatusNotifierItem icons. This renderer targets the independent
/// left-side tray subsurface; system indicators are rendered separately below.
pub fn render_tray_icons_into_canvas(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale: f64,
    items: &[TrayItem],
    alpha: f32,
    font_manager: &mut FontManager,
    debug: bool,
) {
    if phys_width == 0 || phys_height == 0 || items.is_empty() {
        return;
    }
    let scale = scale.max(0.25);
    let width = phys_width as i32;
    let height = phys_height as i32;
    let (logical_width, logical_height) = tray_logical_size(items.len());
    if debug {}
    // Match the right-hand utility panel: same 76x72 footprint, full rounded
    // silhouette, corner radius, and dark panel treatment.
    draw_rounded_panel_with_radius(
        canvas,
        width,
        height,
        0,
        0,
        logical_width,
        logical_height,
        scale,
        7.0,
        alpha * 0.96,
        [28, 27, 33],
    );

    for (index, item) in items.iter().enumerate() {
        let (logical_x, logical_y) = tray_icon_position(index, items.len()).unwrap_or((0, 0));
        let phys_x = (logical_x as f64 * scale).round() as i32;
        let phys_y = (logical_y as f64 * scale).round() as i32;
        let icon_size = (TRAY_ICON_SIZE as f64 * scale).round().max(1.0) as i32;
        let mut drew_icon_pixel = false;

        if let Some(icon) = &item.icon {
            let src_w = icon.width.max(1) as usize;
            let src_h = icon.height.max(1) as usize;
            if icon.rgba.len() >= src_w.saturating_mul(src_h).saturating_mul(4) {
                for y in 0..icon_size {
                    let src_y = (((y as f32 + 0.5) * src_h as f32 / icon_size as f32) - 0.5)
                        .clamp(0.0, src_h.saturating_sub(1) as f32);
                    let y0 = src_y.floor() as usize;
                    let y1 = (y0 + 1).min(src_h.saturating_sub(1));
                    let ty = src_y - y0 as f32;
                    for x in 0..icon_size {
                        let dst_x = phys_x + x;
                        let dst_y = phys_y + y;
                        if dst_x < 0 || dst_y < 0 || dst_x >= width || dst_y >= height {
                            continue;
                        }
                        let src_x = (((x as f32 + 0.5) * src_w as f32 / icon_size as f32) - 0.5)
                            .clamp(0.0, src_w.saturating_sub(1) as f32);
                        let x0 = src_x.floor() as usize;
                        let x1 = (x0 + 1).min(src_w.saturating_sub(1));
                        let tx = src_x - x0 as f32;
                        let idx00 = (y0 * src_w + x0) * 4;
                        let idx10 = (y0 * src_w + x1) * 4;
                        let idx01 = (y1 * src_w + x0) * 4;
                        let idx11 = (y1 * src_w + x1) * 4;
                        if [idx00, idx10, idx01, idx11]
                            .iter()
                            .any(|idx| idx + 3 >= icon.rgba.len())
                        {
                            continue;
                        }
                        let mut sample = [0.0f32; 4];
                        for channel in 0..4 {
                            let top = icon.rgba[idx00 + channel] as f32 * (1.0 - tx)
                                + icon.rgba[idx10 + channel] as f32 * tx;
                            let bottom = icon.rgba[idx01 + channel] as f32 * (1.0 - tx)
                                + icon.rgba[idx11 + channel] as f32 * tx;
                            sample[channel] = top * (1.0 - ty) + bottom * ty;
                        }
                        let dst_idx = ((dst_y * width + dst_x) * 4) as usize;
                        if dst_idx + 3 >= canvas.len() {
                            continue;
                        }
                        let src_alpha = sample[3] / 255.0 * alpha.clamp(0.0, 1.0);
                        drew_icon_pixel |= src_alpha > 0.01;
                        let inv = 1.0 - src_alpha;
                        canvas[dst_idx] =
                            (sample[2] * src_alpha + canvas[dst_idx] as f32 * inv) as u8;
                        canvas[dst_idx + 1] =
                            (sample[1] * src_alpha + canvas[dst_idx + 1] as f32 * inv) as u8;
                        canvas[dst_idx + 2] =
                            (sample[0] * src_alpha + canvas[dst_idx + 2] as f32 * inv) as u8;
                        canvas[dst_idx + 3] = 255;
                    }
                }
            }
        }

        if !drew_icon_pixel {
            let label = if item.title.is_empty() {
                &item.service_name
            } else {
                &item.title
            };
            draw_missing_tray_icon(
                canvas,
                width,
                height,
                logical_x,
                logical_y,
                TRAY_ICON_SIZE,
                scale,
                alpha,
                label,
                font_manager,
            );
        }
    }
    if debug {}
}

/// Draws keyboard layout, network rates and date/time in a compact panel below
/// the app row. This is passive UI; the owning subsurface has an empty input region.
pub fn render_system_status_into_canvas(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale: f64,
    bar: &SystemBarSnapshot,
    alpha: f32,
    font_manager: &mut FontManager,
    debug: bool,
) {
    if phys_width == 0 || phys_height == 0 {
        return;
    }
    let scale = scale.max(0.25);
    let width = phys_width as i32;
    let height = phys_height as i32;
    let logical_width = (width as f64 / scale).round() as i32;
    draw_rounded_panel(
        canvas,
        width,
        height,
        0,
        0,
        logical_width,
        SYSTEM_BAR_HEIGHT,
        scale,
        alpha * 0.92,
        [28, 27, 33],
    );

    let keyboard = if bar.keyboard_layout.is_empty() {
        "--"
    } else {
        &bar.keyboard_layout
    };
    let badge_x = 8;
    let badge_y = (SYSTEM_BAR_HEIGHT - KEYBOARD_BADGE_HEIGHT) / 2;
    draw_rounded_panel(
        canvas,
        width,
        height,
        badge_x,
        badge_y,
        KEYBOARD_BADGE_WIDTH,
        KEYBOARD_BADGE_HEIGHT,
        scale,
        alpha,
        [48, 46, 55],
    );
    let keyboard_size = (11.0 * scale).round().max(8.0) as i32;
    let keyboard_text_width = measure_text(font_manager, keyboard, keyboard_size as f32);
    let badge_phys_width = (KEYBOARD_BADGE_WIDTH as f64 * scale).round() as i32;
    let keyboard_text_x =
        (badge_x as f64 * scale).round() as i32 + (badge_phys_width - keyboard_text_width) / 2;
    let baseline = ((SYSTEM_BAR_HEIGHT / 2 + 4) as f64 * scale).round() as i32;
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        keyboard,
        keyboard_size,
        keyboard_text_x,
        baseline,
        (250, 204, 21),
    );

    let (download, upload) = network_label_parts(bar);
    let network_x = 58;
    let arrow_y = ((SYSTEM_BAR_HEIGHT / 2 - 8) as f64 * scale).round() as i32;
    let network_size = (11.0 * scale).round().max(8.0) as i32;
    draw_status_arrow(
        canvas,
        width,
        height,
        (network_x as f64 * scale).round() as i32,
        arrow_y,
        scale,
        true,
        [250, 204, 21],
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        download,
        network_size,
        ((network_x + 12) as f64 * scale).round() as i32,
        baseline,
        (224, 219, 201),
    );
    let upload_x = network_x + 76;
    draw_status_arrow(
        canvas,
        width,
        height,
        (upload_x as f64 * scale).round() as i32,
        arrow_y,
        scale,
        false,
        [250, 204, 21],
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        upload,
        network_size,
        ((upload_x + 12) as f64 * scale).round() as i32,
        baseline,
        (224, 219, 201),
    );

    let date = if bar.date.is_empty() { "--" } else { &bar.date };
    let time = if bar.time.is_empty() {
        "--:--"
    } else {
        &bar.time
    };
    let date_size = (12.0 * scale).round().max(9.0) as i32;
    let time_size = (14.0 * scale).round().max(10.0) as i32;
    let date_width = measure_text(font_manager, date, date_size as f32);
    let time_width = measure_text(font_manager, time, time_size as f32);
    let weather = "--°"; // Placeholder until a weather source is configured.
    let weather_size = (10.0 * scale).round().max(8.0) as i32;
    let weather_width = measure_text(font_manager, weather, weather_size as f32);
    let right_margin = (12.0 * scale).round() as i32;
    let pair_gap = (8.0 * scale).round() as i32;
    let time_x = width - right_margin - time_width;
    let date_x = time_x - pair_gap - date_width;
    let weather_x = date_x - pair_gap - weather_width;
    let panel_x = ((weather_x - (7.0 * scale).round() as i32) as f64 / scale).round() as i32;
    let panel_y = ((SYSTEM_BAR_HEIGHT / 2 - 13) as f64 / scale).round() as i32;
    let panel_h = 26;
    let panel_w = logical_width - panel_x - 5;
    draw_rounded_panel(
        canvas,
        width,
        height,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        scale,
        alpha,
        [250, 204, 21],
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        weather,
        weather_size,
        weather_x,
        baseline,
        (93, 49, 20),
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        date,
        date_size,
        date_x,
        baseline,
        (77, 57, 17),
    );
    draw_text(
        canvas,
        width,
        height,
        font_manager,
        time,
        time_size,
        time_x,
        baseline,
        (34, 30, 24),
    );

    if debug {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_box_matches_widget_box_and_grows_by_rows() {
        assert_eq!(tray_logical_size(0), (0, TRAY_HEIGHT));
        assert_eq!(tray_logical_size(1), (76, 72));
        assert_eq!(tray_logical_size(2), (76, 72));
        assert_eq!(tray_logical_size(3), (76, 72));
        assert_eq!(tray_logical_size(4), (76, 72));
        assert_eq!(tray_logical_size(5), (76, 104));
        assert_eq!(TRAY_BOX_WIDTH, 76);
        assert_eq!(TRAY_HEIGHT, crate::render::widget_bar::WIDGET_BAR_HEIGHT);
    }

    #[test]
    fn tray_icons_are_centered_in_a_two_column_box() {
        assert_eq!(tray_icon_position(0, 1), Some((26, 24)));
        assert_eq!(tray_icon_position(0, 2), Some((10, 24)));
        assert_eq!(tray_icon_position(1, 2), Some((42, 24)));
        assert_eq!(tray_icon_position(0, 3), Some((10, 8)));
        assert_eq!(tray_icon_position(1, 3), Some((42, 8)));
        assert_eq!(tray_icon_position(2, 3), Some((26, 40)));
        assert_eq!(tray_icon_position(4, 4), None);
    }

    #[test]
    fn left_tray_panel_has_rounded_transparent_corners() {
        let mut canvas = vec![0u8; 76 * 72 * 4];
        draw_rounded_panel_with_radius(
            &mut canvas,
            76,
            72,
            0,
            0,
            76,
            72,
            1.0,
            7.0,
            1.0,
            [28, 27, 33],
        );
        assert_eq!(
            canvas[3], 0,
            "top-left rounded corner should be transparent"
        );
        let center_alpha = ((36usize * 76 + 38usize) * 4) + 3;
        assert_eq!(canvas[center_alpha], 255, "panel center should be opaque");
    }

    #[test]
    fn tray_hit_test_matches_grid_icon_regions() {
        assert_eq!(tray_item_at(10, 24, 2), Some(0));
        assert_eq!(tray_item_at(33, 24, 2), Some(0));
        assert_eq!(tray_item_at(34, 24, 2), None);
        assert_eq!(tray_item_at(42, 24, 2), Some(1));
        assert_eq!(tray_item_at(26, 40, 3), Some(2));
        assert_eq!(tray_item_at(0, 0, 2), None);
        assert_eq!(tray_item_at(10, 72, 2), None);
    }

    #[test]
    fn tray_box_is_right_of_and_centered_on_the_app_row() {
        let app_width = preferred_surface_width(4) as i32;
        let (x, y) = tray_bar_position(app_width, 2);
        let (_width, height) = tray_logical_size(2);
        assert_eq!(x, app_width + SIDE_BOX_GAP);
        assert_eq!(
            y,
            (crate::state::dock::DOCK_HEIGHT as i32 - TRAY_HEIGHT) / 2 + TRAY_HEIGHT - height
        );
        let (_, tall_y) = tray_bar_position(app_width, 5);
        assert_eq!(tall_y + tray_logical_size(5).1, y + TRAY_HEIGHT);
    }

    #[test]
    fn system_status_bar_is_below_and_centered_under_the_app_row() {
        let (x, y) = system_bar_position(240);
        assert_eq!(system_bar_logical_width(240), SYSTEM_BAR_MIN_WIDTH);
        assert_eq!(x, (240 - SYSTEM_BAR_MIN_WIDTH) / 2);
        assert_eq!(system_bar_logical_width(420), 420);
        assert_eq!(system_bar_position(420).0, 0);
        assert_eq!(y, crate::state::dock::DOCK_HEIGHT as i32 + TRAY_GAP);
    }

    #[test]
    fn side_boxes_are_separate_from_and_adjacent_to_the_app_row() {
        let app_width = preferred_surface_width(4) as i32;
        let (tray_x, _) = tray_bar_position(app_width, 2);
        let (widgets_x, _) = crate::render::widget_bar::widget_bar_position(app_width, 2, 99);

        assert_eq!(tray_x - app_width, SIDE_BOX_GAP);
        assert_eq!(widgets_x, app_width + 2 * SIDE_BOX_GAP + TRAY_BOX_WIDTH);
        assert_eq!(SIDE_BOX_MIN_WIDTH, 76);
        assert_eq!(crate::render::widget_bar::WIDGET_BAR_WIDTH, 76);
        assert_eq!(crate::render::widget_bar::WIDGET_BAR_HEIGHT, TRAY_HEIGHT);
        assert_eq!(app_width, 240);
    }

    #[test]
    fn main_surface_width_is_independent_of_side_boxes() {
        assert_eq!(preferred_surface_width_with_tray(1, 0), 72);
        assert_eq!(preferred_surface_width_with_tray(4, 0), 240);
        assert_eq!(preferred_surface_width_with_tray(10, 0), 576);
        assert_eq!(preferred_surface_width_with_tray(10, 12), 576);
    }
}
