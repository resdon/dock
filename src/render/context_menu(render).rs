pub use crate::geometry::context_menu::{ContextMenuGeometry, SurfaceGeometry, BASE_ITEM_HEIGHT, BASE_MENU_WIDTH};

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

    let geometry = ContextMenuGeometry::default();
    let item_count = menu_state.items.len();

    // Physical item height for buffer rendering and hit testing
    let item_h_phys = (geometry.base_item_height as f32 * scale_factor).round() as i32;
    if item_h_phys <= 0 {
        return;
    }

    // Evaluate hovered item using physical coordinates matching the physical buffer bounds (`width` and `height`)
    let hovered_item_idx = if local_pointer_x >= 0
        && local_pointer_x < width
        && local_pointer_y >= 0
        && local_pointer_y < height
    {
        let idx = (local_pointer_y / item_h_phys) as usize;
        (idx < item_count).then_some(idx)
    } else {
        None
    };

    // 1. Fill entire background and hover state using physical row/column loops
    for y in 0..height {
        let item_idx = (y / item_h_phys) as usize;
        let is_hovered = item_idx < item_count && Some(item_idx) == hovered_item_idx;

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
        let line_y = i as i32 * item_h_phys;
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
    let text_offset_y = ((item_h_phys as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;

    for (i, item) in menu_state.items.iter().enumerate() {
        let text_y = (i as i32 * item_h_phys) + text_offset_y;
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
