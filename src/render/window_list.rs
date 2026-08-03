use std::collections::HashMap;
use crate::models::WindowDiagnostics;

pub struct PreparedWindowList<'a> {
    pub apps_in_dock: Vec<String>,
    pub running_by_app: HashMap<String, Vec<&'a WindowDiagnostics>>,
}

pub fn render_window_list_surface(
    canvas: &mut [u8],
    menu_width: i32,
    menu_height: i32,
    scale_factor: f32,
    windows: &[&crate::models::WindowDiagnostics],
    font_manager: &crate::FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
) {
    
    if menu_width == 0 || menu_height == 0 {
        return;
    }

    let item_h: i32 = (crate::render::context_menu::BASE_ITEM_HEIGHT as f32 * scale_factor).round() as i32;
    let hovered_item_idx = if local_pointer_x < menu_width as i32 && local_pointer_y < menu_height as i32 {
        Some((local_pointer_y as i32 / item_h as i32) as i32)
    } else {
        None
    };

    // 1. Fill entire background and hover state first to prevent buffer ghosting / text underneath
    for y in 0..menu_height as i32 {
        let item_idx = y / item_h;
        let is_hovered = Some(item_idx) == hovered_item_idx;

        let bg_color = if is_hovered {
            [0x3A, 0x3A, 0x3A, 0xFF]
        } else {
            [0x22, 0x22, 0x22, 0xFF]
        };

        for x in 0..menu_width as i32 {
            let idx = ((y as i32 * menu_width as i32 + x as i32) * 4) as usize;
            if idx + 3 < canvas.len() {
                canvas[idx..idx + 4].copy_from_slice(&bg_color);
            }
        }
    }

    if windows.is_empty() {
        return;
    }

// 2. Draw Items & Text
    for (i, w) in windows.iter().enumerate() {
        let title = if w.title.chars().count() > 22 {
            format!("{}...", w.title.chars().take(19).collect::<String>())
        } else {
            w.title.clone()
        };
        
        let text_x: i32 = (12.0 * scale_factor).round() as i32;
        let text_y: i32 = i as i32 * item_h + ((item_h as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;

        crate::render::text::draw_text(
            canvas,
            menu_width,
            menu_height,
            font_manager,
            &title,
            (14.0 * scale_factor) as i32,
            text_x as i32,
            text_y as i32,
            (220, 220, 220),
        );

        // =========================================================================
        // DRAW CLOSE BUTTON FOR EACH WINDOW ITEM
        // =========================================================================
        let close_btn_size = (16.0 * scale_factor).round() as i32;
        let close_btn_x = menu_width - close_btn_size - ((8.0 * scale_factor).round() as i32);
        let close_btn_y = i as i32 * item_h + (item_h - close_btn_size) / 2;

        // Check if this close button is hovered (optional: you can use your pointer coordinates to highlight it)
        let is_close_hovered = local_pointer_x >= close_btn_x 
            && local_pointer_x < close_btn_x + close_btn_size
            && local_pointer_y >= close_btn_y 
            && local_pointer_y < close_btn_y + close_btn_size;

        let btn_color = if is_close_hovered {
            [0x50, 0x50, 0xE0, 0xFF] // BGRA for Red hover
        } else {
            [0x50, 0x50, 0xE0, 0xFF] // BGRA for Blue normal
        };

        for cy in 0..close_btn_size {
            for cx in 0..close_btn_size {
                let px = close_btn_x + cx;
                let py = close_btn_y + cy;

                if px >= 0 && px < menu_width && py >= 0 && py < menu_height {
                    let idx = ((py * menu_width + px) * 4) as usize;
                    if idx + 3 < canvas.len() {
                        canvas[idx..idx + 4].copy_from_slice(&btn_color);
                    }
                }
            }
        }
        // =========================================================================

        // Draw separator line between items
        if i > 0 {
            let line_y = i as i32 * item_h;
            for x in 0..(menu_width as i32) {
                let idx = ((line_y * menu_width as i32 + x as i32) * 4) as usize;
                if idx + 3 < canvas.len() {
                    canvas[idx..idx + 4].copy_from_slice(&[0x44, 0x44, 0x44, 0xFF]);
                }
            }
        }
    }
}

/// Prepares and groups open window instances alongside pinned applications.
pub fn prepare_window_list<'a>(
    pinned_apps: &[String],
    open_windows: &'a HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
) -> PreparedWindowList<'a> {
    let mut apps_in_dock = Vec::new();
    let mut running_by_app: HashMap<String, Vec<&'a WindowDiagnostics>> = HashMap::new();

    for app_id in pinned_apps {
        if !apps_in_dock.contains(app_id) {
            apps_in_dock.push(app_id.clone());
        }
    }

    let mut sorted_windows: Vec<&'a WindowDiagnostics> = open_windows.values().collect();
    sorted_windows.sort_by(|a, b| {
        a.app_name
            .cmp(&b.app_name)
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

        running_by_app.entry(app_id.clone()).or_default().push(w);
        if !apps_in_dock.contains(&app_id) {
            apps_in_dock.push(app_id);
        }
    }

    PreparedWindowList {
        apps_in_dock,
        running_by_app,
    }
}

pub fn get_hover_menu_bounds(
    dock_width: i32,
    dock_height: i32,
    total_apps: usize,
    hovered_app_index: usize,
    menu_width: i32,
    menu_height: i32,
    scale_factor: i32,
) -> (i32, i32, i32, i32) {
    // Divide total dock width evenly among all apps in the dock
    let slot_width = dock_width / std::cmp::max(1, total_apps as i32);
    
    // Center the menu horizontally above the hovered app icon
    let mut menu_x = (hovered_app_index as i32 * slot_width) + (slot_width / 2) - (menu_width / 2);

    // Position the menu vertically right above the dock
    let menu_y = -menu_height;

    (menu_x, menu_y, menu_width, menu_height)
}