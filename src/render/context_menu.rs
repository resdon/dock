pub use crate::geometry::context_menu::{
    ContextMenuGeometry, SurfaceGeometry, BASE_ITEM_HEIGHT, BASE_MENU_WIDTH,
};

use crate::render::text::draw_text;
use crate::{FontManager, MenuItemType, MenuState};

pub const HOVER_ITEM_HEIGHT: i32 = BASE_ITEM_HEIGHT;
pub const DOCK_HEIGHT: i32 = 60;

pub const MENU_WIDTH: i32 = BASE_MENU_WIDTH;
pub const HOVER_MENU_WIDTH: i32 = 200;

pub const MENU_ITEM_COUNT: i32 = 5;
pub const MENU_HEIGHT: i32 = BASE_ITEM_HEIGHT * MENU_ITEM_COUNT;

pub fn calculate_context_menu_geometry(
    anchor_x: i32,
    anchor_y: i32,
    monitor_width: i32,
    monitor_height: i32,
    item_count: i32,
    scale_factor: f32,
) -> SurfaceGeometry {
    ContextMenuGeometry::default().compute_bounds(
        anchor_x,
        anchor_y,
        monitor_width,
        monitor_height,
        item_count as usize,
        scale_factor as f64,
    )
}

pub fn get_context_menu_bounds(
    cursor_x: i32,
    cursor_y: i32,
    surface_width: i32,
    surface_height: i32,
    item_count: i32,
    scale_factor: f32,
) -> (i32, i32, i32, i32, i32, i32) {
    ContextMenuGeometry::default().compute_bounds_tuple(
        cursor_x,
        cursor_y,
        surface_width,
        surface_height,
        item_count as usize,
        scale_factor as f64,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn render_context_menu_surface(
    buffer: &mut [u8],
    width: i32,
    height: i32,
    scale_factor: f32,
    menu_state: &MenuState,
    font_manager: &mut FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
) {
    if menu_state.items.is_empty() || width <= 0 || height <= 0 {
        buffer.fill(0);
        return;
    }

    let expected_len = (width as usize) * (height as usize) * 4;
    if buffer.len() < expected_len {
        return;
    }

    let item_count = menu_state.items.len();
    if item_count == 0 {
        return;
    }

    // Logical height of the menu and proportional item height
    let logical_menu_h = height as f32 / scale_factor;
    let item_h_log = logical_menu_h / item_count as f32;

    // Evaluate hovered item using logical coordinates matching handle_menu_release
    let hovered_item_idx = if (local_pointer_x as f32) >= 0.0
        && (local_pointer_x as f32) < (width as f32 / scale_factor)
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
