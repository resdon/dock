//! Rclone panel renderer.
//!
//! Renders the paginated rclone mount list with transfer rates.

use crate::models::RcloneSnapshot;
use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::render::theme;

pub const RCLONE_POPUP_WIDTH: i32 = 270;
pub const RCLONE_POPUP_HEIGHT: i32 = 250;
pub const HEADER_MINIMIZE_X: i32 = 242;
pub const HEADER_MINIMIZE_Y: i32 = 4;
pub const HEADER_MINIMIZE_W: i32 = 20;
pub const HEADER_MINIMIZE_H: i32 = 16;
pub const ROW_START_Y: i32 = 48;
pub const ROW_HEIGHT: i32 = 43;
pub const ROWS_PER_PAGE: usize = 4;
pub const PREV_BUTTON: (i32, i32, i32, i32) = (184, 226, 32, 17);
pub const NEXT_BUTTON: (i32, i32, i32, i32) = (220, 226, 42, 17);

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

fn short(value: &str, max: usize) -> String {
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= max {
        value.to_string()
    } else {
        format!(
            "{}…",
            chars.iter().take(max.saturating_sub(1)).collect::<String>()
        )
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", value as u64, UNITS[unit])
    } else {
        format!("{:.1}{}", value, UNITS[unit])
    }
}

fn draw_button(
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
        if active {
            theme::YELLOW_ARGB
        } else {
            theme::INSET_ARGB
        },
    );
}

pub fn render_rclone_surface(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f32,
    snapshot: &RcloneSnapshot,
    page: usize,
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
        "Rclone mounts",
        s(12.0),
        s(8.0),
        s(17.0),
        theme::TEXT,
    );
    theme::fill_rounded(
        canvas,
        w,
        h,
        s(HEADER_MINIMIZE_X as f32),
        s(HEADER_MINIMIZE_Y as f32),
        s(HEADER_MINIMIZE_W as f32),
        s(HEADER_MINIMIZE_H as f32),
        s(6.0),
        theme::RAISED_ARGB,
    );
    rect(
        canvas,
        w,
        h,
        s((HEADER_MINIMIZE_X + 6) as f32),
        s((HEADER_MINIMIZE_Y + 8) as f32),
        s(8.0),
        s(2.0),
        theme::YELLOW_ARGB,
    );
    label(
        canvas,
        w,
        h,
        font,
        &format!("{} mounted", snapshot.mounts.len()),
        s(7.0),
        s(8.0),
        s(31.0),
        theme::MUTED,
    );
    rect(
        canvas,
        w,
        h,
        0,
        s(37.0),
        w,
        s(1.0).max(1),
        theme::HAIRLINE_ARGB,
    );

    if let Some(error) = snapshot.error.as_deref() {
        label(
            canvas,
            w,
            h,
            font,
            "Mount table unavailable",
            s(9.0),
            s(8.0),
            s(67.0),
            (255, 214, 105),
        );
        label(
            canvas,
            w,
            h,
            font,
            &short(error, 40),
            s(7.0),
            s(8.0),
            s(83.0),
            theme::MUTED,
        );
        return;
    }
    if snapshot.mounts.is_empty() {
        label(
            canvas,
            w,
            h,
            font,
            "No rclone mounts found",
            s(9.0),
            s(8.0),
            s(75.0),
            theme::TEXT,
        );
        label(
            canvas,
            w,
            h,
            font,
            "Run: rclone mount remote: PATH",
            s(7.0),
            s(8.0),
            s(92.0),
            theme::MUTED,
        );
        label(
            canvas,
            w,
            h,
            font,
            "Only active FUSE mounts are listed",
            s(7.0),
            s(8.0),
            s(108.0),
            theme::MUTED,
        );
        return;
    }

    let page_count = snapshot.mounts.len().div_ceil(ROWS_PER_PAGE).max(1);
    let page = page.min(page_count - 1);
    let start = page * ROWS_PER_PAGE;
    for (row, mount) in snapshot
        .mounts
        .iter()
        .skip(start)
        .take(ROWS_PER_PAGE)
        .enumerate()
    {
        let y = ROW_START_Y + row as i32 * ROW_HEIGHT;
        theme::fill_rounded(
            canvas,
            w,
            h,
            s(7.0),
            s(y as f32),
            s(256.0),
            s(39.0),
            s(8.0),
            theme::INSET_ARGB,
        );
        let mount_name = mount
            .mount_point
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or(&mount.mount_point);
        let identity = if mount.source.is_empty() {
            mount_name.to_string()
        } else {
            format!("{} <- {}", mount_name, mount.source)
        };
        label(
            canvas,
            w,
            h,
            font,
            &short(&identity, 42),
            s(8.0),
            s(12.0),
            s((y + 12) as f32),
            theme::TEXT,
        );
        if mount.total_bytes > 0 {
            let pct =
                ((mount.used_bytes as f64 / mount.total_bytes as f64) * 100.0).clamp(0.0, 100.0);
            let capacity = format!(
                "{} / {}  ({:.0}%)",
                format_bytes(mount.used_bytes),
                format_bytes(mount.total_bytes),
                pct
            );
            label(
                canvas,
                w,
                h,
                font,
                &short(&capacity, 34),
                s(7.0),
                s(12.0),
                s((y + 24) as f32),
                (218, 210, 179),
            );
            let bar_x = s(12.0);
            let bar_y = s((y + 30) as f32);
            let bar_w = s(238.0).max(1);
            let bar_h = s(4.0).max(2);
            theme::fill_rounded(
                canvas,
                w,
                h,
                bar_x,
                bar_y,
                bar_w,
                bar_h,
                bar_h / 2,
                theme::RAISED_ARGB,
            );
            rect(
                canvas,
                w,
                h,
                bar_x,
                bar_y,
                ((bar_w as f64 * pct / 100.0).round() as i32).clamp(0, bar_w),
                bar_h,
                if pct >= 90.0 {
                    theme::RED_ARGB
                } else {
                    theme::YELLOW_ARGB
                },
            );
        } else if mount.used_bytes > 0 && mount.available_bytes > 0 {
            let capacity = format!(
                "{} used / {} free",
                format_bytes(mount.used_bytes),
                format_bytes(mount.available_bytes),
            );
            label(
                canvas,
                w,
                h,
                font,
                &short(&capacity, 34),
                s(7.0),
                s(12.0),
                s((y + 24) as f32),
                (218, 210, 179),
            );
            label(
                canvas,
                w,
                h,
                font,
                "Total quota unavailable",
                s(6.0),
                s(12.0),
                s((y + 34) as f32),
                theme::MUTED,
            );
        } else if mount.used_bytes > 0 {
            let capacity = format!("{} used", format_bytes(mount.used_bytes));
            label(
                canvas,
                w,
                h,
                font,
                &capacity,
                s(7.0),
                s(12.0),
                s((y + 24) as f32),
                (218, 210, 179),
            );
            label(
                canvas,
                w,
                h,
                font,
                "Total quota unavailable",
                s(6.0),
                s(12.0),
                s((y + 34) as f32),
                theme::MUTED,
            );
        } else if mount.available_bytes > 0 {
            let capacity = format!("{} free", format_bytes(mount.available_bytes));
            label(
                canvas,
                w,
                h,
                font,
                &capacity,
                s(7.0),
                s(12.0),
                s((y + 24) as f32),
                (218, 210, 179),
            );
            label(
                canvas,
                w,
                h,
                font,
                "Used total unavailable",
                s(6.0),
                s(12.0),
                s((y + 34) as f32),
                theme::MUTED,
            );
        } else {
            label(
                canvas,
                w,
                h,
                font,
                "Capacity unavailable",
                s(7.0),
                s(12.0),
                s((y + 24) as f32),
                theme::MUTED,
            );
            label(
                canvas,
                w,
                h,
                font,
                &short(&mount.source, 38),
                s(6.0),
                s(12.0),
                s((y + 34) as f32),
                theme::MUTED,
            );
        }
    }

    label(
        canvas,
        w,
        h,
        font,
        &format!("Page {}/{}", page + 1, page_count),
        s(7.0),
        s(8.0),
        s(239.0),
        theme::MUTED,
    );
    if page > 0 {
        draw_button(
            canvas,
            w,
            h,
            s(PREV_BUTTON.0 as f32),
            s(PREV_BUTTON.1 as f32),
            s(PREV_BUTTON.2 as f32),
            s(PREV_BUTTON.3 as f32),
            true,
        );
    }
    if page + 1 < page_count {
        draw_button(
            canvas,
            w,
            h,
            s(NEXT_BUTTON.0 as f32),
            s(NEXT_BUTTON.1 as f32),
            s(NEXT_BUTTON.2 as f32),
            s(NEXT_BUTTON.3 as f32),
            true,
        );
    }
    label(
        canvas,
        w,
        h,
        font,
        "<",
        s(8.0),
        s(196.0),
        s(239.0),
        (224, 232, 241),
    );
    label(
        canvas,
        w,
        h,
        font,
        ">",
        s(8.0),
        s(237.0),
        s(239.0),
        (224, 232, 241),
    );
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (HEADER_MINIMIZE_X..HEADER_MINIMIZE_X + HEADER_MINIMIZE_W).contains(&x)
        && (HEADER_MINIMIZE_Y..HEADER_MINIMIZE_Y + HEADER_MINIMIZE_H).contains(&y)
}

pub fn page_delta_at(x: f64, y: f64) -> Option<i32> {
    let x = x.round() as i32;
    let y = y.round() as i32;
    if (PREV_BUTTON.1..PREV_BUTTON.1 + PREV_BUTTON.3).contains(&y)
        && (PREV_BUTTON.0..PREV_BUTTON.0 + PREV_BUTTON.2).contains(&x)
    {
        Some(-1)
    } else if (NEXT_BUTTON.1..NEXT_BUTTON.1 + NEXT_BUTTON.3).contains(&y)
        && (NEXT_BUTTON.0..NEXT_BUTTON.0 + NEXT_BUTTON.2).contains(&x)
    {
        Some(1)
    } else {
        None
    }
}
