#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Calculates relative local pointer position inside a bounding box.
    /// Used like: `let (x, y) = global_ptr.to_local_coords(bounds_x, bounds_y, scale);`
    pub fn to_local_coords(&self, bounds_x: f64, bounds_y: f64, scale: f64) -> (i32, i32) {
        let local_x = ((self.x * scale) - bounds_x).max(0.0) as i32;
        let local_y = ((self.y * scale) - bounds_y).max(0.0) as i32;
        (local_x, local_y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
    }

    pub fn scale(&self, scale_factor: f64) -> Self {
        Self {
            x: self.x * scale_factor,
            y: self.y * scale_factor,
            width: self.width * scale_factor,
            height: self.height * scale_factor,
        }
    }

    pub fn inset(&self, border: f64) -> Self {
        Self {
            x: self.x + border,
            y: self.y + border,
            width: (self.width - 2.0 * border).max(0.0),
            height: (self.height - 2.0 * border).max(0.0),
        }
    }
}

/// Helper to scale physical screen dimensions to logical units
pub fn to_logical_size(phys_w: u32, phys_h: u32, scale: f32) -> (i32, i32) {
    (
        (phys_w as f32 / scale).round() as i32,
        (phys_h as f32 / scale).round() as i32,
    )
}
