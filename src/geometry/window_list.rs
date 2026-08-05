// src/geometry/window_list.rs

use crate::render::window_list::get_hover_menu_bounds;

pub const BASE_WINDOW_LIST_WIDTH: f64 = 180.0;
pub const BASE_ITEM_HEIGHT: f32 = 30.0;

#[derive(Clone, Copy, Debug)]
pub struct WindowListGeometry {
    pub base_width: f64,
    pub item_height: f64,
}

impl Default for WindowListGeometry {
    fn default() -> Self {
        Self {
            base_width: BASE_WINDOW_LIST_WIDTH,
            item_height: BASE_ITEM_HEIGHT as f64,
        }
    }
}

impl WindowListGeometry {
    /// Computes physical coordinates and logical dimensions for the window list popup.
    /// Returns: `(phys_x, phys_y, phys_width, phys_height, logical_width, logical_height)`
    pub fn compute_bounds(
        &self,
        dock_width: i32,
        dock_height: i32,
        total_apps: usize,
        hovered_app_index: usize,
        win_count: usize,
        scale_factor: f64,
    ) -> (i32, i32, i32, i32, u32, u32) {
        let count = win_count.max(1);
        let phys_w = (self.base_width * scale_factor).round() as i32;
        let phys_h = (count as f64 * self.item_height * scale_factor).round() as i32;
        let scale_i32 = scale_factor.round() as i32;

        let (menu_x, menu_y, _, _) = get_hover_menu_bounds(
            dock_width,
            dock_height,
            total_apps,
            hovered_app_index,
            phys_w,
            phys_h,
            scale_i32,
        );

        let logical_w = (phys_w as f64 / scale_factor).round() as u32;
        let logical_h = (phys_h as f64 / scale_factor).round() as u32;

        (menu_x, menu_y, phys_w, phys_h, logical_w, logical_h)
    }
}
