//! Force types and application for the physics engine.
//!
//! Forces are applied to bodies to simulate physical behavior:
//! - **Spring forces** attract bodies toward their rest positions
//! - **Drag forces** resist motion (air resistance)
//! - **Repulsion forces** push bodies away from each other
//! - **Wind forces** apply global directional force
//! - **Attraction forces** pull bodies toward a point

use crate::geometry::{Point, Vector2D};
use super::body::PhysicsBody;

/// A spring force configuration for attracting a body to a target position.
#[derive(Debug, Clone, Copy)]
pub struct SpringForce {
    /// Spring stiffness (N/m). Higher = stiffer spring.
    pub stiffness: f32,
    /// Damping ratio (0.0–1.0). Higher = more damping.
    pub damping: f32,
    /// Target position.
    pub target: Point,
}

impl SpringForce {
    /// Create a spring force with the given stiffness and damping.
    pub fn new(stiffness: f32, damping: f32, target: Point) -> Self {
        Self {
            stiffness,
            damping,
            target,
        }
    }

    /// Create a default spring force suitable for dock icons.
    pub fn default_dock_icon(target: Point) -> Self {
        Self {
            stiffness: 500.0,
            damping: 0.8,
            target,
        }
    }

    /// Compute the spring force on a body.
    pub fn compute(&self, body: &PhysicsBody) -> Vector2D {
        let displacement = Vector2D::new(
            self.target.x - body.position.x,
            self.target.y - body.position.y,
        );

        // Spring force: F = -k * x
        let spring = Vector2D::new(
            displacement.x * self.stiffness,
            displacement.y * self.stiffness,
        );

        // Damping force: F = -c * v
        let damping = Vector2D::new(
            -body.velocity.x * self.damping * self.stiffness,
            -body.velocity.y * self.damping * self.stiffness,
        );

        spring + damping
    }

    /// Apply the spring force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute(body);
        body.apply_force(force, dt);
    }

    /// Check if the body is close enough to the target to consider the spring settled.
    pub fn is_settled(&self, body: &PhysicsBody, threshold: f32) -> bool {
        let dx = body.position.x - self.target.x;
        let dy = body.position.y - self.target.y;
        let dist_sq = dx * dx + dy * dy;
        let vel_sq = body.velocity.length_squared();
        dist_sq < threshold * threshold && vel_sq < threshold * threshold
    }
}

/// A drag force that resists motion proportional to velocity.
#[derive(Debug, Clone, Copy)]
pub struct DragForce {
    /// Drag coefficient (0.0 = no drag, 1.0 = maximum drag).
    pub coefficient: f32,
}

impl DragForce {
    /// Create a drag force with the given coefficient.
    pub fn new(coefficient: f32) -> Self {
        Self { coefficient }
    }

    /// Compute the drag force on a body.
    pub fn compute(&self, body: &PhysicsBody) -> Vector2D {
        Vector2D::new(
            -body.velocity.x * self.coefficient,
            -body.velocity.y * self.coefficient,
        )
    }

    /// Apply the drag force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute(body);
        body.apply_force(force, dt);
    }
}

/// A repulsion force that pushes bodies away from a point.
#[derive(Debug, Clone, Copy)]
pub struct RepulsionForce {
    /// Source point of repulsion.
    pub source: Point,
    /// Strength of repulsion.
    pub strength: f32,
    /// Maximum distance for repulsion effect.
    pub max_distance: f32,
}

impl RepulsionForce {
    /// Create a repulsion force.
    pub fn new(source: Point, strength: f32, max_distance: f32) -> Self {
        Self {
            source,
            strength,
            max_distance,
        }
    }

    /// Compute the repulsion force on a body.
    pub fn compute(&self, body: &PhysicsBody) -> Vector2D {
        let dx = body.position.x - self.source.x;
        let dy = body.position.y - self.source.y;
        let dist_sq = dx * dx + dy * dy;

        if dist_sq > self.max_distance * self.max_distance || dist_sq < 0.0001 {
            return Vector2D::ZERO;
        }

        let dist = dist_sq.sqrt();
        // Force decreases with distance: F = strength / dist
        let magnitude = self.strength / dist;
        let nx = dx / dist;
        let ny = dy / dist;

        Vector2D::new(nx * magnitude, ny * magnitude)
    }

    /// Apply the repulsion force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute(body);
        body.apply_force(force, dt);
    }
}

/// An attraction force that pulls bodies toward a point.
#[derive(Debug, Clone, Copy)]
pub struct AttractionForce {
    /// Target point of attraction.
    pub target: Point,
    /// Strength of attraction.
    pub strength: f32,
    /// Maximum distance for attraction effect.
    pub max_distance: f32,
}

impl AttractionForce {
    /// Create an attraction force.
    pub fn new(target: Point, strength: f32, max_distance: f32) -> Self {
        Self {
            target,
            strength,
            max_distance,
        }
    }

    /// Compute the attraction force on a body.
    pub fn compute(&self, body: &PhysicsBody) -> Vector2D {
        let dx = self.target.x - body.position.x;
        let dy = self.target.y - body.position.y;
        let dist_sq = dx * dx + dy * dy;

        if dist_sq > self.max_distance * self.max_distance || dist_sq < 0.0001 {
            return Vector2D::ZERO;
        }

        let dist = dist_sq.sqrt();
        let magnitude = self.strength / dist;
        let nx = dx / dist;
        let ny = dy / dist;

        Vector2D::new(nx * magnitude, ny * magnitude)
    }

    /// Apply the attraction force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute(body);
        body.apply_force(force, dt);
    }
}

/// A wind force that applies a constant directional force.
#[derive(Debug, Clone, Copy)]
pub struct WindForce {
    /// Direction and magnitude of wind.
    pub direction: Vector2D,
    /// Strength multiplier.
    pub strength: f32,
}

impl WindForce {
    /// Create a wind force.
    pub fn new(direction: Vector2D, strength: f32) -> Self {
        Self {
            direction,
            strength,
        }
    }

    /// Compute the wind force.
    pub fn compute(&self) -> Vector2D {
        Vector2D::new(
            self.direction.x * self.strength,
            self.direction.y * self.strength,
        )
    }

    /// Apply the wind force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute();
        body.apply_force(force, dt);
    }
}

/// A push force applied at a specific point, radiating outward.
#[derive(Debug, Clone, Copy)]
pub struct PushForce {
    /// Origin point of the push.
    pub origin: Point,
    /// Direction of the push.
    pub direction: Vector2D,
    /// Strength of the push.
    pub strength: f32,
    /// Radius of effect.
    pub radius: f32,
}

impl PushForce {
    /// Create a push force.
    pub fn new(origin: Point, direction: Vector2D, strength: f32, radius: f32) -> Self {
        Self {
            origin,
            direction,
            strength,
            radius,
        }
    }

    /// Compute the push force on a body.
    pub fn compute(&self, body: &PhysicsBody) -> Vector2D {
        let dx = body.position.x - self.origin.x;
        let dy = body.position.y - self.origin.y;
        let dist_sq = dx * dx + dy * dy;

        if dist_sq > self.radius * self.radius {
            return Vector2D::ZERO;
        }

        // Falloff based on distance
        let dist = dist_sq.sqrt();
        let falloff = 1.0 - (dist / self.radius).min(1.0);

        Vector2D::new(
            self.direction.x * self.strength * falloff,
            self.direction.y * self.strength * falloff,
        )
    }

    /// Apply the push force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute(body);
        body.apply_force(force, dt);
    }
}

/// Body-to-body repulsion for collision avoidance.
#[derive(Debug, Clone, Copy)]
pub struct BodyRepulsion {
    /// Repulsion strength.
    pub strength: f32,
    /// Minimum distance to maintain.
    pub min_distance: f32,
}

impl BodyRepulsion {
    /// Create body-to-body repulsion.
    pub fn new(strength: f32, min_distance: f32) -> Self {
        Self {
            strength,
            min_distance,
        }
    }

    /// Compute the repulsion force between two bodies.
    pub fn compute(&self, a: &PhysicsBody, b: &PhysicsBody) -> Vector2D {
        let dx = b.position.x - a.position.x;
        let dy = b.position.y - a.position.y;
        let dist_sq = dx * dx + dy * dy;

        if dist_sq > self.min_distance * self.min_distance || dist_sq < 0.0001 {
            return Vector2D::ZERO;
        }

        let dist = dist_sq.sqrt();
        // Overlap amount
        let overlap = self.min_distance - dist;
        let magnitude = self.strength * overlap;
        let nx = dx / dist;
        let ny = dy / dist;

        Vector2D::new(nx * magnitude, ny * magnitude)
    }

    /// Apply repulsion between two bodies.
    pub fn apply(&self, a: &mut PhysicsBody, b: &mut PhysicsBody, dt: f32) {
        let force = self.compute(a, b);
        a.apply_force(Vector2D::new(-force.x, -force.y), dt);
        b.apply_force(force, dt);
    }
}

/// Force accumulator for collecting forces before applying them.
#[derive(Debug, Clone)]
pub struct ForceAccumulator {
    /// Accumulated force vector.
    pub force: Vector2D,
    /// Accumulated torque.
    pub torque: f32,
}

impl ForceAccumulator {
    /// Create a new empty accumulator.
    pub fn new() -> Self {
        Self {
            force: Vector2D::ZERO,
            torque: 0.0,
        }
    }

    /// Add a force to the accumulator.
    pub fn add_force(&mut self, force: Vector2D) {
        self.force.x += force.x;
        self.force.y += force.y;
    }

    /// Add a torque to the accumulator.
    pub fn add_torque(&mut self, torque: f32) {
        self.torque += torque;
    }

    /// Apply the accumulated forces to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        if self.force.length_squared() > 0.0001 {
            body.apply_force(self.force, dt);
        }
        if self.torque.abs() > 0.0001 {
            body.angular_velocity += self.torque * body.inv_inertia * dt;
        }
    }

    /// Clear the accumulator.
    pub fn clear(&mut self) {
        self.force = Vector2D::ZERO;
        self.torque = 0.0;
    }

    /// Check if there are any accumulated forces.
    pub fn is_empty(&self) -> bool {
        self.force.length_squared() < 0.0001 && self.torque.abs() < 0.0001
    }
}

impl Default for ForceAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Apply spring forces to all bodies with rest positions.
pub fn apply_rest_springs(bodies: &mut [PhysicsBody], stiffness: f32, damping: f32, dt: f32) {
    let spring = SpringForce::new(stiffness, damping, Point::ORIGIN);
    for body in bodies.iter_mut() {
        if let Some(rest) = body.rest_position {
            let s = SpringForce {
                target: rest,
                ..spring
            };
            s.apply(body, dt);
        }
    }
}

/// Apply mutual repulsion between all pairs of bodies.
pub fn apply_mutual_repulsion(
    bodies: &mut [PhysicsBody],
    strength: f32,
    min_distance: f32,
    dt: f32,
) {
    let repulsion = BodyRepulsion::new(strength, min_distance);
    let len = bodies.len();
    // Collect forces first to avoid double mutable borrow
    let mut forces: Vec<Vector2D> = vec![Vector2D::ZERO; len];
    for i in 0..len {
        for j in (i + 1)..len {
            let f = repulsion.compute(&bodies[i], &bodies[j]);
            forces[i].x -= f.x;
            forces[i].y -= f.y;
            forces[j].x += f.x;
            forces[j].y += f.y;
        }
    }
    for (body, f) in bodies.iter_mut().zip(forces) {
        body.apply_force(f, dt);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, BodyType, CollisionShape};

    #[test]
    fn test_spring_force_pulls_toward_target() {
        let spring = SpringForce::default_dock_icon(Point::new(100.0, 100.0));
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute(&body);
        assert!(force.x > 0.0);
        assert!(force.y > 0.0);
    }

    #[test]
    fn test_spring_force_zero_at_target() {
        let spring = SpringForce::default_dock_icon(Point::new(50.0, 50.0));
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = spring.compute(&body);
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_spring_settled() {
        let spring = SpringForce::default_dock_icon(Point::new(50.0, 50.0));
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.1, 50.1),
            CollisionShape::circle(10.0),
            1.0,
        );
        assert!(spring.is_settled(&body, 1.0));
    }

    #[test]
    fn test_drag_force_opposes_motion() {
        let drag = DragForce::new(0.5);
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        let force = drag.compute(&body);
        assert!(force.x < 0.0);
        assert!((force.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_repulsion_force_pushes_away() {
        let repulsion = RepulsionForce::new(Point::ORIGIN, 100.0, 100.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = repulsion.compute(&body);
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_repulsion_zero_outside_range() {
        let repulsion = RepulsionForce::new(Point::ORIGIN, 100.0, 50.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = repulsion.compute(&body);
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_attraction_force_pulls_toward() {
        let attraction = AttractionForce::new(Point::new(100.0, 0.0), 100.0, 200.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = attraction.compute(&body);
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_wind_force() {
        let wind = WindForce::new(Vector2D::new(1.0, 0.0), 10.0);
        let force = wind.compute();
        assert!((force.x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_push_force_falloff() {
        let push = PushForce::new(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            50.0,
        );
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(10.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(40.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let f1 = push.compute(&body1);
        let f2 = push.compute(&body2);
        assert!(f1.length() > f2.length());
    }

    #[test]
    fn test_body_repulsion() {
        let repulsion = BodyRepulsion::new(100.0, 50.0);
        let mut a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(10.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        repulsion.apply(&mut a, &mut b, 1.0);
        // A should move left, B should move right
        assert!(a.velocity.x < 0.0);
        assert!(b.velocity.x > 0.0);
    }

    #[test]
    fn test_force_accumulator() {
        let mut acc = ForceAccumulator::new();
        acc.add_force(Vector2D::new(10.0, 0.0));
        acc.add_force(Vector2D::new(0.0, 20.0));
        assert!(acc.force.x > 0.0);
        assert!(acc.force.y > 0.0);
        assert!(!acc.is_empty());
        acc.clear();
        assert!(acc.is_empty());
    }

    #[test]
    fn test_apply_rest_springs() {
        let mut bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(100.0, 100.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        ];
        bodies[0].set_rest_position(Point::new(50.0, 50.0));
        bodies[1].set_rest_position(Point::new(50.0, 50.0));
        apply_rest_springs(&mut bodies, 500.0, 0.8, 0.016);
        // Body 0 should move toward (50, 50)
        assert!(bodies[0].velocity.x > 0.0);
        assert!(bodies[0].velocity.y > 0.0);
        // Body 1 should move toward (50, 50)
        assert!(bodies[1].velocity.x < 0.0);
        assert!(bodies[1].velocity.y < 0.0);
    }

    #[test]
    fn test_apply_mutual_repulsion() {
        let mut bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(10.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        ];
        apply_mutual_repulsion(&mut bodies, 100.0, 50.0, 1.0);
        assert!(bodies[0].velocity.x < 0.0);
        assert!(bodies[1].velocity.x > 0.0);
    }

    #[test]
    fn test_push_force_outside_radius() {
        let push = PushForce::new(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            50.0,
        );
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = push.compute(&body);
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_spring_damping_opposes_velocity() {
        let spring = SpringForce::default_dock_icon(Point::new(50.0, 50.0));
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        let force = spring.compute(&body);
        // At target position, only damping force remains, opposing velocity
        assert!(force.x < 0.0);
    }
}
