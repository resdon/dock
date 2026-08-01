use std::collections::HashMap;
use crate::models::WindowDiagnostics;
use crate::modules::context_menu::DOCK_HEIGHT;
use crate::{MenuState, HoverState, FontManager};

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
                        canvas[canvas_idx] = ((color.0 as f32 * alpha) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 1] = ((color.1 as f32 * alpha) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 2] = ((color.2 as f32 * alpha) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 3] = 255;
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
	// Sort by app_name first, then fall back to title (or a unique window ID) for stability
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
                            canvas[canvas_idx]     = ((icon_pixels[src_idx + 2] as f32 * alpha * dim) + (canvas[canvas_idx] as f32 * (1.0 - alpha))) as u8; // Blue
                            canvas[canvas_idx + 1] = ((icon_pixels[src_idx + 1] as f32 * alpha * dim) + (canvas[canvas_idx + 1] as f32 * (1.0 - alpha))) as u8; // Green
                            canvas[canvas_idx + 2] = ((icon_pixels[src_idx]     as f32 * alpha * dim) + (canvas[canvas_idx + 2] as f32 * (1.0 - alpha))) as u8; // Red
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }
        } else {
            // Fallback tile with gradient and centered letter
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

            let hash = app_id.bytes().fold(0u32, |acc, b| acc.wrapping_add(b as u32));
            let hue = (hash % 360) as f32;
            let base_color = hsl_to_rgb(hue, 0.6, 0.55);
            let grad_color = hsl_to_rgb(hue, 0.6, 0.40);

            for y in 0..box_size {
                for x in 0..box_size {
                    let canvas_x = start_x + x;
                    let canvas_y = start_y + y;
                    if canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                        if is_inside_rounded_rect(x, y) {
                            let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                            let t = y as f32 / box_size as f32;
                            let dim = if is_running { 1.0 } else { 0.5 };
                            let r = (base_color.0 as f32 * (1.0 - t) + grad_color.0 as f32 * t) * dim;
                            let g = (base_color.1 as f32 * (1.0 - t) + grad_color.1 as f32 * t) * dim;
                            let b = (base_color.2 as f32 * (1.0 - t) + grad_color.2 as f32 * t) * dim;
                            
                            canvas[canvas_idx] = r as u8;
                            canvas[canvas_idx + 1] = g as u8;
                            canvas[canvas_idx + 2] = b as u8;
                            canvas[canvas_idx + 3] = 255;
                        }
                    }
                }
            }

            // Capitalized first letter
            let letter = app_id.chars().next()
                .unwrap_or('?')
                .to_uppercase()
                .to_string();

            // Render letter centered (scaled)
            let font_size = 24.0 * scale_factor as f32;
            let offset_x = (16.0 * scale_factor).round() as usize;
            let offset_y = (11.0 * scale_factor).round() as usize;

            draw_text(canvas, phys_width, phys_height, font_manager, &letter, font_size, start_x + offset_x, start_y + offset_y, (255, 255, 255));
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
                                // Yellow in BGRA format: Blue = 0x00, Green = 0xFF, Red = 0xFF
                                canvas[canvas_idx]     = 0x00; 
                                canvas[canvas_idx + 1] = 0xFF; 
                                canvas[canvas_idx + 2] = 0xFF; 
                            } else {
                                // Inactive open windows: subtle dim gray/white
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
    }

    // 4. Render Context Menu
    if menu_state.is_open {
        let menu_width = crate::modules::context_menu::MENU_WIDTH as usize;
        let menu_height = crate::modules::context_menu::MENU_HEIGHT as usize;
        let menu_x = menu_state.x.min((phys_width as usize).saturating_sub(menu_width));
        let menu_y = menu_state.y.saturating_sub(menu_height);

        for y in 0..menu_height {
            for x in 0..menu_width {
                let canvas_x = menu_x + x;
                let canvas_y = menu_y + y;
                if canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                    let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                    canvas[canvas_idx] = 0x22; canvas[canvas_idx + 1] = 0x22; canvas[canvas_idx + 2] = 0x22; canvas[canvas_idx + 3] = 0xFF;
                }
            }
        }
        let item_h = crate::modules::context_menu::MENU_ITEM_HEIGHT as usize;
        let is_pinned = menu_state.target_app_id.as_ref().map(|id| pinned_apps.contains(id)).unwrap_or(false);
        let actions = ["Focus", "Open new", "Minimize", "Close", if is_pinned { "Unpin" } else { "Pin" }];

        for (i, label) in actions.iter().enumerate() {
            draw_text(canvas, phys_width, phys_height, font_manager, label, 14.0, menu_x + 10, menu_y + 5 + i * item_h, (255, 255, 255));
            if i > 0 {
                let line_y = menu_y + i * item_h;
                for x in 0..menu_width {
                    let canvas_x = menu_x + x;
                    if canvas_x < phys_width as usize && line_y < phys_height as usize {
                        let canvas_idx = (line_y * (phys_width as usize) + canvas_x) * 4;
                        canvas[canvas_idx] = 0x44; canvas[canvas_idx + 1] = 0x44; canvas[canvas_idx + 2] = 0x44;
                    }
                }
            }
        }
    }

    // 5. Render Hover Preview Menu
    if hover_state.is_visible && !menu_state.is_open && !is_dragging {
        if let Some(ref app_id) = hover_state.app_id {
            if let Some(windows) = running_by_app.get(app_id) {
                let (menu_x, menu_y, menu_width, menu_height) = crate::modules::context_menu::get_hover_menu_bounds(
                    hover_state.x, phys_width, phys_height, windows.len()
                );

                for y in 0..menu_height {
                    for x in 0..menu_width {
                        let canvas_x = menu_x + x;
                        let canvas_y = menu_y + y;
                        if canvas_x < phys_width as usize && canvas_y < phys_height as usize {
                            let canvas_idx = (canvas_y * (phys_width as usize) + canvas_x) * 4;
                            canvas[canvas_idx] = 0x33; canvas[canvas_idx + 1] = 0x33; canvas[canvas_idx + 2] = 0x33; canvas[canvas_idx + 3] = 0xEE;
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
                    draw_text(canvas, phys_width, phys_height, font_manager, &title, 14.0, menu_x + 5, menu_y + i * item_h + 5, (255, 255, 255));

                    let sq_size = 20;
                    let sq_x = menu_x + menu_width - sq_size - 5;
                    let sq_y = menu_y + i * item_h + 5;
                    
                    for dy in 0..sq_size {
                        for dx in 0..sq_size {
                            let cx = sq_x + dx;
                            let cy = sq_y + dy;
                            if cx < phys_width as usize && cy < phys_height as usize {
                                let idx = (cy * phys_width as usize + cx) * 4;
                                canvas[idx] = 0xE0; canvas[idx + 1] = 0x30; canvas[idx + 2] = 0x30; canvas[idx + 3] = 0xFF;
                            }
                        }
                    }
                    
                    let cross_margin = 5;
                    let cross_size = sq_size - 2 * cross_margin;
                    for t in 0..cross_size {
                        for w in 0..2 {
                            let px1 = sq_x + cross_margin + t + w;
                            let py1 = sq_y + cross_margin + t;
                            if px1 < phys_width as usize && py1 < phys_height as usize {
                                let idx = (py1 * phys_width as usize + px1) * 4;
                                canvas[idx] = 0xFF; canvas[idx+1] = 0xFF; canvas[idx+2] = 0xFF; canvas[idx+3] = 0xFF;
                            }
                            let px2 = sq_x + cross_margin + t + w;
                            let py2 = sq_y + sq_size - cross_margin - t - 1;
                            if px2 < phys_width as usize && py2 < phys_height as usize {
                                let idx = (py2 * phys_width as usize + px2) * 4;
                                canvas[idx] = 0xFF; canvas[idx+1] = 0xFF; canvas[idx+2] = 0xFF; canvas[idx+3] = 0xFF;
                            }
                        }
                    }

                    if i > 0 {
                        let line_y = menu_y + i * item_h;
                        for x in 0..menu_width {
                            let canvas_x = menu_x + x;
                            if canvas_x < phys_width as usize && line_y < phys_height as usize {
                                let canvas_idx = (line_y * (phys_width as usize) + canvas_x) * 4;
                                canvas[canvas_idx] = 0x55; canvas[canvas_idx + 1] = 0x55; canvas[canvas_idx + 2] = 0x55;
                            }
                        }
                    }
                }
            }
        }
    }

    // 6. Render Floating Dragged Icon under mouse cursor
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
