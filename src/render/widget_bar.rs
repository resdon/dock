//! Four compact system widgets positioned to the right of the StatusNotifier tray.

use crate::models::BluetoothSnapshot;

pub const WIDGET_BAR_WIDTH: i32 = 76;
pub const WIDGET_BAR_HEIGHT: i32 = 72;
pub const WIDGET_BAR_GAP: i32 = 8;
pub const BUTTON_SIZE: i32 = 28;
pub const BUTTON_GAP: i32 = 6;
pub const RCLONE_INDEX: usize = 0;
pub const PROCESS_INDEX: usize = 1;
pub const AUDIO_INDEX: usize = 2;
pub const BLUETOOTH_INDEX: usize = 3;

// Two-column, two-row arrangement: storage/processes above, audio/Bluetooth below.
const BUTTON_XS: [i32; 4] = [7, 41, 7, 41];
const BUTTON_YS: [i32; 4] = [5, 5, 39, 39];

/// Parent-surface-local position. Utility controls sit right of the tray box.
pub fn widget_bar_position(
    dock_width: i32,
    tray_item_count: usize,
    _surface_height: i32,
) -> (i32, i32) {
    let app_row_height = crate::state::dock::DOCK_HEIGHT as i32;
    let y = (app_row_height - WIDGET_BAR_HEIGHT) / 2;
    let tray_width = if tray_item_count == 0 {
        0
    } else {
        crate::render::tray::TRAY_BOX_WIDTH
    };
    (
        dock_width.max(0)
            + WIDGET_BAR_GAP
            + tray_width
            + if tray_width > 0 { WIDGET_BAR_GAP } else { 0 },
        y,
    )
}

pub fn button_center_x(index: usize) -> i32 {
    BUTTON_XS.get(index).copied().unwrap_or(0) + BUTTON_SIZE / 2
}

pub fn button_center_y(index: usize) -> i32 {
    BUTTON_YS.get(index).copied().unwrap_or(0) + BUTTON_SIZE / 2
}

pub fn button_at(index: usize, x: i32, y: i32) -> bool {
    let (Some(left), Some(top)) = (BUTTON_XS.get(index).copied(), BUTTON_YS.get(index).copied())
    else {
        return false;
    };
    (left..left + BUTTON_SIZE).contains(&x) && (top..top + BUTTON_SIZE).contains(&y)
}

fn blend(canvas: &mut [u8], width: i32, height: i32, x: i32, y: i32, rgb: [u8; 3], alpha: f32) {
    if x < 0 || y < 0 || x >= width || y >= height {
        return;
    }
    let i = ((y * width + x) * 4) as usize;
    if i + 3 >= canvas.len() {
        return;
    }
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    // wl_shm ARGB8888 on little-endian systems is BGRA in memory.
    canvas[i] = (rgb[2] as f32 * a + canvas[i] as f32 * inv).round() as u8;
    canvas[i + 1] = (rgb[1] as f32 * a + canvas[i + 1] as f32 * inv).round() as u8;
    canvas[i + 2] = (rgb[0] as f32 * a + canvas[i + 2] as f32 * inv).round() as u8;
    canvas[i + 3] = (255.0 * a + canvas[i + 3] as f32 * inv).round() as u8;
}

fn fill_rect(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: [u8; 3],
    alpha: f32,
) {
    for py in y..y + h {
        for px in x..x + w {
            blend(canvas, width, height, px, py, color, alpha);
        }
    }
}

fn line(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    color: [u8; 3],
    alpha: f32,
) {
    let mut x = x0;
    let mut y = y0;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        blend(canvas, width, height, x, y, color, alpha);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

fn rounded_panel(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    rgb: [u8; 3],
    alpha: f32,
) {
    for py in 0..h {
        for px in 0..w {
            let cx = if px < radius {
                radius
            } else if px >= w - radius {
                w - radius - 1
            } else {
                px
            };
            let cy = if py < radius {
                radius
            } else if py >= h - radius {
                h - radius - 1
            } else {
                py
            };
            let dx = px - cx;
            let dy = py - cy;
            if (px < radius || px >= w - radius)
                && (py < radius || py >= h - radius)
                && dx * dx + dy * dy > radius * radius
            {
                continue;
            }
            blend(canvas, width, height, x + px, y + py, rgb, alpha);
        }
    }
}

pub fn render_status_widget_bar(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale: f64,
    alpha: f32,
    bluetooth: &BluetoothSnapshot,
) {
    if phys_width == 0
        || phys_height == 0
        || canvas.len() < phys_width as usize * phys_height as usize * 4
    {
        return;
    }
    let scale = scale.max(0.25);
    let w = phys_width as i32;
    let h = phys_height as i32;
    let s = |v: i32| (v as f64 * scale).round() as i32;
    rounded_panel(
        canvas,
        w,
        h,
        0,
        0,
        s(WIDGET_BAR_WIDTH),
        s(WIDGET_BAR_HEIGHT),
        s(10).max(1),
        [28, 27, 33],
        alpha * 0.96,
    );

    for index in 0..4 {
        rounded_panel(
            canvas,
            w,
            h,
            s(BUTTON_XS[index]),
            s(BUTTON_YS[index]),
            s(BUTTON_SIZE),
            s(BUTTON_SIZE - 2),
            s(4).max(1),
            [48, 46, 55],
            alpha,
        );
    }

    let white = [250, 204, 21];
    let muted = [137, 134, 146];
    // Rclone: storage drive.
    let x = s(BUTTON_XS[RCLONE_INDEX]);
    let y = s(BUTTON_YS[RCLONE_INDEX]);
    line(
        canvas,
        w,
        h,
        x + s(7),
        y + s(8),
        x + s(21),
        y + s(8),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(7),
        y + s(8),
        x + s(7),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(7),
        y + s(18),
        x + s(21),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(21),
        y + s(8),
        x + s(21),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    fill_rect(
        canvas,
        w,
        h,
        x + s(10),
        y + s(13),
        s(8).max(1),
        s(1).max(1),
        [250, 204, 21],
        alpha,
    );

    // Process monitor: CPU die with pins.
    let x = s(BUTTON_XS[PROCESS_INDEX]);
    let y = s(BUTTON_YS[PROCESS_INDEX]);
    line(
        canvas,
        w,
        h,
        x + s(9),
        y + s(8),
        x + s(19),
        y + s(8),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(9),
        y + s(18),
        x + s(19),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(9),
        y + s(8),
        x + s(9),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(19),
        y + s(8),
        x + s(19),
        y + s(18),
        [250, 204, 21],
        alpha,
    );
    for n in 0..4 {
        let pin_x = x + s(11 + n * 2);
        line(
            canvas,
            w,
            h,
            pin_x,
            y + s(5),
            pin_x,
            y + s(8),
            [250, 204, 21],
            alpha,
        );
        line(
            canvas,
            w,
            h,
            pin_x,
            y + s(18),
            pin_x,
            y + s(21),
            [250, 204, 21],
            alpha,
        );
    }

    // Audio: speaker and two sound-wave strokes.
    let x = s(BUTTON_XS[AUDIO_INDEX]);
    let y = s(BUTTON_YS[AUDIO_INDEX]);
    fill_rect(canvas, w, h, x + s(7), y + s(10), s(3), s(8), white, alpha);
    line(
        canvas,
        w,
        h,
        x + s(10),
        y + s(10),
        x + s(16),
        y + s(6),
        white,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(10),
        y + s(18),
        x + s(16),
        y + s(22),
        white,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(16),
        y + s(6),
        x + s(16),
        y + s(22),
        white,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(19),
        y + s(10),
        x + s(21),
        y + s(14),
        white,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(21),
        y + s(14),
        x + s(19),
        y + s(18),
        white,
        alpha,
    );

    // Bluetooth: the symbol dims when the adapter is unavailable or powered off.
    let x = s(BUTTON_XS[BLUETOOTH_INDEX]);
    let y = s(BUTTON_YS[BLUETOOTH_INDEX]);
    let bt = if !bluetooth.available || !bluetooth.powered {
        muted
    } else if bluetooth.discovering {
        [220, 38, 38]
    } else {
        [250, 204, 21]
    };
    line(
        canvas,
        w,
        h,
        x + s(14),
        y + s(3),
        x + s(14),
        y + s(23),
        bt,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(14),
        y + s(3),
        x + s(21),
        y + s(9),
        bt,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(21),
        y + s(9),
        x + s(7),
        y + s(17),
        bt,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(7),
        y + s(9),
        x + s(21),
        y + s(17),
        bt,
        alpha,
    );
    line(
        canvas,
        w,
        h,
        x + s(21),
        y + s(17),
        x + s(14),
        y + s(23),
        bt,
        alpha,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_four_buttons_have_non_overlapping_hitboxes() {
        for index in 0..4 {
            assert!(button_at(
                index,
                button_center_x(index),
                button_center_y(index)
            ));
            for other in 0..4 {
                if index != other {
                    assert!(!button_at(
                        other,
                        button_center_x(index),
                        button_center_y(index)
                    ));
                }
            }
        }
    }

    #[test]
    fn widget_buttons_are_arranged_in_two_rows() {
        assert_eq!(
            button_center_y(RCLONE_INDEX),
            button_center_y(PROCESS_INDEX)
        );
        assert_eq!(
            button_center_y(AUDIO_INDEX),
            button_center_y(BLUETOOTH_INDEX)
        );
        assert!(button_center_y(AUDIO_INDEX) > button_center_y(RCLONE_INDEX));
        assert_eq!(button_center_x(RCLONE_INDEX), button_center_x(AUDIO_INDEX));
        assert_eq!(
            button_center_x(PROCESS_INDEX),
            button_center_x(BLUETOOTH_INDEX)
        );
        assert_eq!(WIDGET_BAR_WIDTH, 76);
        assert_eq!(WIDGET_BAR_HEIGHT, 72);
    }

    #[test]
    fn rendering_into_a_cleared_buffer_respects_hidden_alpha() {
        let width = WIDGET_BAR_WIDTH as u32;
        let height = WIDGET_BAR_HEIGHT as u32;
        let mut canvas = vec![0u8; width as usize * height as usize * 4];

        render_status_widget_bar(
            &mut canvas,
            width,
            height,
            1.0,
            1.0,
            &BluetoothSnapshot::default(),
        );
        let interior = ((36usize * width as usize + 38usize) * 4) + 3;
        assert!(canvas[interior] > 200, "visible panel should be opaque");

        // This is the same clean-buffer redraw performed by app::widget_bar.
        canvas.fill(0);
        render_status_widget_bar(
            &mut canvas,
            width,
            height,
            1.0,
            0.2,
            &BluetoothSnapshot::default(),
        );
        assert!(
            (40..=52).contains(&canvas[interior]),
            "hidden panel should retain 20% alpha"
        );

        let corner_alpha = canvas[3];
        assert_eq!(
            corner_alpha, 0,
            "rounded panel corner must remain transparent"
        );
    }

    #[test]
    fn bar_tracks_resized_dock_edge() {
        let a = widget_bar_position(240, 2, 99);
        let b = widget_bar_position(360, 2, 99);
        assert_eq!(a, (332, -6));
        assert_eq!(b.0 - a.0, 120);
        assert_eq!(a.1, b.1);
    }
}
