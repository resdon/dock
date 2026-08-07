use super::primitives::Point;

/// Calculates relative local pointer position inside a bounding box
pub fn to_local_coords(global_ptr: Point, bounds_x: f64, bounds_y: f64, scale: f64) -> (i32, i32) {
    let local_x = ((global_ptr.x * scale) - bounds_x).max(0.0) as i32;
    let local_y = ((global_ptr.y * scale) - bounds_y).max(0.0) as i32;
    (local_x, local_y)
}

/// Helper to scale physical screen dimensions to logical units
pub fn to_logical_size(phys_w: u32, phys_h: u32, scale: f32) -> (i32, i32) {
    (
        (phys_w as f32 / scale).round() as i32,
        (phys_h as f32 / scale).round() as i32,
    )
}
