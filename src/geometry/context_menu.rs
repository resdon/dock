// src/geometry/context_menu.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeometry {
    pub x: i32,
    pub y: i32,
    pub phys_width: i32,
    pub phys_height: i32,
    pub logical_width: i32,
    pub logical_height: i32,
}

pub const BASE_ITEM_HEIGHT: i32 = 30;
pub const BASE_MENU_WIDTH: i32 = 160;

#[derive(Debug, Clone, Copy)]
pub struct ContextMenuGeometry {
    pub base_item_height: i32,
    pub base_menu_width: i32,
}

impl Default for ContextMenuGeometry {
    fn default() -> Self {
        Self {
            base_item_height: BASE_ITEM_HEIGHT,
            base_menu_width: BASE_MENU_WIDTH,
        }
    }
}

impl ContextMenuGeometry {
    pub fn new(base_item_height: i32, base_menu_width: i32) -> Self {
        Self {
            base_item_height,
            base_menu_width,
        }
    }

    /// Computes context menu bounding geometry, explicitly separating physical dimensions 
    /// (for buffer allocation) and logical dimensions (for Wayland input regions and surface bounds).
    pub fn compute_bounds(
        &self,
        anchor_x: i32,
        anchor_y: i32,
        dock_width: i32,
        _dock_height: i32,
        item_count: usize,
        scale_factor: f64,
    ) -> SurfaceGeometry {
        let phys_item_h = (self.base_item_height as f64 * scale_factor).round() as i32;
        let phys_menu_w = (self.base_menu_width as f64 * scale_factor).round() as i32;
        let phys_menu_h = (item_count as i32 * phys_item_h).max(0);

        let logical_width = (phys_menu_w as f64 / scale_factor).round() as i32;
        let logical_height = (phys_menu_h as f64 / scale_factor).round() as i32;

        // Position the menu vertically ABOVE the anchor point (top edge of the dock).
        let menu_y = anchor_y - phys_menu_h;

        // Center horizontally on the anchor X coordinate, clamped within dock surface bounds.
        let max_x = (dock_width - phys_menu_w).max(0);
        let menu_x = (anchor_x - phys_menu_w / 2).clamp(0, max_x);

        SurfaceGeometry {
            x: menu_x,
            y: menu_y,
            phys_width: phys_menu_w,
            phys_height: phys_menu_h,
            logical_width,
            logical_height,
        }
    }

    /// Convenience wrapper returning bounds as a tuple containing coordinates and dimensions.
    pub fn compute_bounds_tuple(
        &self,
        anchor_x: i32,
        anchor_y: i32,
        dock_width: i32,
        dock_height: i32,
        item_count: usize,
        scale_factor: f64,
    ) -> (i32, i32, i32, i32, i32, i32) {
        let geom = self.compute_bounds(
            anchor_x,
            anchor_y,
            dock_width,
            dock_height,
            item_count,
            scale_factor,
        );
        (
            geom.x,
            geom.y,
            geom.phys_width,
            geom.phys_height,
            geom.logical_width,
            geom.logical_height,
        )
    }
}
