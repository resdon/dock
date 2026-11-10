//! Floating-point geometry types for layout and transforms.
//!
//! All coordinates use `f32` floating-point values. Conversion to physical
//! pixels (integer coordinates) happens only when preparing the rendering
//! pass, not in application code.

use std::ops::{Add, Div, Mul, Sub};

/// Opaque identifier for a rendering node in the scene graph.
///
/// Used to reference rendered elements for hit-testing, animation targets,
/// and scene graph manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    /// Create a new NodeId from a raw value.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw u64 value.
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for NodeId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// A 2D vector representing direction and magnitude.
///
/// Used for offsets, velocities, and directional calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Vector2D {
    /// X component.
    pub x: f32,
    /// Y component.
    pub y: f32,
}

impl Vector2D {
    /// Origin vector (0, 0).
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Unit X vector (1, 0).
    pub const X: Self = Self { x: 1.0, y: 0.0 };

    /// Unit Y vector (0, 1).
    pub const Y: Self = Self { x: 0.0, y: 1.0 };

    /// Create a new vector.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Compute the length (magnitude) of the vector.
    pub fn length(&self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    /// Compute the squared length (avoids sqrt for comparisons).
    pub fn length_squared(&self) -> f32 {
        self.x * self.x + self.y * self.y
    }

    /// Return a normalized unit vector.
    pub fn normalize(&self) -> Self {
        let len = self.length();
        if len > 0.0 {
            Self {
                x: self.x / len,
                y: self.y / len,
            }
        } else {
            *self
        }
    }

    /// Dot product with another vector.
    pub fn dot(&self, other: &Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    /// Cross product (z-component) with another vector.
    pub fn cross(&self, other: &Self) -> f32 {
        self.x * other.y - self.y * other.x
    }

    /// Linear interpolation between two vectors.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Self {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        }
    }

    /// Convert to a Point (same coordinates).
    pub fn to_point(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Convert to a Size (absolute values).
    pub fn to_size(&self) -> Size {
        Size::new(self.x.abs(), self.y.abs())
    }
}

impl Add for Vector2D {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Vector2D {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

impl Mul<f32> for Vector2D {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            x: self.x * rhs,
            y: self.y * rhs,
        }
    }
}

impl Div<f32> for Vector2D {
    type Output = Self;
    fn div(self, rhs: f32) -> Self {
        Self {
            x: self.x / rhs,
            y: self.y / rhs,
        }
    }
}

impl Add<Vector2D> for Point {
    type Output = Self;
    fn add(self, rhs: Vector2D) -> Self {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}

impl Sub for Point {
    type Output = Vector2D;
    fn sub(self, rhs: Self) -> Vector2D {
        Vector2D::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Sub<Vector2D> for Point {
    type Output = Self;
    fn sub(self, rhs: Vector2D) -> Self {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}

/// A 2D point representing a position in screen space.
///
/// Coordinates are in floating-point logical pixels. The origin (0, 0) is
/// typically the top-left corner of the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Point {
    /// X coordinate (horizontal, increases rightward).
    pub x: f32,
    /// Y coordinate (vertical, increases downward).
    pub y: f32,
}

impl Point {
    /// Origin point (0, 0).
    pub const ORIGIN: Self = Self { x: 0.0, y: 0.0 };

    /// Create a new point.
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Create from integer coordinates.
    pub fn from_i32(x: i32, y: i32) -> Self {
        Self {
            x: x as f32,
            y: y as f32,
        }
    }

    /// Round to the nearest integer coordinates.
    pub fn round(&self) -> (i32, i32) {
        (self.x.round() as i32, self.y.round() as i32)
    }

    /// Floor to integer coordinates.
    pub fn floor(&self) -> (i32, i32) {
        (self.x.floor() as i32, self.y.floor() as i32)
    }

    /// Ceiling to integer coordinates.
    pub fn ceil(&self) -> (i32, i32) {
        (self.x.ceil() as i32, self.y.ceil() as i32)
    }

    /// Compute distance to another point.
    pub fn distance_to(&self, other: &Self) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    /// Linear interpolation between two points.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Self {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        }
    }

    /// Check if this point is inside a rectangle.
    pub fn is_inside(&self, rect: &Rect) -> bool {
        self.x >= rect.x
            && self.x < rect.x + rect.width
            && self.y >= rect.y
            && self.y < rect.y + rect.height
    }

    /// Convert to a Vector2D.
    pub fn to_vector(&self) -> Vector2D {
        Vector2D::new(self.x, self.y)
    }
}

/// A 2D size representing width and height.
///
/// Used for surface dimensions, texture sizes, and layout measurements.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Size {
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
}

impl Size {
    /// Zero size.
    pub const ZERO: Self = Self {
        width: 0.0,
        height: 0.0,
    };

    /// Create a new size.
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }

    /// Create from integer dimensions.
    pub fn from_u32(width: u32, height: u32) -> Self {
        Self {
            width: width as f32,
            height: height as f32,
        }
    }

    /// Create from integer dimensions (i32).
    pub fn from_i32(width: i32, height: i32) -> Self {
        Self {
            width: width as f32,
            height: height as f32,
        }
    }

    /// Convert to integer dimensions (rounded).
    pub fn to_u32(&self) -> (u32, u32) {
        (self.width.round() as u32, self.height.round() as u32)
    }

    /// Compute the area.
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Compute the aspect ratio (width / height).
    pub fn aspect_ratio(&self) -> f32 {
        if self.height > 0.0 {
            self.width / self.height
        } else {
            f32::INFINITY
        }
    }

    /// Scale the size by a factor.
    pub fn scale(&self, factor: f32) -> Self {
        Self {
            width: self.width * factor,
            height: self.height * factor,
        }
    }

    /// Fit this size within a maximum size, preserving aspect ratio.
    pub fn fit_within(&self, max_size: &Self) -> Self {
        let scale_w = if max_size.width > 0.0 {
            max_size.width / self.width
        } else {
            0.0
        };
        let scale_h = if max_size.height > 0.0 {
            max_size.height / self.height
        } else {
            0.0
        };
        let scale = scale_w.min(scale_h).min(1.0);
        self.scale(scale)
    }

    /// Cover a target size, preserving aspect ratio.
    pub fn cover(&self, target: &Self) -> Self {
        let scale_w = if self.width > 0.0 {
            target.width / self.width
        } else {
            f32::INFINITY
        };
        let scale_h = if self.height > 0.0 {
            target.height / self.height
        } else {
            f32::INFINITY
        };
        let scale = scale_w.max(scale_h);
        self.scale(scale)
    }

    /// Check if this size is empty.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
}

impl Add for Size {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self {
            width: self.width + rhs.width,
            height: self.height + rhs.height,
        }
    }
}

impl Sub for Size {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self {
            width: self.width - rhs.width,
            height: self.height - rhs.height,
        }
    }
}

impl Mul<f32> for Size {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self {
            width: self.width * rhs,
            height: self.height * rhs,
        }
    }
}

/// A 2D axis-aligned rectangle.
///
/// Defined by its top-left corner (x, y) and dimensions (width, height).
/// Coordinates are in floating-point logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Rect {
    /// X coordinate of the left edge.
    pub x: f32,
    /// Y coordinate of the top edge.
    pub y: f32,
    /// Width (positive, extends rightward).
    pub width: f32,
    /// Height (positive, extends downward).
    pub height: f32,
}

impl Rect {
    /// Empty rectangle at origin.
    pub const ZERO: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
    };

    /// Create a new rectangle.
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Create from a point and a size.
    pub fn from_origin_size(origin: &Point, size: &Size) -> Self {
        Self {
            x: origin.x,
            y: origin.y,
            width: size.width,
            height: size.height,
        }
    }

    /// Create a rectangle from min and max points.
    pub fn from_points(min: &Point, max: &Point) -> Self {
        Self {
            x: min.x,
            y: min.y,
            width: max.x - min.x,
            height: max.y - min.y,
        }
    }

    /// Get the top-left corner.
    pub fn origin(&self) -> Point {
        Point::new(self.x, self.y)
    }

    /// Get the bottom-right corner.
    pub fn bottom_right(&self) -> Point {
        Point::new(self.x + self.width, self.y + self.height)
    }

    /// Get the center point.
    pub fn center(&self) -> Point {
        Point::new(self.x + self.width / 2.0, self.y + self.height / 2.0)
    }

    /// Get the size (width, height).
    pub fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }

    /// Get the right edge coordinate.
    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    /// Get the bottom edge coordinate.
    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Get the top edge coordinate.
    pub fn top(&self) -> f32 {
        self.y
    }

    /// Get the left edge coordinate.
    pub fn left(&self) -> f32 {
        self.x
    }

    /// Check if this rectangle contains a point.
    pub fn contains(&self, point: &Point) -> bool {
        point.is_inside(self)
    }

    /// Check if this rectangle contains another rectangle.
    pub fn contains_rect(&self, other: &Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.right() >= other.right()
            && self.bottom() >= other.bottom()
    }

    /// Check if this rectangle intersects another rectangle.
    pub fn intersects(&self, other: &Self) -> bool {
        self.x < other.right()
            && self.right() > other.x
            && self.y < other.bottom()
            && self.bottom() > other.y
    }

    /// Compute the intersection with another rectangle.
    /// Returns None if the rectangles do not intersect.
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());

        if right > x && bottom > y {
            Some(Self {
                x,
                y,
                width: right - x,
                height: bottom - y,
            })
        } else {
            None
        }
    }

    /// Compute the union with another rectangle (smallest rect containing both).
    pub fn union(&self, other: &Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self.right().max(other.right());
        let bottom = self.bottom().max(other.bottom());
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }

    /// Inflate the rectangle by a given amount on all sides.
    pub fn inflate(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x - dx,
            y: self.y - dy,
            width: self.width + dx * 2.0,
            height: self.height + dy * 2.0,
        }
    }

    /// Translate the rectangle by a vector.
    pub fn translate(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            width: self.width,
            height: self.height,
        }
    }

    /// Scale the rectangle from its origin.
    pub fn scale_from_origin(&self, sx: f32, sy: f32) -> Self {
        Self {
            x: self.x * sx,
            y: self.y * sy,
            width: self.width * sx,
            height: self.height * sy,
        }
    }

    /// Round all coordinates to integers.
    pub fn round(&self) -> Self {
        Self {
            x: self.x.round(),
            y: self.y.round(),
            width: self.width.round(),
            height: self.height.round(),
        }
    }

    /// Floor all coordinates to integers.
    pub fn floor(&self) -> Self {
        Self {
            x: self.x.floor(),
            y: self.y.floor(),
            width: self.width.ceil(),
            height: self.height.ceil(),
        }
    }

    /// Check if this rectangle is empty.
    pub fn is_empty(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }

    /// Compute the area.
    pub fn area(&self) -> f32 {
        self.width * self.height
    }

    /// Get the four corner points in order: top-left, top-right, bottom-right, bottom-left.
    pub fn corners(&self) -> [Point; 4] {
        [
            Point::new(self.x, self.y),
            Point::new(self.x + self.width, self.y),
            Point::new(self.x + self.width, self.y + self.height),
            Point::new(self.x, self.y + self.height),
        ]
    }

    /// Get the midpoints of each edge: top, right, bottom, left.
    pub fn midpoints(&self) -> [Point; 4] {
        [
            Point::new(self.x + self.width / 2.0, self.y),
            Point::new(self.x + self.width, self.y + self.height / 2.0),
            Point::new(self.x + self.width / 2.0, self.y + self.height),
            Point::new(self.x, self.y + self.height / 2.0),
        ]
    }
}

impl From<Point> for Rect {
    fn from(p: Point) -> Self {
        Self {
            x: p.x,
            y: p.y,
            width: 0.0,
            height: 0.0,
        }
    }
}

impl From<Size> for Rect {
    fn from(s: Size) -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: s.width,
            height: s.height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_point_distance() {
        let a = Point::new(0.0, 0.0);
        let b = Point::new(3.0, 4.0);
        assert!((a.distance_to(&b) - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rect_contains() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        assert!(rect.contains(&Point::new(50.0, 50.0)));
        assert!(!rect.contains(&Point::new(100.0, 50.0)));
    }

    #[test]
    fn test_rect_intersection() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        let intersection = a.intersection(&b).unwrap();
        assert!((intersection.x - 50.0).abs() < f32::EPSILON);
        assert!((intersection.width - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_vector_operations() {
        let a = Vector2D::new(1.0, 2.0);
        let b = Vector2D::new(3.0, 4.0);
        let c = a + b;
        assert!((c.x - 4.0).abs() < f32::EPSILON);
        assert!((c.y - 6.0).abs() < f32::EPSILON);
        assert!((a.dot(&b) - 11.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_size_fit_within() {
        let size = Size::new(800.0, 600.0);
        let max = Size::new(400.0, 300.0);
        let fitted = size.fit_within(&max);
        assert!((fitted.width - 400.0).abs() < f32::EPSILON);
        assert!((fitted.height - 300.0).abs() < f32::EPSILON);
    }
}
