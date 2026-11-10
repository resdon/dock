//! Spring constraints between physics bodies.
//!
//! Springs connect two bodies and exert forces to maintain a target distance,
//! creating elastic behavior like a physical spring. Used for:
//! - Connecting dock icons to their rest positions
//! - Creating chain-like behavior between adjacent icons
//! - Simulating elastic connections between groups

use super::body::{BodyId, PhysicsBody};
use crate::geometry::{Point, Vector2D};

/// A spring constraint connecting two bodies.
#[derive(Debug, Clone)]
pub struct SpringConstraint {
    /// First body ID.
    pub body_a: BodyId,
    /// Second body ID.
    pub body_b: BodyId,
    /// Rest length of the spring.
    pub rest_length: f32,
    /// Spring stiffness (higher = stiffer).
    pub stiffness: f32,
    /// Damping ratio (0.0–1.0).
    pub damping: f32,
    /// Whether the spring is currently active.
    pub enabled: bool,
}

impl SpringConstraint {
    /// Create a new spring constraint.
    pub fn new(
        body_a: BodyId,
        body_b: BodyId,
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            rest_length,
            stiffness,
            damping,
            enabled: true,
        }
    }

    /// Create a spring from body A to a fixed point.
    pub fn to_fixed_point(
        body_a: BodyId,
        fixed_point: Point,
        stiffness: f32,
        damping: f32,
    ) -> Self {
        Self {
            body_a,
            body_b: BodyId::new(0), // sentinel for fixed point
            rest_length: 0.0,
            stiffness,
            damping,
            enabled: true,
        }
    }

    /// Compute the spring force between two bodies.
    pub fn compute_force(&self, a: &PhysicsBody, b: &PhysicsBody) -> Vector2D {
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let current_length = (dx * dx + dy * dy).sqrt();

        if current_length < 0.0001 {
            return Vector2D::ZERO;
        }

        // Direction from A to B
        let nx = dx / current_length;
        let ny = dy / current_length;

        // Spring displacement
        let displacement = current_length - self.rest_length;

        // Relative velocity along spring axis
        let rel_vel_x = b.velocity.x - a.velocity.x;
        let rel_vel_y = b.velocity.y - a.velocity.y;
        let rel_vel_along_axis = rel_vel_x * nx + rel_vel_y * ny;

        // Spring force: F = -k * displacement
        let spring_force = -self.stiffness * displacement;

        // Damping force: F = -c * relative_velocity
        let damping_force = -self.damping * self.stiffness * rel_vel_along_axis;

        let total_force = spring_force + damping_force;

        Vector2D::new(nx * total_force, ny * total_force)
    }

    /// Compute the spring force to a fixed point.
    pub fn compute_force_to_point(&self, body: &PhysicsBody, fixed_point: Point) -> Vector2D {
        let dx = fixed_point.x - body.position.x;
        let dy = fixed_point.y - body.position.y;
        let current_length = (dx * dx + dy * dy).sqrt();

        if current_length < 0.0001 {
            return Vector2D::ZERO;
        }

        let nx = dx / current_length;
        let ny = dy / current_length;
        let displacement = current_length - self.rest_length;
        let rel_vel_along_axis = body.velocity.x * nx + body.velocity.y * ny;

        let spring_force = -self.stiffness * displacement;
        let damping_force = -self.damping * self.stiffness * rel_vel_along_axis;
        let total_force = spring_force + damping_force;

        Vector2D::new(nx * total_force, ny * total_force)
    }

    /// Apply the spring force to both bodies.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody, dt: f32) {
        if !self.enabled {
            return;
        }
        let force = self.compute_force(a, b);
        a.apply_force(Vector2D::new(-force.x, -force.y), dt);
        b.apply_force(force, dt);
    }

    /// Check if the spring is at rest.
    pub fn is_at_rest(&self, a: &PhysicsBody, b: &PhysicsBody, threshold: f32) -> bool {
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let current_length = (dx * dx + dy * dy).sqrt();
        let length_diff = (current_length - self.rest_length).abs();
        let rel_vel = Vector2D::new(b.velocity.x - a.velocity.x, b.velocity.y - a.velocity.y);
        length_diff < threshold && rel_vel.length() < threshold
    }
}

/// A distance constraint that maintains a fixed distance between two bodies.
#[derive(Debug, Clone)]
pub struct DistanceConstraint {
    /// First body ID.
    pub body_a: BodyId,
    /// Second body ID.
    pub body_b: BodyId,
    /// Target distance.
    pub distance: f32,
    /// Stiffness (0.0–1.0, 1.0 = rigid).
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

    /// Apply the distance constraint using position correction.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody) {
        if !self.enabled {
            return;
        }
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let current_distance = (dx * dx + dy * dy).sqrt();

        if current_distance < 0.0001 {
            return;
        }

        let diff = (current_distance - self.distance) / current_distance;
        let nx = dx / current_distance;
        let ny = dy / current_distance;

        let correction_x = nx * diff * self.stiffness * 0.5;
        let correction_y = ny * diff * self.stiffness * 0.5;

        let a_inv = if a.inv_mass > 0.0 { a.inv_mass } else { 0.0 };
        let b_inv = if b.inv_mass > 0.0 { b.inv_mass } else { 0.0 };
        let total_inv = a_inv + b_inv;

        if total_inv > 0.0 {
            a.position.x += correction_x * (a_inv / total_inv);
            a.position.y += correction_y * (a_inv / total_inv);
            b.position.x -= correction_x * (b_inv / total_inv);
            b.position.y -= correction_y * (b_inv / total_inv);
        }
    }
}

/// A set of spring constraints forming a chain.
#[derive(Debug, Clone)]
pub struct SpringChain {
    /// Body IDs in chain order.
    pub bodies: Vec<BodyId>,
    /// Spring stiffness.
    pub stiffness: f32,
    /// Spring damping.
    pub damping: f32,
    /// Rest length between adjacent bodies.
    pub rest_length: f32,
}

impl SpringChain {
    /// Create a new spring chain.
    pub fn new(bodies: Vec<BodyId>, rest_length: f32, stiffness: f32, damping: f32) -> Self {
        Self {
            bodies,
            stiffness,
            damping,
            rest_length,
        }
    }

    /// Get the number of springs in the chain.
    pub fn spring_count(&self) -> usize {
        self.bodies.len().saturating_sub(1)
    }

    /// Get the spring constraint between two adjacent bodies.
    pub fn get_spring(&self, index: usize) -> Option<SpringConstraint> {
        if index >= self.spring_count() {
            return None;
        }
        Some(SpringConstraint::new(
            self.bodies[index],
            self.bodies[index + 1],
            self.rest_length,
            self.stiffness,
            self.damping,
        ))
    }

    /// Apply all springs in the chain.
    pub fn apply(&self, bodies_map: &mut std::collections::HashMap<BodyId, PhysicsBody>, dt: f32) {
        // Collect forces first to avoid double mutable borrow
        let mut force_map: std::collections::HashMap<BodyId, crate::geometry::Vector2D> =
            std::collections::HashMap::new();

        for i in 0..self.spring_count() {
            let spring = self.get_spring(i).unwrap();
            let a_body = bodies_map.get(&spring.body_a);
            let b_body = bodies_map.get(&spring.body_b);
            match (a_body, b_body) {
                (Some(a), Some(b)) => {
                    let f = spring.compute_force(a, b);
                    let entry_a = force_map
                        .entry(spring.body_a)
                        .or_insert_with(|| crate::geometry::Vector2D::ZERO);
                    entry_a.x -= f.x;
                    entry_a.y -= f.y;
                    let entry_b = force_map
                        .entry(spring.body_b)
                        .or_insert_with(|| crate::geometry::Vector2D::ZERO);
                    entry_b.x += f.x;
                    entry_b.y += f.y;
                }
                _ => continue,
            }
        }

        // Apply accumulated forces
        for (id, force) in force_map {
            if let Some(body) = bodies_map.get_mut(&id) {
                body.apply_force(force, dt);
            }
        }
    }
}

/// A radial spring system that attracts bodies toward a center point.
#[derive(Debug, Clone)]
pub struct RadialSpringSystem {
    /// Center point.
    pub center: Point,
    /// Spring stiffness.
    pub stiffness: f32,
    /// Spring damping.
    pub damping: f32,
    /// Maximum radius.
    pub max_radius: f32,
}

impl RadialSpringSystem {
    /// Create a new radial spring system.
    pub fn new(center: Point, stiffness: f32, damping: f32, max_radius: f32) -> Self {
        Self {
            center,
            stiffness,
            damping,
            max_radius,
        }
    }

    /// Compute the spring force on a body.
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        let dx = self.center.x - body.position.x;
        let dy = self.center.y - body.position.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < 0.0001 {
            return Vector2D::ZERO;
        }

        // Only apply force if outside the rest radius
        if dist > self.max_radius {
            let nx = dx / dist;
            let ny = dy / dist;
            let displacement = dist - self.max_radius;

            let rel_vel = body.velocity.x * nx + body.velocity.y * ny;
            let spring_force = self.stiffness * displacement;
            let damping_force = self.damping * self.stiffness * rel_vel;
            let total = spring_force + damping_force;

            Vector2D::new(nx * total, ny * total)
        } else {
            Vector2D::ZERO
        }
    }

    /// Apply the radial spring force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_spring_force_pulls_bodies_together() {
        let spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 50.0, 100.0, 0.5);
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute_force(&a, &b);
        // Bodies are farther apart than rest length, so force pulls them together
        assert!(force.x < 0.0); // Force on A is toward B (positive), but compute returns force direction
    }

    #[test]
    fn test_spring_force_pushes_bodies_apart() {
        let spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 100.0, 100.0, 0.5);
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute_force(&a, &b);
        // Bodies are closer than rest length, so force pushes them apart
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_spring_at_rest_length() {
        let spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 50.0, 100.0, 0.5);
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute_force(&a, &b);
        // At rest length with zero velocity, force should be near zero
        assert!(force.length() < 1.0);
    }

    #[test]
    fn test_spring_apply_changes_velocity() {
        let spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 50.0, 100.0, 0.5);
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
        spring.apply(&mut a, &mut b, 0.016);
        // A should move right, B should move left
        assert!(a.velocity.x > 0.0);
        assert!(b.velocity.x < 0.0);
    }

    #[test]
    fn test_disabled_spring() {
        let mut spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 50.0, 100.0, 0.5);
        spring.enabled = false;
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
        spring.apply(&mut a, &mut b, 0.016);
        assert!(a.velocity.length() < 0.01);
        assert!(b.velocity.length() < 0.01);
    }

    #[test]
    fn test_distance_constraint() {
        let constraint = DistanceConstraint::new(BodyId::new(1), BodyId::new(2), 50.0);
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
        constraint.apply(&mut a, &mut b);
        // Bodies should move closer together
        let dx = b.position.x - a.position.x;
        assert!(dx < 100.0);
    }

    #[test]
    fn test_spring_chain() {
        let chain = SpringChain::new(
            vec![BodyId::new(1), BodyId::new(2), BodyId::new(3)],
            50.0,
            100.0,
            0.5,
        );
        assert_eq!(chain.spring_count(), 2);
        assert!(chain.get_spring(0).is_some());
        assert!(chain.get_spring(1).is_some());
        assert!(chain.get_spring(2).is_none());
    }

    #[test]
    fn test_radial_spring_system() {
        let system = RadialSpringSystem::new(Point::new(0.0, 0.0), 100.0, 0.5, 50.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = system.compute_force(&body);
        // Body is outside max radius, should be pulled toward center
        assert!(force.x < 0.0);
    }

    #[test]
    fn test_radial_spring_inside_radius() {
        let system = RadialSpringSystem::new(Point::new(0.0, 0.0), 100.0, 0.5, 100.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = system.compute_force(&body);
        // Body is inside max radius, no force
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_spring_is_at_rest() {
        let spring = SpringConstraint::new(BodyId::new(1), BodyId::new(2), 50.0, 100.0, 0.5);
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        assert!(spring.is_at_rest(&a, &b, 1.0));
    }

    #[test]
    fn test_fixed_point_spring() {
        let spring =
            SpringConstraint::to_fixed_point(BodyId::new(1), Point::new(100.0, 100.0), 100.0, 0.5);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute_force_to_point(&body, Point::new(100.0, 100.0));
        assert!(force.x > 0.0);
        assert!(force.y > 0.0);
    }
}
