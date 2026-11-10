//! 2D affine transformation matrix.
//!
//! All transforms use a 3x3 matrix in row-major order.

use crate::geometry::Point;

/// A 3x3 matrix for 2D affine transformations.
///
/// Stored in row-major order for intuitive indexing.
/// The matrix represents:
/// ```text
/// | m00 m01 m02 |
/// | m10 m11 m12 |
/// |  0   0   1  |
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Matrix3 {
    /// Matrix elements in row-major order.
    pub elements: [f32; 9],
}

impl Matrix3 {
    /// Identity matrix.
    pub const IDENTITY: Self = Self {
        elements: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
    };

    /// Create a new matrix from row-major elements.
    pub const fn from_elements(e: [f32; 9]) -> Self {
        Self { elements: e }
    }

    /// Get element at row r, column c.
    #[inline]
    pub fn get(&self, r: usize, c: usize) -> f32 {
        self.elements[r * 3 + c]
    }

    /// Set element at row r, column c.
    pub fn set(&mut self, r: usize, c: usize, value: f32) {
        self.elements[r * 3 + c] = value;
    }

    /// Multiply two matrices (self * other).
    pub fn multiply(&self, other: &Self) -> Self {
        let a = &self.elements;
        let b = &other.elements;
        Self {
            elements: [
                a[0] * b[0] + a[1] * b[3] + a[2] * b[6],
                a[0] * b[1] + a[1] * b[4] + a[2] * b[7],
                a[0] * b[2] + a[1] * b[5] + a[2] * b[8],
                a[3] * b[0] + a[4] * b[3] + a[5] * b[6],
                a[3] * b[1] + a[4] * b[4] + a[5] * b[7],
                a[3] * b[2] + a[4] * b[5] + a[5] * b[8],
                a[6] * b[0] + a[7] * b[3] + a[8] * b[6],
                a[6] * b[1] + a[7] * b[4] + a[8] * b[7],
                a[6] * b[2] + a[7] * b[5] + a[8] * b[8],
            ],
        }
    }

    /// Transform a point by this matrix.
    pub fn transform_point(&self, p: &Point) -> Point {
        let e = &self.elements;
        Point::new(
            e[0] * p.x + e[1] * p.y + e[2],
            e[3] * p.x + e[4] * p.y + e[5],
        )
    }

    /// Compute the inverse matrix (if it exists).
    pub fn inverse(&self) -> Option<Self> {
        let e = &self.elements;
        // For affine matrices, the last row is always [0, 0, 1]
        let a = e[0];
        let b = e[1];
        let c = e[2];
        let d = e[3];
        let e_ = e[4];
        let f = e[5];

        let det = a * e_ - b * d;
        if det.abs() < f32::EPSILON {
            return None;
        }

        let inv_det = 1.0 / det;
        Some(Self {
            elements: [
                e_ * inv_det,
                -b * inv_det,
                (b * f - e_ * c) * inv_det,
                -d * inv_det,
                a * inv_det,
                (d * c - a * f) * inv_det,
                0.0,
                0.0,
                1.0,
            ],
        })
    }

    /// Decompose into translation, rotation, and scale components.
    pub fn decompose(&self) -> (Point, f32, (f32, f32)) {
        let e = &self.elements;
        let translation = Point::new(e[2], e[5]);
        let scale_x = (e[0] * e[0] + e[3] * e[3]).sqrt();
        let scale_y = (e[1] * e[1] + e[4] * e[4]).sqrt();
        let rotation = e[3].atan2(e[0]);
        (translation, rotation, (scale_x, scale_y))
    }
}

impl Default for Matrix3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A 2D affine transformation.
///
/// Wraps a Matrix3 with convenience methods for building transforms.
/// Transforms are composable and can be applied to points, rectangles,
/// or entire rendering contexts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    /// The underlying 3x3 matrix.
    pub matrix: Matrix3,
}

impl Transform {
    /// Identity transform (no transformation).
    pub const IDENTITY: Self = Self {
        matrix: Matrix3::IDENTITY,
    };

    /// Create a new identity transform.
    pub fn new() -> Self {
        Self::IDENTITY
    }

    /// Create a translation transform.
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            matrix: Matrix3 {
                elements: [1.0, 0.0, x, 0.0, 1.0, y, 0.0, 0.0, 1.0],
            },
        }
    }

    /// Create a scale transform.
    pub fn scale(x: f32, y: f32) -> Self {
        Self {
            matrix: Matrix3 {
                elements: [x, 0.0, 0.0, 0.0, y, 0.0, 0.0, 0.0, 1.0],
            },
        }
    }

    /// Create a uniform scale transform.
    pub fn scale_uniform(s: f32) -> Self {
        Self::scale(s, s)
    }

    /// Create a rotation transform (angle in radians, clockwise).
    pub fn rotate(angle: f32) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        Self {
            matrix: Matrix3 {
                elements: [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0],
            },
        }
    }

    /// Create a rotation transform around a specific point.
    pub fn rotate_around(angle: f32, cx: f32, cy: f32) -> Self {
        let c = angle.cos();
        let s = angle.sin();
        let tx = cx * (1.0 - c) + cy * s;
        let ty = cy * (1.0 - c) - cx * s;
        Self {
            matrix: Matrix3 {
                elements: [c, -s, tx, s, c, ty, 0.0, 0.0, 1.0],
            },
        }
    }

    /// Create a skew transform.
    pub fn skew(x_angle: f32, y_angle: f32) -> Self {
        Self {
            matrix: Matrix3 {
                elements: [
                    1.0,
                    x_angle.tan(),
                    0.0,
                    y_angle.tan(),
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                ],
            },
        }
    }

    /// Compose this transform with another (self * other).
    pub fn compose(&self, other: &Self) -> Self {
        Self {
            matrix: self.matrix.multiply(&other.matrix),
        }
    }

    /// Pre-multiply by another transform (other * self).
    pub fn pre_multiply(&self, other: &Self) -> Self {
        Self {
            matrix: other.matrix.multiply(&self.matrix),
        }
    }

    /// Transform a point.
    pub fn transform_point(&self, p: &Point) -> Point {
        self.matrix.transform_point(p)
    }

    /// Compute the inverse transform.
    pub fn inverse(&self) -> Option<Self> {
        self.matrix.inverse().map(|m| Self { matrix: m })
    }

    /// Apply this transform to a point in-place.
    pub fn apply(&self, p: &mut Point) {
        *p = self.transform_point(p);
    }

    /// Check if this is the identity transform.
    pub fn is_identity(&self) -> bool {
        self.matrix == Matrix3::IDENTITY
    }

    /// Get the translation component.
    pub fn translation(&self) -> Point {
        let e = &self.matrix.elements;
        Point::new(e[2], e[5])
    }

    /// Get the scale component.
    pub fn scale_component(&self) -> (f32, f32) {
        let e = &self.matrix.elements;
        let sx = (e[0] * e[0] + e[1] * e[1]).sqrt();
        let sy = (e[3] * e[3] + e[4] * e[4]).sqrt();
        (sx, sy)
    }

    /// Get the rotation angle in radians.
    pub fn rotation_angle(&self) -> f32 {
        let e = &self.matrix.elements;
        e[1].atan2(e[0])
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Matrix3> for Transform {
    fn from(matrix: Matrix3) -> Self {
        Self { matrix }
    }
}

impl From<Transform> for Matrix3 {
    fn from(t: Transform) -> Self {
        t.matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate() {
        let t = Transform::translate(10.0, 20.0);
        let p = t.transform_point(&Point::ORIGIN);
        assert!((p.x - 10.0).abs() < f32::EPSILON);
        assert!((p.y - 20.0).abs() < f32::EPSILON);
        assert_eq!(t.translation(), Point::new(10.0, 20.0));
    }

    #[test]
    fn test_scale() {
        let t = Transform::scale(2.0, 3.0);
        let p = t.transform_point(&Point::new(1.0, 1.0));
        assert!((p.x - 2.0).abs() < f32::EPSILON);
        assert!((p.y - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_compose() {
        let t1 = Transform::translate(10.0, 0.0);
        let t2 = Transform::scale(2.0, 2.0);
        let composed = t1.compose(&t2);
        let p = composed.transform_point(&Point::new(1.0, 0.0));
        assert!((p.x - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_inverse() {
        let t = Transform::translate(10.0, 20.0);
        let inv = t.inverse().unwrap();
        let p = t.transform_point(&Point::new(5.0, 5.0));
        let back = inv.transform_point(&p);
        assert!((back.x - 5.0).abs() < f32::EPSILON);
        assert!((back.y - 5.0).abs() < f32::EPSILON);
    }
}
