//! Physics world — the central simulation environment.
//!
//! `PhysicsWorld` orchestrates three distinct physics subsystems:
//!
//! ## A. Spring-based UI Physics
//!
//! Damped springs attract bodies to rest positions.  Used for dock icons
//! returning to preferred positions, smooth dragging, magnetic snapping,
//! elastic resizing, and natural movement after releasing an object.
//!
//! Formula: `F = -k(x - x_target) - c·v`
//!
//! ## B. Rigid-body Physics
//!
//! Collision detection and impulse-based resolution for objects that
//! physically collide, bounce, slide, or stack.  Uses the [`solver`]
//! and [`collision`] modules.
//!
//! ## C. Field-based Interaction Physics
//!
//! Force fields (magnetic, cohesion, separation, radial, directional)
//! provide expressive group-level behavior.  See the [`fields`] module.
//!
//! ## Simulation Loop
//!
//! Each frame:
//! 1. **System A** — Apply spring forces (rest position attraction)
//! 2. **System C** — Apply field forces (magnetic, cohesion, separation)
//! 3. **System B** — Detect and resolve collisions
//! 4. Apply global forces (gravity)
//! 5. Integrate positions
//! 6. Sleep idle bodies

use std::collections::HashMap;

use crate::geometry::{Point, Rect, Vector2D};
use super::body::{BodyId, PhysicsBody};
use super::forces::SpringForce;
use super::solver::PhysicsSolver;
use super::fields::ForceField;

/// Configuration for the physics world.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsWorldConfig {
    /// Gravity vector (pixels/s²). Default: (0, 0) for UI.
    pub gravity: Vector2D,
    /// Fixed timestep for simulation (seconds). Default: 1/60.
    pub timestep: f32,
    /// Maximum substeps for stability. Default: 4.
    pub max_substeps: u32,
    /// Number of solver iterations for constraints. Default: 8.
    pub solver_iterations: u32,
    /// Velocity threshold for sleep detection. Default: 0.01.
    pub sleep_threshold: f32,
    /// Whether to enable sleeping. Default: true.
    pub enable_sleep: bool,
    /// Maximum velocity clamp (pixels/s). Default: 10000.
    pub max_velocity: f32,
    /// Position correction percentage (0.0–1.0). Default: 0.2.
    pub position_correction: f32,
    /// Minimum velocity to consider a body awake. Default: 0.001.
    pub min_wake_velocity: f32,
}

impl Default for PhysicsWorldConfig {
    fn default() -> Self {
        Self {
            gravity: Vector2D::ZERO,
            timestep: 1.0 / 60.0,
            max_substeps: 4,
            solver_iterations: 8,
            sleep_threshold: 0.01,
            enable_sleep: true,
            max_velocity: 10000.0,
            position_correction: 0.2,
            min_wake_velocity: 0.001,
        }
    }
}

/// Statistics about the physics simulation.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsStats {
    /// Total number of bodies.
    pub body_count: usize,
    /// Number of dynamic bodies.
    pub dynamic_count: usize,
    /// Number of sleeping bodies.
    pub sleeping_count: usize,
    /// Number of collision pairs detected.
    pub collision_pairs: usize,
    /// Number of solver iterations performed.
    pub solver_iterations: u32,
    /// Time taken for the last simulation step (seconds).
    pub step_time: f32,
}

impl PhysicsStats {
    /// Create empty stats.
    pub fn new() -> Self {
        Self {
            body_count: 0,
            dynamic_count: 0,
            sleeping_count: 0,
            collision_pairs: 0,
            solver_iterations: 0,
            step_time: 0.0,
        }
    }
}

impl Default for PhysicsStats {
    fn default() -> Self {
        Self::new()
    }
}

/// The central physics simulation environment.
///
/// Orchestrates three physics subsystems:
/// - **A**: Spring-based UI physics (damped springs to rest positions)
/// - **B**: Rigid-body physics (collision detection and resolution)
/// - **C**: Field-based interaction physics (magnetic, cohesion, separation)
pub struct PhysicsWorld {
    /// Configuration.
    pub config: PhysicsWorldConfig,
    /// All bodies indexed by ID.
    bodies: HashMap<BodyId, PhysicsBody>,
    /// Next body ID to assign.
    next_id: u64,
    /// Statistics from the last step.
    pub stats: PhysicsStats,
    /// Spring system (A): stiffness for rest-position attraction.
    pub spring_stiffness: f32,
    /// Spring system (A): damping for rest-position attraction.
    pub spring_damping: f32,
    /// Whether spring forces are enabled (System A).
    pub springs_enabled: bool,
    /// Whether rigid-body collision resolution is enabled (System B).
    pub collisions_enabled: bool,
    /// Field system (C): optional force field applied each step.
    pub force_field: Option<ForceField>,
    /// Physics solver for rigid-body collision resolution (System B).
    solver: PhysicsSolver,
}

impl PhysicsWorld {
    /// Create a new physics world with default configuration.
    pub fn new() -> Self {
        Self {
            config: PhysicsWorldConfig::default(),
            bodies: HashMap::new(),
            next_id: 1,
            stats: PhysicsStats::new(),
            spring_stiffness: 500.0,
            spring_damping: 0.8,
            springs_enabled: true,
            collisions_enabled: true,
            force_field: None,
            solver: PhysicsSolver::new(),
        }
    }

    /// Create a new physics world with custom configuration.
    pub fn with_config(config: PhysicsWorldConfig) -> Self {
        Self {
            config,
            bodies: HashMap::new(),
            next_id: 1,
            stats: PhysicsStats::new(),
            spring_stiffness: 500.0,
            spring_damping: 0.8,
            springs_enabled: true,
            collisions_enabled: true,
            force_field: None,
            solver: PhysicsSolver::new(),
        }
    }

    /// Generate a new unique body ID.
    pub fn generate_body_id(&mut self) -> BodyId {
        let id = BodyId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Add a body to the world.
    pub fn add_body(&mut self, body: PhysicsBody) -> BodyId {
        let id = body.id;
        self.bodies.insert(id, body);
        id
    }

    /// Add a body with an auto-generated ID.
    pub fn add_body_with_id(&mut self, mut body: PhysicsBody) -> BodyId {
        let id = self.generate_body_id();
        body.id = id;
        self.bodies.insert(id, body);
        id
    }

    /// Remove a body from the world.
    pub fn remove_body(&mut self, id: BodyId) -> Option<PhysicsBody> {
        self.bodies.remove(&id)
    }

    /// Get a reference to a body.
    pub fn get_body(&self, id: BodyId) -> Option<&PhysicsBody> {
        self.bodies.get(&id)
    }

    /// Get a mutable reference to a body.
    pub fn get_body_mut(&mut self, id: BodyId) -> Option<&mut PhysicsBody> {
        self.bodies.get_mut(&id)
    }

    /// Get the total number of bodies.
    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// Check if a body exists.
    pub fn has_body(&self, id: BodyId) -> bool {
        self.bodies.contains_key(&id)
    }

    /// Iterate over all bodies.
    pub fn iter_bodies(&self) -> impl Iterator<Item = (&BodyId, &PhysicsBody)> {
        self.bodies.iter()
    }

    /// Iterate over all bodies mutably.
    pub fn iter_bodies_mut(&mut self) -> impl Iterator<Item = (&BodyId, &mut PhysicsBody)> {
        self.bodies.iter_mut()
    }

    /// Find bodies overlapping a rectangle.
    pub fn query_overlap(&self, rect: &Rect) -> Vec<BodyId> {
        self.bodies
            .iter()
            .filter_map(|(id, body)| {
                if body.enabled && !body.is_sleeping {
                    let body_rect = body.bounding_rect();
                    if body_rect.intersects(rect) {
                        Some(*id)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect()
    }

    /// Find the body at a point (returns the first matching body).
    pub fn query_point(&self, point: &Point) -> Option<BodyId> {
        // Search in reverse insertion order — later-added bodies have higher IDs
        self.bodies
            .iter()
            .filter(|(_, body)| body.enabled && body.contains_point(point))
            .max_by_key(|(id, _)| *id)
            .map(|(id, _)| *id)
    }

    /// Find all bodies containing a point.
    pub fn query_point_all(&self, point: &Point) -> Vec<BodyId> {
        self.bodies
            .iter()
            .filter_map(|(id, body)| {
                if body.enabled && body.contains_point(point) {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Set the gravity vector.
    pub fn set_gravity(&mut self, gravity: Vector2D) {
        self.config.gravity = gravity;
    }

    /// Set the simulation timestep.
    pub fn set_timestep(&mut self, timestep: f32) {
        self.config.timestep = timestep.max(0.001);
    }

    /// Apply gravity to all dynamic bodies.
    pub fn apply_gravity(&mut self, dt: f32) {
        let gx = self.config.gravity.x;
        let gy = self.config.gravity.y;
        if gx == 0.0 && gy == 0.0 {
            return;
        }
        for (_, body) in self.iter_bodies_mut() {
            if body.body_type == crate::physics::body::BodyType::Dynamic
                && body.enabled
                && !body.is_sleeping
            {
                body.velocity.x += gx * dt;
                body.velocity.y += gy * dt;
            }
        }
    }

    /// Clamp all body velocities to the maximum.
    pub fn clamp_velocities(&mut self) {
        let max = self.config.max_velocity;
        let max_sq = max * max;
        for (_, body) in self.iter_bodies_mut() {
            let vel_sq = body.velocity.length_squared();
            if vel_sq > max_sq {
                let scale = max / vel_sq.sqrt();
                body.velocity.x *= scale;
                body.velocity.y *= scale;
            }
        }
    }

    /// Integrate all bodies for one timestep.
    pub fn integrate_all(&mut self, dt: f32) {
        for (_, body) in self.iter_bodies_mut() {
            body.integrate(dt);
        }
        // Reset accumulated acceleration for the next step
        for (_, body) in self.iter_bodies_mut() {
            body.reset_acceleration();
        }
    }

    /// Put all idle bodies to sleep.
    pub fn sleep_idle_bodies(&mut self) {
        let enable_sleep = self.config.enable_sleep;
        if !enable_sleep {
            return;
        }
        let threshold = self.config.sleep_threshold;
        for (_, body) in self.iter_bodies_mut() {
            if body.should_sleep(threshold) {
                body.put_to_sleep();
            }
        }
    }

    /// Wake up all bodies.
    pub fn wake_all(&mut self) {
        for (_, body) in self.iter_bodies_mut() {
            body.wake_up();
        }
    }

    /// Wake up bodies overlapping a region.
    pub fn wake_region(&mut self, rect: &Rect) {
        for (_, body) in self.iter_bodies_mut() {
            let body_rect = body.bounding_rect();
            if body_rect.intersects(rect) {
                body.wake_up();
            }
        }
    }

    // ---------------------------------------------------------------------
    // System A: Spring-based UI Physics
    // ---------------------------------------------------------------------

    /// Apply spring forces to all bodies with rest positions (System A).
    ///
    /// Each body with a rest_position is attracted toward it using a damped
    /// spring: `F = -k(x - x_target) - c·v`
    pub fn apply_springs(&mut self, dt: f32) {
        let enabled = self.springs_enabled;
        if !enabled {
            return;
        }
        let stiffness = self.spring_stiffness;
        let damping = self.spring_damping;
        for (_, body) in self.iter_bodies_mut() {
            if let Some(rest) = body.rest_position {
                let spring = SpringForce::new(stiffness, damping, rest);
                spring.apply(body, dt);
            }
        }
    }

    /// Set spring parameters for System A.
    pub fn set_spring_params(&mut self, stiffness: f32, damping: f32) {
        self.spring_stiffness = stiffness;
        self.spring_damping = damping;
    }

    /// Enable or disable spring forces (System A).
    pub fn set_springs_enabled(&mut self, enabled: bool) {
        self.springs_enabled = enabled;
    }

    // ---------------------------------------------------------------------
    // System B: Rigid-body Physics
    // ---------------------------------------------------------------------

    /// Resolve collisions between all bodies (System B).
    pub fn resolve_collisions(&mut self) {
        if !self.collisions_enabled {
            return;
        }
        let body_vec: Vec<PhysicsBody> = self.bodies.values().cloned().collect();
        let body_ids: Vec<BodyId> = self.bodies.keys().copied().collect();

        let pairs = super::collision::broad_phase(&body_vec);
        self.stats.collision_pairs = pairs.len();

        for &(i, j) in &pairs {
            let id_a = body_ids[i];
            let id_b = body_ids[j];

            let a_clone = self.bodies.get(&id_a).cloned();
            let b_clone = self.bodies.get(&id_b).cloned();
            match (a_clone, b_clone) {
                (Some(a), Some(b)) => {
                    if let Some(collision) = super::collision::detect_collision(&a, &b) {
                        // Apply impulse results directly
                        let impulse = collision.compute_impulse(&a, &b);
                        if impulse.length_squared() > 0.0001 {
                            if let Some(mut_a) = self.bodies.get_mut(&id_a) {
                                mut_a.apply_impulse(crate::geometry::Vector2D::new(-impulse.x, -impulse.y));
                            }
                            if let Some(mut_b) = self.bodies.get_mut(&id_b) {
                                mut_b.apply_impulse(impulse);
                            }
                        }
                        // Position correction
                        let friction = collision.compute_friction_impulse(&a, &b, &impulse);
                        if friction.length_squared() > 0.0001 {
                            if let Some(mut_a) = self.bodies.get_mut(&id_a) {
                                mut_a.apply_impulse(crate::geometry::Vector2D::new(-friction.x, -friction.y));
                            }
                            if let Some(mut_b) = self.bodies.get_mut(&id_b) {
                                mut_b.apply_impulse(friction);
                            }
                        }
                        let correction_percent = self.config.position_correction;
                        let correction = collision.penetration * correction_percent;
                        let correction_vec = crate::geometry::Vector2D::new(
                            collision.normal.x * correction,
                            collision.normal.y * correction,
                        );
                        let a_inv = a.inv_mass;
                        let b_inv = b.inv_mass;
                        let total_inv = a_inv + b_inv;
                        if total_inv > 0.0 {
                            if let Some(mut_a) = self.bodies.get_mut(&id_a) {
                                mut_a.position.x -= correction_vec.x * (a_inv / total_inv);
                                mut_a.position.y -= correction_vec.y * (a_inv / total_inv);
                            }
                            if let Some(mut_b) = self.bodies.get_mut(&id_b) {
                                mut_b.position.x += correction_vec.x * (b_inv / total_inv);
                                mut_b.position.y += correction_vec.y * (b_inv / total_inv);
                            }
                        }
                    }
                }
                _ => continue,
            }
        }
    }

    /// Enable or disable collision resolution (System B).
    pub fn set_collisions_enabled(&mut self, enabled: bool) {
        self.collisions_enabled = enabled;
    }

    /// Get a reference to the collision solver.
    pub fn solver(&self) -> &PhysicsSolver {
        &self.solver
    }

    /// Get a mutable reference to the collision solver.
    pub fn solver_mut(&mut self) -> &mut PhysicsSolver {
        &mut self.solver
    }

    // ---------------------------------------------------------------------
    // System C: Field-based Interaction Physics
    // ---------------------------------------------------------------------

    /// Apply field forces to all bodies (System C).
    pub fn apply_fields(&mut self, dt: f32) {
        let has_field = self.force_field.is_some();
        if !has_field {
            return;
        }
        // Collect forces first
        let mut forces: Vec<(BodyId, crate::geometry::Vector2D)> = Vec::new();
        if let Some(ref field) = self.force_field {
            for (id, body) in self.iter_bodies() {
                let f = field.compute_force(body);
                forces.push((*id, f));
            }
        }
        for (id, f) in forces {
            if let Some(body) = self.bodies.get_mut(&id) {
                body.apply_force(f, dt);
            }
        }
    }

    /// Set the force field for System C.
    pub fn set_force_field(&mut self, field: ForceField) {
        self.force_field = Some(field);
    }

    /// Remove the force field.
    pub fn clear_force_field(&mut self) {
        self.force_field = None;
    }

    /// Get a reference to the current force field.
    pub fn force_field(&self) -> Option<&ForceField> {
        self.force_field.as_ref()
    }

    /// Get a mutable reference to the current force field.
    pub fn force_field_mut(&mut self) -> Option<&mut ForceField> {
        self.force_field.as_mut()
    }

    // ---------------------------------------------------------------------
    // Unified simulation step
    // ---------------------------------------------------------------------

    /// Run one complete simulation step orchestrating all three systems.
    ///
    /// Order:
    /// 1. **System A** — Spring forces (rest position attraction)
    /// 2. **System C** — Field forces (magnetic, cohesion, separation)
    /// 3. **Global** — Gravity
    /// 4. **System B** — Collision detection and resolution
    /// 5. **Global** — Clamp velocities, integrate, sleep
    pub fn step(&mut self) {
        let start = std::time::Instant::now();
        let dt = self.config.timestep;

        // 1. System A: Spring forces
        self.apply_springs(dt);

        // 2. System C: Field forces
        self.apply_fields(dt);

        // 3. Global: Gravity
        self.apply_gravity(dt);

        // 4. System B: Collision resolution
        self.resolve_collisions();

        // 5. Clamp velocities
        self.clamp_velocities();

        // 6. Integrate positions
        self.integrate_all(dt);

        // 7. Sleep idle bodies
        self.sleep_idle_bodies();

        // 8. Update stats
        let elapsed = start.elapsed();
        self.stats.step_time = elapsed.as_secs_f32();
        self.stats.body_count = self.bodies.len();
        self.stats.dynamic_count = self.bodies.values().filter(|b| {
            b.body_type == crate::physics::body::BodyType::Dynamic
        }).count();
        self.stats.sleeping_count = self.bodies.values().filter(|b| b.is_sleeping).count();
    }

    /// Run multiple simulation substeps for stability.
    ///
    /// Each substep runs all three physics systems for finer granularity.
    pub fn step_substeps(&mut self, substeps: u32) {
        let sub_dt = self.config.timestep / substeps as f32;
        for _ in 0..substeps {
            // System A: Springs
            self.apply_springs(sub_dt);
            // System C: Fields
            self.apply_fields(sub_dt);
            // Global: Gravity
            self.apply_gravity(sub_dt);
            // System B: Collisions
            self.resolve_collisions();
            // Global: Clamp and integrate
            self.clamp_velocities();
            self.integrate_all(sub_dt);
        }
        self.sleep_idle_bodies();
    }

    /// Update statistics.
    pub fn update_stats(&mut self) {
        self.stats.body_count = self.bodies.len();
        self.stats.dynamic_count = self.bodies.values().filter(|b| {
            b.body_type == crate::physics::body::BodyType::Dynamic
        }).count();
        self.stats.sleeping_count = self.bodies.values().filter(|b| b.is_sleeping).count();
    }

    /// Clear all bodies from the world.
    pub fn clear(&mut self) {
        self.bodies.clear();
        self.stats = PhysicsStats::new();
    }

    /// Get the world bounds (bounding rect of all bodies).
    pub fn get_bounds(&self) -> Option<Rect> {
        let mut first = true;
        let mut bounds = Rect::ZERO;

        for (_, body) in self.iter_bodies() {
            let b = body.bounding_rect();
            if first {
                bounds = b;
                first = false;
            } else {
                bounds = bounds.union(&b);
            }
        }

        if first {
            None
        } else {
            Some(bounds)
        }
    }
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Vector2D};
    use crate::physics::body::{BodyId, PhysicsBody, CollisionShape, BodyType};

    #[test]
    fn test_create_world() {
        let world = PhysicsWorld::new();
        assert_eq!(world.body_count(), 0);
        assert_eq!(world.config.gravity, Vector2D::ZERO);
    }

    #[test]
    fn test_add_and_get_body() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);
        assert_eq!(world.body_count(), 1);
        assert!(world.has_body(BodyId::new(1)));
        let retrieved = world.get_body(BodyId::new(1)).unwrap();
        assert!((retrieved.position.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_auto_generate_id() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(0),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        let id = world.add_body_with_id(body);
        assert_eq!(id, BodyId::new(1));
        assert!(world.has_body(id));
    }

    #[test]
    fn test_remove_body() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);
        let removed = world.remove_body(BodyId::new(1));
        assert!(removed.is_some());
        assert_eq!(world.body_count(), 0);
    }

    #[test]
    fn test_query_point() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);
        let found = world.query_point(&Point::new(50.0, 50.0));
        assert_eq!(found, Some(BodyId::new(1)));
        let not_found = world.query_point(&Point::new(200.0, 200.0));
        assert_eq!(not_found, None);
    }

    #[test]
    fn test_query_overlap() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(200.0, 200.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body1);
        world.add_body(body2);
        let overlap = world.query_overlap(&Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(overlap.len(), 1);
        assert!(overlap.contains(&BodyId::new(1)));
    }

    #[test]
    fn test_gravity_affects_velocity() {
        let mut world = PhysicsWorld::with_config(PhysicsWorldConfig {
            gravity: Vector2D::new(0.0, 980.0),
            ..Default::default()
        });
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);
        world.apply_gravity(0.016);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.velocity.y - 15.68).abs() < 0.1);
    }

    #[test]
    fn test_step_integrates_bodies() {
        let mut world = PhysicsWorld::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        world.add_body(body);
        world.step();
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);
    }

    #[test]
    fn test_wake_all() {
        let mut world = PhysicsWorld::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.is_sleeping = true;
        world.add_body(body);
        world.wake_all();
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(!body.is_sleeping);
    }

    #[test]
    fn test_wake_region() {
        let mut world = PhysicsWorld::new();
        let mut body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(200.0, 200.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body1.is_sleeping = true;
        body2.is_sleeping = true;
        world.add_body(body1);
        world.add_body(body2);
        world.wake_region(&Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(!world.get_body(BodyId::new(1)).unwrap().is_sleeping);
        assert!(world.get_body(BodyId::new(2)).unwrap().is_sleeping);
    }

    #[test]
    fn test_clear_world() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);
        world.clear();
        assert_eq!(world.body_count(), 0);
    }

    #[test]
    fn test_get_bounds() {
        let mut world = PhysicsWorld::new();
        assert!(world.get_bounds().is_none());
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(50.0),
            1.0,
        );
        world.add_body(body);
        let bounds = world.get_bounds().unwrap();
        assert!((bounds.x - 50.0).abs() < f32::EPSILON);
        assert!((bounds.width - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_velocity_clamping() {
        let mut world = PhysicsWorld::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(20000.0, 0.0);
        world.add_body(body);
        world.clamp_velocities();
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.velocity.length() <= world.config.max_velocity);
    }

    #[test]
    fn test_static_body_ignores_gravity() {
        let mut world = PhysicsWorld::with_config(PhysicsWorldConfig {
            gravity: Vector2D::new(0.0, 980.0),
            ..Default::default()
        });
        let body = PhysicsBody::static_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
        );
        world.add_body(body);
        world.apply_gravity(1.0);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.velocity.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_substeps() {
        let mut world = PhysicsWorld::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        world.add_body(body);
        world.step_substeps(4);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);
    }

    #[test]
    fn test_stats_update() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::static_body(
            BodyId::new(2),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
        );
        world.add_body(body1);
        world.add_body(body2);
        world.update_stats();
        assert_eq!(world.stats.body_count, 2);
        assert_eq!(world.stats.dynamic_count, 1);
    }

    #[test]
    fn test_query_point_all() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(30.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 50.0),
            CollisionShape::circle(20.0),
            1.0,
        );
        world.add_body(body1);
        world.add_body(body2);
        let found = world.query_point_all(&Point::new(50.0, 50.0));
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn test_set_timestep() {
        let mut world = PhysicsWorld::new();
        world.set_timestep(1.0 / 120.0);
        assert!((world.config.timestep - 1.0 / 120.0).abs() < f32::EPSILON);
        // Minimum timestep clamp
        world.set_timestep(0.0);
        assert!(world.config.timestep >= 0.001);
    }
}
