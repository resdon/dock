//! Constraint system for the physics engine.
//!
//! A constraint describes how objects are allowed or encouraged to relate to
//! each other.  This module provides a rich set of constraint types that make
//! the physics useful for more than individual objects.
//!
//! ## Constraint Types
//!
//! | Constraint  | Behavior                                    |
//! |-------------|---------------------------------------------|
//! | Spring      | Maintains approximate distance              |
//! | Fixed       | Keeps objects attached                      |
//! | Distance    | Maintains a specified distance              |
//! | Axis        | Restricts movement to one axis              |
//! | Path        | Restricts movement to a curve               |
//! | Alignment   | Aligns objects                              |
//! | Separation  | Keeps objects apart                         |
//! | Attraction  | Pulls objects together                      |
//! | Group       | Maintains collective arrangement            |
//! | Anchor      | Connects an object to a fixed point         |
//!
//! For example, a dock icon can have:
//! - An axis constraint to keep it on the dock.
//! - A separation constraint to avoid overlap.
//! - A spring constraint to return it to its assigned slot.
//! - A temporary pointer constraint while dragging.
//!
//! ## Unified Constraint Enum
//!
//! [`Constraint`] is a unified enum that wraps all constraint types, enabling
//! a single collection managed by [`ConstraintSystem`].  The legacy
//! [`ConstraintSet`] struct remains for backward compatibility.

use std::collections::HashMap;

use super::body::{BodyId, PhysicsBody};
use crate::geometry::{Point, Rect, Vector2D};

/// A constraint that anchors a body to a fixed point.
#[derive(Debug, Clone)]
pub struct AnchorConstraint {
    /// Body ID.
    pub body: BodyId,
    /// Fixed anchor point.
    pub anchor_point: Point,
    /// Stiffness (0.0–1.0, 1.0 = rigid).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl AnchorConstraint {
    /// Create a new anchor constraint.
    pub fn new(body: BodyId, anchor_point: Point) -> Self {
        Self {
            body,
            anchor_point,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the anchor constraint using position correction.
    pub fn apply(&self, body: &mut PhysicsBody) {
        if !self.enabled || body.inv_mass == 0.0 {
            return;
        }
        let dx = self.anchor_point.x - body.position.x;
        let dy = self.anchor_point.y - body.position.y;
        body.position.x += dx * self.stiffness;
        body.position.y += dy * self.stiffness;
    }
}

/// A hinge joint that allows rotation around a shared point.
#[derive(Debug, Clone)]
pub struct HingeJoint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Anchor point on body A (local).
    pub anchor_a: Point,
    /// Anchor point on body B (local).
    pub anchor_b: Point,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the joint is active.
    pub enabled: bool,
}

impl HingeJoint {
    /// Create a new hinge joint.
    pub fn new(body_a: BodyId, body_b: BodyId, anchor_a: Point, anchor_b: Point) -> Self {
        Self {
            body_a,
            body_b,
            anchor_a,
            anchor_b,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Get the world-space anchor point for body A.
    pub fn world_anchor_a(&self, body: &PhysicsBody) -> Point {
        Point::new(
            body.position.x + self.anchor_a.x,
            body.position.y + self.anchor_a.y,
        )
    }

    /// Get the world-space anchor point for body B.
    pub fn world_anchor_b(&self, body: &PhysicsBody) -> Point {
        Point::new(
            body.position.x + self.anchor_b.x,
            body.position.y + self.anchor_b.y,
        )
    }

    /// Apply the hinge constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let wa = self.world_anchor_a(a);
        let wb = self.world_anchor_b(b);

        // Target: both anchors should be at the same point
        let target_x = (wa.x + wb.x) / 2.0;
        let target_y = (wa.y + wb.y) / 2.0;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            let dx_a = (target_x - wa.x) * self.stiffness;
            let dy_a = (target_y - wa.y) * self.stiffness;
            let dx_b = (target_x - wb.x) * self.stiffness;
            let dy_b = (target_y - wb.y) * self.stiffness;

            a.position.x += dx_a * (a_inv / total);
            a.position.y += dy_a * (a_inv / total);
            b.position.x += dx_b * (b_inv / total);
            b.position.y += dy_b * (b_inv / total);
        }
    }
}

/// A weld joint that locks two bodies together.
#[derive(Debug, Clone)]
pub struct WeldJoint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Offset from A to B.
    pub offset: Vector2D,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the joint is active.
    pub enabled: bool,
}

impl WeldJoint {
    /// Create a new weld joint.
    pub fn new(body_a: BodyId, body_b: BodyId, offset: Vector2D) -> Self {
        Self {
            body_a,
            body_b,
            offset,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the weld constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        // B should be at A + offset
        let target_b_x = a.position.x + self.offset.x;
        let target_b_y = a.position.y + self.offset.y;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            let dx = (target_b_x - b.position.x) * self.stiffness;
            let dy = (target_b_y - b.position.y) * self.stiffness;

            b.position.x += dx * (b_inv / total);
            b.position.y += dy * (b_inv / total);
            a.position.x -= dx * (a_inv / total);
            a.position.y -= dy * (a_inv / total);
        }
    }
}

/// A prismatic joint that allows motion along a single axis.
#[derive(Debug, Clone)]
pub struct PrismaticJoint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Axis of motion.
    pub axis: Vector2D,
    /// Minimum distance along axis.
    pub min_limit: f32,
    /// Maximum distance along axis.
    pub max_limit: f32,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the joint is active.
    pub enabled: bool,
}

impl PrismaticJoint {
    /// Create a new prismatic joint.
    pub fn new(
        body_a: BodyId,
        body_b: BodyId,
        axis: Vector2D,
        min_limit: f32,
        max_limit: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            axis,
            min_limit,
            max_limit,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the prismatic constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let rel = Vector2D::new(b.position.x - a.position.x, b.position.y - a.position.y);
        let dist = rel.dot(&self.axis);

        if dist < self.min_limit {
            let correction = (self.min_limit - dist) * self.stiffness;
            b.position.x += self.axis.x * correction;
            b.position.y += self.axis.y * correction;
        } else if dist > self.max_limit {
            let correction = (self.max_limit - dist) * self.stiffness;
            b.position.x += self.axis.x * correction;
            b.position.y += self.axis.y * correction;
        }

        // Constrain perpendicular motion
        let perp = Vector2D::new(-self.axis.y, self.axis.x);
        let perp_dist = rel.dot(&perp);
        if perp_dist.abs() > 0.001 {
            let correction = -perp_dist * self.stiffness;
            b.position.x += perp.x * correction;
            b.position.y += perp.y * correction;
        }
    }
}

/// A region constraint that keeps a body within a rectangle.
#[derive(Debug, Clone)]
pub struct RegionConstraint {
    /// Body ID.
    pub body: BodyId,
    /// Boundary rectangle.
    pub region: Rect,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl RegionConstraint {
    /// Create a new region constraint.
    pub fn new(body: BodyId, region: Rect) -> Self {
        Self {
            body,
            region,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the region constraint.
    pub fn apply(&self, body: &mut PhysicsBody) {
        if !self.enabled || body.inv_mass == 0.0 {
            return;
        }
        let mut dx = 0.0;
        let mut dy = 0.0;

        if body.position.x < self.region.x {
            dx = self.region.x - body.position.x;
        } else if body.position.x > self.region.right() {
            dx = self.region.right() - body.position.x;
        }

        if body.position.y < self.region.y {
            dy = self.region.y - body.position.y;
        } else if body.position.y > self.region.bottom() {
            dy = self.region.bottom() - body.position.y;
        }

        body.position.x += dx * self.stiffness;
        body.position.y += dy * self.stiffness;
    }
}

/// A slider constraint that restricts motion to a line segment.
#[derive(Debug, Clone)]
pub struct SliderConstraint {
    /// Body ID.
    pub body: BodyId,
    /// Start of the slider track.
    pub start: Point,
    /// End of the slider track.
    pub end: Point,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl SliderConstraint {
    /// Create a new slider constraint.
    pub fn new(body: BodyId, start: Point, end: Point) -> Self {
        Self {
            body,
            start,
            end,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the slider constraint.
    pub fn apply(&self, body: &mut PhysicsBody) {
        if !self.enabled || body.inv_mass == 0.0 {
            return;
        }
        let dir = Vector2D::new(self.end.x - self.start.x, self.end.y - self.start.y);
        let dir_len = dir.length();
        if dir_len < 0.001 {
            return;
        }
        let ndir = Vector2D::new(dir.x / dir_len, dir.y / dir_len);

        // Project body position onto the line
        let rel = Vector2D::new(
            body.position.x - self.start.x,
            body.position.y - self.start.y,
        );
        let proj = rel.dot(&ndir);
        let clamped_proj = proj.clamp(0.0, dir_len);

        // Target point on the line
        let target = Point::new(
            self.start.x + ndir.x * clamped_proj,
            self.start.y + ndir.y * clamped_proj,
        );

        let dx = target.x - body.position.x;
        let dy = target.y - body.position.y;
        body.position.x += dx * self.stiffness;
        body.position.y += dy * self.stiffness;
    }

    /// Get the current position along the slider (0.0–1.0).
    pub fn get_position(&self, body: &PhysicsBody) -> f32 {
        let dir = Vector2D::new(self.end.x - self.start.x, self.end.y - self.start.y);
        let dir_len = dir.length();
        if dir_len < 0.001 {
            return 0.0;
        }
        let ndir = Vector2D::new(dir.x / dir_len, dir.y / dir_len);
        let rel = Vector2D::new(
            body.position.x - self.start.x,
            body.position.y - self.start.y,
        );
        let proj = rel.dot(&ndir);
        (proj / dir_len).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// New constraint types from the constraint system design
// ---------------------------------------------------------------------------

/// A spring constraint that maintains an approximate distance between two bodies.
///
/// Unlike a rigid distance constraint, a spring allows some flexibility: the
/// bodies are attracted toward the rest length but can stretch or compress.
///
/// Formula: `F = -k * (current_dist - rest_length) - c * relative_velocity`
#[derive(Debug, Clone)]
pub struct SpringConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Rest length of the spring.
    pub rest_length: f32,
    /// Spring stiffness (higher = stiffer).
    pub stiffness: f32,
    /// Damping coefficient.
    pub damping: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl SpringConstraint {
    /// Create a new spring constraint between two bodies.
    pub fn new(body_a: BodyId, body_b: BodyId, rest_length: f32) -> Self {
        Self {
            body_a,
            body_b,
            rest_length,
            stiffness: 500.0,
            damping: 10.0,
            enabled: true,
        }
    }

    /// Apply the spring constraint using position correction.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let current_dist = (dx * dx + dy * dy).sqrt();
        if current_dist < 0.001 {
            return;
        }

        let nx = dx / current_dist;
        let ny = dy / current_dist;

        // Spring displacement
        let displacement = current_dist - self.rest_length;
        let correction = displacement * self.stiffness / 1000.0;

        // Relative velocity along the spring axis
        let rel_vx = b.velocity.x - a.velocity.x;
        let rel_vy = b.velocity.y - a.velocity.y;
        let rel_vel = rel_vx * nx + rel_vy * ny;
        let damping_correction = rel_vel * self.damping / 1000.0;

        let total_correction = (correction + damping_correction) * 0.5;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            a.position.x += nx * total_correction * (a_inv / total);
            a.position.y += ny * total_correction * (a_inv / total);
            b.position.x -= nx * total_correction * (b_inv / total);
            b.position.y -= ny * total_correction * (b_inv / total);
        }
    }
}

/// A fixed constraint that keeps two bodies rigidly attached.
///
/// The two bodies maintain a fixed offset between them, effectively welding
/// them together without a separate weld joint.
#[derive(Debug, Clone)]
pub struct FixedConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Offset from A to B (locked at creation time).
    pub offset: Vector2D,
    /// Stiffness (0.0–1.0, 1.0 = rigid).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl FixedConstraint {
    /// Create a new fixed constraint, computing the offset from current positions.
    pub fn new(body_a: BodyId, body_b: BodyId, a_pos: Point, b_pos: Point) -> Self {
        Self {
            body_a,
            body_b,
            offset: Vector2D::new(b_pos.x - a_pos.x, b_pos.y - a_pos.y),
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the fixed constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let target_b_x = a.position.x + self.offset.x;
        let target_b_y = a.position.y + self.offset.y;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            let dx = (target_b_x - b.position.x) * self.stiffness;
            let dy = (target_b_y - b.position.y) * self.stiffness;

            b.position.x += dx * (b_inv / total);
            b.position.y += dy * (b_inv / total);
            a.position.x -= dx * (a_inv / total);
            a.position.y -= dy * (a_inv / total);
        }
    }
}

/// A distance constraint that maintains a specified distance between two bodies.
///
/// Unlike a spring, this is a hard constraint: the distance is corrected
/// directly rather than through a spring force.
#[derive(Debug, Clone)]
pub struct DistanceConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Target distance.
    pub distance: f32,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl DistanceConstraint {
    /// Create a new distance constraint.
    pub fn new(body_a: BodyId, body_b: BodyId, distance: f32) -> Self {
        Self {
            body_a,
            body_b,
            distance,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the distance constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let current_dist = (dx * dx + dy * dy).sqrt();
        if current_dist < 0.001 {
            return;
        }

        let nx = dx / current_dist;
        let ny = dy / current_dist;
        let diff = (current_dist - self.distance) * self.stiffness;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            a.position.x += nx * diff * 0.5 * (a_inv / total);
            a.position.y += ny * diff * 0.5 * (a_inv / total);
            b.position.x -= nx * diff * 0.5 * (b_inv / total);
            b.position.y -= ny * diff * 0.5 * (b_inv / total);
        }
    }
}

/// An axis constraint that restricts a body's movement to one axis.
///
/// Used to keep dock icons on the dock axis, for example.
#[derive(Debug, Clone)]
pub struct AxisConstraint {
    /// Body ID.
    pub body: BodyId,
    /// Axis direction (normalized).
    pub axis: Vector2D,
    /// Perpendicular center position (the body is constrained to this line).
    pub center: Point,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl AxisConstraint {
    /// Create a new axis constraint.
    pub fn new(body: BodyId, axis: Vector2D, center: Point) -> Self {
        Self {
            body,
            axis,
            center,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the axis constraint, restricting perpendicular motion.
    pub fn apply(&self, body: &mut PhysicsBody) {
        if !self.enabled || body.inv_mass == 0.0 {
            return;
        }
        // Perpendicular direction
        let perp = Vector2D::new(-self.axis.y, self.axis.x);

        // Vector from center to body
        let rel = Vector2D::new(
            body.position.x - self.center.x,
            body.position.y - self.center.y,
        );
        let perp_dist = rel.dot(&perp);

        if perp_dist.abs() > 0.001 {
            let correction = -perp_dist * self.stiffness;
            body.position.x += perp.x * correction;
            body.position.y += perp.y * correction;
        }
    }

    /// Get the body's position along the axis (0.0–1.0 range relative to center).
    pub fn get_position_along(&self, body: &PhysicsBody) -> f32 {
        let rel = Vector2D::new(
            body.position.x - self.center.x,
            body.position.y - self.center.y,
        );
        rel.dot(&self.axis)
    }
}

/// A path constraint that restricts a body's movement to a curve defined by waypoints.
#[derive(Debug, Clone)]
pub struct PathConstraint {
    /// Body ID.
    pub body: BodyId,
    /// Waypoints defining the path curve.
    pub waypoints: Vec<Point>,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl PathConstraint {
    /// Create a new path constraint from waypoints.
    pub fn new(body: BodyId, waypoints: Vec<Point>) -> Self {
        Self {
            body,
            waypoints,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the path constraint, pulling the body toward the nearest point on the path.
    pub fn apply(&self, body: &mut PhysicsBody) {
        if !self.enabled || body.inv_mass == 0.0 || self.waypoints.len() < 2 {
            return;
        }

        // Find the nearest point on the path
        let mut min_dist = f32::MAX;
        let mut nearest = self.waypoints[0];

        for i in 0..self.waypoints.len() - 1 {
            let p1 = self.waypoints[i];
            let p2 = self.waypoints[i + 1];
            let pt = Self::nearest_point_on_segment(body.position, p1, p2);
            let dx = body.position.x - pt.x;
            let dy = body.position.y - pt.y;
            let dist = dx * dx + dy * dy;
            if dist < min_dist {
                min_dist = dist;
                nearest = pt;
            }
        }

        // Also check the last waypoint
        let dx = body.position.x - self.waypoints[self.waypoints.len() - 1].x;
        let dy = body.position.y - self.waypoints[self.waypoints.len() - 1].y;
        let dist = dx * dx + dy * dy;
        if dist < min_dist {
            nearest = self.waypoints[self.waypoints.len() - 1];
        }

        let dx = nearest.x - body.position.x;
        let dy = nearest.y - body.position.y;
        body.position.x += dx * self.stiffness;
        body.position.y += dy * self.stiffness;
    }

    /// Find the nearest point on a line segment to a given point.
    fn nearest_point_on_segment(p: Point, a: Point, b: Point) -> Point {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len_sq = dx * dx + dy * dy;
        if len_sq < 0.0001 {
            return a;
        }
        let mut t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / len_sq;
        t = t.clamp(0.0, 1.0);
        Point::new(a.x + t * dx, a.y + t * dy)
    }

    /// Get the current progress along the path (0.0–1.0).
    pub fn get_progress(&self, body: &PhysicsBody) -> f32 {
        if self.waypoints.len() < 2 {
            return 0.0;
        }

        // Compute total path length
        let mut total_len = 0.0;
        for i in 0..self.waypoints.len() - 1 {
            let dx = self.waypoints[i + 1].x - self.waypoints[i].x;
            let dy = self.waypoints[i + 1].y - self.waypoints[i].y;
            total_len += (dx * dx + dy * dy).sqrt();
        }
        if total_len < 0.001 {
            return 0.0;
        }

        // Compute distance from start to nearest point on path
        let mut traveled = 0.0;
        for i in 0..self.waypoints.len() - 1 {
            let p1 = self.waypoints[i];
            let p2 = self.waypoints[i + 1];
            let pt = Self::nearest_point_on_segment(body.position, p1, p2);
            let dx = body.position.x - pt.x;
            let dy = body.position.y - pt.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < 5.0 {
                // Body is close to this segment
                let seg_len =
                    ((p2.x - p1.x) * (p2.x - p1.x) + (p2.y - p1.y) * (p2.y - p1.y)).sqrt();
                let t = if seg_len > 0.001 {
                    ((body.position.x - p1.x) * (p2.x - p1.x)
                        + (body.position.y - p1.y) * (p2.y - p1.y))
                        / (seg_len * seg_len)
                } else {
                    0.0
                };
                return (traveled + t * seg_len) / total_len;
            }
            traveled += ((p2.x - p1.x) * (p2.x - p1.x) + (p2.y - p1.y) * (p2.y - p1.y)).sqrt();
        }
        1.0
    }
}

/// An alignment constraint that aligns two bodies along one or both axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignmentAxis {
    /// Align horizontally (same X).
    Horizontal,
    /// Align vertically (same Y).
    Vertical,
    /// Align on both axes.
    Both,
}

#[derive(Debug, Clone)]
pub struct AlignmentConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Which axis to align on.
    pub axis: AlignmentAxis,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl AlignmentConstraint {
    /// Create a new alignment constraint.
    pub fn new(body_a: BodyId, body_b: BodyId, axis: AlignmentAxis) -> Self {
        Self {
            body_a,
            body_b,
            axis,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the alignment constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total == 0.0 {
            return;
        }

        let align_x = matches!(
            self.axis,
            AlignmentAxis::Horizontal | AlignmentAxis::Both
        );
        let align_y = matches!(self.axis, AlignmentAxis::Vertical | AlignmentAxis::Both);

        if align_x {
            let mid_x = (a.position.x * b_inv + b.position.x * a_inv) / total;
            a.position.x += (mid_x - a.position.x) * self.stiffness;
            b.position.x += (mid_x - b.position.x) * self.stiffness;
        }
        if align_y {
            let mid_y = (a.position.y * b_inv + b.position.y * a_inv) / total;
            a.position.y += (mid_y - a.position.y) * self.stiffness;
            b.position.y += (mid_y - b.position.y) * self.stiffness;
        }
    }
}

/// A separation constraint that keeps two bodies apart by a minimum distance.
///
/// If the bodies are closer than the minimum distance, they are pushed apart.
#[derive(Debug, Clone)]
pub struct SeparationConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Minimum distance to maintain.
    pub min_distance: f32,
    /// Stiffness (0.0–1.0).
    pub stiffness: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl SeparationConstraint {
    /// Create a new separation constraint.
    pub fn new(body_a: BodyId, body_b: BodyId, min_distance: f32) -> Self {
        Self {
            body_a,
            body_b,
            min_distance,
            stiffness: 1.0,
            enabled: true,
        }
    }

    /// Apply the separation constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < self.min_distance && dist > 0.001 {
            let nx = dx / dist;
            let ny = dy / dist;
            let penetration = self.min_distance - dist;
            let correction = penetration * self.stiffness * 0.5;

            let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
            let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
            let total = a_inv + b_inv;

            if total > 0.0 {
                a.position.x -= nx * correction * (a_inv / total);
                a.position.y -= ny * correction * (a_inv / total);
                b.position.x += nx * correction * (b_inv / total);
                b.position.y += ny * correction * (b_inv / total);
            }
        }
    }
}

/// An attraction constraint that pulls two bodies toward each other.
///
/// Unlike a spring, this only pulls bodies together and does not push them apart.
#[derive(Debug, Clone)]
pub struct AttractionConstraint {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Maximum distance at which attraction is active.
    pub max_distance: f32,
    /// Attraction strength.
    pub strength: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl AttractionConstraint {
    /// Create a new attraction constraint.
    pub fn new(body_a: BodyId, body_b: BodyId, max_distance: f32, strength: f32) -> Self {
        Self {
            body_a,
            body_b,
            max_distance,
            strength,
            enabled: true,
        }
    }

    /// Apply the attraction constraint.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > self.max_distance || dist < 0.001 {
            return;
        }

        let nx = dx / dist;
        let ny = dy / dist;
        // Attraction strength decreases with distance
        let factor = (1.0 - dist / self.max_distance) * self.strength;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total = a_inv + b_inv;

        if total > 0.0 {
            a.position.x += nx * factor * (a_inv / total);
            a.position.y += ny * factor * (a_inv / total);
            b.position.x -= nx * factor * (b_inv / total);
            b.position.y -= ny * factor * (b_inv / total);
        }
    }
}

/// A group constraint that maintains a collective arrangement of multiple bodies.
///
/// Bodies in a group are attracted toward a common center while maintaining
/// their relative positions.  This is useful for keeping a group of icons
/// together as a cohesive unit.
#[derive(Debug, Clone)]
pub struct GroupConstraint {
    /// Body IDs in the group.
    pub bodies: Vec<BodyId>,
    /// Center of the group (computed from body positions).
    pub center: Point,
    /// Cohesion strength (attraction toward center).
    pub cohesion_strength: f32,
    /// Separation strength (repulsion between members).
    pub separation_strength: f32,
    /// Minimum separation distance between members.
    pub min_separation: f32,
    /// Whether the constraint is active.
    pub enabled: bool,
}

impl GroupConstraint {
    /// Create a new group constraint.
    pub fn new(bodies: Vec<BodyId>) -> Self {
        Self {
            bodies,
            center: Point::ORIGIN,
            cohesion_strength: 100.0,
            separation_strength: 50.0,
            min_separation: 20.0,
            enabled: true,
        }
    }

    /// Recompute the group center from current body positions.
    pub fn update_center(&mut self, bodies: &[&PhysicsBody]) {
        if bodies.is_empty() {
            self.center = Point::ORIGIN;
            return;
        }
        let mut x = 0.0;
        let mut y = 0.0;
        for body in bodies {
            x += body.position.x;
            y += body.position.y;
        }
        self.center.x = x / bodies.len() as f32;
        self.center.y = y / bodies.len() as f32;
    }

    /// Apply the group constraint: cohesion toward center + separation between members.
    pub fn apply(
        &self,
        body_map: &HashMap<BodyId, PhysicsBody>,
        bodies_mut: &mut HashMap<BodyId, PhysicsBody>,
    ) {
        if !self.enabled || self.bodies.len() < 2 {
            return;
        }

        // Gather current positions
        let mut positions: Vec<(BodyId, Point)> = Vec::new();
        for &bid in &self.bodies {
            if let Some(body) = body_map.get(&bid) {
                positions.push((bid, body.position));
            }
        }

        if positions.is_empty() {
            return;
        }

        // Compute center
        let mut cx = 0.0;
        let mut cy = 0.0;
        for (_, pos) in &positions {
            cx += pos.x;
            cy += pos.y;
        }
        cx /= positions.len() as f32;
        cy /= positions.len() as f32;

        // Apply cohesion and separation
        for (bid, pos) in &positions {
            if let Some(body) = bodies_mut.get_mut(bid) {
                if body.inv_mass == 0.0 {
                    continue;
                }

                // Cohesion: pull toward center
                let cdx = cx - pos.x;
                let cdy = cy - pos.y;
                body.position.x += cdx * self.cohesion_strength * 0.01;
                body.position.y += cdy * self.cohesion_strength * 0.01;

                // Separation: push away from other members
                for (other_bid, other_pos) in &positions {
                    if *other_bid == *bid {
                        continue;
                    }
                    let dx = pos.x - other_pos.x;
                    let dy = pos.y - other_pos.y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist < self.min_separation && dist > 0.001 {
                        let nx = dx / dist;
                        let ny = dy / dist;
                        let push = (self.min_separation - dist) * self.separation_strength * 0.01;
                        body.position.x += nx * push;
                        body.position.y += ny * push;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Unified constraint enum
// ---------------------------------------------------------------------------

/// A unified enum wrapping all constraint types.
///
/// Enables storing heterogeneous constraints in a single collection managed
/// by [`ConstraintSystem`].
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Anchors a body to a fixed point.
    Anchor(AnchorConstraint),
    /// Hinge joint between two bodies.
    Hinge(HingeJoint),
    /// Weld joint between two bodies.
    Weld(WeldJoint),
    /// Prismatic joint with axis-limited motion.
    Prismatic(PrismaticJoint),
    /// Keeps a body within a rectangular region.
    Region(RegionConstraint),
    /// Restricts motion to a line segment.
    Slider(SliderConstraint),
    /// Maintains approximate distance (spring-like).
    Spring(SpringConstraint),
    /// Keeps two bodies rigidly attached.
    Fixed(FixedConstraint),
    /// Maintains a specified distance.
    Distance(DistanceConstraint),
    /// Restricts movement to one axis.
    Axis(AxisConstraint),
    /// Restricts movement to a curve.
    Path(PathConstraint),
    /// Aligns two bodies.
    Alignment(AlignmentConstraint),
    /// Keeps two bodies apart.
    Separation(SeparationConstraint),
    /// Pulls two bodies together.
    Attraction(AttractionConstraint),
    /// Maintains collective group arrangement.
    Group(GroupConstraint),
}

impl Constraint {
    /// Get the body IDs involved in this constraint.
    pub fn body_ids(&self) -> Vec<BodyId> {
        match self {
            Constraint::Anchor(c) => vec![c.body],
            Constraint::Hinge(c) => vec![c.body_a, c.body_b],
            Constraint::Weld(c) => vec![c.body_a, c.body_b],
            Constraint::Prismatic(c) => vec![c.body_a, c.body_b],
            Constraint::Region(c) => vec![c.body],
            Constraint::Slider(c) => vec![c.body],
            Constraint::Spring(c) => vec![c.body_a, c.body_b],
            Constraint::Fixed(c) => vec![c.body_a, c.body_b],
            Constraint::Distance(c) => vec![c.body_a, c.body_b],
            Constraint::Axis(c) => vec![c.body],
            Constraint::Path(c) => vec![c.body],
            Constraint::Alignment(c) => vec![c.body_a, c.body_b],
            Constraint::Separation(c) => vec![c.body_a, c.body_b],
            Constraint::Attraction(c) => vec![c.body_a, c.body_b],
            Constraint::Group(c) => c.bodies.clone(),
        }
    }

    /// Check if this constraint involves a specific body.
    pub fn involves_body(&self, body: BodyId) -> bool {
        self.body_ids().contains(&body)
    }
}

/// A constraint system that manages a collection of heterogeneous constraints.
///
/// Unlike [`ConstraintSet`] which stores each constraint type in a separate
/// vector, `ConstraintSystem` uses a single vector of [`Constraint`] enums,
/// making it easier to add, remove, and iterate over constraints uniformly.
#[derive(Debug, Clone)]
pub struct ConstraintSystem {
    /// All constraints.
    constraints: Vec<Constraint>,
}

impl ConstraintSystem {
    /// Create a new empty constraint system.
    pub fn new() -> Self {
        Self {
            constraints: Vec::new(),
        }
    }

    /// Total number of constraints.
    pub fn len(&self) -> usize {
        self.constraints.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.constraints.is_empty()
    }

    /// Add a constraint.
    pub fn add(&mut self, constraint: Constraint) {
        self.constraints.push(constraint);
    }

    /// Remove a constraint by index.
    pub fn remove_at(&mut self, index: usize) -> Option<Constraint> {
        if index < self.constraints.len() {
            Some(self.constraints.remove(index))
        } else {
            None
        }
    }

    /// Remove all constraints involving a specific body.
    pub fn remove_body(&mut self, body: BodyId) {
        self.constraints.retain(|c| !c.involves_body(body));
    }

    /// Remove constraints between two specific bodies.
    pub fn remove_pair(&mut self, body_a: BodyId, body_b: BodyId) {
        self.constraints.retain(|c| {
            let ids = c.body_ids();
            !(ids.contains(&body_a) && ids.contains(&body_b))
        });
    }

    /// Iterate over all constraints.
    pub fn iter(&self) -> impl Iterator<Item = &Constraint> {
        self.constraints.iter()
    }

    /// Iterate over all constraints mutably.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Constraint> {
        self.constraints.iter_mut()
    }

    /// Apply all constraints using the provided body map.
    ///
    /// To avoid double-mutable-borrow issues, we collect the body IDs first,
    /// then use a helper to split mutable references safely.
    pub fn apply_all(&self, bodies: &mut HashMap<BodyId, PhysicsBody>) {
        // Collect body IDs for lookup
        let ids: Vec<BodyId> = bodies.keys().copied().collect();

        for constraint in &self.constraints {
            match constraint {
                Constraint::Anchor(c) => {
                    if let Some(body) = bodies.get_mut(&c.body) {
                        c.apply(body);
                    }
                }
                Constraint::Hinge(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Weld(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Prismatic(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Region(c) => {
                    if let Some(body) = bodies.get_mut(&c.body) {
                        c.apply(body);
                    }
                }
                Constraint::Slider(c) => {
                    if let Some(body) = bodies.get_mut(&c.body) {
                        c.apply(body);
                    }
                }
                Constraint::Spring(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Fixed(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Distance(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Axis(c) => {
                    if let Some(body) = bodies.get_mut(&c.body) {
                        c.apply(body);
                    }
                }
                Constraint::Path(c) => {
                    if let Some(body) = bodies.get_mut(&c.body) {
                        c.apply(body);
                    }
                }
                Constraint::Alignment(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Separation(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Attraction(c) => {
                    Self::apply_two_bodies(bodies, &ids, c.body_a, c.body_b, |a, b| c.apply(a, b));
                }
                Constraint::Group(c) => {
                    let body_map: HashMap<BodyId, PhysicsBody> =
                        bodies.iter().map(|(k, v)| (*k, v.clone())).collect();
                    c.apply(&body_map, bodies);
                }
            }
        }
    }

    /// Helper: apply a two-body constraint by cloning, applying, and writing back.
    fn apply_two_bodies<F>(
        bodies: &mut HashMap<BodyId, PhysicsBody>,
        _ids: &[BodyId],
        body_a: BodyId,
        body_b: BodyId,
        f: F,
    ) where
        F: FnOnce(&mut PhysicsBody, &mut PhysicsBody),
    {
        // Clone the two bodies, apply the constraint, write back
        let a_clone = match bodies.get(&body_a) {
            Some(b) => b.clone(),
            None => return,
        };
        let b_clone = match bodies.get(&body_b) {
            Some(b) => b.clone(),
            None => return,
        };
        let mut a_work = a_clone;
        let mut b_work = b_clone;
        f(&mut a_work, &mut b_work);
        bodies.insert(body_a, a_work);
        bodies.insert(body_b, b_work);
    }

    /// Clear all constraints.
    pub fn clear(&mut self) {
        self.constraints.clear();
    }
}

impl Default for ConstraintSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl ConstraintSystem {
    /// Convenience: add a spring constraint.
    pub fn add_spring(&mut self, body_a: BodyId, body_b: BodyId, rest_length: f32) {
        self.add(Constraint::Spring(SpringConstraint::new(
            body_a,
            body_b,
            rest_length,
        )));
    }

    /// Convenience: add a fixed constraint.
    pub fn add_fixed(&mut self, body_a: BodyId, body_b: BodyId, a_pos: Point, b_pos: Point) {
        self.add(Constraint::Fixed(FixedConstraint::new(
            body_a, body_b, a_pos, b_pos,
        )));
    }

    /// Convenience: add a distance constraint.
    pub fn add_distance(&mut self, body_a: BodyId, body_b: BodyId, distance: f32) {
        self.add(Constraint::Distance(DistanceConstraint::new(
            body_a, body_b, distance,
        )));
    }

    /// Convenience: add an axis constraint.
    pub fn add_axis(&mut self, body: BodyId, axis: Vector2D, center: Point) {
        self.add(Constraint::Axis(AxisConstraint::new(body, axis, center)));
    }

    /// Convenience: add a path constraint.
    pub fn add_path(&mut self, body: BodyId, waypoints: Vec<Point>) {
        self.add(Constraint::Path(PathConstraint::new(body, waypoints)));
    }

    /// Convenience: add an alignment constraint.
    pub fn add_alignment(&mut self, body_a: BodyId, body_b: BodyId, axis: AlignmentAxis) {
        self.add(Constraint::Alignment(AlignmentConstraint::new(
            body_a, body_b, axis,
        )));
    }

    /// Convenience: add a separation constraint.
    pub fn add_separation(&mut self, body_a: BodyId, body_b: BodyId, min_distance: f32) {
        self.add(Constraint::Separation(SeparationConstraint::new(
            body_a,
            body_b,
            min_distance,
        )));
    }

    /// Convenience: add an attraction constraint.
    pub fn add_attraction(
        &mut self,
        body_a: BodyId,
        body_b: BodyId,
        max_distance: f32,
        strength: f32,
    ) {
        self.add(Constraint::Attraction(AttractionConstraint::new(
            body_a,
            body_b,
            max_distance,
            strength,
        )));
    }

    /// Convenience: add a group constraint.
    pub fn add_group(&mut self, bodies: Vec<BodyId>) {
        self.add(Constraint::Group(GroupConstraint::new(bodies)));
    }
}

/// Collection of constraints for a body group.
#[derive(Debug, Clone)]
pub struct ConstraintSet {
    /// Anchor constraints.
    pub anchors: Vec<AnchorConstraint>,
    /// Hinge joints.
    pub hinges: Vec<HingeJoint>,
    /// Weld joints.
    pub welds: Vec<WeldJoint>,
    /// Prismatic joints.
    pub prismatic: Vec<PrismaticJoint>,
    /// Region constraints.
    pub regions: Vec<RegionConstraint>,
    /// Slider constraints.
    pub sliders: Vec<SliderConstraint>,
}

impl ConstraintSet {
    /// Create a new empty constraint set.
    pub fn new() -> Self {
        Self {
            anchors: Vec::new(),
            hinges: Vec::new(),
            welds: Vec::new(),
            prismatic: Vec::new(),
            regions: Vec::new(),
            sliders: Vec::new(),
        }
    }

    /// Total number of constraints.
    pub fn len(&self) -> usize {
        self.anchors.len()
            + self.hinges.len()
            + self.welds.len()
            + self.prismatic.len()
            + self.regions.len()
            + self.sliders.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Add an anchor constraint.
    pub fn add_anchor(&mut self, body: BodyId, point: Point) {
        self.anchors.push(AnchorConstraint::new(body, point));
    }

    /// Add a hinge joint.
    pub fn add_hinge(&mut self, body_a: BodyId, body_b: BodyId, anchor_a: Point, anchor_b: Point) {
        self.hinges
            .push(HingeJoint::new(body_a, body_b, anchor_a, anchor_b));
    }

    /// Add a weld joint.
    pub fn add_weld(&mut self, body_a: BodyId, body_b: BodyId, offset: Vector2D) {
        self.welds.push(WeldJoint::new(body_a, body_b, offset));
    }

    /// Add a region constraint.
    pub fn add_region(&mut self, body: BodyId, region: Rect) {
        self.regions.push(RegionConstraint::new(body, region));
    }

    /// Add a slider constraint.
    pub fn add_slider(&mut self, body: BodyId, start: Point, end: Point) {
        self.sliders.push(SliderConstraint::new(body, start, end));
    }

    /// Remove constraints for a specific body.
    pub fn remove_body(&mut self, body: BodyId) {
        self.anchors.retain(|c| c.body != body);
        self.hinges.retain(|c| c.body_a != body && c.body_b != body);
        self.welds.retain(|c| c.body_a != body && c.body_b != body);
        self.prismatic
            .retain(|c| c.body_a != body && c.body_b != body);
        self.regions.retain(|c| c.body != body);
        self.sliders.retain(|c| c.body != body);
    }
}

impl Default for ConstraintSet {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_anchor_constraint() {
        let constraint = AnchorConstraint::new(BodyId::new(1), Point::new(100.0, 100.0));
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        constraint.apply(&mut body);
        assert!((body.position.x - 100.0).abs() < f32::EPSILON);
        assert!((body.position.y - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_anchor_partial_stiffness() {
        let mut constraint = AnchorConstraint::new(BodyId::new(1), Point::new(100.0, 100.0));
        constraint.stiffness = 0.5;
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        constraint.apply(&mut body);
        assert!((body.position.x - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_hinge_joint() {
        let joint = HingeJoint::new(BodyId::new(1), BodyId::new(2), Point::ORIGIN, Point::ORIGIN);
        let mut a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        joint.apply(&mut a, &mut b);
        // Both should move toward the midpoint
        assert!(a.position.x > 0.0);
        assert!(b.position.x < 50.0);
    }

    #[test]
    fn test_weld_joint() {
        let joint = WeldJoint::new(BodyId::new(1), BodyId::new(2), Vector2D::new(50.0, 0.0));
        let mut a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        joint.apply(&mut a, &mut b);
        // B should move toward A + offset = (50, 0)
        assert!(b.position.x < 100.0);
    }

    #[test]
    fn test_region_constraint() {
        let constraint = RegionConstraint::new(BodyId::new(1), Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(150.0, 150.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        constraint.apply(&mut body);
        assert!(body.position.x <= 100.0);
        assert!(body.position.y <= 100.0);
    }

    #[test]
    fn test_region_constraint_inside() {
        let constraint = RegionConstraint::new(BodyId::new(1), Rect::new(0.0, 0.0, 100.0, 100.0));
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let orig_x = body.position.x;
        let orig_y = body.position.y;
        constraint.apply(&mut body);
        assert!((body.position.x - orig_x).abs() < f32::EPSILON);
        assert!((body.position.y - orig_y).abs() < f32::EPSILON);
    }

    #[test]
    fn test_slider_constraint() {
        let constraint =
            SliderConstraint::new(BodyId::new(1), Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        constraint.apply(&mut body);
        // Body should be pulled to the line y=0
        assert!((body.position.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_slider_position() {
        let constraint =
            SliderConstraint::new(BodyId::new(1), Point::new(0.0, 0.0), Point::new(100.0, 0.0));
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let pos = constraint.get_position(&body);
        assert!((pos - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_prismatic_joint() {
        let joint = PrismaticJoint::new(
            BodyId::new(1),
            BodyId::new(2),
            Vector2D::new(1.0, 0.0),
            0.0,
            100.0,
        );
        let mut a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(150.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        joint.apply(&mut a, &mut b);
        // B should be pulled back to max_limit
        assert!(b.position.x < 150.0);
    }

    #[test]
    fn test_constraint_set() {
        let mut set = ConstraintSet::new();
        set.add_anchor(BodyId::new(1), Point::ORIGIN);
        set.add_region(BodyId::new(2), Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(set.len(), 2);
        set.remove_body(BodyId::new(1));
        assert_eq!(set.len(), 1);
        set.remove_body(BodyId::new(2));
        assert!(set.is_empty());
    }

    #[test]
    fn test_disabled_constraint() {
        let mut constraint = AnchorConstraint::new(BodyId::new(1), Point::new(100.0, 100.0));
        constraint.enabled = false;
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        constraint.apply(&mut body);
        assert!((body.position.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_static_body_anchor() {
        let constraint = AnchorConstraint::new(BodyId::new(1), Point::new(100.0, 100.0));
        let mut body = PhysicsBody::static_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
        );
        constraint.apply(&mut body);
        // Static body has inv_mass = 0, so anchor should not move it
        assert!((body.position.x - 0.0).abs() < f32::EPSILON);
    }
}
