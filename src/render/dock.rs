//! Dock row renderer.
//!
//! Renders the main dock row with icons, dashes, and hover highlights.

use crate::models::BadgeUpdate;
use crate::render::font::FontManager;
use crate::render::femtovg_wgpu::{
    draw_dock_background, draw_icon_placeholder, draw_loading_spinner,
    draw_text, FemtoVGIconCache,
};
use crate::render::{draw_canvas_badge, hsl_to_rgb};
use crate::state::WindowState;
use crate::types::DockState;

use femtovg::{Canvas, Color, Paint, Path};
use std::collections::HashMap;
use std::collections::HashSet;

pub struct DockRenderResources<'a> {
    pub phys_width: u32,
    pub phys_height: u32,
    pub running_by_app: &'a HashMap<String, Vec<&'a WindowState>>,
    pub icon_cache: &'a HashMap<String, (Vec<u8>, u32)>,
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

pub fn render_dock_background(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    dock_y: usize,
    dock_height: usize,
    alpha: f32,
) {
    let dock_start_y = dock_y.min(phys_height as usize);
    let dock_end_y = dock_start_y
        .saturating_add(dock_height)
        .min(phys_height as usize);
    let alpha = alpha.clamp(0.0, 1.0);
    canvas.fill(0);
    let stride = phys_width as usize * 4;
    let inset_x = 1usize.min(phys_width as usize / 2);
    let radius = (dock_height.min(phys_width as usize) / 3).max(1);
    for y in dock_start_y..dock_end_y {
        for x in inset_x..(phys_width as usize).saturating_sub(inset_x) {
            let local_y = y - dock_start_y;
            let cx = if x < inset_x + radius {
                inset_x + radius
            } else if x >= phys_width as usize - inset_x - radius {
                phys_width as usize - inset_x - radius - 1
            } else {
                x
            };
            let cy = if local_y < radius {
                radius
            } else if local_y >= dock_height.saturating_sub(radius) {
                dock_height.saturating_sub(radius).saturating_sub(1)
            } else {
                local_y
            };
            let dx = x as i32 - cx as i32;
            let dy = local_y as i32 - cy as i32;
            if (x < inset_x + radius || x >= phys_width as usize - inset_x - radius)
                && (local_y < radius || local_y >= dock_height.saturating_sub(radius))
                && (dx * dx + dy * dy) as usize > radius * radius
            {
                continue;
            }
            let border = x == inset_x
                || x + inset_x + 1 >= phys_width as usize
                || local_y == 0
                || local_y + 1 == dock_height;
            // Colors from reference project: #4C4954 border, #1D1C22CC background
            let (r, g, b) = if border {
                (76.0, 73.0, 84.0)  // #4C4954
            } else {
                (29.0, 28.0, 34.0)  // #1D1C22
            };
            let i = y * stride + x * 4;
            if i + 3 < canvas.len() {
                // Preserve Wayland's little-endian BGRA ordering and fade.
                canvas[i] = (b * alpha) as u8;
                canvas[i + 1] = (g * alpha) as u8;
                canvas[i + 2] = (r * alpha) as u8;
                canvas[i + 3] = (if border { 255.0 } else { 204.0 } * alpha) as u8; // CC alpha = 204
            }
        }
    }
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
            .map(|(rgba, size)| (rgba.as_slice(), *size));
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
                let src_img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(src_w, src_h, icon_pixels.to_vec());
                if let Some(src) = src_img {
                    // Create destination image buffer
                    let mut dst = fast_image_resize::images::Image::new(
                        dst_w, dst_h,
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
                                        canvas[canvas_idx] =
                                            (src_b * alpha + canvas[canvas_idx] as f32 * (1.0 - alpha)) as u8;
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
                                            canvas[canvas_idx] = 0xFF;     // B
                                            canvas[canvas_idx + 1] = 0xE5; // G
                                            canvas[canvas_idx + 2] = 0x00; // R
                                        } else {
                                            // Inactive running apps use silver (#9CA3AF) - BGRA bytes
                                            canvas[canvas_idx] = 0xAF;     // B
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
        .map(|(rgba, size)| (rgba.as_slice(), *size));
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

// ============================================================================
// FemtoVG-based dock rendering (GPU-accelerated, replaces pixel canvas)
// ============================================================================

/// Render the dock surface using FemtoVG draw commands.
/// This is the GPU-accelerated replacement for `render_dock_surface`.
pub fn render_dock_surface_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    state: &DockState,
    res: &mut DockRenderResources,
    icon_cache: &mut FemtoVGIconCache,
) {
    // Clear canvas to transparent
    let mut clear_path = Path::new();
    clear_path.rect(0.0, 0.0, res.phys_width as f32, res.phys_height as f32);
    let clear_paint = Paint::color(Color::rgba(0, 0, 0, 0));
    canvas.fill_path(&clear_path, &clear_paint);

    // Draw dock background
    draw_dock_background(
        canvas,
        state.panel_rect.y.max(0.0) as f32,
        state.panel_rect.height.max(0.0) as f32,
        res.phys_width as f32,
        state.current_visibility_alpha,
    );

    // Draw dock items
    render_dock_items_femtovg(canvas, state, res, icon_cache);

    // Draw dragged icon
    if res.is_dragging {
        if let Some(drag_id) = &res.dragged_app_id {
            render_dragged_icon_femtovg(canvas, state, res, drag_id, icon_cache);
        }
    }
}

/// Render all dock items using FemtoVG.
fn render_dock_items_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    state: &DockState,
    res: &mut DockRenderResources,
    icon_cache: &mut FemtoVGIconCache,
) {
    for pin in &state.pins {
        let box_size = pin.size as f32;
        let start_x = pin.x as f32;
        let start_y = pin.y as f32;

        if start_x + box_size > res.phys_width as f32 {
            continue;
        }

        // Get icon pixels
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
            .map(|(rgba, size)| (rgba.as_slice(), *size));
        let icon = match (window_icon, cached_icon) {
            (Some(window), Some(cached)) if cached.1 >= window.1 => Some(cached),
            (Some(window), _) => Some(window),
            (None, cached) => cached,
        };

        let alpha = if pin.is_running { 1.0 } else { 0.5 };
        let is_hovered = state.hovered_index.map_or(false, |i| {
            // Find the index of this pin in the pins list
            state.pins.iter().position(|p| p.app_id == pin.app_id) == Some(i)
        });

        // Draw icon or placeholder
        if let Some((icon_pixels, img_size_u32)) = icon {
            // Cache the icon
            icon_cache.get_or_create(
                canvas,
                &pin.app_id,
                icon_pixels,
                img_size_u32,
                img_size_u32,
            );

            // Visual effect: Cyan hover outline (from reference project)
            // Draw a slightly larger, cyan-colored version behind the main icon
            if is_hovered {
                let outline_size = box_size + 4.0;
                let outline_x = start_x - 2.0;
                let outline_y = start_y - 1.0;
                icon_cache.draw_colorized(
                    canvas,
                    &pin.app_id,
                    outline_x,
                    outline_y,
                    outline_size,
                    outline_size,
                    // Cyan color #00E5FF with partial opacity for outline effect
                    Color::rgba(0, 229, 255, 180),
                );
            }

            // Draw main icon
            icon_cache.draw(
                canvas,
                &pin.app_id,
                start_x,
                start_y,
                box_size,
                box_size,
                alpha,
            );
        } else if res.loading_apps.contains(&pin.app_id) {
            // Loading spinner
            draw_loading_spinner(
                canvas,
                start_x + box_size / 2.0,
                start_y + box_size / 2.0,
                box_size / 3.0,
            );
        } else {
            // Placeholder
            let hash = pin.app_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let hue = (hash % 360) as f32;
            draw_icon_placeholder(canvas, start_x, start_y, box_size, hue, alpha);
        }

        // Draw multi-dash window count indicator (from reference project design)
        // Up to 5 dashes with varying widths, cyan for focused/active apps
        // Reference: 32px total span, y = dock_height - 2px (58px for 60px dock)
        if pin.is_running {
            let indicator_y = start_y + box_size - 2.0;
            let window_count = pin.running_count.min(5).max(1);

            let is_app_active = windows
                .map_or(pin.is_activated, |wins| wins.iter().any(|w| w.focused))
                || pin.is_activated;

            draw_multi_dash_indicator(
                canvas,
                start_x,
                indicator_y,
                box_size,
                window_count,
                is_app_active,
            );
        }

        // Draw badge
        if let Some(badge) = res.badges.get(&pin.app_id) {
            let badge_x = start_x + box_size - 8.0;
            let badge_y = start_y - 2.0;
            draw_badge_femtovg(canvas, badge_x, badge_y, badge);
        }

    }
}

/// Draw multi-dash window count indicator below an icon.
/// Based on the reference project design: up to 5 dashes with varying widths.
/// Active/focused apps use cyan (#00E5FF), inactive use silver (#9CA3AF).
/// Reference: 32px total span, centered within 48px icon (8px padding each side).
fn draw_multi_dash_indicator(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    icon_x: f32,
    indicator_y: f32,
    icon_size: f32,
    window_count: usize,
    is_active: bool,
) {
    // Total width of the indicator group (32px from reference)
    let total_width = 32.0f32;
    let dash_height = 2.0f32;
    let dash_spacing = 3.0f32;

    // Calculate dash widths based on count (matching reference project proportions)
    // Reference: 1 dash = 32px, 2 dashes = 14.5px each, 3 = 8.67px, 4 = 6.25px, 5 = 4.8px
    let total_spacing = dash_spacing * (window_count as f32 - 1.0);
    let available_width = total_width - total_spacing;
    let dash_width = (available_width / window_count as f32).max(4.0);

    let start_x = icon_x + (icon_size - total_width) / 2.0;

    let color = if is_active {
        // Cyan for active/focused apps (#00E5FF)
        Color::rgb(0, 229, 255)
    } else {
        // Silver for inactive running apps (#9CA3AF)
        Color::rgb(156, 163, 175)
    };

    for i in 0..window_count {
        let dash_x = start_x + i as f32 * (dash_width + dash_spacing);
        let mut path = Path::new();
        path.rounded_rect(dash_x, indicator_y, dash_width, dash_height, 1.0);
        let paint = Paint::color(color);
        canvas.fill_path(&path, &paint);
    }
}

/// Draw a badge using FemtoVG.
fn draw_badge_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    x: f32,
    y: f32,
    badge: &BadgeUpdate,
) {
    let count = badge.count;
    if count == 0 {
        return;
    }

    let size = 14.0;
    let mut path = Path::new();
    path.rounded_rect(x, y, size, size, 3.0);
    let paint = Paint::color(Color::rgb(220, 38, 38));
    canvas.fill_path(&path, &paint);

    let text = format!("{}", count);
    let text_x = x + size / 2.0;
    let text_y = y + size - 2.0;
    draw_text(canvas, &text, 8.0, text_x, text_y, (255, 255, 255));
}

/// Render a dragged icon using FemtoVG.
fn render_dragged_icon_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    state: &DockState,
    res: &mut DockRenderResources,
    drag_id: &String,
    icon_cache: &mut FemtoVGIconCache,
) {
    let box_size = state.pins.first().map(|p| p.size as f32).unwrap_or(48.0);
    let (pointer_x, pointer_y) = res.pointer_position;
    let drag_start_x = pointer_x as f32 - box_size / 2.0;
    let drag_start_y = pointer_y as f32 - box_size / 2.0;

    // Find the icon for the dragged app
    let cached_icon = res.icon_cache.get(drag_id).map(|(rgba, size)| (rgba.as_slice(), *size));

    if let Some((icon_pixels, img_size_u32)) = cached_icon {
        icon_cache.get_or_create(
            canvas,
            drag_id,
            icon_pixels,
            img_size_u32,
            img_size_u32,
        );
        icon_cache.draw(
            canvas,
            drag_id,
            drag_start_x,
            drag_start_y,
            box_size,
            box_size,
            0.85,
        );
    } else {
        // Placeholder for dragged icon
        let hash = drag_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        let hue = (hash % 360) as f32;
        draw_icon_placeholder(canvas, drag_start_x, drag_start_y, box_size, hue, 0.85);
    }
}
