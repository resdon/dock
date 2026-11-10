//! Audio widget renderer.

use crate::models::AudioSnapshot;
use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::render::theme;

// Keep the quick mixer at roughly half the former size in both dimensions.
pub const AUDIO_POPUP_WIDTH: i32 = 180;
pub const AUDIO_POPUP_HEIGHT: i32 = 240;

pub const OUTPUT_SLIDER_Y: i32 = 43;
pub const OUTPUT_MUTE_Y: i32 = 24;
pub const OUTPUT_DEVICE_Y: i32 = 52;
pub const INPUT_SLIDER_Y: i32 = 94;
pub const INPUT_MUTE_Y: i32 = 75;
pub const INPUT_DEVICE_Y: i32 = 103;
pub const DEVICE_ROW_HEIGHT: i32 = 16;
pub const STREAMS_HEADER_Y: i32 = 137;
pub const STREAM_ROW_START_Y: i32 = 143;
pub const STREAM_ROW_HEIGHT: i32 = 31;
pub const MIXER_BUTTON_Y: i32 = 217;

// Header minimize control, in popup-local logical coordinates.
pub const MINIMIZE_BUTTON_X: i32 = 153;
pub const MINIMIZE_BUTTON_Y: i32 = 4;
pub const MINIMIZE_BUTTON_WIDTH: i32 = 20;
pub const MINIMIZE_BUTTON_HEIGHT: i32 = 16;

fn rect(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 4],
) {
    if w <= 0 || h <= 0 {
        return;
    }
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = x.saturating_add(w).min(width);
    let y1 = y.saturating_add(h).min(height);
    if x0 >= x1 || y0 >= y1 || width <= 0 {
        return;
    }

    let row_bytes = (x1 - x0) as usize * 4;
    for py in y0..y1 {
        let start = ((py as usize * width as usize) + x0 as usize) * 4;
        if start >= canvas.len() {
            break;
        }
        let end = start.saturating_add(row_bytes).min(canvas.len() & !3);
        for pixel in canvas[start..end].chunks_exact_mut(4) {
            // wl_shm ARGB8888 on little-endian is BGRA.
            pixel.copy_from_slice(&color);
        }
    }
}

fn label(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    font: &mut FontManager,
    text: &str,
    size: i32,
    x: i32,
    baseline: i32,
    color: (u8, u8, u8),
) {
    draw_text(
        canvas,
        width,
        height,
        font,
        text,
        size.max(7),
        x,
        baseline,
        color,
    );
}

fn short_name(value: &str, max_chars: usize) -> String {
    let compact = value
        .replace("alsa_output.", "")
        .replace("alsa_input.", "")
        .replace("bluez_output.", "BT ")
        .replace("bluez_input.", "BT ");

    let char_count = compact.chars().count();
    if char_count <= max_chars {
        return compact;
    }

    let take = max_chars.saturating_sub(3);
    let mut out = String::with_capacity(compact.len().min(take + 3));
    for (i, c) in compact.chars().enumerate() {
        if i >= take {
            break;
        }
        out.push(c);
    }
    out.push_str("...");
    out
}

fn draw_slider(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    scale: f32,
    x: i32,
    y: i32,
    w: i32,
    value: f32,
    muted: bool,
) {
    let x = (x as f32 * scale).round() as i32;
    let y = (y as f32 * scale).round() as i32;
    let w = (w as f32 * scale).round() as i32;
    let track_h = (3.0 * scale).round().max(2.0) as i32;
    let knob_r = (6.0 * scale).round().max(4.0) as i32;
    rect(
        canvas,
        width,
        height,
        x,
        y - track_h / 2,
        w,
        track_h,
        theme::RAISED_ARGB,
    );
    let fill_w = ((w - knob_r).max(1) as f32 * value.clamp(0.0, 1.0)).round() as i32;
    rect(
        canvas,
        width,
        height,
        x,
        y - track_h / 2,
        fill_w,
        track_h,
        if muted {
            [104, 100, 108, 255]
        } else {
            theme::YELLOW_ARGB
        },
    );
    let knob_x = x + fill_w;
    rect(
        canvas,
        width,
        height,
        knob_x - knob_r / 2,
        y - knob_r / 2,
        knob_r,
        knob_r,
        [232, 229, 219, 255],
    );
}

fn draw_mute_button(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    scale: f32,
    x: i32,
    y: i32,
    muted: bool,
    font: &mut FontManager,
) {
    let x = (x as f32 * scale).round() as i32;
    let y = (y as f32 * scale).round() as i32;
    let w = (22.0 * scale).round() as i32;
    let h = (17.0 * scale).round() as i32;
    theme::fill_rounded(
        canvas,
        width,
        height,
        x,
        y,
        w,
        h,
        (5.0 * scale).round() as i32,
        if muted {
            [90, 36, 39, 255]
        } else {
            theme::INSET_ARGB
        },
    );
    let (r, g, b) = if muted { (255, 150, 140) } else { theme::TEXT };
    label(
        canvas,
        width,
        height,
        font,
        if muted { "M" } else { "U" },
        (9.0 * scale).round() as i32,
        x + (7.0 * scale).round() as i32,
        y + (12.0 * scale).round() as i32,
        (r, g, b),
    );
}

fn draw_device_selector(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    scale: f32,
    y: i32,
    devices: &[crate::models::AudioDevice],
    selected_id: Option<u32>,
    font: &mut FontManager,
) {
    let x = (8.0 * scale).round() as i32;
    let y = (y as f32 * scale).round() as i32;
    let row_w = (164.0 * scale).round() as i32;
    let row_h = (15.0 * scale).round() as i32;
    theme::fill_rounded(
        canvas,
        width,
        height,
        x,
        y,
        row_w,
        row_h,
        (5.0 * scale).round() as i32,
        theme::INSET_ARGB,
    );
    theme::fill_rounded(
        canvas,
        width,
        height,
        x,
        y,
        (17.0 * scale).round() as i32,
        row_h,
        (4.0 * scale).round() as i32,
        theme::RAISED_ARGB,
    );
    theme::fill_rounded(
        canvas,
        width,
        height,
        x + row_w - (17.0 * scale).round() as i32,
        y,
        (17.0 * scale).round() as i32,
        row_h,
        (4.0 * scale).round() as i32,
        theme::RAISED_ARGB,
    );
    let selected = selected_id
        .and_then(|id| devices.iter().find(|device| device.id == id))
        .or_else(|| devices.first());
    label(
        canvas,
        width,
        height,
        font,
        "<",
        (9.0 * scale).round() as i32,
        x + (5.0 * scale).round() as i32,
        y + (11.0 * scale).round() as i32,
        // YELLOW_SOFT in [R, G, B] order for text (theme::_ARGB is [B,G,R,A])
        (255, 226, 106),
    );
    label(
        canvas,
        width,
        height,
        font,
        ">",
        (9.0 * scale).round() as i32,
        x + row_w - (12.0 * scale).round() as i32,
        y + (11.0 * scale).round() as i32,
        // YELLOW_SOFT in [R, G, B] order for text
        (255, 226, 106),
    );
    let name = selected
        .map(|d| short_name(&d.name, 22))
        .unwrap_or_else(|| "No device".to_string());
    label(
        canvas,
        width,
        height,
        font,
        &name,
        (8.0 * scale).round() as i32,
        x + (22.0 * scale).round() as i32,
        y + (11.0 * scale).round() as i32,
        theme::TEXT,
    );
}

fn draw_minimize_button(canvas: &mut [u8], width: i32, height: i32, scale: f32) {
    let x = (MINIMIZE_BUTTON_X as f32 * scale).round() as i32;
    let y = (MINIMIZE_BUTTON_Y as f32 * scale).round() as i32;
    let w = (MINIMIZE_BUTTON_WIDTH as f32 * scale).round() as i32;
    let h = (MINIMIZE_BUTTON_HEIGHT as f32 * scale).round() as i32;
    theme::fill_rounded(
        canvas,
        width,
        height,
        x,
        y,
        w,
        h,
        (5.0 * scale).round() as i32,
        theme::INSET_ARGB,
    );
    let line_w = (8.0 * scale).round().max(3.0) as i32;
    let line_h = (2.0 * scale).round().max(1.0) as i32;
    rect(
        canvas,
        width,
        height,
        x + (w - line_w) / 2,
        y + h / 2,
        line_w,
        line_h,
        theme::YELLOW_ARGB,
    );
}

pub fn render_audio_surface(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f32,
    snapshot: &AudioSnapshot,
    font: &mut FontManager,
) {
    let w = width as i32;
    let h = height as i32;
    if w <= 0 || h <= 0 || canvas.len() < width as usize * height as usize * 4 {
        return;
    }
    canvas.fill(0);
    let s = |v: f32| (v * scale).round() as i32;
    theme::fill_rounded(canvas, w, h, 0, 0, w, h, s(14.0), theme::PANEL_ARGB);
    theme::stroke_rounded(
        canvas,
        w,
        h,
        0,
        0,
        w,
        h,
        s(14.0),
        s(1.0).max(1),
        theme::BORDER_ARGB,
    );
    rect(
        canvas,
        w,
        h,
        s(8.0),
        0,
        s(24.0),
        s(3.0).max(1),
        theme::YELLOW_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        "Audio",
        s(12.0),
        s(8.0),
        s(17.0),
        theme::TEXT,
    );
    label(
        canvas,
        w,
        h,
        font,
        "PIPEWIRE",
        s(7.0),
        s(48.0),
        s(16.0),
        theme::MUTED,
    );
    draw_minimize_button(canvas, w, h, scale);
    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(23.0),
        s(164.0),
        s(1.0),
        theme::HAIRLINE_ARGB,
    );

    if !snapshot.available {
        label(
            canvas,
            w,
            h,
            font,
            "PipeWire unavailable",
            s(10.0),
            s(8.0),
            s(58.0),
            (255, 214, 105),
        );
        label(
            canvas,
            w,
            h,
            font,
            "Check WirePlumber / wpctl",
            s(8.0),
            s(8.0),
            s(75.0),
            theme::MUTED,
        );
        if let Some(error) = snapshot.error.as_deref() {
            label(
                canvas,
                w,
                h,
                font,
                &short_name(error, 28),
                s(7.0),
                s(8.0),
                s(91.0),
                theme::MUTED,
            );
        }
        return;
    }

    // Output master and compact, cycle-through device selector.
    label(
        canvas,
        w,
        h,
        font,
        "OUT",
        s(8.0),
        s(8.0),
        s(35.0),
        (250, 204, 21),
    );
    label(
        canvas,
        w,
        h,
        font,
        &format!("{}%", (snapshot.output_volume * 100.0).round() as u32),
        s(8.0),
        s(124.0),
        s(35.0),
        (224, 219, 201),
    );
    draw_mute_button(
        canvas,
        w,
        h,
        scale,
        151,
        OUTPUT_MUTE_Y,
        snapshot.output_muted,
        font,
    );
    draw_slider(
        canvas,
        w,
        h,
        scale,
        8,
        OUTPUT_SLIDER_Y,
        111,
        snapshot.output_volume,
        snapshot.output_muted,
    );
    draw_device_selector(
        canvas,
        w,
        h,
        scale,
        OUTPUT_DEVICE_Y,
        &snapshot.outputs,
        snapshot.default_output,
        font,
    );

    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(72.0),
        s(164.0),
        s(1.0),
        theme::HAIRLINE_ARGB,
    );

    // Input master and device selector.
    label(
        canvas,
        w,
        h,
        font,
        "IN",
        s(8.0),
        s(8.0),
        s(86.0),
        (250, 204, 21),
    );
    label(
        canvas,
        w,
        h,
        font,
        &format!("{}%", (snapshot.input_volume * 100.0).round() as u32),
        s(8.0),
        s(124.0),
        s(86.0),
        (224, 219, 201),
    );
    draw_mute_button(
        canvas,
        w,
        h,
        scale,
        151,
        INPUT_MUTE_Y,
        snapshot.input_muted,
        font,
    );
    draw_slider(
        canvas,
        w,
        h,
        scale,
        8,
        INPUT_SLIDER_Y,
        111,
        snapshot.input_volume,
        snapshot.input_muted,
    );
    draw_device_selector(
        canvas,
        w,
        h,
        scale,
        INPUT_DEVICE_Y,
        &snapshot.inputs,
        snapshot.default_input,
        font,
    );

    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(123.0),
        s(164.0),
        s(1.0),
        theme::HAIRLINE_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        "APPS",
        s(8.0),
        s(8.0),
        s(STREAMS_HEADER_Y as f32),
        (250, 204, 21),
    );

    if snapshot.streams.is_empty() {
        label(
            canvas,
            w,
            h,
            font,
            "No active streams",
            s(8.0),
            s(8.0),
            s(159.0),
            theme::MUTED,
        );
    } else {
        for (index, stream) in snapshot.streams.iter().take(2).enumerate() {
            let row_y = STREAM_ROW_START_Y + index as i32 * STREAM_ROW_HEIGHT;
            label(
                canvas,
                w,
                h,
                font,
                &short_name(&stream.name, 23),
                s(8.0),
                s(8.0),
                s((row_y + 9) as f32),
                theme::TEXT,
            );
            draw_mute_button(canvas, w, h, scale, 151, row_y, stream.muted, font);
            draw_slider(
                canvas,
                w,
                h,
                scale,
                8,
                row_y + 19,
                111,
                stream.volume,
                stream.muted,
            );
            label(
                canvas,
                w,
                h,
                font,
                &format!("{}%", (stream.volume * 100.0).round() as u32),
                s(7.0),
                s(124.0),
                s((row_y + 22) as f32),
                (224, 219, 201),
            );
        }
    }

    theme::fill_rounded(
        canvas,
        w,
        h,
        s(8.0),
        s(MIXER_BUTTON_Y as f32),
        s(164.0),
        s(17.0),
        s(7.0),
        theme::YELLOW_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        "Open full mixer",
        s(8.0),
        s(54.0),
        s(229.0),
        theme::INK,
    );
}
