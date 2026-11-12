//! Badge renderer.
//!
//! Renders notification badge counts on dock icons.

use crate::models::BadgeUpdate;
use std::path::PathBuf;
use std::sync::OnceLock;

// dbus_unity notifications badge asset (embedded fallback)
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");
static BADGE_ICON_CACHE: OnceLock<Option<(u32, u32, Vec<u8>)>> = OnceLock::new();

fn badge_icon_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets/dialog-warning.png"));
        candidates.push(cwd.join("dock/assets/dialog-warning.png"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("assets/dialog-warning.png"));
            candidates.push(dir.join("../share/dock/assets/dialog-warning.png"));
        }
    }
    candidates
}

fn load_badge_icon_runtime() -> Option<(u32, u32, Vec<u8>)> {
    for path in badge_icon_candidates() {
        if let Ok(img) = image::open(&path) {
            return Some((img.width(), img.height(), img.to_rgba8().into_raw()));
        }
    }
    image::load_from_memory(BADGE_ICON_BYTES)
        .ok()
        .map(|img| (img.width(), img.height(), img.to_rgba8().into_raw()))
}

fn get_badge_icon() -> Option<&'static (u32, u32, Vec<u8>)> {
    BADGE_ICON_CACHE
        .get_or_init(load_badge_icon_runtime)
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

                canvas[dst_idx] = ((src_r * alpha + dst_r * inv_alpha + 127) / 255) as u8;
                canvas[dst_idx + 1] = ((src_g * alpha + dst_g * inv_alpha + 127) / 255) as u8;
                canvas[dst_idx + 2] = ((src_b * alpha + dst_b * inv_alpha + 127) / 255) as u8;
                canvas[dst_idx + 3] = 255;
            }
        }
    }
}

// Removed `draw_dock_item_badge` — dead code using cairo-rs. The live badge
// renderer is `draw_canvas_badge` which uses raw pixel manipulation. Cairo was
// removed as an unused dependency.
