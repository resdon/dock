use crate::models::{BadgeUpdate, WindowDiagnostics};
use crate::render::font::FontManager;
use crate::render::{draw_canvas_badge, draw_text, hsl_to_rgb};
use crate::types::DockState;

use crate::animations::IconAnimation;
use std::collections::HashMap;

pub struct DockRenderResources<'a> {
    pub phys_width: u32,
    pub phys_height: u32,
    pub running_by_app: &'a HashMap<String, Vec<&'a WindowDiagnostics>>,
    pub icon_cache: &'a HashMap<String, (Vec<u8>, u32)>,
    pub badges: &'a HashMap<String, BadgeUpdate>,
    pub font_manager: &'a mut FontManager,
    pub fallback_anim: &'a IconAnimation,
    pub is_dragging: bool,
    pub dragged_app_id: Option<&'a String>,
    pub pointer_position: (i32, i32),
}

pub fn render_dock_surface(canvas: &mut [u8], state: &DockState, res: &mut DockRenderResources) {
    render_dock_background(
        canvas,
        res.phys_width,
        res.phys_height,
        state.panel_rect.height as usize,
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
    dock_height: usize,
    alpha: f32,
) {
    let dock_start_y = (phys_height as usize).saturating_sub(dock_height);
    let bg_color = (0x11 as f32 * alpha) as u8;
    let alpha_byte = (255.0 * alpha.clamp(0.0, 1.0)) as u8;
    let bg_pixel = [bg_color, bg_color, bg_color, alpha_byte];
    let transparent_pixel = [0x00, 0x00, 0x00, 0x00];

    let stride = phys_width as usize * 4;
    for y in 0..phys_height as usize {
        let row_start = y * stride;
        let row_end = row_start + stride;
        if row_end > canvas.len() {
            break;
        }

        let pixel_data = if y >= dock_start_y {
            bg_pixel
        } else {
            transparent_pixel
        };
        for pixel in canvas[row_start..row_end].chunks_exact_mut(4) {
            pixel.copy_from_slice(&pixel_data);
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
        let icon = windows
            .and_then(|v| {
                v.first().and_then(|w| {
                    w.icon_rgba
                        .as_ref()
                        .map(|rgba| (rgba.as_slice(), w.icon_size))
                })
            })
            .or_else(|| {
                res.icon_cache
                    .get(&pin.app_id)
                    .map(|(v, s)| (v.as_slice(), *s))
            });

        if let Some((icon_pixels, img_size_u32)) = icon {
            let img_size = img_size_u32 as usize;
            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;
                    let src_x = (x * img_size) / box_size;
                    let src_y = (y * img_size) / box_size;
                    let src_idx = (src_y * img_size + src_x) * 4;

                    if src_idx + 3 < icon_pixels.len()
                        && canvas_x < phys_width
                        && canvas_y < phys_height
                    {
                        let canvas_idx = (canvas_y * phys_width + canvas_x) * 4;
                        if canvas_idx + 3 < canvas.len() {
                            let alpha = icon_pixels[src_idx + 3] as f32 / 255.0;
                            if alpha > 0.0 {
                                let dim = if pin.is_running { 1.0 } else { 0.5 };
                                let src_r = icon_pixels[src_idx] as f32;
                                let src_g = icon_pixels[src_idx + 1] as f32;
                                let src_b = icon_pixels[src_idx + 2] as f32;

                                // Wayland ARGB8888 Little-Endian Canvas Layout: [B, G, R, A]
                                canvas[canvas_idx] = ((src_b * alpha * dim)
                                    + (canvas[canvas_idx] as f32 * (1.0 - alpha)))
                                    as u8;
                                canvas[canvas_idx + 1] = ((src_g * alpha * dim)
                                    + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha)))
                                    as u8;
                                canvas[canvas_idx + 2] = ((src_r * alpha * dim)
                                    + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha)))
                                    as u8;
                                canvas[canvas_idx + 3] = 255;
                            }
                        }
                    }
                }
            }
        } else if let Some(frame) = res.fallback_anim.current_frame() {
            let frame_size = frame.width as usize;
            let offset_x = start_x + (box_size.saturating_sub(frame_size)) / 2;
            let offset_y = start_y + (box_size.saturating_sub(frame_size)) / 2;

            for fy in 0..frame_size {
                for fx in 0..frame_size {
                    let canvas_x = offset_x + fx;
                    let canvas_y = offset_y + fy;

                    if canvas_x < phys_width && canvas_y < phys_height {
                        let src_idx = (fy * frame_size + fx) * 4;
                        if src_idx + 3 < frame.rgba.len() {
                            let r = frame.rgba[src_idx] as u32;
                            let g = frame.rgba[src_idx + 1] as u32;
                            let b = frame.rgba[src_idx + 2] as u32;
                            let a = frame.rgba[src_idx + 3] as u32;

                            if a > 0 {
                                let canvas_idx = (canvas_y * phys_width + canvas_x) * 4;
                                if canvas_idx + 3 < canvas.len() {
                                    let inv_a = 255 - a;
                                    // Canvas [B, G, R, A]
                                    canvas[canvas_idx] =
                                        ((b * a + canvas[canvas_idx] as u32 * inv_a) / 255) as u8;
                                    canvas[canvas_idx + 1] =
                                        ((g * a + canvas[canvas_idx + 1] as u32 * inv_a) / 255)
                                            as u8;
                                    canvas[canvas_idx + 2] =
                                        ((r * a + canvas[canvas_idx + 2] as u32 * inv_a) / 255)
                                            as u8;
                                    canvas[canvas_idx + 3] = 255;
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let letter = pin
                .app_id
                .chars()
                .next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();
            let font_size = (24.0f32 * pin.scale_factor) as i32;
            let text_x = (start_x as f32 + 16.0f32 * pin.scale_factor).round() as i32;
            let text_y = (start_y as f32 + 11.0f32 * pin.scale_factor).round() as i32;
            draw_text(
                canvas,
                res.phys_width as i32,
                res.phys_height as i32,
                res.font_manager,
                &letter,
                font_size,
                text_x,
                text_y,
                (255, 255, 255),
            );
        }

        // Render tracking indicator dash(es) with configurable thickness
        if pin.is_running {
            let indicator_y = start_y + box_size + 2; // 4 pixels below the icon box
            let dash_height = 3; // Height (thickness) in pixels (e.g., 4px)

            if indicator_y < phys_height {
                let num_dashes = if pin.running_count > 0 {
                    pin.running_count.min(5)
                } else {
                    1
                };

                let total_line_width = 28.0f32; // Fixed total width of the indicator group
                let dash_spacing = 3.0f32; // Spacing between multiple dashes
                let total_spacing = dash_spacing * (num_dashes as f32 - 1.0f32).max(0.0f32);
                let dash_width =
                    ((total_line_width - total_spacing) / num_dashes as f32).max(4.0f32);

                let indicator_start_x =
                    start_x as f32 + (box_size as f32 - total_line_width) / 2.0f32;

                let is_app_active = windows
                    .map_or(pin.is_activated, |wins| wins.iter().any(|w| w.is_activated))
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
                                            // Cyan indicator in [B, G, R, A] format: B=0xFF, G=0xFF, R=0x00
                                            canvas[canvas_idx] = 0xFF;
                                            canvas[canvas_idx + 1] = 0xFF;
                                            canvas[canvas_idx + 2] = 0x00;
                                        } else {
                                            let brightness = 0x66;
                                            canvas[canvas_idx] = brightness;
                                            canvas[canvas_idx + 1] = brightness;
                                            canvas[canvas_idx + 2] = brightness;
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
    let icon = windows
        .and_then(|v| {
            v.first().and_then(|w| {
                w.icon_rgba
                    .as_ref()
                    .map(|rgba| (rgba.as_slice(), w.icon_size))
            })
        })
        .or_else(|| res.icon_cache.get(drag_id).map(|(v, s)| (v.as_slice(), *s)));

    let drag_start_x = res.pointer_position.0.saturating_sub(box_size / 2);
    let drag_start_y = res.pointer_position.1.saturating_sub(box_size / 2);

    if let Some((icon_pixels, img_size_u32)) = icon {
        let img_size = img_size_u32 as i32;
        for y in 0..box_size {
            for x in 0..box_size {
                let canvas_x = drag_start_x + x;
                let canvas_y = drag_start_y + y;
                let src_x = (x * img_size) / box_size;
                let src_y = (y * img_size) / box_size;
                let src_idx = (src_y * img_size + src_x) * 4;

                if canvas_x >= 0
                    && canvas_x < phys_width
                    && canvas_y >= 0
                    && canvas_y < phys_height
                    && src_idx >= 0
                    && (src_idx + 3) < icon_pixels.len() as i32
                {
                    let canvas_idx = ((canvas_y * phys_width + canvas_x) * 4) as usize;
                    if canvas_idx + 3 < canvas.len() {
                        let alpha = (icon_pixels[(src_idx + 3) as usize] as f32 / 255.0) * 0.85;
                        if alpha > 0.0 {
                            let src_r = icon_pixels[src_idx as usize] as f32;
                            let src_g = icon_pixels[(src_idx + 1) as usize] as f32;
                            let src_b = icon_pixels[(src_idx + 2) as usize] as f32;

                            // Canvas [B, G, R, A]
                            canvas[canvas_idx] = ((src_b * alpha)
                                + (canvas[canvas_idx] as f32 * (1.0 - alpha)))
                                as u8;
                            canvas[canvas_idx + 1] = ((src_g * alpha)
                                + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha)))
                                as u8;
                            canvas[canvas_idx + 2] = ((src_r * alpha)
                                + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha)))
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

        let letter = drag_id
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        let font_size = 24;
        let offset_x = 16;
        let offset_y = 11;

        draw_text(
            canvas,
            res.phys_width as i32,
            res.phys_height as i32,
            res.font_manager,
            &letter,
            font_size,
            drag_start_x + offset_x,
            drag_start_y + offset_y,
            (255, 255, 255),
        );
    }
}
