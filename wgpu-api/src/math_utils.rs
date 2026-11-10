//! Math utilities powered by [`glam`](https://docs.rs/glam).
//!
//! This module provides SIMD-optimized math operations using `glam` types
//! (`Vec2`, `Mat3`, `Mat4`) while maintaining compatibility with the existing
//! wgpu-api geometry types (`Vector2D`, `Point`, `Size`, `Rect`, `Matrix3`).
//!
//! ## Design
//!
//! Rather than replacing existing types, this module provides:
//!
//! 1. **Conversion traits** — bidirectional conversion between wgpu-api types
//!    and glam types.
//! 2. **Extended operations** — additional math operations (rotation, projection,
//!    interpolation) that leverage glam's SIMD optimizations.
//! 3. **Batch operations** — vectorized transforms for scene graph processing.
//!
//! ## When to use glam
//!
//! - **Geometry computation**: Use glam for heavy math (transforms, projections,
//!   interpolation) where SIMD matters.
//! - **API boundaries**: Use wgpu-api types (`Point`, `Size`, `Rect`) in public
//!   APIs for consistency.
//! - **GPU data**: Use `#[repr(C)]` wgpu-api types for uniform buffers and
//!   vertex data.
//!
//! ## Integration Points
//!
//! - **geometry.rs**: `Vector2D` ↔ `glam::Vec2`, `Point` ↔ `glam::Vec2`
//! - **transform.rs**: `Matrix3` ↔ `glam::Mat3`, `Transform` operations
//! - **gpu.rs**: `QuadInstance` uniform data, projection matrices
//! - **animation.rs**: Property interpolation with glam vectors
//! - **gif.rs**: Frame compositing with glam-based operations

pub use glam::{Vec2, Mat2, Mat3, Mat3A, Mat4, Quat, Vec3, Vec4};

use crate::geometry::{Vector2D, Point, Size, Rect};
use crate::transform::Matrix3;

// ---------------------------------------------------------------------------
// Conversion traits: wgpu-api ↔ glam
// ---------------------------------------------------------------------------

/// Convert a wgpu-api type to a glam type.
pub trait ToGlam<T> {
    /// Convert to the glam type.
    fn to_glam(&self) -> T;
}

/// Convert a glam type to a wgpu-api type.
pub trait FromGlam<S> {
    /// Convert from the glam type.
    fn from_glam(src: &S) -> Self;
}

// ---------------------------------------------------------------------------
// Vector2D ↔ Vec2
// ---------------------------------------------------------------------------

impl ToGlam<Vec2> for Vector2D {
    fn to_glam(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl FromGlam<Vector2D> for Vec2 {
    fn from_glam(src: &Vector2D) -> Self {
        Vec2::new(src.x, src.y)
    }
}

impl From<Vec2> for Vector2D {
    fn from(v: Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

impl From<Vector2D> for Vec2 {
    fn from(v: Vector2D) -> Self {
        Vec2::new(v.x, v.y)
    }
}

// ---------------------------------------------------------------------------
// Point ↔ Vec2
// ---------------------------------------------------------------------------

impl ToGlam<Vec2> for Point {
    fn to_glam(&self) -> Vec2 {
        Vec2::new(self.x, self.y)
    }
}

impl From<Vec2> for Point {
    fn from(v: Vec2) -> Self {
        Self { x: v.x, y: v.y }
    }
}

// ---------------------------------------------------------------------------
// Size ↔ Vec2
// ---------------------------------------------------------------------------

impl ToGlam<Vec2> for Size {
    fn to_glam(&self) -> Vec2 {
        Vec2::new(self.width, self.height)
    }
}

impl From<Vec2> for Size {
    fn from(v: Vec2) -> Self {
        Self {
            width: v.x,
            height: v.y,
        }
    }
}

// ---------------------------------------------------------------------------
// Matrix3 ↔ glam::Mat3 / Mat3A
// ---------------------------------------------------------------------------

impl ToGlam<Mat3> for Matrix3 {
    fn to_glam(&self) -> Mat3 {
        // Matrix3 stores row-major, glam stores column-major → transpose
        Mat3::from_cols_array_2d(&[
            [self.elements[0], self.elements[3], self.elements[6]],
            [self.elements[1], self.elements[4], self.elements[7]],
            [self.elements[2], self.elements[5], self.elements[8]],
        ])
    }
}

impl ToGlam<Mat3A> for Matrix3 {
    fn to_glam(&self) -> Mat3A {
        Mat3A::from_cols_array_2d(&[
            [self.elements[0], self.elements[3], self.elements[6]],
            [self.elements[1], self.elements[4], self.elements[7]],
            [self.elements[2], self.elements[5], self.elements[8]],
        ])
    }
}

impl From<Mat3> for Matrix3 {
    fn from(m: Mat3) -> Self {
        // glam is column-major, Matrix3 is row-major → transpose
        let cols = m.to_cols_array_2d();
        Self {
            elements: [
                cols[0][0], cols[1][0], cols[2][0],
                cols[0][1], cols[1][1], cols[2][1],
                cols[0][2], cols[1][2], cols[2][2],
            ],
        }
    }
}

impl From<Mat3A> for Matrix3 {
    fn from(m: Mat3A) -> Self {
        let cols = m.to_cols_array_2d();
        Self {
            elements: [
                cols[0][0], cols[1][0], cols[2][0],
                cols[0][1], cols[1][1], cols[2][1],
                cols[0][2], cols[1][2], cols[2][2],
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// Extended Vector Operations
// ---------------------------------------------------------------------------

/// 2D cross product (z-component) for Vec2.
/// glam::Vec2 doesn't have cross(), so we provide it here.
pub fn vec2_cross(a: Vec2, b: Vec2) -> f32 {
    a.x * b.y - a.y * b.x
}

/// Extended 2D vector operations using glam.
pub trait Vec2Ext: Copy {
    /// Rotate this vector by the given angle in radians.
    fn rotate(self, angle_rad: f32) -> Self;

    /// Rotate this vector by the given angle in degrees.
    fn rotate_deg(self, angle_deg: f32) -> Self;

    /// Reflect this vector across the given normal.
    fn reflect(self, normal: Self) -> Self;

    /// Project this vector onto another vector.
    fn project_onto(self, other: Self) -> Self;

    /// Compute the angle between this vector and another (in radians).
    fn angle_to(self, other: Self) -> f32;

    /// Compute the signed angle between this vector and another (in radians).
    fn signed_angle_to(self, other: Self) -> f32;

    /// Clamp the length to a maximum value.
    fn clamp_length(self, max_length: f32) -> Self;

    /// Restrict the length to a range [min, max].
    fn restrict_length(self, min_length: f32, max_length: f32) -> Self;

    /// Smooth damping toward a target vector.
    fn damp_toward(self, target: Self, damping: f32) -> Self;
}

impl Vec2Ext for Vec2 {
    fn rotate(self, angle_rad: f32) -> Self {
        let c = angle_rad.cos();
        let s = angle_rad.sin();
        Mat2::from_cols(Vec2::new(c, -s), Vec2::new(s, c)) * self
    }

    fn rotate_deg(self, angle_deg: f32) -> Self {
        let rad = angle_deg * std::f32::consts::PI / 180.0;
        let c = rad.cos();
        let s = rad.sin();
        Mat2::from_cols(Vec2::new(c, -s), Vec2::new(s, c)) * self
    }

    fn reflect(self, normal: Self) -> Self {
        let n = normal.normalize();
        self - 2.0 * self.dot(n) * n
    }

    fn project_onto(self, other: Self) -> Self {
        let other = other.normalize();
        self.dot(other) * other
    }

    fn angle_to(self, other: Self) -> f32 {
        let dot = self.dot(other).clamp(-1.0, 1.0);
        dot.acos()
    }

    fn signed_angle_to(self, other: Self) -> f32 {
        vec2_cross(self, other).atan2(self.dot(other))
    }

    fn clamp_length(self, max_length: f32) -> Self {
        let len = self.length();
        if len > max_length {
            self * (max_length / len)
        } else {
            self
        }
    }

    fn restrict_length(self, min_length: f32, max_length: f32) -> Self {
        let len = self.length();
        if len == 0.0 {
            return self;
        }
        let clamped = len.clamp(min_length, max_length);
        self * (clamped / len)
    }

    fn damp_toward(self, target: Self, damping: f32) -> Self {
        let diff = target - self;
        let len = diff.length();
        if len < damping {
            target
        } else {
            self + diff * (damping / len)
        }
    }
}

// ---------------------------------------------------------------------------
// Extended Matrix Operations (Mat3)
// ---------------------------------------------------------------------------

/// Extended 2D matrix operations using glam.
pub trait Mat2DExt {
    /// Extract the translation component.
    fn extract_translation(&self) -> Vec2;

    /// Extract the scale component.
    fn extract_scale(&self) -> Vec2;

    /// Extract the rotation angle (radians).
    fn extract_rotation(&self) -> f32;

    /// Decompose into translation, rotation, and scale.
    fn decompose(&self) -> (Vec2, f32, Vec2);

    /// Transform a point.
    fn transform_point(&self, point: Vec2) -> Vec2;

    /// Transform a direction (without translation).
    fn transform_direction(&self, dir: Vec2) -> Vec2;
}

impl Mat2DExt for Mat3 {
    fn extract_translation(&self) -> Vec2 {
        Vec2::new(self.z_axis.x, self.z_axis.y)
    }

    fn extract_scale(&self) -> Vec2 {
        Vec2::new(
            self.x_axis.x.abs() + self.x_axis.y.abs(),
            self.y_axis.x.abs() + self.y_axis.y.abs(),
        )
    }

    fn extract_rotation(&self) -> f32 {
        self.x_axis.y.atan2(self.x_axis.x)
    }

    fn decompose(&self) -> (Vec2, f32, Vec2) {
        let translation = self.extract_translation();
        let rotation = self.extract_rotation();
        let scale = self.extract_scale();
        (translation, rotation, scale)
    }

    fn transform_point(&self, point: Vec2) -> Vec2 {
        let v = *self * Vec3::new(point.x, point.y, 1.0);
        Vec2::new(v.x, v.y)
    }

    fn transform_direction(&self, dir: Vec2) -> Vec2 {
        let v = *self * Vec3::new(dir.x, dir.y, 0.0);
        Vec2::new(v.x, v.y)
    }
}

impl Mat2DExt for Mat3A {
    fn extract_translation(&self) -> Vec2 {
        Vec2::new(self.z_axis.x, self.z_axis.y)
    }

    fn extract_scale(&self) -> Vec2 {
        Vec2::new(
            self.x_axis.x.abs() + self.x_axis.y.abs(),
            self.y_axis.x.abs() + self.y_axis.y.abs(),
        )
    }

    fn extract_rotation(&self) -> f32 {
        self.x_axis.y.atan2(self.x_axis.x)
    }

    fn decompose(&self) -> (Vec2, f32, Vec2) {
        let translation = self.extract_translation();
        let rotation = self.extract_rotation();
        let scale = self.extract_scale();
        (translation, rotation, scale)
    }

    fn transform_point(&self, point: Vec2) -> Vec2 {
        let v = *self * Vec3::new(point.x, point.y, 1.0);
        Vec2::new(v.x, v.y)
    }

    fn transform_direction(&self, dir: Vec2) -> Vec2 {
        let v = *self * Vec3::new(dir.x, dir.y, 0.0);
        Vec2::new(v.x, v.y)
    }
}

// ---------------------------------------------------------------------------
// Matrix builder helpers
// ---------------------------------------------------------------------------

/// Build a 2D orthographic projection matrix (top-left origin).
/// Maps (0,0) to (-1,1) NDC and (width,height) to (1,-1) NDC.
pub fn orthographic_topleft(width: f32, height: f32) -> Mat3 {
    let inv_w = 2.0 / width;
    let inv_h = 2.0 / height;
    // glam Mat3 is column-major: from_cols(x_axis, y_axis, z_axis)
    // We want the row-major matrix:
    // | inv_w   0    -1  |
    // |  0   -inv_h   1  |
    // |  0      0     1  |
    // So columns are:
    // x_axis = (inv_w, 0, 0), y_axis = (0, -inv_h, 0), z_axis = (-1, 1, 1)
    Mat3::from_cols(
        Vec3::new(inv_w, 0.0, 0.0),
        Vec3::new(0.0, -inv_h, 0.0),
        Vec3::new(-1.0, 1.0, 1.0),
    )
}

/// Build a 2D orthographic projection matrix (top-left origin) as Mat3A.
pub fn orthographic_topleft_a(width: f32, height: f32) -> Mat3A {
    let inv_w = 2.0 / width;
    let inv_h = 2.0 / height;
    Mat3A::from_cols_array_2d(&[
        [inv_w, 0.0, 0.0],
        [0.0, -inv_h, 0.0],
        [-1.0, 1.0, 1.0],
    ])
}

/// Build a combined 2D transform: translation + rotation + scale.
pub fn transform_2d_mat3(translation: Vec2, rotation_rad: f32, scale: Vec2) -> Mat3 {
    Mat3::from_scale_angle_translation(scale, rotation_rad, translation)
}

/// Build a combined 2D transform: translation + rotation + scale (SIMD).
pub fn transform_2d_mat3a(translation: Vec2, rotation_rad: f32, scale: Vec2) -> Mat3A {
    Mat3A::from_scale_angle_translation(scale, rotation_rad, translation)
}

// ---------------------------------------------------------------------------
// 4x4 Matrix utilities (for GPU uniform buffers)
// ---------------------------------------------------------------------------

/// Create a 2D orthographic projection matrix (top-left origin).
pub fn orthographic_topleft_2d(width: f32, height: f32, near: f32, far: f32) -> Mat4 {
    let inv_w = 2.0 / width;
    let inv_h = 2.0 / height;
    let inv_d = 2.0 / (far - near);

    Mat4::from_cols(
        Vec4::new(inv_w, 0.0, 0.0, 0.0),
        Vec4::new(0.0, -inv_h, 0.0, 0.0),
        Vec4::new(0.0, 0.0, inv_d, 0.0),
        Vec4::new(-1.0, 1.0, near * inv_d, 1.0),
    )
}

/// Create a 2D transformation matrix with translation, rotation, and scale.
pub fn transform_2d_mat4(translation: Vec2, rotation_rad: f32, scale: Vec2) -> Mat4 {
    let c = rotation_rad.cos();
    let s = rotation_rad.sin();

    Mat4::from_cols(
        Vec4::new(c * scale.x, -s * scale.x, 0.0, 0.0),
        Vec4::new(s * scale.y, c * scale.y, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(translation.x, translation.y, 0.0, 1.0),
    )
}

// ---------------------------------------------------------------------------
// Batch transform operations
// ---------------------------------------------------------------------------

/// Apply a transform to a batch of points.
pub fn transform_points(matrix: &Mat3, points: &[Vec2]) -> Vec<Vec2> {
    points
        .iter()
        .map(|p| {
            let v = *matrix * Vec3::new(p.x, p.y, 1.0);
            Vec2::new(v.x, v.y)
        })
        .collect()
}

/// Apply a transform to a batch of rectangles.
pub fn transform_rects(matrix: &Mat3, rects: &[Rect]) -> Vec<Rect> {
    rects
        .iter()
        .map(|r| {
            let corners = r.corners();
            let transformed: Vec<Point> = corners
                .iter()
                .map(|c| {
                    let v = *matrix * Vec3::new(c.x, c.y, 1.0);
                    Point::from(Vec2::new(v.x, v.y))
                })
                .collect();

            let mut min_x = f32::INFINITY;
            let mut min_y = f32::INFINITY;
            let mut max_x = f32::NEG_INFINITY;
            let mut max_y = f32::NEG_INFINITY;

            for p in &transformed {
                min_x = min_x.min(p.x);
                min_y = min_y.min(p.y);
                max_x = max_x.max(p.x);
                max_y = max_y.max(p.y);
            }

            Rect::new(min_x, min_y, max_x - min_x, max_y - min_y)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Interpolation utilities
// ---------------------------------------------------------------------------

/// Smoothstep interpolation.
pub fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Smooth damping (lerp with delta time).
pub fn damp(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}

/// Smooth damping for vectors.
pub fn damp_vec2(current: Vec2, target: Vec2, speed: f32, dt: f32) -> Vec2 {
    let factor = 1.0 - (-speed * dt).exp();
    current + (target - current) * factor
}

/// Catmull-Rom spline interpolation.
pub fn catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f32) -> Vec2 {
    let t2 = t * t;
    let t3 = t2 * t;

    0.5 * (
        (2.0 * p1)
            + (-p0 + p2) * t
            + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t2
            + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t3
    )
}

// ---------------------------------------------------------------------------
// Noise and random utilities
// ---------------------------------------------------------------------------

/// Simple hash for deterministic random values.
pub fn hash_f32(x: u32, y: u32) -> f32 {
    let n = x.wrapping_mul(374761393u32).wrapping_add(y.wrapping_mul(668265263u32));
    let n = n.wrapping_mul(n.wrapping_mul(n).wrapping_add(n).wrapping_add(3344567289u32));
    let frac = ((n >> 16) as f32 / 32768.0).fract();
    frac.abs()
}

/// Value noise at a 2D point.
pub fn value_noise(pos: Vec2) -> f32 {
    let ix = pos.x.floor() as i32;
    let iy = pos.y.floor() as i32;
    let fx = pos.x - ix as f32;
    let fy = pos.y - iy as f32;

    let x0 = ix as u32;
    let y0 = iy as u32;

    let v00 = hash_f32(x0, y0);
    let v10 = hash_f32(x0 + 1, y0);
    let v01 = hash_f32(x0, y0 + 1);
    let v11 = hash_f32(x0 + 1, y0 + 1);

    let sx = smoothstep(0.0, 1.0, fx);
    let sy = smoothstep(0.0, 1.0, fy);

    let ix0 = v00 + (v10 - v00) * sx;
    let ix1 = v01 + (v11 - v01) * sx;

    ix0 + (ix1 - ix0) * sy
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector2d_to_glam() {
        let v = Vector2D::new(3.0, 4.0);
        let glam_v: Vec2 = v.to_glam();
        assert!((glam_v.x - 3.0).abs() < f32::EPSILON);
        assert!((glam_v.y - 4.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_point_to_glam() {
        let p = Point::new(1.0, 2.0);
        let glam_v: Vec2 = p.to_glam();
        assert!((glam_v.x - 1.0).abs() < f32::EPSILON);
        assert!((glam_v.y - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_size_to_glam() {
        let s = Size::new(100.0, 200.0);
        let glam_v: Vec2 = s.to_glam();
        assert!((glam_v.x - 100.0).abs() < f32::EPSILON);
        assert!((glam_v.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_glam_to_vector2d() {
        let v = Vec2::new(5.0, 10.0);
        let vec2d: Vector2D = v.into();
        assert!((vec2d.x - 5.0).abs() < f32::EPSILON);
        assert!((vec2d.y - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_glam_to_point() {
        let v = Vec2::new(3.0, 7.0);
        let point: Point = v.into();
        assert!((point.x - 3.0).abs() < f32::EPSILON);
        assert!((point.y - 7.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_glam_to_size() {
        let v = Vec2::new(800.0, 600.0);
        let size: Size = v.into();
        assert!((size.width - 800.0).abs() < f32::EPSILON);
        assert!((size.height - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_matrix3_to_glam() {
        let m = Matrix3::IDENTITY;
        let glam_m: Mat3 = m.to_glam();
        assert!(glam_m.is_finite());
    }

    #[test]
    fn test_vec2_rotate() {
        let v = Vec2::new(1.0, 0.0);
        let rotated = Vec2Ext::rotate(v, std::f32::consts::FRAC_PI_2);
        assert!((rotated.x - 0.0).abs() < 0.0001);
        assert!((rotated.y - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_rotate_deg() {
        let v = Vec2::new(1.0, 0.0);
        let rotated = Vec2Ext::rotate_deg(v, 90.0);
        assert!((rotated.x - 0.0).abs() < 0.0001);
        assert!((rotated.y - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_reflect() {
        let v = Vec2::new(1.0, 1.0);
        let normal = Vec2::new(0.0, -1.0);
        let reflected = v.reflect(normal);
        assert!((reflected.x - 1.0).abs() < 0.0001);
        assert!((reflected.y - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_project_onto() {
        let v = Vec2::new(3.0, 4.0);
        let axis = Vec2::new(1.0, 0.0);
        let projected = v.project_onto(axis);
        assert!((projected.x - 3.0).abs() < 0.0001);
        assert!((projected.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_angle_to() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        let angle = a.angle_to(b);
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_signed_angle_to() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        let angle = a.signed_angle_to(b);
        assert!((angle - std::f32::consts::FRAC_PI_2).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_clamp_length() {
        let v = Vec2::new(3.0, 4.0);
        let clamped = Vec2Ext::clamp_length(v, 2.5);
        assert!((clamped.length() - 2.5).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_restrict_length() {
        let v = Vec2::new(1.0, 0.0);
        let restricted = v.restrict_length(2.0, 5.0);
        assert!((restricted.length() - 2.0).abs() < 0.0001);
    }

    #[test]
    fn test_vec2_damp_toward() {
        let current = Vec2::new(0.0, 0.0);
        let target = Vec2::new(10.0, 0.0);
        let damped = current.damp_toward(target, 3.0);
        assert!((damped.x - 3.0).abs() < 0.0001);
        assert!((damped.y - 0.0).abs() < 0.0001);
    }

    #[test]
    fn test_mat3_translation() {
        let m = Mat3::from_translation(Vec2::new(5.0, 10.0));
        let p = m.transform_point(Vec2::new(0.0, 0.0));
        assert!((p.x - 5.0).abs() < 0.0001);
        assert!((p.y - 10.0).abs() < 0.0001);
    }

    #[test]
    fn test_mat3_scaling() {
        let m = Mat3::from_scale(Vec2::new(2.0, 3.0));
        let p = m.transform_point(Vec2::new(1.0, 1.0));
        assert!((p.x - 2.0).abs() < 0.0001);
        assert!((p.y - 3.0).abs() < 0.0001);
    }

    #[test]
    fn test_mat3_rotation() {
        let m = Mat3::from_angle(std::f32::consts::FRAC_PI_2);
        let p = m.transform_point(Vec2::new(1.0, 0.0));
        assert!((p.x - 0.0).abs() < 0.0001);
        assert!((p.y - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_mat3_orthographic() {
        let m = orthographic_topleft(100.0, 100.0);
        let origin = m.transform_point(Vec2::new(0.0, 0.0));
        assert!((origin.x - (-1.0)).abs() < 0.0001);
        assert!((origin.y - 1.0).abs() < 0.0001);

        let corner = m.transform_point(Vec2::new(100.0, 100.0));
        assert!((corner.x - 1.0).abs() < 0.0001);
        assert!((corner.y - (-1.0)).abs() < 0.0001);
    }

    #[test]
    fn test_mat3_decompose() {
        let (translation, rotation, scale) = Mat3::IDENTITY.decompose();
        assert!(translation.length() < 0.0001);
        assert!(rotation.abs() < 0.0001);
        assert!((scale.x - 1.0).abs() < 0.0001);
        assert!((scale.y - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_transform_points() {
        let m = Mat3::from_translation(Vec2::new(10.0, 20.0));
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(2.0, 2.0),
        ];
        let transformed = transform_points(&m, &points);
        assert!((transformed[0].x - 10.0).abs() < 0.0001);
        assert!((transformed[1].x - 11.0).abs() < 0.0001);
    }

    #[test]
    fn test_transform_rects() {
        let m = Mat3::from_scale(Vec2::new(2.0, 2.0));
        let rects = vec![Rect::new(0.0, 0.0, 10.0, 10.0)];
        let transformed = transform_rects(&m, &rects);
        assert!((transformed[0].width - 20.0).abs() < 0.0001);
        assert!((transformed[0].height - 20.0).abs() < 0.0001);
    }

    #[test]
    fn test_smoothstep() {
        assert!((smoothstep(0.0, 1.0, 0.0) - 0.0).abs() < 0.0001);
        assert!((smoothstep(0.0, 1.0, 1.0) - 1.0).abs() < 0.0001);
        assert!((smoothstep(0.0, 1.0, 0.5) - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_damp() {
        let result = damp(0.0, 100.0, 5.0, 0.1);
        assert!(result > 0.0 && result < 100.0);
    }

    #[test]
    fn test_damp_vec2() {
        let result = damp_vec2(Vec2::ZERO, Vec2::new(10.0, 10.0), 5.0, 0.1);
        assert!(result.x > 0.0 && result.x < 10.0);
        assert!(result.y > 0.0 && result.y < 10.0);
    }

    #[test]
    fn test_catmull_rom() {
        let p0 = Vec2::new(0.0, 0.0);
        let p1 = Vec2::new(0.0, 1.0);
        let p2 = Vec2::new(1.0, 1.0);
        let p3 = Vec2::new(1.0, 0.0);

        let start = catmull_rom(p0, p1, p2, p3, 0.0);
        assert!((start.x - 0.0).abs() < 0.0001);
        assert!((start.y - 1.0).abs() < 0.0001);

        let end = catmull_rom(p0, p1, p2, p3, 1.0);
        assert!((end.x - 1.0).abs() < 0.0001);
        assert!((end.y - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_value_noise() {
        let noise = value_noise(Vec2::new(0.0, 0.0));
        assert!(noise >= 0.0 && noise <= 1.0);
    }

    #[test]
    fn test_mat4_orthographic_2d() {
        let m = orthographic_topleft_2d(100.0, 100.0, -1.0, 1.0);
        assert!(m.is_finite());
    }

    #[test]
    fn test_mat4_transform_2d() {
        let m = transform_2d_mat4(
            Vec2::new(10.0, 20.0),
            0.0,
            Vec2::new(1.0, 1.0),
        );
        assert!(m.is_finite());
    }

    #[test]
    fn test_mat3a_conversion() {
        let m = Matrix3::IDENTITY;
        let glam_m: Mat3A = m.to_glam();
        assert!(glam_m.is_finite());
    }

    #[test]
    fn test_mat3a_orthographic() {
        let m = orthographic_topleft_a(100.0, 100.0);
        let origin = m.transform_point(Vec2::new(0.0, 0.0));
        assert!((origin.x - (-1.0)).abs() < 0.0001);
        assert!((origin.y - 1.0).abs() < 0.0001);
    }

    #[test]
    fn test_transform_2d_mat3() {
        let m = transform_2d_mat3(Vec2::new(5.0, 10.0), 0.0, Vec2::new(2.0, 3.0));
        let p = m.transform_point(Vec2::new(1.0, 1.0));
        assert!((p.x - 7.0).abs() < 0.0001);  // 1*2 + 5
        assert!((p.y - 13.0).abs() < 0.0001); // 1*3 + 10
    }

    #[test]
    fn test_vec2_cross() {
        let a = Vec2::new(1.0, 0.0);
        let b = Vec2::new(0.0, 1.0);
        assert!((vec2_cross(a, b) - 1.0).abs() < 0.0001);
    }
}
