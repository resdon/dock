//! Dock row renderer.
//!
//! Renders the main dock row with icons, dashes, and hover highlights.

use crate::models::BadgeUpdate;
use crate::render::font::FontManager;
use crate::render::{draw_canvas_badge, hsl_to_rgb};
use crate::state::WindowState;
use crate::types::DockState;

use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;

pub struct DockRenderResources<'a> {
    pub phys_width: u32,
    pub phys_height: u32,
    pub running_by_app: &'a HashMap<String, Vec<&'a WindowState>>,
    pub icon_cache: &'a HashMap<String, (Arc<[u8]>, u32)>,
    pub loading_apps: &'a HashSet<String>,
    pub badges: &'a HashMap<String, BadgeUpdate>,
    pub font_manager: &'a mut FontManager,
    pub is_dragging: bool,
    pub dragged_app_id: Option<&'a String>,
    pub pointer_position: (i32, i32),
}

pub fn render_dock_surface(canvas: &mut [u8], state: &DockState, res: &mut DockRenderResources) {
    render_dock_background(
        canvas,
        res.phys_width,
        res.phys_height,
        state.panel_rect.y.max(0.0) as usize,
        state.panel_rect.height.max(0.0) as usize,
        state.current_visibility_alpha,
    );

    render_dock_items(canvas, state, res);

    if res.is_dragging {
        if let Some(drag_id) = res.dragged_app_id.cloned() {
            let drag_id = drag_id.clone();
            render_dragged_icon(canvas, state, res, &drag_id);
        }
    }
}

/// Render the dock background.
///
/// This function is a no-op now that the main dock uses Slint for rendering.
/// The background is rendered by Slint's widget system, not by CPU pixel operations.
///
/// # Deprecation
/// This function is kept for API compatibility but does nothing. The background
/// is now rendered by the Slint dock widget.
pub fn render_dock_background(
    canvas: &mut [u8],
    _phys_width: u32,
    _phys_height: u32,
    _dock_y: usize,
    _dock_height: usize,
    _alpha: f32,
) {
    // Clear canvas to transparent black
    canvas.fill(0);
}

pub fn render_dock_items(canvas: &mut [u8], state: &DockState, res: &mut DockRenderResources) {
    let phys_width = res.phys_width as usize;
    let phys_height = res.phys_height as usize;

    for pin in &state.pins {
        let box_size = pin.size;
        let start_x = pin.x as usize;
        let start_y = pin.y as usize;

        if start_x + box_size > phys_width {
            continue;
        }

        let windows = res.running_by_app.get(&pin.app_id);
        let window_icon = windows.and_then(|v| {
            v.first().and_then(|w| {
                w.icon_rgba
                    .as_ref()
                    .map(|rgba| (rgba.as_slice(), w.icon_size))
            })
        });
        let cached_icon = res
            .icon_cache
            .get(&pin.app_id)
            .map(|(rgba, size)| (&rgba[..], *size));
        // Prefer the high-resolution desktop/icon-theme source over the often
        // small compositor-provided foreign-toplevel thumbnail icon.
        let icon = match (window_icon, cached_icon) {
            (Some(window), Some(cached)) if cached.1 >= window.1 => Some(cached),
            (Some(window), _) => Some(window),
            (None, cached) => cached,
        };

        if let Some((icon_pixels, img_size_u32)) = icon {
            // fast_image_resize: SIMD-optimized Lanczos-3 downscaling
            // Replaces the custom 6×6 pixel-by-pixel Lanczos-3 loop above.
            let src_w = img_size_u32;
            let src_h = img_size_u32;
            let dst_w = box_size as u32;
            let dst_h = box_size as u32;

            if src_w > 0 && src_h > 0 && dst_w > 0 && dst_h > 0 {
                // Create source image from raw RGBA bytes
                let src_img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
                    src_w,
                    src_h,
                    icon_pixels.to_vec(),
                );
                if let Some(src) = src_img {
                    // Create destination image buffer
                    let mut dst = fast_image_resize::images::Image::new(
                        dst_w,
                        dst_h,
                        fast_image_resize::pixels::PixelType::U8x4,
                    );
                    // Resize using SIMD-optimized Lanczos-3
                    let mut resizer = fast_image_resize::Resizer::new();
                    if resizer.resize(&src, &mut dst, None).is_ok() {
                        let dst_pixels = dst.buffer();
                        let dim = if pin.is_running { 1.0 } else { 0.5 };
                        for y in 0..box_size {
                            for x in 0..box_size {
                                let canvas_x = start_x + x;
                                let canvas_y = start_y + y;
                                if canvas_x >= phys_width || canvas_y >= phys_height {
                                    continue;
                                }
                                let src_idx = (y as usize * dst_w as usize + x as usize) * 4;
                                if src_idx + 3 >= dst_pixels.len() {
                                    continue;
                                }
                                let alpha = dst_pixels[src_idx + 3] as f32 / 255.0;
                                if alpha > 0.0 {
                                    let src_r = dst_pixels[src_idx] as f32 * dim;
                                    let src_g = dst_pixels[src_idx + 1] as f32 * dim;
                                    let src_b = dst_pixels[src_idx + 2] as f32 * dim;
                                    let canvas_idx = (canvas_y * phys_width + canvas_x) * 4;
                                    if canvas_idx + 3 < canvas.len() {
                                        // Wayland ARGB8888 little-endian canvas is [B,G,R,A].
                                        canvas[canvas_idx] = (src_b * alpha
                                            + canvas[canvas_idx] as f32 * (1.0 - alpha))
                                            as u8;
                                        canvas[canvas_idx + 1] = (src_g * alpha
                                            + canvas[canvas_idx + 1] as f32 * (1.0 - alpha))
                                            as u8;
                                        canvas[canvas_idx + 2] = (src_r * alpha
                                            + canvas[canvas_idx + 2] as f32 * (1.0 - alpha))
                                            as u8;
                                        canvas[canvas_idx + 3] = 255;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else if res.loading_apps.contains(&pin.app_id) {
            // A quiet animated-loader motif replaces the old large initial
            // glyph. The tiny dots are a static first frame and never force a
            // permanent repaint loop while an icon request is outstanding.
            let cx = start_x as i32 + box_size as i32 / 2;
            let cy = start_y as i32 + box_size as i32 / 2;
            let ring = (box_size as i32 / 4).max(5);
            for (dx, dy, radius, color) in [
                (0, -ring, 3, [21, 204, 250, 255]),
                (ring, 0, 2, [78, 173, 245, 255]),
                (0, ring, 2, [102, 115, 134, 255]),
                (-ring, 0, 2, [133, 103, 87, 255]),
                (ring / 2, -ring / 2, 2, [38, 38, 220, 255]),
                (-ring / 2, -ring / 2, 2, [68, 148, 220, 255]),
                (ring / 2, ring / 2, 2, [92, 92, 108, 255]),
                (-ring / 2, ring / 2, 2, [92, 92, 108, 255]),
            ] {
                for oy in -radius..=radius {
                    for ox in -radius..=radius {
                        if ox * ox + oy * oy > radius * radius {
                            continue;
                        }
                        let px = cx + dx + ox;
                        let py = cy + dy + oy;
                        if px < 0
                            || py < 0
                            || px as usize >= phys_width
                            || py as usize >= phys_height
                        {
                            continue;
                        }
                        let i = (py as usize * phys_width + px as usize) * 4;
                        if i + 3 < canvas.len() {
                            canvas[i..i + 4].copy_from_slice(&color);
                        }
                    }
                }
            }
        } // Keep the neutral tile empty until the asynchronous icon lookup
          // returns. App-ID initials here caused an oversized startup flash.

        // Draw multi-dash window count indicator (from reference project design)
        // Up to 5 dashes with varying widths, cyan for focused/active apps
        // Reference: 32px total span, centered within 48px icon (8px padding each side)
        // y position: 58px from dock top for 60px dock height (2px from bottom)
        if pin.is_running {
            // Indicator sits at bottom of dock: dock_height - 2px = 58px for 60px dock
            let indicator_y = start_y + box_size - 2;

            if indicator_y < phys_height {
                let num_dashes = if pin.running_count > 0 {
                    pin.running_count.min(5)
                } else {
                    1
                };

                // Reference project uses 32px total span with fixed dash positions
                let total_line_width = 32.0f32;
                let dash_height = 2; // 2px height from reference
                let dash_spacing = 3.0f32;
                let total_spacing = dash_spacing * (num_dashes as f32 - 1.0f32).max(0.0f32);
                let dash_width =
                    ((total_line_width - total_spacing) / num_dashes as f32).max(4.0f32);

                // Center the 32px span within the icon
                let indicator_start_x =
                    start_x as f32 + (box_size as f32 - total_line_width) / 2.0f32;

                let is_app_active = windows
                    .map_or(pin.is_activated, |wins| wins.iter().any(|w| w.focused))
                    || pin.is_activated;

                for i in 0..num_dashes {
                    let dash_start_x = indicator_start_x + (i as f32 * (dash_width + dash_spacing));
                    let dash_end_x = dash_start_x + dash_width;

                    let x_start = dash_start_x.round() as usize;
                    let x_end = dash_end_x.round() as usize;

                    for dy in 0..dash_height {
                        let canvas_y = indicator_y + dy;
                        if canvas_y < phys_height {
                            for canvas_x in x_start..x_end {
                                if canvas_x < phys_width {
                                    let canvas_idx = (canvas_y * phys_width + canvas_x) * 4;
                                    if canvas_idx + 3 < canvas.len() {
                                        if is_app_active {
                                            // Active apps use cyan accent (#00E5FF) - BGRA bytes
                                            canvas[canvas_idx] = 0xFF; // B
                                            canvas[canvas_idx + 1] = 0xE5; // G
                                            canvas[canvas_idx + 2] = 0x00; // R
                                        } else {
                                            // Inactive running apps use silver (#9CA3AF) - BGRA bytes
                                            canvas[canvas_idx] = 0xAF; // B
                                            canvas[canvas_idx + 1] = 0xA3; // G
                                            canvas[canvas_idx + 2] = 0x9C; // R
                                        }
                                        canvas[canvas_idx + 3] = 0xFF;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw Notification Badge overlay
        if let Some(badge) = res.badges.get(&pin.app_id) {
            draw_canvas_badge(
                canvas,
                res.phys_width,
                res.phys_height,
                badge,
                start_x as i32,
                start_y as i32,
                box_size as i32,
            );
        }
    }
}

pub fn render_dragged_icon(
    canvas: &mut [u8],
    _state: &DockState,
    res: &mut DockRenderResources,
    drag_id: &str,
) {
    let phys_width = res.phys_width as i32;
    let phys_height = res.phys_height as i32;
    let box_size = 48;

    let windows = res.running_by_app.get(drag_id);
    let window_icon = windows.and_then(|v| {
        v.first().and_then(|w| {
            w.icon_rgba
                .as_ref()
                .map(|rgba| (rgba.as_slice(), w.icon_size))
        })
    });
    let cached_icon = res
        .icon_cache
        .get(drag_id)
        .map(|(rgba, size)| (&rgba[..], *size));
    let icon = match (window_icon, cached_icon) {
        (Some(window), Some(cached)) if cached.1 >= window.1 => Some(cached),
        (Some(window), _) => Some(window),
        (None, cached) => cached,
    };

    let drag_start_x = res.pointer_position.0.saturating_sub(box_size / 2);
    let drag_start_y = res.pointer_position.1.saturating_sub(box_size / 2);

    if let Some((icon_pixels, img_size_u32)) = icon {
        let img_size = img_size_u32 as usize;
        if img_size > 0 {
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = drag_start_x + x;
                    let canvas_y = drag_start_y + y;
                    if canvas_x < 0
                        || canvas_x >= phys_width
                        || canvas_y < 0
                        || canvas_y >= phys_height
                    {
                        continue;
                    }

                    // Match the main dock's bilinear sampling so the dragged
                    // preview does not fall back to nearest-neighbour blur.
                    let src_x = (((x as f32 + 0.5) * img_size as f32 / box_size as f32) - 0.5)
                        .clamp(0.0, img_size.saturating_sub(1) as f32);
                    let src_y = (((y as f32 + 0.5) * img_size as f32 / box_size as f32) - 0.5)
                        .clamp(0.0, img_size.saturating_sub(1) as f32);
                    let x0 = src_x.floor() as usize;
                    let y0 = src_y.floor() as usize;
                    let x1 = (x0 + 1).min(img_size - 1);
                    let y1 = (y0 + 1).min(img_size - 1);
                    let tx = src_x - x0 as f32;
                    let ty = src_y - y0 as f32;
                    let idx00 = (y0 * img_size + x0) * 4;
                    let idx10 = (y0 * img_size + x1) * 4;
                    let idx01 = (y1 * img_size + x0) * 4;
                    let idx11 = (y1 * img_size + x1) * 4;
                    if [idx00, idx10, idx01, idx11]
                        .iter()
                        .any(|idx| idx + 3 >= icon_pixels.len())
                    {
                        continue;
                    }

                    let mut sample = [0.0f32; 4];
                    for channel in 0..4 {
                        let top = icon_pixels[idx00 + channel] as f32 * (1.0 - tx)
                            + icon_pixels[idx10 + channel] as f32 * tx;
                        let bottom = icon_pixels[idx01 + channel] as f32 * (1.0 - tx)
                            + icon_pixels[idx11 + channel] as f32 * tx;
                        sample[channel] = top * (1.0 - ty) + bottom * ty;
                    }

                    let canvas_idx = ((canvas_y * phys_width + canvas_x) * 4) as usize;
                    if canvas_idx + 3 < canvas.len() {
                        let alpha = (sample[3] / 255.0) * 0.85;
                        if alpha > 0.0 {
                            // Canvas [B, G, R, A]. Source remains canonical RGBA.
                            canvas[canvas_idx] = (sample[2] * alpha
                                + canvas[canvas_idx] as f32 * (1.0 - alpha))
                                as u8;
                            canvas[canvas_idx + 1] = (sample[1] * alpha
                                + canvas[canvas_idx + 1] as f32 * (1.0 - alpha))
                                as u8;
                            canvas[canvas_idx + 2] = (sample[0] * alpha
                                + canvas[canvas_idx + 2] as f32 * (1.0 - alpha))
                                as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        }
    } else {
        let radius = 10.0;
        let size_f = box_size as f64;
        let is_inside_rounded_rect = |px: usize, py: usize| -> bool {
            let x = px as f64;
            let y = py as f64;
            if x < radius && y < radius {
                (x - radius).powi(2) + (y - radius).powi(2) <= radius.powi(2)
            } else if x > size_f - radius && y < radius {
                (x - (size_f - radius)).powi(2) + (y - radius).powi(2) <= radius.powi(2)
            } else if x < radius && y > size_f - radius {
                (x - radius).powi(2) + (y - (size_f - radius)).powi(2) <= radius.powi(2)
            } else if x > size_f - radius && y > size_f - radius {
                (x - (size_f - radius)).powi(2) + (y - (size_f - radius)).powi(2) <= radius.powi(2)
            } else {
                true
            }
        };

        let hash = drag_id
            .bytes()
            .fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        let hue = (hash % 360) as f32;
        let base_color = hsl_to_rgb(hue, 0.6, 0.55);
        let grad_color = hsl_to_rgb(hue, 0.6, 0.40);

        for y in 0..box_size {
            for x in 0..box_size {
                let canvas_x = drag_start_x + x;
                let canvas_y = drag_start_y + y;
                if canvas_x >= 0
                    && canvas_x < phys_width
                    && canvas_y >= 0
                    && canvas_y < phys_height
                    && is_inside_rounded_rect(x as usize, y as usize)
                {
                    let canvas_idx = ((canvas_y * phys_width + canvas_x) * 4) as usize;
                    if canvas_idx + 3 < canvas.len() {
                        let t = y as f32 / box_size as f32;
                        let r = base_color.0 as f32 * (1.0 - t) + grad_color.0 as f32 * t;
                        let g = base_color.1 as f32 * (1.0 - t) + grad_color.1 as f32 * t;
                        let b = base_color.2 as f32 * (1.0 - t) + grad_color.2 as f32 * t;

                        // Canvas [B, G, R, A]
                        canvas[canvas_idx] = b as u8;
                        canvas[canvas_idx + 1] = g as u8;
                        canvas[canvas_idx + 2] = r as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }

        // Keep an unavailable dragged icon blank instead of drawing text.
    }
}
