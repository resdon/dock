//! Bluetooth widget renderer.

use crate::models::BluetoothSnapshot;
use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::render::theme;

pub const BLUETOOTH_POPUP_WIDTH: i32 = 220;
pub const BLUETOOTH_POPUP_HEIGHT: i32 = 240;

pub const HEADER_MINIMIZE_X: i32 = 193;
pub const HEADER_MINIMIZE_Y: i32 = 4;
pub const HEADER_MINIMIZE_W: i32 = 20;
pub const HEADER_MINIMIZE_H: i32 = 16;

pub const POWER_BUTTON_X: i32 = 8;
pub const POWER_BUTTON_Y: i32 = 29;
pub const POWER_BUTTON_W: i32 = 82;
pub const POWER_BUTTON_H: i32 = 22;
pub const SCAN_BUTTON_X: i32 = 96;
pub const SCAN_BUTTON_Y: i32 = 29;
pub const SCAN_BUTTON_W: i32 = 116;
pub const SCAN_BUTTON_H: i32 = 22;

pub const DEVICE_LIST_Y: i32 = 70;
pub const DEVICE_ROW_HEIGHT: i32 = 38;
pub const DEVICE_ROW_COUNT: usize = 4;
pub const DEVICE_ACTION_X: i32 = 158;
pub const DEVICE_ACTION_W: i32 = 54;
pub const DEVICE_ACTION_Y_OFFSET: i32 = 6;
pub const DEVICE_ACTION_H: i32 = 24;

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
    let x1 = (x + w).min(width);
    let y1 = (y + h).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            let index = ((py * width + px) * 4) as usize;
            if index + 3 < canvas.len() {
                canvas[index..index + 4].copy_from_slice(&color);
            }
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

fn short_name(value: &str, limit: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= limit {
        value.to_string()
    } else {
        format!(
            "{}…",
            chars
                .iter()
                .take(limit.saturating_sub(1))
                .collect::<String>()
        )
    }
}

fn button(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    active: bool,
) {
    rect(
        canvas,
        width,
        height,
        x,
        y,
        w,
        h,
        if active { theme::YELLOW_ARGB } else { theme::INSET_ARGB },
    );
}

fn draw_minimize(canvas: &mut [u8], width: i32, height: i32) {
    theme::fill_rounded(
        canvas,
        width,
        height,
        HEADER_MINIMIZE_X,
        HEADER_MINIMIZE_Y,
        HEADER_MINIMIZE_W,
        HEADER_MINIMIZE_H,
        6,
        theme::RAISED_ARGB,
    );
    rect(
        canvas,
        width,
        height,
        HEADER_MINIMIZE_X + 6,
        HEADER_MINIMIZE_Y + 8,
        8,
        2,
        theme::YELLOW_ARGB,
    );
}

pub fn render_bluetooth_surface(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f32,
    snapshot: &BluetoothSnapshot,
    font: &mut FontManager,
) {
    let w = width as i32;
    let h = height as i32;
    if w <= 0 || h <= 0 || canvas.len() < width as usize * height as usize * 4 {
        return;
    }

    canvas.fill(0);
    theme::fill_rounded(
        canvas,
        w,
        h,
        0,
        0,
        w,
        h,
        (14.0 * scale).round() as i32,
        theme::PANEL_ARGB,
    );
    theme::stroke_rounded(
        canvas,
        w,
        h,
        0,
        0,
        w,
        h,
        (14.0 * scale).round() as i32,
        (scale.round() as i32).max(1),
        theme::BORDER_ARGB,
    );
    rect(
        canvas,
        w,
        h,
        (8.0 * scale) as i32,
        0,
        (24.0 * scale) as i32,
        (3.0 * scale).max(1.0) as i32,
        theme::YELLOW_ARGB,
    );

    let s = |value: f32| (value * scale).round() as i32;
    label(
        canvas,
        w,
        h,
        font,
        "Bluetooth",
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
        &short_name(&snapshot.adapter_name, 22),
        s(7.0),
        s(8.0),
        s(25.0),
        theme::MUTED,
    );
    draw_minimize(canvas, w, h);
    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(26.0),
        s(204.0),
        s(1.0),
        theme::HAIRLINE_ARGB,
    );

    if !snapshot.available || snapshot.adapter_path.is_none() {
        label(
            canvas,
            w,
            h,
            font,
            if snapshot.available {
                "No Bluetooth adapter"
            } else {
                "BlueZ unavailable"
            },
            s(10.0),
            s(8.0),
            s(68.0),
            (240, 190, 150),
        );
        if let Some(error) = snapshot.error.as_deref() {
            label(
                canvas,
                w,
                h,
                font,
                &short_name(error, 31),
                s(7.0),
                s(8.0),
                s(84.0),
                theme::MUTED,
            );
        }
        label(
            canvas,
            w,
            h,
            font,
            if snapshot.available {
                "Check adapter / hci0"
            } else {
                "Check bluetooth.service"
            },
            s(8.0),
            s(8.0),
            s(103.0),
            theme::MUTED,
        );
        return;
    }

    button(
        canvas,
        w,
        h,
        s(POWER_BUTTON_X as f32),
        s(POWER_BUTTON_Y as f32),
        s(POWER_BUTTON_W as f32),
        s(POWER_BUTTON_H as f32),
        snapshot.powered,
    );
    button(
        canvas,
        w,
        h,
        s(SCAN_BUTTON_X as f32),
        s(SCAN_BUTTON_Y as f32),
        s(SCAN_BUTTON_W as f32),
        s(SCAN_BUTTON_H as f32),
        snapshot.discovering,
    );
    label(
        canvas,
        w,
        h,
        font,
        if snapshot.powered {
            "Power: ON"
        } else {
            "Power: OFF"
        },
        s(8.0),
        s(POWER_BUTTON_X as f32 + 13.0),
        s(POWER_BUTTON_Y as f32 + 15.0),
        if snapshot.powered {
            theme::TEXT
        } else {
            theme::MUTED
        },
    );
    label(
        canvas,
        w,
        h,
        font,
        if snapshot.discovery_owned {
            "Stop scan"
        } else if snapshot.discovering {
            "Scan also"
        } else {
            "Scan devices"
        },
        s(8.0),
        s(SCAN_BUTTON_X as f32 + 27.0),
        s(SCAN_BUTTON_Y as f32 + 15.0),
        theme::TEXT,
    );

    rect(
        canvas,
        w,
        h,
        s(8.0),
        s(58.0),
        s(204.0),
        s(1.0),
        theme::HAIRLINE_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        "DEVICES",
        s(8.0),
        s(8.0),
        s(68.0),
        (250, 204, 21),
    );

    if snapshot.devices.is_empty() {
        label(
            canvas,
            w,
            h,
            font,
            if snapshot.discovering {
                "Searching nearby devices…"
            } else {
                "No devices found"
            },
            s(8.0),
            s(8.0),
            s(91.0),
            theme::MUTED,
        );
        label(
            canvas,
            w,
            h,
            font,
            "Use Scan devices to discover",
            s(7.0),
            s(8.0),
            s(105.0),
            theme::MUTED,
        );
    } else {
        for (index, device) in snapshot.devices.iter().take(DEVICE_ROW_COUNT).enumerate() {
            let row_y = DEVICE_LIST_Y + index as i32 * DEVICE_ROW_HEIGHT;
            theme::fill_rounded(
                canvas,
                w,
                h,
                s(8.0),
                s(row_y as f32),
                s(204.0),
                s(34.0),
                s(8.0),
                theme::INSET_ARGB,
            );
            let status = if device.connected {
                "Connected"
            } else if device.paired {
                "Paired"
            } else {
                "New device"
            };
            label(
                canvas,
                w,
                h,
                font,
                &short_name(&device.name, 22),
                s(8.0),
                s(13.0),
                s((row_y + 14) as f32),
                theme::TEXT,
            );
            label(
                canvas,
                w,
                h,
                font,
                status,
                s(7.0),
                s(13.0),
                s((row_y + 26) as f32),
                if device.connected {
                    (125, 196, 151)
                } else {
                    theme::MUTED
                },
            );

            button(
                canvas,
                w,
                h,
                s(DEVICE_ACTION_X as f32),
                s((row_y + DEVICE_ACTION_Y_OFFSET) as f32),
                s(DEVICE_ACTION_W as f32),
                s(DEVICE_ACTION_H as f32),
                device.connected,
            );
            let action_label = if device.connected {
                "Disconnect"
            } else if device.paired {
                "Connect"
            } else {
                "Pair"
            };
            let label_x = match action_label {
                "Disconnect" => DEVICE_ACTION_X + 3,
                "Connect" => DEVICE_ACTION_X + 6,
                _ => DEVICE_ACTION_X + 17,
            };
            label(
                canvas,
                w,
                h,
                font,
                action_label,
                s(7.0),
                s(label_x as f32),
                s((row_y + 21) as f32),
                theme::TEXT,
            );
        }
    }

    if let Some(message) = snapshot.status_message.as_deref() {
        label(
            canvas,
            w,
            h,
            font,
            &short_name(message, 35),
            s(7.0),
            s(8.0),
            s(235.0),
            (180, 194, 211),
        );
    } else if snapshot.devices.len() > DEVICE_ROW_COUNT {
        label(
            canvas,
            w,
            h,
            font,
            &format!(
                "+{} more device(s)",
                snapshot.devices.len() - DEVICE_ROW_COUNT
            ),
            s(7.0),
            s(8.0),
            s(235.0),
            theme::MUTED,
        );
    } else if !snapshot.powered {
        label(
            canvas,
            w,
            h,
            font,
            "Turn Bluetooth on to scan",
            s(7.0),
            s(8.0),
            s(235.0),
            theme::MUTED,
        );
    }
}
