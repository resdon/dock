// src/render/badges.rs

use cairo;
use image::GenericImageView;
use crate::models::{BadgeUpdate, DockItem};

// dbus_unity notifications badge asset
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");

pub fn draw_canvas_badge(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    badge: &BadgeUpdate,
    start_x: usize,
    start_y: usize,
    box_size: usize,
) {
    if !badge.count_visible || badge.count <= 0 {
        return;
    }

    let Ok(img) = image::load_from_memory(BADGE_ICON_BYTES) else {
        return;
    };
    
    let (badge_w, badge_h) = img.dimensions();
    let badge_rgba = img.to_rgba8();
    let raw_bytes = badge_rgba.as_raw();

    let overlay_x = (start_x + box_size).saturating_sub(badge_w as usize);
    let overlay_y = start_y;

    for y in 0..badge_h as usize {
        let canvas_y = overlay_y + y;
        if canvas_y >= canvas_height as usize { break; }

        for x in 0..badge_w as usize {
            let canvas_x = overlay_x + x;
            if canvas_x >= canvas_width as usize { break; }

            let src_idx = (y * badge_w as usize + x) * 4;
            let src_r = raw_bytes[src_idx] as u32;
            let src_g = raw_bytes[src_idx + 1] as u32;
            let src_b = raw_bytes[src_idx + 2] as u32;
            let src_a = raw_bytes[src_idx + 3] as u32;

            if src_a == 0 { continue; }

            let dst_idx = (canvas_y * canvas_width as usize + canvas_x) * 4;

            let alpha = src_a;
            let inv_alpha = 255 - alpha;

            let dst_b = canvas[dst_idx] as u32;
            let dst_g = canvas[dst_idx + 1] as u32;
            let dst_r = canvas[dst_idx + 2] as u32;

            canvas[dst_idx]     = ((src_b * alpha + dst_b * inv_alpha) / 255) as u8;
            canvas[dst_idx + 1] = ((src_g * alpha + dst_g * inv_alpha) / 255) as u8;
            canvas[dst_idx + 2] = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
            canvas[dst_idx + 3] = 255;
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
        
        let extents = cr.text_extents(&badge_text).unwrap();
        
        let text_x = badge_cx - (extents.width() / 2.0 + extents.x_bearing());
        let text_y = badge_cy - (extents.height() / 2.0 + extents.y_bearing());

        cr.move_to(text_x, text_y);
        let _ = cr.show_text(&badge_text);
    }
}