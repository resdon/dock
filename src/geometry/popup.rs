#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PopupType {
    ContextMenu,
    WindowList,
    Custom { width: i32, height: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceGeometry {
    pub x: i32,
    pub y: i32,
    pub phys_width: i32,
    pub phys_height: i32,
    pub logical_width: i32,
    pub logical_height: i32,
}

pub struct PopupGeometry;

impl PopupGeometry {
    pub const BASE_ITEM_HEIGHT: i32 = 30;
    pub const BASE_MENU_WIDTH: i32 = 160;
    pub const BASE_WINDOW_LIST_WIDTH: i32 = 160;

    /// Computes popup bounding geometry, explicitly separating physical dimensions
    /// (for buffer allocation) and logical dimensions (for input regions and surface bounds).
    pub fn compute_bounds(
        popup_type: PopupType,
        anchor_x: i32,
        anchor_y: i32,
        dock_width: i32,
        _dock_height: i32,
        item_count: i32,
        scale_factor: f32,
    ) -> SurfaceGeometry {
        let (base_width, item_height) = match popup_type {
            PopupType::ContextMenu => (Self::BASE_MENU_WIDTH, Self::BASE_ITEM_HEIGHT),
            PopupType::WindowList => (Self::BASE_WINDOW_LIST_WIDTH, Self::BASE_ITEM_HEIGHT),
            PopupType::Custom { width, height } => (width, height),
        };

        // Convert inputs to f64 for precision during scaling math
        let scale = scale_factor as f64;
        
        let phys_item_h = (item_height as f64 * scale).round() as i32;
        let phys_w = (base_width as f64 * scale).round() as i32;
        let phys_h = (item_count * phys_item_h).max(0);

        let logical_width = (phys_w as f64 / scale).round() as i32;
        let logical_height = (phys_h as f64 / scale).round() as i32;

        // Position the menu vertically ABOVE the anchor point (top edge of the dock).
        let menu_y = anchor_y - phys_h;

        // Center horizontally on the anchor X coordinate, clamped within dock surface bounds.
        let max_x = (dock_width - phys_w).max(0);
        let menu_x = (anchor_x - phys_w / 2).clamp(0, max_x);

        SurfaceGeometry {
            x: menu_x,
            y: menu_y,
            phys_width: phys_w,
            phys_height: phys_h,
            logical_width,
            logical_height,
        }
    }
}