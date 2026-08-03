// src/render/context_menu.rs

use crate::FontManager;
use crate::MenuState;
use crate::render::text::draw_text;

/// Physical surface placement clamped to screen boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

// Layout & Dimension Constants
pub const BASE_ITEM_HEIGHT: i32 = 30;
pub const HOVER_ITEM_HEIGHT: i32 = BASE_ITEM_HEIGHT;

pub const DOCK_HEIGHT: i32 = 60;

pub const MENU_WIDTH: i32 = 160;
pub const BASE_MENU_WIDTH: i32 = MENU_WIDTH;
pub const HOVER_MENU_WIDTH: i32 = 200;

// Derived Menu Dimensions
pub const MENU_ITEM_COUNT: i32 = 5;
pub const MENU_HEIGHT: i32 = BASE_ITEM_HEIGHT * MENU_ITEM_COUNT;

/// Calculates physical geometry with smart vertical flipping and boundary clamping.
pub fn calculate_context_menu_geometry(
    cursor_x: i32,
    cursor_y: i32,
    monitor_width: i32,
    monitor_height: i32,
    item_count: i32,
    scale_factor: f32,
) -> SurfaceGeometry {
    let item_h = (BASE_ITEM_HEIGHT as f32 * scale_factor).round() as i32;
    let menu_w = (BASE_MENU_WIDTH as f32 * scale_factor).round() as i32;
    
    // Total required height based on items, capped to physical monitor height
    let raw_h = (item_count as i32 * item_h).max(item_h);
    let menu_h = raw_h.min(monitor_height) as i32;

    let cx = (cursor_x as f32 * scale_factor).round() as i32;
    let cy = (cursor_y as f32 * scale_factor).round() as i32;

    // Horizontal placement clamped within screen
    let max_x = (monitor_width as i32 - menu_w as i32).max(0) as i32;
    let menu_x = cx.clamp(0, max_x) as i32;

    // Smart vertical placement:
    // 1. Prefer opening below cursor (cy)
    // 2. Flip above cursor (cy - menu_h) if opening below overflows bottom
    // 3. Hard-clamp to screen bounds (0..max_y) if menu height is close to or equal to monitor height
    let menu_y = if cy + menu_h <= monitor_height as i32 {
        cy
    } else if cy - menu_h as i32 >= 0 {
        cy - menu_h as i32
    } else {
        let max_y = (monitor_height as i32 - menu_h as i32).max(0);
        (cy - menu_h as i32).clamp(0, max_y)
    };

    SurfaceGeometry {
        x: menu_x,
        y: menu_y,
        width: menu_w,
        height: menu_h,
    }
}

/// Calculates context menu surface bounds as floating-point coordinates.
pub fn get_context_menu_bounds(
    cursor_x: i32,
    cursor_y: i32,
    surface_width: i32,
    _surface_height: i32,
    item_count: i32,
    scale_factor: f32,
) -> (i32, i32, i32, i32) {
    let item_h = (BASE_ITEM_HEIGHT as f32 * scale_factor).round() as i32;
    let menu_w = (MENU_WIDTH as f32 * scale_factor).round() as i32;
    let total_menu_h = (item_count as i32 * item_h as i32).max(item_h) as i32;

    let cx = (cursor_x as f32 * scale_factor).round() as i32;
    let cy = (cursor_y as f32 * scale_factor).round() as i32;

    let surface_w_scaled = (surface_width as f32 * scale_factor).round() as i32;
    let max_x = (surface_w_scaled as i32 - menu_w as i32).max(0) as i32;
    let menu_x = cx.clamp(0, max_x) as i32;

    // Position above the cursor/dock boundary cleanly
    let menu_y = cy as i32 - total_menu_h as i32;

    (menu_x, menu_y, menu_w, total_menu_h)
}

/// Renders ONLY the Context Menu surface into a tightly bounded RGBA buffer.
pub fn render_context_menu_surface(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    scale_factor: f32,
    menu_state: &MenuState,
    font_manager: &FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
) {
    buffer.fill(0);
    
    if menu_state.items.is_empty() || width <= 0 || height <= 0 {
        return;
    }

    // Strict validation to prevent stale buffer memory ghosting
    let expected_len = (width as usize) * (height as usize) * 4;
    if buffer.len() < expected_len {
        return; 
    }

    let item_h = (BASE_ITEM_HEIGHT as f32 * scale_factor).round() as i32;
    
    // Safe bounds check including negative pointer offsets
    let hovered_item_idx = if local_pointer_x >= 0 
        && local_pointer_y >= 0 
        && local_pointer_x < width 
        && local_pointer_y < height 
    {
        Some((local_pointer_y / item_h) as usize)
    } else {
        None
    };

    // 1. Draw Menu background & hover state
    for y in 0..height {
        let item_idx = (y / item_h) as usize;
        let is_hovered = Some(item_idx) == hovered_item_idx;

        let bg_color = if is_hovered {
            [0x3A, 0x3A, 0x3A, 0xFF]
        } else {
            [0x22, 0x22, 0x22, 0xFF]
        };

        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            buffer[idx..idx + 4].copy_from_slice(&bg_color);
        }
    }

    // 2. Render Text labels and separators
    for (i, item) in menu_state.items.iter().enumerate() {
        let text_x = (12.0 * scale_factor).round() as i32;
        let text_y = (i as i32 * item_h) + ((item_h as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;
        let text_color = if matches!(item.item_type, crate::MenuItemType::CloseApp) {
            (255, 100, 100)
        } else {
            (220, 220, 220)
        };

        draw_text(
            buffer,
            width as i32,
            height as i32,
            font_manager,
            &item.label,
            (14.0 * scale_factor) as i32,
            text_x,
            text_y,
            text_color,
        );

        if i > 0 {
            let line_y = i as i32 * item_h;
            for x in 0..width {
                let idx = ((line_y * width + x) as usize) * 4;
                buffer[idx..idx + 4].copy_from_slice(&[0x44, 0x44, 0x44, 0xFF]);
            }
        }
    }
}