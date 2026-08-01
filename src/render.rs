use std::collections::HashMap;
use crate::models::WindowDiagnostics;
use crate::modules::context_menu::DOCK_HEIGHT;
use crate::{MenuState, HoverState, FontManager};
use crate::modules::dock_item::DockItem;
use crate::modules::dbus_unity::BadgeUpdate;
use crate::modules::context_menu::get_hover_menu_bounds;

use cairo;
use image::GenericImageView;

// dbus_unity notifications
// Software badge drawer for raw BGRA canvas
// Embed the image bytes directly into the compiled binary
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../assets/dialog-warning.png");

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
    // Get the raw &[u8] slice from RGBA buffer
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

            if src_a == 0 { continue; } // Skip transparent pixels

            let dst_idx = (canvas_y * canvas_width as usize + canvas_x) * 4;

            // BGRA software canvas alpha blending
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

        // Progress Track Background
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.7);
        cr.rectangle(bar_x, bar_y, bar_w, bar_h);
        let _ = cr.fill();

        // Progress Fill
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

        // Badge Red Background Pill
        cr.set_source_rgb(0.9, 0.2, 0.2); // Vibrant red
        cr.arc(badge_cx, badge_cy, radius, 0.0, 2.0 * std::f64::consts::PI);
        let _ = cr.fill();

        // Badge Text (White)
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
// ------
pub fn draw_text(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    font_manager: &FontManager,
    text: &str,
    size: f32,
    start_x: usize,
    start_y: usize,
    color: (u8, u8, u8),
) {
    let mut x_offset = start_x;
    for c in text.chars() {
        let font = font_manager.get_font(c);
        let (metrics, bitmap) = font.rasterize(c, size);
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let canvas_x = (x_offset as isize + x as isize + metrics.xmin as isize) as usize;
                let canvas_y = (start_y as isize + y as isize + (size as isize - metrics.height as isize - metrics.ymin as isize)) as usize;

                if canvas_x < canvas_width as usize && canvas_y < canvas_height as usize {
                    let canvas_idx = (canvas_y * canvas_width as usize + canvas_x) * 4;
                    let alpha = bitmap[y * metrics.width + x] as f32 / 255.0;
                    if alpha > 0.0 {
                        let b = color.2 as f32; // Blue component
                        let g = color.1 as f32; // Green component
                        let r = color.0 as f32; // Red component

                        let cur_b = canvas[canvas_idx] as f32;
                        let cur_g = canvas[canvas_idx + 1] as f32;
                        let cur_r = canvas[canvas_idx + 2] as f32;

                        // Apply alpha blending with BGRA layout
                        canvas[canvas_idx]     = ((b * alpha) + (cur_b * (1.0 - alpha))) as u8; // Blue
                        canvas[canvas_idx + 1] = ((g * alpha) + (cur_g * (1.0 - alpha))) as u8; // Green
                        canvas[canvas_idx + 2] = ((r * alpha) + (cur_r * (1.0 - alpha))) as u8; // Red
                        canvas[canvas_idx + 3] = 255;                                            // Alpha
                    }
                }
            }
        }
        x_offset += metrics.advance_width as usize;
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (
        ((r + m) * 255.0) as u8,
        ((g + m) * 255.0) as u8,
        ((b + m) * 255.0) as u8,
    )
}

pub fn render_windows(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale_factor: f64,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    pinned_apps: &Vec<String>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    menu_state: &MenuState,
    hover_state: &HoverState,
    font_manager: &FontManager,
    is_dragging: bool,
    dragged_app_id: Option<&String>,
    pointer_x: usize,
    pointer_y: usize,
    fallback_anim: &dockman_lib::animations::IconAnimation,
    badges: &HashMap<String, BadgeUpdate>,
) {
    // 1. Clear background
    let dock_height = (DOCK_HEIGHT as f64 * scale_factor).round() as usize;
    for y in 0..phys_height as usize {
        for x in 0..phys_width as usize {
            let canvas_idx = (y * (phys_width as usize) + x) * 4;
            if y >= (phys_height as usize - dock_height) {
                canvas[canvas_idx] = 0x11; canvas[canvas_idx + 1] = 0x11; canvas[canvas_idx + 2] = 0x11; canvas[canvas_idx + 3] = 0xFF;
            } else {
                canvas[canvas_idx] = 0x00; canvas[canvas_idx + 1] = 0x00; canvas[canvas_idx + 2] = 0x00; canvas[canvas_idx + 3] = 0x00;
            }
        }
    }

    // 2. Prepare items for rendering with GROUPING
    let mut apps_in_dock = Vec::new();
    let mut running_by_app: HashMap<String, Vec<&WindowDiagnostics>> = HashMap::new();
    
    // Order from pinned apps first
    for app_id in pinned_apps {
        if !apps_in_dock.contains(app_id) {
            apps_in_dock.push(app_id.clone());
        }
    }

    // Then add running apps not in pinned
    let mut sorted_windows: Vec<&WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by(|a, b| {
        a.app_name.cmp(&b.app_name)
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| std::ptr::from_ref(*a).cmp(&std::ptr::from_ref(*b)))
    });
        
    for w in sorted_windows {
        let app_id = if !w.app_id.is_empty() {
            w.app_id.clone()
        } else if !w.title.is_empty() {
            w.title.clone()
        } else {
            "Unknown".to_string()
        };

        running_by_app.entry(app_id.clone()).or_insert_with(Vec::new).push(w);
        if !apps_in_dock.contains(&app_id) {
            apps_in_dock.push(app_id);
        }
    }

    // Layout configuration
    let box_size = (48.0 * scale_factor).round() as usize; 
    let spacing = (12.0 * scale_factor).round() as usize;
    let total_items = apps_in_dock.len();
    let content_width = if total_items > 0 { total_items * box_size + (total_items + 1) * spacing } else { 0 };
    let start_offset_x = if (phys_width as usize) > content_width { (phys_width as usize - content_width) / 2 } else { 0 };
    let start_y: usize = (phys_height as usize - dock_height) + (dock_height - box_size) / 2;

    for (index, app_id) in apps_in_dock.iter().enumerate() {
        let start_x = start_offset_x + spacing + index * (box_size + spacing);
        if start_x + box_size > phys_width as usize { break; }

        let windows = running_by_app.get(app_id);
        let is_running = windows.is_some();
        let is_activated = windows.map(|v| v.iter().any(|w| w.is_activated)).unwrap_or(false);

        // Get icon from cache or from first running window
        let icon = windows.and_then(|v| v.first().and_then(|w| w.icon_rgba.as_ref().map(|rgba| (rgba.as_slice(), w.icon_size))))
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

                    if src_idx + 3 < icon_pixels.len() && canvas_x < phys_width as usize && canvas_y < phys_height as usize {
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
            // Draw current active frame from IconAnimation controller
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
            // Fallback text icon if animation fails/empty
            let letter = app_id.chars().next().unwrap_or('?').to_uppercase().to_string();
            let font_size = 24.0 * scale_factor as f32;
            let text_x = start_x + (16.0 * scale_factor).round() as usize;
            let text_y = start_y + (11.0 * scale_factor).round() as usize;
            draw_text(canvas, phys_width, phys_height, font_manager, &letter, font_size, text_x, text_y, (255, 255, 255));
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
                start_x,
                start_y,
                box_size,
            );
        }
    }

    // Context Menu Rendering
    if menu_state.is_open && !menu_state.items.is_empty() {
        // 1. Calculate menu dimensions
        let item_h = (30.0 * scale_factor).round() as usize;
        let menu_width = (180.0 * scale_factor).round() as usize;

        // Cap total_menu_h so it NEVER exceeds the screen height itself
        let raw_menu_h = menu_state.items.len() * item_h;
        let total_menu_h = raw_menu_h.min(phys_height as usize);

        // 2. Convert cursor coordinates with scaling
        let cursor_x = (menu_state.x as f64 * scale_factor).round() as usize;
        let cursor_y = (menu_state.y as f64 * scale_factor).round() as usize;

        // 3. Horizontal clamping (prevent going off right edge)
        let max_x = (phys_width as usize).saturating_sub(menu_width);
        let menu_x = cursor_x.min(max_x);

        // 4. Vertical positioning & clamping (prevent going off bottom edge)
        let ideal_y = cursor_y.saturating_sub(total_menu_h);
        let max_y = (phys_height as usize).saturating_sub(total_menu_h);

        // menu_y will now sit right above the cursor UNLESS that pushes the bottom past screen bounds
        let menu_y = ideal_y.min(max_y);
        
        // 1. Calculate which item row is hovered ONCE outside the loop
        let hovered_item_idx = if pointer_x >= menu_x
            && pointer_x < menu_x + menu_width
            && pointer_y >= menu_y
            && pointer_y < menu_y + total_menu_h
        {
            Some((pointer_y - menu_y) / item_h)
        } else {
            None
        };

        // 2. Draw background and hover effects
        for y in 0..total_menu_h {
            let item_idx = y / item_h;
            let is_hovered = Some(item_idx) == hovered_item_idx;

            // Select color slice based on hover state (B, G, R, A)
            let bg_color = if is_hovered {
                [0x3A, 0x3A, 0x3A, 0xFF] // Hover highlight
            } else {
                [0x22, 0x22, 0x22, 0xFF] // Default background
            };

            let cy = menu_y + y;
            if cy >= phys_height as usize {
                continue;
            }

            for x in 0..menu_width {
                let cx = menu_x + x;
                if cx < phys_width as usize {
                    let idx = (cy * phys_width as usize + cx) * 4;
                    canvas[idx..idx + 4].copy_from_slice(&bg_color);
                }
            }
        }

        // Render menu item labels
        for (i, item) in menu_state.items.iter().enumerate() {
            let text_x = menu_x + (12.0 * scale_factor).round() as usize;
            let text_y = menu_y + i * item_h + (6.0 * scale_factor).round() as usize;
            let text_color = if matches!(item.item_type, crate::MenuItemType::CloseApp) {
                (255, 100, 100) // Soft red highlight for Quit
            } else {
                (220, 220, 220)
            };

            draw_text(
                canvas,
                phys_width,
                phys_height,
                font_manager,
                &item.label,
                14.0 * scale_factor as f32,
                text_x,
                text_y,
                text_color,
            );
        }
    }
    // Hover Preview Menu Rendering
    if hover_state.is_visible && !menu_state.is_open && !is_dragging {
        if let Some(ref app_id) = hover_state.app_id {
            if let Some(windows) = running_by_app.get(app_id) {
                let (menu_x, menu_y, menu_width, menu_height) = get_hover_menu_bounds(
                    hover_state.x,
                    phys_width as usize,
                    phys_height as usize,
                    windows.len(),
                    scale_factor,
                );

                for y in 0..(menu_height as usize) {
                    for x in 0..(menu_width as usize) {
                        let canvas_x = menu_x + (x as f64);
                        let canvas_y = menu_y + (y as f64);
                        if canvas_x >= 0.0 && canvas_y >= 0.0 {
                            let cx = canvas_x as usize;
                            let cy = canvas_y as usize;
                            if cx < phys_width as usize && cy < phys_height as usize {
                                let canvas_idx = (cy * (phys_width as usize) + cx) * 4;
                                canvas[canvas_idx] = 0x33; canvas[canvas_idx + 1] = 0x33; canvas[canvas_idx + 2] = 0x33; canvas[canvas_idx + 3] = 0xEE;
                            }
                        }
                    }
                }

                let item_h = crate::modules::context_menu::HOVER_ITEM_HEIGHT as usize;
                for (i, w) in windows.iter().enumerate() {
                    let title = if w.title.chars().count() > 20 { 
                        format!("{}...", w.title.chars().take(17).collect::<String>()) 
                    } else { 
                        w.title.clone() 
                    };
                    draw_text(
                        canvas, 
                        phys_width, 
                        phys_height, 
                        font_manager, 
                        &title, 
                        14.0, 
                        (menu_x + 5.0) as usize, 
                        (menu_y + (i as f64) * (item_h as f64) + 5.0) as usize, 
                        (255, 255, 255)
                    );

                    let sq_size = 20.0;
                    let sq_x = menu_x + menu_width - sq_size - 5.0;
                    let sq_y = menu_y + (i as f64) * (item_h as f64) + 5.0;
                    
                    for dy in 0..(sq_size as usize) {
                        for dx in 0..(sq_size as usize) {
                            let cx = sq_x + (dx as f64);
                            let cy = sq_y + (dy as f64);
                            if cx >= 0.0 && cy >= 0.0 {
                                let cxi = cx as usize;
                                let cyi = cy as usize;
                                if cxi < phys_width as usize && cyi < phys_height as usize {
                                    let idx = (cyi * phys_width as usize + cxi) * 4;
                                    canvas[idx] = 0xE0; canvas[idx + 1] = 0x30; canvas[idx + 2] = 0x30; canvas[idx + 3] = 0xFF;
                                }
                            }
                        }
                    }
                    
                    let cross_margin = 5.0;
                    let cross_size = sq_size - 2.0 * cross_margin;
                    for t in 0..(cross_size as usize) {
                        for w_idx in 0..2 {
                            let px1 = sq_x + cross_margin + (t as f64) + (w_idx as f64);
                            let py1 = sq_y + cross_margin + (t as f64);
                            if px1 >= 0.0 && py1 >= 0.0 {
                                let px1i = px1 as usize;
                                let py1i = py1 as usize;
                                if px1i < phys_width as usize && py1i < phys_height as usize {
                                    let idx = (py1i * phys_width as usize + px1i) * 4;
                                    canvas[idx] = 0xFF; canvas[idx+1] = 0xFF; canvas[idx+2] = 0xFF; canvas[idx+3] = 0xFF;
                                }
                            }
                            let px2 = sq_x + cross_margin + (t as f64) + (w_idx as f64);
                            let py2 = sq_y + sq_size - cross_margin - (t as f64) - 1.0;
                            if px2 >= 0.0 && py2 >= 0.0 {
                                let px2i = px2 as usize;
                                let py2i = py2 as usize;
                                if px2i < phys_width as usize && py2i < phys_height as usize {
                                    let idx = (py2i * phys_width as usize + px2i) * 4;
                                    canvas[idx] = 0xFF; canvas[idx+1] = 0xFF; canvas[idx+2] = 0xFF; canvas[idx+3] = 0xFF;
                                }
                            }
                        }
                    }

                    if i > 0 {
                        let line_y = (menu_y + (i as f64) * (item_h as f64)) as usize;
                        for x in 0..(menu_width as usize) {
                            let canvas_x = (menu_x as usize) + x;
                            if canvas_x < phys_width as usize && line_y < phys_height as usize {
                                let canvas_idx = (line_y * (phys_width as usize) + canvas_x) * 4;
                                canvas[canvas_idx] = 0x55; canvas[canvas_idx + 1] = 0x55; canvas[canvas_idx + 2] = 0x55; canvas[canvas_idx + 3] = 0xFF;
                            }
                        }
                    }
                }
            }
        }
    }

    // Dragged Icon Rendering
    if is_dragging {
        if let Some(drag_id) = dragged_app_id {
            let windows = running_by_app.get(drag_id);
            let icon = windows.and_then(|v| v.first().and_then(|w| w.icon_rgba.as_ref().map(|rgba| (rgba.as_slice(), w.icon_size))))
                        .or_else(|| icon_cache.get(drag_id).map(|(v, s)| (v.as_slice(), *s)));

            let drag_start_x = pointer_x.saturating_sub(box_size / 2);
            let drag_start_y = pointer_y.saturating_sub(box_size / 2);

            if let Some((icon_pixels, img_size_u32)) = icon {
                let img_size = img_size_u32 as usize;
                for y in 0..box_size {
                    for x in 0..box_size {
                        let canvas_x = drag_start_x + x;
                        let canvas_y = drag_start_y + y;
                        let src_x = (x * img_size) / box_size;
                        let src_y = (y * img_size) / box_size;
                        let src_idx = (src_y * img_size + src_x) * 4;

                        if src_idx + 3 < icon_pixels.len() && canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                            let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                            let alpha = (icon_pixels[src_idx + 3] as f32 / 255.0) * 0.85;
                            if alpha > 0.0 {
                                canvas[canvas_idx]     = ((icon_pixels[src_idx + 2] as f32 * alpha) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                                canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                                canvas[canvas_idx + 2] = ((icon_pixels[src_idx]     as f32 * alpha) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
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
                        if canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                            if is_inside_rounded_rect(x, y) {
                                let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                                let t = y as f32 / box_size as f32;
                                let r = base_color.0 as f32 * (1.0 - t) + grad_color.0 as f32 * t;
                                let g = base_color.1 as f32 * (1.0 - t) + grad_color.1 as f32 * t;
                                let b = base_color.2 as f32 * (1.0 - t) + grad_color.2 as f32 * t;
                                
                                canvas[canvas_idx] = r as u8;
                                canvas[canvas_idx + 1] = g as u8;
                                canvas[canvas_idx + 2] = b as u8;
                                canvas[canvas_idx + 3] = 255;
                            }
                        }
                    }
                }

                let letter = drag_id.chars().next()
                    .unwrap_or('?')
                    .to_uppercase()
                    .to_string();

                let font_size = 24.0 * scale_factor as f32;
                let offset_x = (16.0 * scale_factor).round() as usize;
                let offset_y = (11.0 * scale_factor).round() as usize;

                draw_text(canvas, phys_width, phys_height, font_manager, &letter, font_size, drag_start_x + offset_x, drag_start_y + offset_y, (255, 255, 255));
            }
        }
    }
}