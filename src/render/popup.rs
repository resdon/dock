use std::collections::HashMap;

use crate::geometry::popup::{PopupGeometry, SurfaceGeometry};
use crate::models::WindowDiagnostics;

use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::types::{MenuItemType, MenuState};

pub const HOVER_ITEM_HEIGHT: i32 = PopupGeometry::BASE_ITEM_HEIGHT;
pub const DOCK_HEIGHT: i32 = 60;

pub const MENU_WIDTH: i32 = PopupGeometry::BASE_MENU_WIDTH;
pub const HOVER_MENU_WIDTH: i32 = 200;

pub const MENU_ITEM_COUNT: i32 = 5;
pub const MENU_HEIGHT: i32 = PopupGeometry::BASE_ITEM_HEIGHT * MENU_ITEM_COUNT;

pub struct PreparedWindowList<'a> {
    pub apps_in_dock: Vec<String>,
    pub running_by_app: HashMap<String, Vec<&'a WindowDiagnostics>>,
}

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

    // Stable row identity: use the window diagnostic id, not pointer addresses.
    sorted_windows.sort_by_key(|w| w.id);

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

#[allow(clippy::too_many_arguments)]
pub fn render_window_list_surface(
    canvas: &mut [u8],
    geometry: &SurfaceGeometry,
    scale_factor: f32,
    windows: &[&WindowDiagnostics],
    font_manager: &mut FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
) {
    if windows.is_empty() || geometry.phys_width <= 0 || geometry.phys_height <= 0 {
        canvas.fill(0);
        return;
    }

    let expected_len = (geometry.phys_width as usize) * (geometry.phys_height as usize) * 4;
    if canvas.len() < expected_len {
        return;
    }

    let item_count = windows.len();
    let menu_width = geometry.phys_width;
    let menu_height = geometry.phys_height;

    // Logical height of the menu and proportional item height
    let logical_menu_h = geometry.logical_height as f32;
    let item_h_log = logical_menu_h / item_count as f32;

    // Evaluate hovered item using logical coordinates matching context_menu
    let hovered_item_idx = if (local_pointer_x as f32) >= 0.0
        && (local_pointer_x as f32) < (geometry.logical_width as f32)
        && (local_pointer_y as f32) >= 0.0
        && (local_pointer_y as f32) < logical_menu_h
    {
        let idx = ((local_pointer_y as f32 / item_h_log) as i32).clamp(0, (item_count - 1) as i32)
            as usize;
        Some(idx)
    } else {
        None
    };

    // 1. Fill background and hover state using proportional row divisions
    for y in 0..menu_height {
        let item_idx = ((y as usize * item_count) / menu_height as usize).min(item_count - 1);
        let is_hovered = Some(item_idx) == hovered_item_idx;

        let bg_color = if is_hovered {
            [0x3A, 0x3A, 0x3A, 0xFF]
        } else {
            [0x22, 0x22, 0x22, 0xFF]
        };

        for x in 0..menu_width {
            let idx = ((y * menu_width + x) * 4) as usize;
            if idx + 3 < canvas.len() {
                canvas[idx..idx + 4].copy_from_slice(&bg_color);
            }
        }
    }

    // 2. Render separator lines between items
    for i in 1..item_count {
        let line_y = (i as i32 * menu_height) / item_count as i32;
        if line_y < menu_height {
            for x in 0..menu_width {
                let idx = ((line_y * menu_width + x) * 4) as usize;
                if idx + 3 < canvas.len() {
                    canvas[idx..idx + 4].copy_from_slice(&[0x44, 0x44, 0x44, 0xFF]);
                }
            }
        }
    }

    // 3. Render Text labels & Close buttons
    let font_size = (14.0 * scale_factor) as i32;
    let text_x = (12.0 * scale_factor).round() as i32;

    for (i, w) in windows.iter().enumerate() {
        let item_top_phys = (i as i32 * menu_height) / item_count as i32;
        let next_item_top_phys = ((i as i32 + 1) * menu_height) / item_count as i32;
        let current_item_h_phys = next_item_top_phys - item_top_phys;

        let title = if w.title.chars().count() > 22 {
            format!("{}...", w.title.chars().take(19).collect::<String>())
        } else {
            w.title.clone()
        };

        let text_offset_y =
            ((current_item_h_phys as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;
        let text_y = item_top_phys + text_offset_y;

        if text_y < menu_height {
            crate::render::text::draw_text(
                canvas,
                menu_width,
                menu_height,
                font_manager,
                &title,
                font_size,
                text_x,
                text_y,
                (220, 220, 220),
            );
        }

        // Render Close Button
        let close_btn_size = (16.0 * scale_factor).round() as i32;
        let close_btn_x = menu_width - close_btn_size - ((8.0 * scale_factor).round() as i32);
        let close_btn_y = item_top_phys + (current_item_h_phys - close_btn_size) / 2;

        // Convert logical pointer coordinates to physical pixels for close button bounds check
        let ptr_phys_x = (local_pointer_x as f32 * scale_factor).round() as i32;
        let ptr_phys_y = (local_pointer_y as f32 * scale_factor).round() as i32;

        let is_close_hovered = ptr_phys_x >= close_btn_x
            && ptr_phys_x < close_btn_x + close_btn_size
            && ptr_phys_y >= close_btn_y
            && ptr_phys_y < close_btn_y + close_btn_size;

        let btn_color = if is_close_hovered {
            [0x50, 0x50, 0xE0, 0xFF]
        } else {
            [0x3A, 0x3A, 0xAA, 0xFF]
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
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render_context_menu_surface(
    buffer: &mut [u8],
    geometry: &SurfaceGeometry,
    scale_factor: f32,
    menu_state: &MenuState,
    font_manager: &mut FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
) {
    if menu_state.items.is_empty() || geometry.phys_width <= 0 || geometry.phys_height <= 0 {
        buffer.fill(0);
        return;
    }

    let expected_len = (geometry.phys_width as usize) * (geometry.phys_height as usize) * 4;
    if buffer.len() < expected_len {
        return;
    }

    let item_count = menu_state.items.len();
    if item_count == 0 {
        return;
    }

    let width = geometry.phys_width;
    let height = geometry.phys_height;

    // Logical height of the menu and proportional item height
    let logical_menu_h = geometry.logical_height as f32;
    let item_h_log = logical_menu_h / item_count as f32;

    // Evaluate hovered item using logical coordinates matching handle_menu_release
    let hovered_item_idx = if (local_pointer_x as f32) >= 0.0
        && (local_pointer_x as f32) < (geometry.logical_width as f32)
        && (local_pointer_y as f32) >= 0.0
        && (local_pointer_y as f32) < logical_menu_h
    {
        let idx = ((local_pointer_y as f32 / item_h_log) as i32).clamp(0, (item_count - 1) as i32)
            as usize;
        Some(idx)
    } else {
        None
    };

    // 1. Fill background and hover state using exact proportional row divisions
    for y in 0..height {
        let item_idx = ((y as usize * item_count) / height as usize).min(item_count - 1);
        let is_hovered = Some(item_idx) == hovered_item_idx;

        let bg_color = if is_hovered {
            [0x3A, 0x3A, 0x3A, 0xFF]
        } else {
            [0x22, 0x22, 0x22, 0xFF]
        };

        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(&bg_color);
            }
        }
    }

    // 2. Render separator lines between items
    for i in 1..item_count {
        let line_y = (i as i32 * height) / item_count as i32;
        if line_y < height {
            for x in 0..width {
                let idx = ((line_y * width + x) * 4) as usize;
                if idx + 3 < buffer.len() {
                    buffer[idx..idx + 4].copy_from_slice(&[0x44, 0x44, 0x44, 0xFF]);
                }
            }
        }
    }

    // 3. Render Text labels
    let font_size = (14.0 * scale_factor) as i32;
    let text_x = (12.0 * scale_factor).round() as i32;

    for (i, item) in menu_state.items.iter().enumerate() {
        let item_top_phys = (i as i32 * height) / item_count as i32;
        let next_item_top_phys = ((i as i32 + 1) * height) / item_count as i32;
        let current_item_h_phys = next_item_top_phys - item_top_phys;

        let text_offset_y =
            ((current_item_h_phys as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;
        let text_y = item_top_phys + text_offset_y;

        if text_y >= height {
            break;
        }

        let text_color = match item.item_type {
            MenuItemType::CloseApp => (255, 100, 100),
            _ => (220, 220, 220),
        };

        draw_text(
            buffer,
            width,
            height,
            font_manager,
            &item.label,
            font_size,
            text_x,
            text_y,
            text_color,
        );
    }
}
