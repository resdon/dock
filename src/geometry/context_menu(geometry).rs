// src/geometry/context_menu.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
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

    /// Computes context menu bounding geometry as a subsurface relative to the dock,
    /// positioning the menu above the dock anchor without clipping inside dock bounds.
    pub fn compute_bounds(
        &self,
        anchor_x: i32,
        anchor_y: i32,
        dock_width: i32,
        _dock_height: i32,
        item_count: usize,
        scale_factor: f64,
    ) -> SurfaceGeometry {
        let item_h = (self.base_item_height as f64 * scale_factor).round() as i32;
        let menu_w = (self.base_menu_width as f64 * scale_factor).round() as i32;
        let menu_h = (item_count as i32 * item_h).max(0);

        // Position the menu vertically ABOVE the anchor point (top edge of the dock).
        // This allows negative local Y coordinates so the subsurface renders above the dock surface.
        let menu_y = anchor_y - menu_h;

        // Center horizontally on the anchor X coordinate, clamped within dock surface bounds.
        let max_x = (dock_width - menu_w).max(0);
        let menu_x = (anchor_x - menu_w / 2).clamp(0, max_x);

        SurfaceGeometry {
            x: menu_x,
            y: menu_y,
            width: menu_w,
            height: menu_h,
        }
    }

    /// Convenience wrapper returning bounds as a `(x, y, width, height)` tuple.
    pub fn compute_bounds_tuple(
        &self,
        anchor_x: i32,
        anchor_y: i32,
        dock_width: i32,
        dock_height: i32,
        item_count: usize,
        scale_factor: f64,
    ) -> (i32, i32, i32, i32) {
        let geom = self.compute_bounds(
            anchor_x,
            anchor_y,
            dock_width,
            dock_height,
            item_count,
            scale_factor,
        );
        (geom.x, geom.y, geom.width, geom.height)
    }
}
