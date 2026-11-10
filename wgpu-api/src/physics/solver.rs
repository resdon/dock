//! Iterative physics solver for constraint resolution.
//!
//! The solver runs multiple iterations per frame to resolve collisions and
//! constraints, producing stable stacking and realistic behavior.

use crate::geometry::{Point, Vector2D};
use super::body::{BodyId, PhysicsBody};
use super::collision::{self, Collision};

/// Configuration for the solver.
#[derive(Debug, Clone, Copy)]
pub struct SolverConfig {
    /// Number of iterations per step.
    pub iterations: u32,
    /// Position correction percentage.
    pub position_correction: f32,
    /// Velocity threshold for warm starting.
    pub velocity_threshold: f32,
    /// Bias for contact stability.
    pub contact_bias: f32,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            iterations: 8,
            position_correction: 0.2,
            velocity_threshold: 0.01,
            contact_bias: 0.0,
        }
    }
}

/// Statistics from a solver step.
#[derive(Debug, Clone, Copy)]
pub struct SolverStats {
    /// Number of collision pairs resolved.
    pub collision_pairs: usize,
    /// Number of iterations performed.
    pub iterations: u32,
    /// Maximum penetration found.
    pub max_penetration: f32,
}

impl SolverStats {
    pub fn new() -> Self {
        Self {
            collision_pairs: 0,
            iterations: 0,
            max_penetration: 0.0,
        }
    }
}

impl Default for SolverStats {
    fn default() -> Self {
        Self::new()
    }
}

/// The iterative physics solver.
pub struct PhysicsSolver {
    /// Solver configuration.
    pub config: SolverConfig,
    /// Statistics from the last solve.
    pub stats: SolverStats,
}

impl PhysicsSolver {
    /// Create a new solver with default configuration.
    pub fn new() -> Self {
        Self {
            config: SolverConfig::default(),
            stats: SolverStats::new(),
        }
    }

    /// Create a new solver with custom configuration.
    pub fn with_config(config: SolverConfig) -> Self {
        Self {
            config,
            stats: SolverStats::new(),
        }
    }

    /// Solve all collisions for a set of bodies.
    pub fn solve(&mut self, bodies: &mut Vec<PhysicsBody>) {
        self.stats = SolverStats::new();
        let iterations = self.config.iterations;
        let position_correction = self.config.position_correction;

        for _ in 0..iterations {
            // Clone bodies for broad phase to avoid borrow conflicts
            let bodies_clone: Vec<PhysicsBody> = bodies.iter().cloned().collect();
            let pairs = collision::broad_phase(&bodies_clone);
            self.stats.collision_pairs = pairs.len();

            // Collect all collisions first
            let mut collisions: Vec<(usize, usize, Collision)> = Vec::new();
            for &(i, j) in &pairs {
                let a = bodies_clone[i].clone();
                let b = bodies_clone[j].clone();
                let collision = match collision::detect_collision(&a, &b) {
                    Some(c) => c,
                    None => continue,
                };

                self.stats.max_penetration = self
                    .stats
                    .max_penetration
                    .max(collision.penetration);

                collisions.push((i, j, collision));
            }

            // Apply resolutions
            for (i, j, collision) in collisions {
                if i == j {
                    continue;
                }
                let (left, right) = if i < j { (i, j) } else { (j, i) };
                let (a, b) = bodies.split_at_mut(right);
                let (a, b) = (
                    a.get_mut(left).unwrap(),
                    b.get_mut(0).unwrap(),
                );
                collision::resolve_collision(
                    &collision,
                    a,
                    b,
                    position_correction,
                );
            }
        }

        self.stats.iterations = iterations;
    }

    /// Solve collisions with a HashMap-based body store.
    pub fn solve_map(
        &mut self,
        bodies: &mut std::collections::HashMap<BodyId, PhysicsBody>,
    ) {
        self.stats = SolverStats::new();
        let iterations = self.config.iterations;
        let position_correction = self.config.position_correction;

        // Convert to vec for broad phase
        let body_ids: Vec<BodyId> = bodies.keys().copied().collect();
        let body_vec: Vec<PhysicsBody> = bodies.values().cloned().collect();

        for _ in 0..iterations {
            let pairs = collision::broad_phase(&body_vec);

            // Collect all collisions first
            let mut collisions: Vec<(BodyId, BodyId, Collision)> = Vec::new();
            for &(i, j) in &pairs {
                let id_a = body_ids[i];
                let id_b = body_ids[j];

                let a_clone = body_vec[i].clone();
                let b_clone = body_vec[j].clone();
                if let Some(c) = collision::detect_collision(&a_clone, &b_clone) {
                    self.stats.max_penetration = self.stats.max_penetration.max(c.penetration);
                    collisions.push((id_a, id_b, c));
                }
            }

            // Apply resolutions — compute impulse and correction, then apply
            for (id_a, id_b, collision) in collisions {
                // Get clones for computation
                let a = match bodies.get(&id_a) {
                    Some(b) => b.clone(),
                    None => continue,
                };
                let b = match bodies.get(&id_b) {
                    Some(b) => b.clone(),
                    None => continue,
                };

                // Compute impulse
                let impulse = collision.compute_impulse(&a, &b);
                let friction = collision.compute_friction_impulse(&a, &b, &impulse);
                let correction = collision.penetration * position_correction;
                let a_inv = a.inv_mass;
                let b_inv = b.inv_mass;
                let total_inv = a_inv + b_inv;

                // Apply to a
                if let Some(mut_a) = bodies.get_mut(&id_a) {
                    mut_a.velocity.x -= impulse.x / a.mass;
                    mut_a.velocity.y -= impulse.y / a.mass;
                    mut_a.velocity.x -= friction.x / a.mass;
                    mut_a.velocity.y -= friction.y / a.mass;
                    if total_inv > 0.0 {
                        mut_a.position.x -= collision.normal.x * correction * (a_inv / total_inv);
                        mut_a.position.y -= collision.normal.y * correction * (a_inv / total_inv);
                    }
                }
                // Apply to b
                if let Some(mut_b) = bodies.get_mut(&id_b) {
                    mut_b.velocity.x += impulse.x / b.mass;
                    mut_b.velocity.y += impulse.y / b.mass;
                    mut_b.velocity.x += friction.x / b.mass;
                    mut_b.velocity.y += friction.y / b.mass;
                    if total_inv > 0.0 {
                        mut_b.position.x += collision.normal.x * correction * (b_inv / total_inv);
                        mut_b.position.y += collision.normal.y * correction * (b_inv / total_inv);
                    }
                }
            }
        }

        self.stats.iterations = iterations;
    }

    /// Apply velocity thresholds to sleeping bodies.
    pub fn apply_sleep_threshold(&self, bodies: &mut Vec<PhysicsBody>) {
        for body in bodies.iter_mut() {
            if body.should_sleep(self.config.velocity_threshold) {
                body.put_to_sleep();
            }
        }
    }

    /// Warm start: apply previous frame's impulses.
    pub fn warm_start(
        &self,
        _bodies: &mut Vec<PhysicsBody>,
        _previous_impulses: &[(BodyId, BodyId, Vector2D)],
    ) {
        // Warm starting caches previous impulses for faster convergence
        // Implementation deferred for simplicity
    }
}

impl Default for PhysicsSolver {
    fn default() -> Self {
        Self::new()
    }
}

/// A contact manifold for persistent contact tracking.
#[derive(Debug, Clone)]
pub struct ContactManifold {
    /// Body A.
    pub body_a: BodyId,
    /// Body B.
    pub body_b: BodyId,
    /// Contact points.
    pub contacts: Vec<ContactPoint>,
}

impl ContactManifold {
    /// Create a new manifold from a collision.
    pub fn from_collision(collision: &Collision) -> Self {
        Self {
            body_a: collision.body_a,
            body_b: collision.body_b,
            contacts: vec![ContactPoint {
                point: collision.contact_point,
                normal: collision.normal,
                penetration: collision.penetration,
            }],
        }
    }

    /// Number of contact points.
    pub fn contact_count(&self) -> usize {
        self.contacts.len()
    }
}

/// A single contact point.
#[derive(Debug, Clone, Copy)]
pub struct ContactPoint {
    /// Position of the contact.
    pub point: Point,
    /// Normal direction.
    pub normal: Vector2D,
    /// Penetration depth.
    pub penetration: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, PhysicsBody, CollisionShape};

    #[test]
    fn test_solver_resolves_overlap() {
        let mut solver = PhysicsSolver::new();
        let mut bodies = vec![
            PhysicsBody::dynamic_from_rect(
                BodyId::new(1),
                &crate::geometry::Rect::new(0.0, 0.0, 50.0, 50.0),
                1.0,
            ),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(2),
                &crate::geometry::Rect::new(25.0, 25.0, 50.0, 50.0),
                1.0,
            ),
        ];
        solver.solve(&mut bodies);
        assert!(solver.stats.collision_pairs >= 1);
        assert!(solver.stats.max_penetration > 0.0);
    }

    #[test]
    fn test_solver_no_collision() {
        let mut solver = PhysicsSolver::new();
        let mut bodies = vec![
            PhysicsBody::dynamic_from_rect(
                BodyId::new(1),
                &crate::geometry::Rect::new(0.0, 0.0, 50.0, 50.0),
                1.0,
            ),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(2),
                &crate::geometry::Rect::new(200.0, 200.0, 50.0, 50.0),
                1.0,
            ),
        ];
        solver.solve(&mut bodies);
        assert_eq!(solver.stats.collision_pairs, 0);
    }

    #[test]
    fn test_solver_iterations() {
        let mut solver = PhysicsSolver::with_config(SolverConfig {
            iterations: 4,
            ..Default::default()
        });
        let mut bodies = vec![
            PhysicsBody::dynamic_from_rect(
                BodyId::new(1),
                &crate::geometry::Rect::new(0.0, 0.0, 50.0, 50.0),
                1.0,
            ),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(2),
                &crate::geometry::Rect::new(25.0, 25.0, 50.0, 50.0),
                1.0,
            ),
        ];
        solver.solve(&mut bodies);
        assert_eq!(solver.stats.iterations, 4);
    }

    #[test]
    fn test_contact_manifold() {
        let collision = Collision::new(
            BodyId::new(1),
            BodyId::new(2),
            Point::new(50.0, 50.0),
            Vector2D::new(1.0, 0.0),
            10.0,
        );
        let manifold = ContactManifold::from_collision(&collision);
        assert_eq!(manifold.contact_count(), 1);
        assert_eq!(manifold.body_a, BodyId::new(1));
        assert_eq!(manifold.body_b, BodyId::new(2));
    }

    #[test]
    fn test_solver_with_circles() {
        let mut solver = PhysicsSolver::new();
        let mut bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(25.0),
                1.0,
            ),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(30.0, 0.0),
                CollisionShape::circle(25.0),
                1.0,
            ),
        ];
        solver.solve(&mut bodies);
        assert!(solver.stats.collision_pairs >= 1);
    }

    #[test]
    fn test_sleep_threshold() {
        let solver = PhysicsSolver::new();
        let mut bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::ORIGIN,
                CollisionShape::circle(10.0),
                1.0,
            ),
        ];
        // Zero velocity should trigger sleep
        solver.apply_sleep_threshold(&mut bodies);
        assert!(bodies[0].is_sleeping);
    }

    #[test]
    fn test_solver_map() {
        let mut solver = PhysicsSolver::new();
        let mut bodies = std::collections::HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(1),
                &crate::geometry::Rect::new(0.0, 0.0, 50.0, 50.0),
                1.0,
            ),
        );
        bodies.insert(
            BodyId::new(2),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(2),
                &crate::geometry::Rect::new(25.0, 25.0, 50.0, 50.0),
                1.0,
            ),
        );
        solver.solve_map(&mut bodies);
        assert!(solver.stats.collision_pairs >= 1);
    }

    #[test]
    fn test_solver_config_defaults() {
        let config = SolverConfig::default();
        assert_eq!(config.iterations, 8);
        assert!((config.position_correction - 0.2).abs() < f32::EPSILON);
    }
}
