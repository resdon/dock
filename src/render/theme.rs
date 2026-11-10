//! Shared charcoal, vinyl-yellow and signal-red visual language.
//!
//! These primitives operate on the dock's 4-byte software surfaces. Colors are
//! stored in the same byte order as the surface's current pixel buffer.

pub const PANEL: [u8; 4] = [29, 28, 34, 255];
pub const INSET: [u8; 4] = [40, 39, 47, 255];
pub const RAISED: [u8; 4] = [51, 49, 58, 255];
pub const BORDER: [u8; 4] = [76, 73, 84, 255];
pub const HAIRLINE: [u8; 4] = [60, 58, 68, 255];
pub const YELLOW: [u8; 4] = [250, 204, 21, 255];
pub const YELLOW_SOFT: [u8; 4] = [255, 226, 106, 255];
pub const RED: [u8; 4] = [220, 38, 38, 255];

// Argb8888 in Wayland SHM is a little-endian 32-bit integer.
// The 32-bit value is 0xAARRGGBB, but in memory (little-endian) the bytes are [B, G, R, A].
pub const PANEL_ARGB: [u8; 4] = [34, 28, 29, 255];
pub const INSET_ARGB: [u8; 4] = [47, 39, 40, 255];
pub const BORDER_ARGB: [u8; 4] = [84, 73, 76, 255];
pub const HAIRLINE_ARGB: [u8; 4] = [68, 58, 60, 255];
pub const YELLOW_ARGB: [u8; 4] = [21, 204, 250, 255];
pub const YELLOW_SOFT_ARGB: [u8; 4] = [106, 226, 255, 255];
pub const RED_ARGB: [u8; 4] = [38, 38, 220, 255];
pub const RAISED_ARGB: [u8; 4] = [58, 49, 51, 255];
pub const TEXT: (u8, u8, u8) = (247, 243, 230);
pub const MUTED: (u8, u8, u8) = (164, 160, 173);
pub const MUTE: (u8, u8, u8) = MUTED;
pub const TEXT_MUTED: (u8, u8, u8) = MUTED;
pub const INK: (u8, u8, u8) = (34, 30, 24);
pub const YELLOW_SOFT_RGB: [u8; 3] = [YELLOW_SOFT[0], YELLOW_SOFT[1], YELLOW_SOFT[2]];
pub const MUTE_RGB: [u8; 3] = [MUTED.0, MUTED.1, MUTED.2];
pub const TEXT_MUTED_RGB: [u8; 3] = [MUTED.0, MUTED.1, MUTED.2];

pub fn fill_rounded(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    color: [u8; 4],
) {
    if width <= 0 || height <= 0 || w <= 0 || h <= 0 {
        return;
    }
    let radius = radius.max(0).min(w.min(h) / 2);
    let x0 = x.max(0);
    let y0 = y.max(0);
    let x1 = (x + w).min(width);
    let y1 = (y + h).min(height);
    for py in y0..y1 {
        for px in x0..x1 {
            let cx = if px < x + radius {
                x + radius
            } else if px >= x + w - radius {
                x + w - radius - 1
            } else {
                px
            };
            let cy = if py < y + radius {
                y + radius
            } else if py >= y + h - radius {
                y + h - radius - 1
            } else {
                py
            };
            let dx = px - cx;
            let dy = py - cy;
            if (px < x + radius || px >= x + w - radius)
                && (py < y + radius || py >= y + h - radius)
                && dx * dx + dy * dy > radius * radius
            {
                continue;
            }
            let i = ((py * width + px) * 4) as usize;
            if i + 3 < canvas.len() {
                canvas[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}

pub fn stroke_rounded(
    canvas: &mut [u8],
    width: i32,
    height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    radius: i32,
    thickness: i32,
    color: [u8; 4],
) {
    if w <= 0 || h <= 0 || thickness <= 0 {
        return;
    }
    let outer = radius.max(0).min(w.min(h) / 2);
    let inner = (outer - thickness).max(0);
    for py in y.max(0)..(y + h).min(height) {
        for px in x.max(0)..(x + w).min(width) {
            let on_outer = rounded_contains(px - x, py - y, w, h, outer);
            let on_inner = rounded_contains(
                px - x - thickness,
                py - y - thickness,
                w - thickness * 2,
                h - thickness * 2,
                inner,
            );
            if on_outer && !on_inner {
                let i = ((py * width + px) * 4) as usize;
                if i + 3 < canvas.len() {
                    canvas[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }
}

fn rounded_contains(px: i32, py: i32, w: i32, h: i32, radius: i32) -> bool {
    if w <= 0 || h <= 0 || px < 0 || py < 0 || px >= w || py >= h {
        return false;
    }
    if radius <= 0 {
        return true;
    }
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
    !((px < radius || px >= w - radius)
        && (py < radius || py >= h - radius)
        && dx * dx + dy * dy > radius * radius)
}
