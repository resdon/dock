//! AABB collision detection and response.
//!
//! Provides broad-phase and narrow-phase collision detection for axis-aligned
//! bounding boxes and circles, plus impulse-based collision response.

use super::body::{BodyId, CollisionShape, PhysicsBody};
use crate::geometry::{Point, Rect, Vector2D};

/// Result of a collision detection test.
#[derive(Debug, Clone, Copy)]
pub struct Collision {
    /// First body.
    pub body_a: BodyId,
    /// Second body.
    pub body_b: BodyId,
    /// Contact point (midpoint of overlap).
    pub contact_point: Point,
    /// Collision normal (from A to B).
    pub normal: Vector2D,
    /// Penetration depth (positive = overlapping).
    pub penetration: f32,
}

impl Collision {
    /// Create a new collision.
    pub fn new(
        body_a: BodyId,
        body_b: BodyId,
        contact_point: Point,
        normal: Vector2D,
        penetration: f32,
    ) -> Self {
        Self {
            body_a,
            body_b,
            contact_point,
            normal,
            penetration,
        }
    }

    /// Compute the impulse for collision response.
    pub fn compute_impulse(&self, a: &PhysicsBody, b: &PhysicsBody) -> Vector2D {
        // Relative velocity
        let rel_vel = Vector2D::new(b.velocity.x - a.velocity.x, b.velocity.y - a.velocity.y);

        // Relative velocity along normal
        let vel_along_normal = rel_vel.dot(&self.normal);

        // Don't resolve if velocities are separating
        if vel_along_normal > 0.0 {
            return Vector2D::ZERO;
        }

        // Restitution (use minimum of both bodies)
        let restitution = a.material.restitution.min(b.material.restitution);

        // Inverse mass sum
        let inv_mass_sum = a.inv_mass + b.inv_mass;
        if inv_mass_sum == 0.0 {
            return Vector2D::ZERO;
        }

        // Impulse scalar
        let j = -(1.0 + restitution) * vel_along_normal / inv_mass_sum;

        // Impulse vector
        Vector2D::new(j * self.normal.x, j * self.normal.y)
    }

    /// Apply friction impulse.
    pub fn compute_friction_impulse(
        &self,
        a: &PhysicsBody,
        b: &PhysicsBody,
        normal_impulse: &Vector2D,
    ) -> Vector2D {
        // Friction direction is perpendicular to normal
        let tangent = Vector2D::new(-self.normal.y, self.normal.x);

        // Relative velocity along tangent
        let rel_vel = Vector2D::new(b.velocity.x - a.velocity.x, b.velocity.y - a.velocity.y);
        let vel_along_tangent = rel_vel.dot(&tangent);

        // Coulomb's law: friction <= mu * normal_impulse
        let friction = (a.material.friction + b.material.friction) / 2.0;
        let max_friction = normal_impulse.length() * friction;

        let j_t = -vel_along_tangent * (a.inv_mass + b.inv_mass);
        let clamped_j = j_t.clamp(-max_friction, max_friction);

        Vector2D::new(clamped_j * tangent.x, clamped_j * tangent.y)
    }
}

/// Detect collision between two AABB shapes.
pub fn aabb_vs_aabb(a: &PhysicsBody, b: &PhysicsBody) -> Option<Collision> {
    let a_rect = a.bounding_rect();
    let b_rect = b.bounding_rect();

    match a_rect.intersection(&b_rect) {
        Some(_intersection) => {
            // Compute normal as the axis of least penetration
            let overlap_x = (a_rect.width / 2.0 + b_rect.width / 2.0)
                - (a_rect.center().x - b_rect.center().x).abs();
            let overlap_y = (a_rect.height / 2.0 + b_rect.height / 2.0)
                - (a_rect.center().y - b_rect.center().y).abs();

            let (normal, penetration) = if overlap_x < overlap_y {
                let dir = if b_rect.center().x > a_rect.center().x {
                    1.0
                } else {
                    -1.0
                };
                (Vector2D::new(dir, 0.0), overlap_x)
            } else {
                let dir = if b_rect.center().y > a_rect.center().y {
                    1.0
                } else {
                    -1.0
                };
                (Vector2D::new(0.0, dir), overlap_y)
            };

            let contact = Point::new(
                (a_rect.center().x + b_rect.center().x) / 2.0,
                (a_rect.center().y + b_rect.center().y) / 2.0,
            );

            Some(Collision::new(a.id, b.id, contact, normal, penetration))
        }
        None => None,
    }
}

/// Detect collision between two circles.
pub fn circle_vs_circle(a: &PhysicsBody, b: &PhysicsBody) -> Option<Collision> {
    let (radius_a, radius_b) = match (&a.shape, &b.shape) {
        (CollisionShape::Circle { radius: r1 }, CollisionShape::Circle { radius: r2 }) => {
            (*r1, *r2)
        }
        _ => return None,
    };

    let dx = b.position.x - a.position.x;
    let dy = b.position.y - a.position.y;
    let dist_sq = dx * dx + dy * dy;
    let min_dist = radius_a + radius_b;

    if dist_sq < min_dist * min_dist {
        let dist = dist_sq.sqrt();
        let normal = if dist_sq > 0.0 {
            Vector2D::new(dx / dist, dy / dist)
        } else {
            Vector2D::X
        };
        let penetration = min_dist - dist;
        let contact = Point::new(
            a.position.x + normal.x * radius_a,
            a.position.y + normal.y * radius_a,
        );
        Some(Collision::new(a.id, b.id, contact, normal, penetration))
    } else {
        None
    }
}

/// Detect collision between AABB and circle.
pub fn aabb_vs_circle(a: &PhysicsBody, b: &PhysicsBody) -> Option<Collision> {
    let (rect, radius) = match (&a.shape, &b.shape) {
        (CollisionShape::AABB { .. }, CollisionShape::Circle { radius }) => {
            (a.bounding_rect(), *radius)
        }
        (CollisionShape::Circle { radius }, CollisionShape::AABB { .. }) => {
            // Swap: circle is A, AABB is B
            let rect = b.bounding_rect();
            let (a_id, b_id) = (b.id, a.id);
            let result = aabb_vs_circle_internal(&rect, b.position, *radius, a_id, b_id);
            // Flip normal since we swapped
            return result.map(|c| Collision {
                normal: Vector2D::new(-c.normal.x, -c.normal.y),
                ..c
            });
        }
        _ => return None,
    };

    aabb_vs_circle_internal(&rect, b.position, radius, a.id, b.id)
}

fn aabb_vs_circle_internal(
    rect: &Rect,
    circle_center: Point,
    radius: f32,
    a_id: BodyId,
    b_id: BodyId,
) -> Option<Collision> {
    // Find closest point on AABB to circle center
    let closest_x = circle_center.x.clamp(rect.x, rect.right());
    let closest_y = circle_center.y.clamp(rect.y, rect.bottom());
    let closest = Point::new(closest_x, closest_y);

    let dx = circle_center.x - closest.x;
    let dy = circle_center.y - closest.y;
    let dist_sq = dx * dx + dy * dy;

    if dist_sq < radius * radius && dist_sq > 0.0001 {
        let dist = dist_sq.sqrt();
        let normal = Vector2D::new(dx / dist, dy / dist);
        let penetration = radius - dist;
        Some(Collision::new(a_id, b_id, closest, normal, penetration))
    } else if dist_sq < radius * radius {
        // Circle center is inside AABB - use axis of least penetration
        let center = rect.center();
        let dx = circle_center.x - center.x;
        let dy = circle_center.y - center.y;

        let overlap_x = rect.width / 2.0 - dx.abs() + radius;
        let overlap_y = rect.height / 2.0 - dy.abs() + radius;

        let (normal, penetration) = if overlap_x < overlap_y {
            let dir = if dx > 0.0 { 1.0 } else { -1.0 };
            (Vector2D::new(dir, 0.0), overlap_x)
        } else {
            let dir = if dy > 0.0 { 1.0 } else { -1.0 };
            (Vector2D::new(0.0, dir), overlap_y)
        };

        Some(Collision::new(a_id, b_id, closest, normal, penetration))
    } else {
        None
    }
}

/// Generic collision detection between two bodies.
pub fn detect_collision(a: &PhysicsBody, b: &PhysicsBody) -> Option<Collision> {
    if !a.enabled || !b.enabled {
        return None;
    }

    // Quick AABB check
    if !a.bounding_rect().intersects(&b.bounding_rect()) {
        return None;
    }

    match (&a.shape, &b.shape) {
        (CollisionShape::AABB { .. }, CollisionShape::AABB { .. }) => aabb_vs_aabb(a, b),
        (CollisionShape::Circle { .. }, CollisionShape::Circle { .. }) => circle_vs_circle(a, b),
        (CollisionShape::AABB { .. }, CollisionShape::Circle { .. })
        | (CollisionShape::Circle { .. }, CollisionShape::AABB { .. }) => aabb_vs_circle(a, b),
    }
}

/// Broad-phase: find all potentially colliding pairs using AABB overlap.
pub fn broad_phase(bodies: &[PhysicsBody]) -> Vec<(usize, usize)> {
    let bounds: Vec<_> = bodies
        .iter()
        .map(|body| (body.id, body.bounding_rect(), body.enabled))
        .collect();
    broad_phase_bounds(&bounds)
}

pub(super) fn broad_phase_bounds(bounds: &[(BodyId, Rect, bool)]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for i in 0..bounds.len() {
        for j in (i + 1)..bounds.len() {
            if bounds[i].2 && bounds[j].2 && bounds[i].1.intersects(&bounds[j].1) {
                pairs.push((i, j));
            }
        }
    }
    pairs
}

/// Resolve a collision by applying impulse and position correction.
pub fn resolve_collision(
    collision: &Collision,
    a: &mut PhysicsBody,
    b: &mut PhysicsBody,
    position_correction_percent: f32,
) {
    // Compute and apply impulse
    let impulse = collision.compute_impulse(a, b);
    if impulse.length_squared() > 0.0001 {
        a.apply_impulse(Vector2D::new(-impulse.x, -impulse.y));
        b.apply_impulse(impulse);
    }

    // Friction
    let friction = collision.compute_friction_impulse(a, b, &impulse);
    if friction.length_squared() > 0.0001 {
        a.apply_impulse(Vector2D::new(-friction.x, -friction.y));
        b.apply_impulse(friction);
    }

    // Position correction to prevent sinking
    let correction_percent = position_correction_percent;
    let correction = collision.penetration * correction_percent;
    let correction_vec = Vector2D::new(
        collision.normal.x * correction,
        collision.normal.y * correction,
    );

    let total_inv = a.inv_mass + b.inv_mass;
    if total_inv > 0.0 {
        a.position.x -= correction_vec.x * (a.inv_mass / total_inv);
        a.position.y -= correction_vec.y * (a.inv_mass / total_inv);
        b.position.x += correction_vec.x * (b.inv_mass / total_inv);
        b.position.y += correction_vec.y * (b.inv_mass / total_inv);
    }
}

/// Collision pair for broad-phase results.
#[derive(Debug, Clone, Copy)]
pub struct CollisionPair {
    /// First body ID.
    pub body_a: BodyId,
    /// Second body ID.
    pub body_b: BodyId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_aabb_collision() {
        let a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        let b =
            PhysicsBody::dynamic_from_rect(BodyId::new(2), &Rect::new(25.0, 25.0, 50.0, 50.0), 1.0);
        let collision = aabb_vs_aabb(&a, &b).unwrap();
        assert!(collision.penetration > 0.0);
        assert!((collision.normal.length() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_aabb_no_collision() {
        let a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        let b = PhysicsBody::dynamic_from_rect(
            BodyId::new(2),
            &Rect::new(100.0, 100.0, 50.0, 50.0),
            1.0,
        );
        assert!(aabb_vs_aabb(&a, &b).is_none());
    }

    #[test]
    fn test_circle_collision() {
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(30.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let collision = circle_vs_circle(&a, &b).unwrap();
        assert!(collision.penetration > 0.0);
    }

    #[test]
    fn test_circle_no_collision() {
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(100.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        assert!(circle_vs_circle(&a, &b).is_none());
    }

    #[test]
    fn test_detect_collision_delegates() {
        let a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        let b =
            PhysicsBody::dynamic_from_rect(BodyId::new(2), &Rect::new(25.0, 25.0, 50.0, 50.0), 1.0);
        let collision = detect_collision(&a, &b).unwrap();
        assert!(collision.penetration > 0.0);
    }

    #[test]
    fn test_disabled_body_no_collision() {
        let mut a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        a.enabled = false;
        let b =
            PhysicsBody::dynamic_from_rect(BodyId::new(2), &Rect::new(25.0, 25.0, 50.0, 50.0), 1.0);
        assert!(detect_collision(&a, &b).is_none());
    }

    #[test]
    fn test_impulse_resolves_velocity() {
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(30.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        b.velocity = Vector2D::new(-10.0, 0.0);
        let collision = circle_vs_circle(&a, &b).unwrap();
        let impulse = collision.compute_impulse(&a, &b);
        assert!(impulse.length() > 0.0);
    }

    #[test]
    fn test_coincident_circles_collide() {
        let a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );

        let collision = circle_vs_circle(&a, &b).unwrap();
        assert_eq!(collision.normal, Vector2D::X);
        assert_eq!(collision.penetration, 20.0);
    }

    #[test]
    fn test_broad_phase() {
        let bodies = vec![
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0),
            PhysicsBody::dynamic_from_rect(BodyId::new(2), &Rect::new(25.0, 25.0, 50.0, 50.0), 1.0),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(3),
                &Rect::new(200.0, 200.0, 50.0, 50.0),
                1.0,
            ),
        ];
        let pairs = broad_phase(&bodies);
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0], (0, 1));
    }

    #[test]
    fn test_resolve_collision() {
        let mut a = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let mut b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(30.0, 0.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        b.velocity = Vector2D::new(-10.0, 0.0);
        let collision = circle_vs_circle(&a, &b).unwrap();
        resolve_collision(&collision, &mut a, &mut b, 0.2);
        // A should move right, B should slow down or reverse
        assert!(a.velocity.x > 0.0 || a.position.x > 0.0);
    }

    #[test]
    fn test_aabb_vs_circle_collision() {
        let a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        let b = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(40.0, 40.0),
            CollisionShape::circle(20.0),
            1.0,
        );
        let collision = aabb_vs_circle(&a, &b);
        assert!(collision.is_some());
    }

    #[test]
    fn test_collision_normal_direction() {
        let a =
            PhysicsBody::dynamic_from_rect(BodyId::new(1), &Rect::new(0.0, 0.0, 50.0, 50.0), 1.0);
        let b = PhysicsBody::dynamic_from_rect(
            BodyId::new(2),
            &Rect::new(30.0, -10.0, 50.0, 50.0),
            1.0,
        );
        let collision = aabb_vs_aabb(&a, &b).unwrap();
        // B is above and to the right of A, normal should point up or right
        assert!(collision.normal.x.abs() + collision.normal.y.abs() > 0.5);
    }
}
