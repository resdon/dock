use crate::render::font::World;
use resvg::tiny_skia::{Pixmap, Rect};
use crate::render::draw_text;
use crate::FontManager;

pub const MENU_WIDTH: u32 = 180;
pub const MENU_HEIGHT: u32 = 150; // Increased to accommodate 5 potential items
pub const MENU_ITEM_COUNT: u32 = 5;
pub const MENU_ITEM_HEIGHT: u32 = MENU_HEIGHT / MENU_ITEM_COUNT;

pub const HOVER_MENU_WIDTH: u32 = 200;
pub const HOVER_ITEM_HEIGHT: u32 = 30;
pub const DOCK_HEIGHT: u32 = 60;

pub fn get_context_menu_bounds(
    cursor_x: usize,
    cursor_y: usize,
    surface_width: usize,
    surface_height: usize,
    item_count: usize,
    scale_factor: f64,
) -> (f64, f64, f64, f64) {
    let item_h = 30.0 * scale_factor;
    let menu_width = 180.0 * scale_factor;
    let surface_h_scaled = surface_height as f64 * scale_factor;
    let surface_w_scaled = surface_width as f64 * scale_factor;

    let total_menu_h = (item_count as f64 * item_h).min(surface_h_scaled);

    let cx = cursor_x as f64 * scale_factor;
    let cy = cursor_y as f64 * scale_factor;

    let max_x = (surface_w_scaled - menu_width).max(0.0);
    let menu_x = cx.min(max_x);

    let ideal_y = cy - total_menu_h;
    let max_y = (surface_h_scaled - total_menu_h).max(0.0);
    let menu_y = ideal_y.clamp(0.0, max_y);

    (menu_x, menu_y, menu_width, total_menu_h)
}

pub fn get_hover_menu_bounds(
    icon_x: usize,
    dock_width: usize,
    dock_height: usize,
    window_count: usize,
    scale_factor: f64,
) -> (f64, f64, f64, f64) {
    let item_height = 30.0 * scale_factor;
    let menu_width = 200.0 * scale_factor;
    let menu_height = item_height * window_count.max(1) as f64;

    let dock_h_scaled = 60.0 * scale_factor;
    let margin = 8.0 * scale_factor;

    // Center the menu relative to icon_x
    let menu_x = (icon_x as f64) - (menu_width / 2.0);
    // Position menu above the dock
    let menu_y = (dock_height as f64) - dock_h_scaled - margin - menu_height;

    // Clamp menu_x to stay within surface bounds
    let max_x = (dock_width as f64) - menu_width;
    let menu_x = menu_x.clamp(0.0, max_x.max(0.0));

    (menu_x, menu_y, menu_width, menu_height)
}

/// Renders a context menu with items.
/// Returns a tuple of (menu_pixmap, item_rects) where item_rects are in menu-local coordinates.
pub fn render_context_menu(
    _world: &mut World,
    is_pinned: bool,
    font_manager: &FontManager,
) -> (Pixmap, Vec<Rect>) {
    let width = MENU_WIDTH;
    let height = MENU_HEIGHT;
    // Create a raw RGBA buffer for the menu
    let mut frame: Vec<u8> = vec![0; (width * height * 4) as usize];

    // Fill background with a dark gray color
    for y in 0..height {
        for x in 0..width {
            let idx = ((y * width + x) * 4) as usize;
            frame[idx..idx + 4].copy_from_slice(&[40, 40, 40, 255]);
        }
    }

    // Menu items
    let items = [
        "Focus".to_string(),
        "Open".to_string(), 
        "Minimize".to_string(),
        "Close".to_string(),
        if is_pinned { "Unpin".to_string() } else { "Pin".to_string() },
    ];

    let text_size = 14.0;
    let item_height = MENU_ITEM_HEIGHT;

    let mut item_rects = Vec::new();

    for (idx, label) in items.iter().enumerate() {
        let y = (idx as u32 * item_height) as f32;
        let rect = Rect::from_xywh(0.0, y, width as f32, item_height as f32).unwrap();
        item_rects.push(rect);

        // Draw text centered in the item
        let text_width = label.len() as f32 * (text_size / 2.0); // Simple approximation
        let text_x = ((width as f32 - text_width) / 2.0) as usize;
        let baseline_y = (y + item_height as f32 / 2.0 + text_size / 2.0) as usize;

        draw_text(
            &mut frame,
            width as u32,
            height as u32,
            font_manager,
            label,        // ✅ Changed from &item.label to label
            text_size,    // ✅ Used text_size variable instead of hardcoded 14.0
            text_x,
            baseline_y,   // ✅ Changed from text_y to baseline_y
            (255, 255, 255),
        );
    }

    // Convert the raw frame buffer into a Pixmap.
    let mut pixmap = Pixmap::new(width, height).expect("Failed to create menu pixmap");
    pixmap.data_mut().copy_from_slice(&frame);

    (pixmap, item_rects)
}
