use crate::models::{BadgeUpdate, DockItem};
use std::sync::OnceLock;

// dbus_unity notifications badge asset
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");
static BADGE_ICON_CACHE: OnceLock<Option<(u32, u32, Vec<u8>)>> = OnceLock::new();

fn get_badge_icon() -> Option<&'static (u32, u32, Vec<u8>)> {
    BADGE_ICON_CACHE
        .get_or_init(|| {
            image::load_from_memory(BADGE_ICON_BYTES)
                .ok()
                .map(|img| (img.width(), img.height(), img.to_rgba8().into_raw()))
        })
        .as_ref()
}

pub fn draw_canvas_badge(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    badge: &BadgeUpdate,
    start_x: i32,
    start_y: i32,
    box_size: i32,
) {
    if !badge.count_visible || badge.count <= 0 {
        return;
    }

    let Some(&(badge_w, badge_h, ref raw_bytes)) = get_badge_icon() else {
        return;
    };

    let overlay_x = (start_x + box_size).saturating_sub(badge_w as i32);
    let overlay_y = start_y;

    let x_min = overlay_x.clamp(0, canvas_width as i32);
    let x_max = (overlay_x + badge_w as i32).clamp(0, canvas_width as i32);
    let y_min = overlay_y.clamp(0, canvas_height as i32);
    let y_max = (overlay_y + badge_h as i32).clamp(0, canvas_height as i32);

    for canvas_y in y_min..y_max {
        let src_y = canvas_y - overlay_y;
        for canvas_x in x_min..x_max {
            let src_x = canvas_x - overlay_x;
            let src_idx = ((src_y * badge_w as i32 + src_x) * 4) as usize;

            if src_idx + 3 >= raw_bytes.len() {
                continue;
            }

            let src_r = raw_bytes[src_idx] as u32;
            let src_g = raw_bytes[src_idx + 1] as u32;
            let src_b = raw_bytes[src_idx + 2] as u32;
            let src_a = raw_bytes[src_idx + 3] as u32;

            if src_a == 0 {
                continue;
            }

            let dst_idx = ((canvas_y * canvas_width as i32 + canvas_x) * 4) as usize;
            if dst_idx + 3 < canvas.len() {
                let alpha = src_a;
                let inv_alpha = 255 - alpha;

                let dst_r = canvas[dst_idx] as u32;
                let dst_g = canvas[dst_idx + 1] as u32;
                let dst_b = canvas[dst_idx + 2] as u32;

                canvas[dst_idx] = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
                canvas[dst_idx + 1] = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
                canvas[dst_idx + 2] = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
                canvas[dst_idx + 3] = 255;
            }
        }
    }
}

pub fn draw_dock_item_badge(
    cr: &cairo::Context,
    item: &DockItem,
    icon_x: f64,
    icon_y: f64,
    icon_size: f64,
) {
    // 1. Draw Download Progress Bar
    if item.show_progress {
        let bar_h = 4.0;
        let bar_w = icon_size * 0.8;
        let bar_x = icon_x + (icon_size - bar_w) / 2.0;
        let bar_y = icon_y + icon_size - bar_h;

        cr.set_source_rgba(0.2, 0.2, 0.2, 0.7);
        cr.rectangle(bar_x, bar_y, bar_w, bar_h);
        let _ = cr.fill();

        cr.set_source_rgba(0.2, 0.6, 1.0, 0.9);
        cr.rectangle(bar_x, bar_y, bar_w * item.progress, bar_h);
        let _ = cr.fill();
    }

    // 2. Draw Unread Count Badge
    if item.show_badge {
        let badge_text = if item.badge_count > 99 {
            "99+".to_string()
        } else {
            item.badge_count.to_string()
        };

        let radius = 9.0;
        let badge_cx = icon_x + icon_size - radius;
        let badge_cy = icon_y + radius;

        cr.set_source_rgb(0.9, 0.2, 0.2);
        cr.arc(badge_cx, badge_cy, radius, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();

        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.set_font_size(10.0);
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);

        if let Ok(extents) = cr.text_extents(&badge_text) {
            let text_x = badge_cx - (extents.width() / 2.0 + extents.x_bearing());
            let text_y = badge_cy - (extents.height() / 2.0 + extents.y_bearing());

            cr.move_to(text_x, text_y);
            let _ = cr.show_text(&badge_text);
        }
    }
}
