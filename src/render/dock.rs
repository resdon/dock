use std::collections::HashMap;
use crate::models::{BadgeUpdate, WindowDiagnostics};
use crate::FontManager;
use crate::render::{draw_canvas_badge, draw_text, hsl_to_rgb};

pub fn render_dock_background(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    dock_height: usize,
) {
    for y in 0..phys_height as usize {
        for x in 0..phys_width as usize {
            let canvas_idx = (y * (phys_width as usize) + x) * 4;
            if y >= (phys_height as usize - dock_height) {
                canvas[canvas_idx]     = 0x11;
                canvas[canvas_idx + 1] = 0x11;
                canvas[canvas_idx + 2] = 0x11;
                canvas[canvas_idx + 3] = 0xFF;
            } else {
                canvas[canvas_idx]     = 0x00;
                canvas[canvas_idx + 1] = 0x00;
                canvas[canvas_idx + 2] = 0x00;
                canvas[canvas_idx + 3] = 0x00;
            }
        }
    }
}

pub fn render_dock_items(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale_factor: f64,
    dock_height: usize,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<&WindowDiagnostics>>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    font_manager: &FontManager,
    fallback_anim: &dockman_lib::animations::IconAnimation,
    badges: &HashMap<String, BadgeUpdate>,
) {
    // 1. Draw dock background locally (no need to offset by phys_height anymore)
    for y in 0..dock_height {
        for x in 0..phys_width as usize {
            let idx = (y * phys_width as usize + x) * 4;
            canvas[idx..idx + 4].copy_from_slice(&[0x11, 0x11, 0x11, 0xFF]);
        }
    }

    let box_size = (48.0 * scale_factor).round() as usize;
    let spacing = (12.0 * scale_factor).round() as usize;
    let total_items = apps_in_dock.len();
    let content_width = if total_items > 0 {
        total_items * box_size + (total_items + 1) * spacing
    } else {
        0
    };
    let start_offset_x = if (phys_width as usize) > content_width {
        (phys_width as usize - content_width) / 2
    } else {
        0
    };
    let start_y: usize = (phys_height as usize - dock_height) + (dock_height - box_size) / 2;

    for (index, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_offset_x + spacing + index * (box_size + spacing);
        if start_x + box_size > phys_width as usize {
            break;
        }

        let windows = running_by_app.get(app_id);
        let is_running = windows.is_some();
        let is_activated = windows.map(|v| v.iter().any(|w| w.is_activated)).unwrap_or(false);

        let icon = windows
            .and_then(|v| v.first().and_then(|w| w.icon_rgba.as_ref().map(|rgba| (rgba.as_slice(), w.icon_size))))
            .or_else(|| icon_cache.get(app_id).map(|(v, s)| (v.as_slice(), *s)));

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
                        && canvas_x < phys_width as usize
                        && canvas_y < phys_height as usize
                    {
                        let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                        let alpha = icon_pixels[src_idx + 3] as f32 / 255.0;
                        if alpha > 0.0 {
                            let dim = if is_running { 1.0 } else { 0.5 };
                            canvas[canvas_idx]     = ((icon_pixels[src_idx + 2] as f32 * alpha * dim) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha * dim) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 2] = ((icon_pixels[src_idx]     as f32 * alpha * dim) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        } else if let Some(frame) = fallback_anim.current_frame() {
            let frame_size = frame.width as usize;
            let offset_x = start_x + (box_size.saturating_sub(frame_size)) / 2;
            let offset_y = start_y + (box_size.saturating_sub(frame_size)) / 2;

            for fy in 0..frame_size {
                for fx in 0..frame_size {
                    let canvas_x = offset_x + fx;
                    let canvas_y = offset_y + fy;

                    if canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                        let src_idx = (fy * frame_size + fx) * 4;
                        let r = frame.rgba[src_idx] as u32;
                        let g = frame.rgba[src_idx + 1] as u32;
                        let b = frame.rgba[src_idx + 2] as u32;
                        let a = frame.rgba[src_idx + 3] as u32;

                        if a > 0 {
                            let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                            let inv_a = 255 - a;

                            canvas[canvas_idx]     = ((b * a + canvas[canvas_idx] as u32 * inv_a) / 255) as u8;
                            canvas[canvas_idx + 1] = ((g * a + canvas[canvas_idx + 1] as u32 * inv_a) / 255) as u8;
                            canvas[canvas_idx + 2] = ((r * a + canvas[canvas_idx + 2] as u32 * inv_a) / 255) as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        } else {
            let letter = app_id.chars().next().unwrap_or('?').to_uppercase().to_string();
            let font_size = 24.0 * scale_factor as f32;
            let text_x = start_x + (16.0 * scale_factor).round() as usize;
            let text_y = start_y + (11.0 * scale_factor).round() as usize;
            draw_text(canvas, phys_width as i32, phys_height as i32, font_manager, &letter, font_size as i32, text_x as i32, text_y as i32, (255, 255, 255));
        }

        // Render tracking indicator dash(es)
        if is_running {
            let indicator_y = start_y + box_size + (4.0 * scale_factor).round() as usize;
            if indicator_y < phys_height as usize {
                let running_count = windows.map_or(1, |v| v.len());
                let max_dashes = 5;
                let num_dashes = running_count.min(max_dashes);

                let total_line_width = 32.0 * scale_factor;
                let spacing = (2.0 * scale_factor).round();
                let total_spacing = spacing * (num_dashes as f64 - 1.0).max(0.0);
                let dash_width = ((total_line_width - total_spacing) / num_dashes as f64).max(1.0);

                let indicator_start_x = start_x as f64 + (box_size as f64 - total_line_width) / 2.0;

                for i in 0..num_dashes {
                    let dash_start_x = indicator_start_x + (i as f64 * (dash_width + spacing));
                    let dash_end_x = dash_start_x + dash_width;

                    let x_start = dash_start_x.round() as usize;
                    let x_end = dash_end_x.round() as usize;

                    for canvas_x in x_start..x_end {
                        if canvas_x < phys_width as usize {
                            let canvas_idx = (indicator_y * (phys_width as usize) + canvas_x) * 4;
                            if is_activated {
                                canvas[canvas_idx]     = 0x00;
                                canvas[canvas_idx + 1] = 0xFF;
                                canvas[canvas_idx + 2] = 0xFF;
                            } else {
                                let brightness = 0x66;
                                canvas[canvas_idx]     = brightness;
                                canvas[canvas_idx + 1] = brightness;
                                canvas[canvas_idx + 2] = brightness;
                            }
                            canvas[canvas_idx + 3] = 0xFF;
                        }
                    }
                }
            }
        }

        // Draw Notification Badge overlay
        if let Some(badge) = badges.get(app_id) {
            draw_canvas_badge(
                canvas,
                phys_width,
                phys_height,
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
    phys_width: u32,
    phys_height: u32,
    scale_factor: f64,
    drag_id: &str,
    running_by_app: &HashMap<String, Vec<&WindowDiagnostics>>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    pointer_x: i32,
    pointer_y: i32,
    font_manager: &FontManager,
) {
    let box_size = (48.0 * scale_factor).round() as i32;
    let windows = running_by_app.get(drag_id);
    let icon = windows
        .and_then(|v| v.first().and_then(|w| w.icon_rgba.as_ref().map(|rgba| (rgba.as_slice(), w.icon_size))))
        .or_else(|| icon_cache.get(drag_id).map(|(v, s)| (v.as_slice(), *s)));

    let drag_start_x = pointer_x.saturating_sub(box_size / 2);
    let drag_start_y = pointer_y.saturating_sub(box_size / 2);

    if let Some((icon_pixels, img_size_u32)) = icon {
        let img_size = img_size_u32 as i32;
        for y in 0..box_size {
            for x in 0..box_size {
                let canvas_x = drag_start_x + x;
                let canvas_y = drag_start_y + y;
                let src_x = (x * img_size) / box_size;
                let src_y = (y * img_size) / box_size;
                let src_idx = (src_y * img_size + src_x) * 4;

                if src_idx + 3 < icon_pixels.len() as i32
                    && src_idx + 3 < icon_pixels.len() as i32
                    && canvas_x < phys_width as i32
                    && canvas_y < phys_height as i32
                    && canvas_x >= 0
                    && canvas_y >= 0
                {
                    let canvas_idx = ((canvas_y * (phys_width as i32) + canvas_x) * 4) as usize;
                    let alpha = (icon_pixels[(src_idx + 3) as usize] as f32 / 255.0) * 0.85;
                    if alpha > 0.0 {
                        canvas[canvas_idx]     = ((icon_pixels[(src_idx + 2) as usize] as f32 * alpha) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 1] = ((icon_pixels[(src_idx + 1) as usize] as f32 * alpha) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 2] = ((icon_pixels[(src_idx) as usize]     as f32 * alpha) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }
    } else {
        let radius = 10.0 * scale_factor;
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

        let hash = drag_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
        let hue = (hash % 360) as f32;
        let base_color = hsl_to_rgb(hue, 0.6, 0.55);
        let grad_color = hsl_to_rgb(hue, 0.6, 0.40);

        for y in 0..box_size {
            for x in 0..box_size {
                let canvas_x = drag_start_x + x;
                let canvas_y = drag_start_y + y;
                if canvas_x < phys_width as i32 && canvas_y < phys_height as i32 {
                    if is_inside_rounded_rect(x as usize, y as usize) {
                        let canvas_idx = ((canvas_y * (phys_width as i32) + canvas_x) * 4) as usize;
                        let t = y as f32 / box_size as f32;
                        let r = base_color.0 as f32 * (1.0 - t) + grad_color.0 as f32 * t;
                        let g = base_color.1 as f32 * (1.0 - t) + grad_color.1 as f32 * t;
                        let b = base_color.2 as f32 * (1.0 - t) + grad_color.2 as f32 * t;

                        canvas[canvas_idx]     = r as u8;
                        canvas[canvas_idx + 1] = g as u8;
                        canvas[canvas_idx + 2] = b as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }

        let letter = drag_id.chars().next().unwrap_or('?').to_uppercase().to_string();
        let font_size = 24.0 * scale_factor as f32;
        let offset_x = (16.0 * scale_factor).round() as i32;
        let offset_y = (11.0 * scale_factor).round() as i32;

        draw_text(
            canvas,
            phys_width as i32,
            phys_height as i32,
            font_manager,
            &letter,
            font_size as i32,
            drag_start_x + offset_x,
            drag_start_y + offset_y,
            (255, 255, 255),
        );
    }
}