//! Frame information for the render loop.
//!
//! Provides metadata about each frame being rendered, including timing
//! and surface information.

use crate::geometry::Size;
use crate::surface::SurfaceId;

/// Information about the current frame being rendered.
///
/// Passed to frame callbacks so application code can access timing
/// and surface information without coupling to the renderer internals.
#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    /// Monotonic time when the frame started (seconds).
    pub start_time: f64,
    /// Time elapsed since the previous frame (seconds).
    pub delta_time: f64,
    /// Frame number (starts at 0).
    pub frame_number: u64,
    /// Target surface being rendered to.
    pub surface_id: SurfaceId,
    /// Logical size of the target surface.
    pub surface_size: Size,
    /// Device pixel ratio.
    pub scale_factor: f32,
    /// Target frame rate (0 = unlimited).
    pub target_fps: f32,
}

impl FrameInfo {
    /// Create a new FrameInfo.
    pub fn new(surface_id: SurfaceId, surface_size: Size, scale_factor: f32) -> Self {
        Self {
            start_time: 0.0,
            delta_time: 0.016, // ~60 FPS default
            frame_number: 0,
            surface_id,
            surface_size,
            scale_factor,
            target_fps: 0.0,
        }
    }

    /// Get the physical size of the surface.
    pub fn physical_size(&self) -> Size {
        let w = (self.surface_size.width * self.scale_factor).round();
        let h = (self.surface_size.height * self.scale_factor).round();
        Size::new(w, h)
    }

    /// Check if animation should be enabled (surface is visible and large enough).
    pub fn should_animate(&self) -> bool {
        self.surface_size.width > 0.0 && self.surface_size.height > 0.0
    }

    /// Compute the target frame duration in seconds.
    pub fn target_frame_duration(&self) -> f64 {
        if self.target_fps > 0.0 {
            1.0 / self.target_fps as f64
        } else {
            1.0 / 60.0 // Default 60 FPS
        }
    }
}

impl Default for FrameInfo {
    fn default() -> Self {
        Self {
            start_time: 0.0,
            delta_time: 0.016,
            frame_number: 0,
            surface_id: SurfaceId::new(0),
            surface_size: Size::ZERO,
            scale_factor: 1.0,
            target_fps: 0.0,
        }
    }
}

/// Cursor/pointer position and state.
///
/// Used for hit-testing and interaction handling.
#[derive(Debug, Clone, Copy)]
pub struct PointerLocation {
    /// Physical pixel position of the pointer.
    pub physical_x: i32,
    /// Physical pixel position of the pointer.
    pub physical_y: i32,
    /// Logical position (adjusted for scale factor).
    pub logical_x: f32,
    /// Logical position (adjusted for scale factor).
    pub logical_y: f32,
}

impl PointerLocation {
    /// Create from physical coordinates and scale factor.
    pub fn from_physical(physical_x: i32, physical_y: i32, scale_factor: f32) -> Self {
        let logical_x = physical_x as f32 / scale_factor;
        let logical_y = physical_y as f32 / scale_factor;
        Self {
            physical_x,
            physical_y,
            logical_x,
            logical_y,
        }
    }

    /// Get the logical position as a Point.
    pub fn to_point(&self) -> crate::geometry::Point {
        crate::geometry::Point::new(self.logical_x, self.logical_y)
    }

    /// Check if the pointer is inside a rectangle (in logical coordinates).
    pub fn is_inside(&self, rect: &crate::geometry::Rect) -> bool {
        rect.contains(&self.to_point())
    }
}

/// Mouse button state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    /// Left mouse button.
    Left,
    /// Middle mouse button.
    Middle,
    /// Right mouse button.
    Right,
    /// Back button (if available).
    Back,
    /// Forward button (if available).
    Forward,
}

/// Pointer event type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerEvent {
    /// Pointer entered the surface.
    Entered,
    /// Pointer moved.
    Moved,
    /// Pointer pressed a button.
    Pressed(MouseButton),
    /// Pointer released a button.
    Released(MouseButton),
    /// Pointer left the surface.
    Exited,
    /// Mouse wheel scrolled.
    Scrolled {
        /// Horizontal delta.
        delta_x: f32,
        /// Vertical delta.
        delta_y: f32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    #[test]
    fn test_frame_info_physical_size() {
        let info = FrameInfo::new(SurfaceId::new(1), Size::new(800.0, 600.0), 2.0);
        let phys = info.physical_size();
        assert!((phys.width - 1600.0).abs() < f32::EPSILON);
        assert!((phys.height - 1200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pointer_location() {
        let loc = PointerLocation::from_physical(400, 300, 2.0);
        assert!((loc.logical_x - 200.0).abs() < f32::EPSILON);
        assert!((loc.logical_y - 150.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pointer_is_inside() {
        let loc = PointerLocation::from_physical(100, 100, 1.0);
        let rect = Rect::new(0.0, 0.0, 200.0, 200.0);
        assert!(loc.is_inside(&rect));
    }

    #[test]
    fn test_target_frame_duration() {
        let mut info = FrameInfo::default();
        info.target_fps = 30.0;
        assert!((info.target_frame_duration() - 1.0 / 30.0).abs() < f64::EPSILON);
    }
}
