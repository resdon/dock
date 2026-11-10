//! Popup geometry constants.
//!
//! Legacy constants for popup dimensions — use config values instead.

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
    /// Legacy constants — kept for backward compatibility. Use config values instead.
    pub const BASE_ITEM_HEIGHT: i32 = 29;
    pub const BASE_MENU_WIDTH: i32 = 144;
    pub const BASE_WINDOW_LIST_WIDTH: i32 = 160;
    /// Logical height of the PipeWire capture frame area (rendered as separate overlay)
    pub const WINDOW_LIST_FRAME_HEIGHT: i32 = 240;

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
        let (base_width, item_height, extra_height) = match popup_type {
            PopupType::ContextMenu => (Self::BASE_MENU_WIDTH, Self::BASE_ITEM_HEIGHT, 0),
            // WindowList includes the PipeWire capture frame height at the top
            PopupType::WindowList => (
                Self::BASE_WINDOW_LIST_WIDTH,
                Self::BASE_ITEM_HEIGHT,
                Self::WINDOW_LIST_FRAME_HEIGHT,
            ),
            PopupType::Custom { width, height } => (width, height, 0),
        };

        // Convert inputs to f64 for precision during scaling math
        let scale = scale_factor as f64;

        let phys_item_h = (item_height as f64 * scale).round() as i32;
        let phys_w = (base_width as f64 * scale).round() as i32;
        let phys_extra_h = (extra_height as f64 * scale).round() as i32;
        let phys_h = (item_count * phys_item_h + phys_extra_h).max(0);

        let logical_width = (phys_w as f64 / scale).round() as i32;
        let logical_height = (phys_h as f64 / scale).round() as i32;

        // Position the menu vertically ABOVE the anchor point (top edge of the dock).
        let menu_y = anchor_y - phys_h;

        // App menus stay inside the main surface. A negative anchor denotes
        // the left-side SNI box, so allow its menu to extend outside the parent
        // layer surface alongside that sibling subsurface.
        let max_x = (dock_width - phys_w).max(0);
        let menu_x = if anchor_x < 0 {
            anchor_x - phys_w / 2
        } else {
            (anchor_x - phys_w / 2).clamp(0, max_x)
        };

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
