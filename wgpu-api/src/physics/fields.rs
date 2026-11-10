//! Field-based interaction physics (System C).
//!
//! Force fields provide expressive, group-level behavior without rigid-body
//! simulation.  Each field defines a region of influence and a force law that
//! acts on any body inside that region.
//!
//! ## Field Types
//!
//! - **Magnetic** — attracts (or repels) bodies toward/away from a center point.
//!   Formula: `F_i = k · (p - x_i) / (‖p - x_i‖ + ε)^n`
//! - **Cohesion** — pulls bodies toward the group's center of mass.
//! - **Separation** — pushes bodies away from each other to avoid crowding.
//! - **Radial** — pushes bodies outward from (or inward toward) a center.
//! - **Directional** — pushes bodies along a fixed direction within a region.
//!
//! ## Design
//!
//! Fields are lightweight and can be combined.  A single body can be affected
//! by multiple fields simultaneously.  Fields do not require a physics engine —
//! they compute forces directly and apply them to body velocities.

use super::body::PhysicsBody;
use crate::geometry::{Point, Vector2D};

// ---------------------------------------------------------------------------
// Field type enum
// ---------------------------------------------------------------------------

/// Type of force field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Attracts bodies toward a center point.
    Attraction,
    /// Repels bodies away from a center point.
    Repulsion,
    /// Pulls bodies toward the group center of mass.
    Cohesion,
    /// Pushes bodies away from neighbors.
    Separation,
    /// Pushes bodies radially outward from center.
    RadialOut,
    /// Pulls bodies radially inward toward center.
    RadialIn,
    /// Pushes bodies along a fixed direction.
    Directional,
}

// ---------------------------------------------------------------------------
// Field configuration
// ---------------------------------------------------------------------------

/// Configuration for a force field.
#[derive(Debug, Clone, Copy)]
pub struct FieldConfig {
    /// Strength multiplier for the field force.
    pub strength: f32,
    /// Radius of influence (0.0 = infinite).
    pub radius: f32,
    /// Falloff exponent (higher = sharper falloff).
    /// Force scales as `1 / (distance + epsilon) ^ falloff`.
    pub falloff: f32,
    /// Small constant to avoid division by zero.
    pub epsilon: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl Default for FieldConfig {
    fn default() -> Self {
        Self {
            strength: 100.0,
            radius: f32::MAX,
            falloff: 2.0,
            epsilon: 0.01,
            enabled: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Field source (anchor point for a field)
// ---------------------------------------------------------------------------

/// A point source that generates a force field.
#[derive(Debug, Clone, Copy)]
pub struct FieldSource {
    /// Center position of the field.
    pub position: Point,
    /// Field configuration.
    pub config: FieldConfig,
    /// Type of field.
    pub field_type: FieldType,
    /// For directional fields: the push direction.
    pub direction: Vector2D,
}

impl FieldSource {
    /// Create a new field source.
    pub fn new(position: Point, field_type: FieldType) -> Self {
        Self {
            position,
            config: FieldConfig::default(),
            field_type,
            direction: Vector2D::new(1.0, 0.0),
        }
    }

    /// Create an attraction field.
    pub fn attraction(position: Point, strength: f32, radius: f32) -> Self {
        Self {
            position,
            config: FieldConfig {
                strength,
                radius,
                ..Default::default()
            },
            field_type: FieldType::Attraction,
            direction: Vector2D::ZERO,
        }
    }

    /// Create a repulsion field.
    pub fn repulsion(position: Point, strength: f32, radius: f32) -> Self {
        Self {
            position,
            config: FieldConfig {
                strength,
                radius,
                ..Default::default()
            },
            field_type: FieldType::Repulsion,
            direction: Vector2D::ZERO,
        }
    }

    /// Compute the force on a body.
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        if !self.config.enabled {
            return Vector2D::ZERO;
        }

        match self.field_type {
            FieldType::Attraction => self.compute_attraction(body),
            FieldType::Repulsion => self.compute_repulsion(body),
            FieldType::Cohesion => Vector2D::ZERO, // Cohesion needs group context
            FieldType::Separation => Vector2D::ZERO, // Separation needs group context
            FieldType::RadialOut => self.compute_radial(body, -1.0),
            FieldType::RadialIn => self.compute_radial(body, 1.0),
            FieldType::Directional => self.compute_directional(body),
        }
    }

    /// Apply the field force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }

    /// Check if a body is within the field's radius.
    fn in_range(&self, body: &PhysicsBody) -> bool {
        if self.config.radius >= f32::MAX {
            return true;
        }
        let dx = body.position.x - self.position.x;
        let dy = body.position.y - self.position.y;
        dx * dx + dy * dy <= self.config.radius * self.config.radius
    }

    /// Compute falloff factor based on distance.
    fn falloff_factor(&self, dist: f32) -> f32 {
        if self.config.falloff == 0.0 {
            return 1.0;
        }
        let denominator = (dist + self.config.epsilon).powf(self.config.falloff);
        1.0 / denominator
    }

    fn compute_attraction(&self, body: &PhysicsBody) -> Vector2D {
        if !self.in_range(body) {
            return Vector2D::ZERO;
        }
        let dx = self.position.x - body.position.x;
        let dy = self.position.y - body.position.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < self.config.epsilon {
            return Vector2D::ZERO;
        }
        let factor = self.falloff_factor(dist);
        let magnitude = self.config.strength * factor;
        Vector2D::new(dx / dist * magnitude, dy / dist * magnitude)
    }

    fn compute_repulsion(&self, body: &PhysicsBody) -> Vector2D {
        if !self.in_range(body) {
            return Vector2D::ZERO;
        }
        let dx = body.position.x - self.position.x;
        let dy = body.position.y - self.position.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < self.config.epsilon {
            return Vector2D::ZERO;
        }
        let factor = self.falloff_factor(dist);
        let magnitude = self.config.strength * factor;
        Vector2D::new(dx / dist * magnitude, dy / dist * magnitude)
    }

    fn compute_radial(&self, body: &PhysicsBody, sign: f32) -> Vector2D {
        if !self.in_range(body) {
            return Vector2D::ZERO;
        }
        let dx = body.position.x - self.position.x;
        let dy = body.position.y - self.position.y;
        let dist = (dx * dx + dy * dy).sqrt();
        if dist < self.config.epsilon {
            return Vector2D::ZERO;
        }
        let factor = self.falloff_factor(dist);
        let magnitude = sign * self.config.strength * factor;
        Vector2D::new(dx / dist * magnitude, dy / dist * magnitude)
    }

    fn compute_directional(&self, body: &PhysicsBody) -> Vector2D {
        if !self.in_range(body) {
            return Vector2D::ZERO;
        }
        let dx = body.position.x - self.position.x;
        let dy = body.position.y - self.position.y;
        let dist = (dx * dx + dy * dy).sqrt();
        let factor = self.falloff_factor(dist);
        let magnitude = self.config.strength * factor;
        Vector2D::new(self.direction.x * magnitude, self.direction.y * magnitude)
    }
}

// ---------------------------------------------------------------------------
// Magnetic Field
// ---------------------------------------------------------------------------

/// A magnetic field that attracts or repels bodies toward/away from a center.
///
/// The force follows the formula from the issue:
/// `F_i = k · (p - x_i) / (‖p - x_i‖ + ε)^n`
///
/// Where:
/// - `k` is the strength
/// - `p` is the field center
/// - `x_i` is the body position
/// - `ε` is a small constant to avoid singularity
/// - `n` is the falloff exponent
#[derive(Debug, Clone)]
pub struct MagneticField {
    /// Center of the magnetic field.
    pub center: Point,
    /// Strength (positive = attract, negative = repel).
    pub strength: f32,
    /// Falloff exponent.
    pub falloff: f32,
    /// Epsilon to avoid division by zero.
    pub epsilon: f32,
    /// Maximum distance (0 = infinite).
    pub max_distance: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl MagneticField {
    /// Create a new magnetic field.
    pub fn new(center: Point, strength: f32) -> Self {
        Self {
            center,
            strength,
            falloff: 2.0,
            epsilon: 0.01,
            max_distance: f32::MAX,
            enabled: true,
        }
    }

    /// Create an attractive magnetic field.
    pub fn attractive(center: Point, strength: f32) -> Self {
        Self::new(center, strength.max(0.0))
    }

    /// Create a repulsive magnetic field.
    pub fn repulsive(center: Point, strength: f32) -> Self {
        Self::new(center, (-strength).max(-10000.0))
    }

    /// Compute the magnetic force on a body.
    ///
    /// `F = k · (p - x) / (‖p - x‖ + ε)^n`
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        if !self.enabled {
            return Vector2D::ZERO;
        }

        let dx = self.center.x - body.position.x;
        let dy = self.center.y - body.position.y;
        let dist = (dx * dx + dy * dy).sqrt();

        // Check max distance
        if self.max_distance < f32::MAX && dist > self.max_distance {
            return Vector2D::ZERO;
        }

        // Avoid singularity
        if dist < self.epsilon {
            return Vector2D::ZERO;
        }

        // F = k · (p - x) / (‖p - x‖ + ε)^n
        let denominator = (dist + self.epsilon).powf(self.falloff);
        let magnitude = self.strength / denominator;

        Vector2D::new(dx * magnitude, dy * magnitude)
    }

    /// Apply the magnetic force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }
}

impl Default for MagneticField {
    fn default() -> Self {
        Self::new(Point::ORIGIN, 100.0)
    }
}

// ---------------------------------------------------------------------------
// Cohesion Field
// ---------------------------------------------------------------------------

/// A cohesion field that pulls bodies toward the group's center of mass.
///
/// Useful for keeping a group of icons clustered together.
#[derive(Debug, Clone, Copy)]
pub struct CohesionField {
    /// Strength of the cohesion force.
    pub strength: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl CohesionField {
    /// Create a new cohesion field.
    pub fn new(strength: f32) -> Self {
        Self {
            strength,
            enabled: true,
        }
    }

    /// Compute the center of mass for a group of bodies.
    pub fn center_of_mass(bodies: &[PhysicsBody]) -> Point {
        if bodies.is_empty() {
            return Point::ORIGIN;
        }
        let mut cx = 0.0;
        let mut cy = 0.0;
        let mut total_mass = 0.0;

        for body in bodies {
            if body.mass > 0.0 && body.mass.is_finite() {
                cx += body.position.x * body.mass;
                cy += body.position.y * body.mass;
                total_mass += body.mass;
            }
        }

        if total_mass > 0.0 {
            Point::new(cx / total_mass, cy / total_mass)
        } else {
            let n = bodies.len() as f32;
            Point::new(cx / n, cy / n)
        }
    }

    /// Compute the cohesion force on a body toward the group center.
    pub fn compute_force(&self, body: &PhysicsBody, center: Point) -> Vector2D {
        if !self.enabled {
            return Vector2D::ZERO;
        }
        let dx = center.x - body.position.x;
        let dy = center.y - body.position.y;
        Vector2D::new(dx * self.strength, dy * self.strength)
    }

    /// Apply cohesion to all bodies in a group.
    pub fn apply_to_group(&self, bodies: &mut [PhysicsBody], dt: f32) {
        if !self.enabled || bodies.is_empty() {
            return;
        }
        let center = Self::center_of_mass(bodies);
        let strength = self.strength;
        let mut forces: Vec<Vector2D> = bodies
            .iter()
            .map(|b| {
                let dx = center.x - b.position.x;
                let dy = center.y - b.position.y;
                Vector2D::new(dx * strength, dy * strength)
            })
            .collect();
        for (body, force) in bodies.iter_mut().zip(forces.into_iter()) {
            body.apply_force(force, dt);
        }
    }
}

// ---------------------------------------------------------------------------
// Separation Field
// ---------------------------------------------------------------------------

/// A separation field that pushes bodies away from each other.
///
/// Prevents crowding and maintains minimum distance between group members.
#[derive(Debug, Clone, Copy)]
pub struct SeparationField {
    /// Strength of the separation force.
    pub strength: f32,
    /// Minimum desired distance between bodies.
    pub min_distance: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl SeparationField {
    /// Create a new separation field.
    pub fn new(strength: f32, min_distance: f32) -> Self {
        Self {
            strength,
            min_distance,
            enabled: true,
        }
    }

    /// Compute the separation force on a body from all other bodies.
    pub fn compute_force(&self, body: &PhysicsBody, others: &[PhysicsBody]) -> Vector2D {
        if !self.enabled {
            return Vector2D::ZERO;
        }
        let mut force = Vector2D::ZERO;
        let mut count = 0;

        for other in others {
            if other.id == body.id {
                continue;
            }
            let dx = body.position.x - other.position.x;
            let dy = body.position.y - other.position.y;
            let dist_sq = dx * dx + dy * dy;

            if dist_sq < self.min_distance * self.min_distance && dist_sq > 0.0001 {
                let dist = dist_sq.sqrt();
                // Force increases as bodies get closer
                let overlap = self.min_distance - dist;
                let magnitude = self.strength * overlap / dist;
                force.x += dx * magnitude;
                force.y += dy * magnitude;
                count += 1;
            }
        }

        // Average the force
        if count > 0 {
            force.x /= count as f32;
            force.y /= count as f32;
        }
        force
    }

    /// Apply separation to all bodies in a group.
    pub fn apply_to_group(&self, bodies: &mut [PhysicsBody], dt: f32) {
        if !self.enabled || bodies.len() < 2 {
            return;
        }
        let strength = self.strength;
        let min_dist = self.min_distance;
        let len = bodies.len();
        let mut forces: Vec<Vector2D> = vec![Vector2D::ZERO; len];
        let mut counts: Vec<usize> = vec![0; len];

        for i in 0..len {
            for j in 0..len {
                if i == j {
                    continue;
                }
                let dx = bodies[i].position.x - bodies[j].position.x;
                let dy = bodies[i].position.y - bodies[j].position.y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq < min_dist * min_dist && dist_sq > 0.0001 {
                    let dist = dist_sq.sqrt();
                    let overlap = min_dist - dist;
                    let magnitude = strength * overlap / dist;
                    forces[i].x += dx * magnitude;
                    forces[i].y += dy * magnitude;
                    counts[i] += 1;
                }
            }
        }

        for i in 0..len {
            if counts[i] > 0 {
                forces[i].x /= counts[i] as f32;
                forces[i].y /= counts[i] as f32;
            }
            bodies[i].apply_force(forces[i], dt);
        }
    }
}

// ---------------------------------------------------------------------------
// Radial Field
// ---------------------------------------------------------------------------

/// A radial field that pushes or pulls bodies along the radial axis from a center.
#[derive(Debug, Clone, Copy)]
pub struct RadialField {
    /// Center of the radial field.
    pub center: Point,
    /// Strength (positive = push out, negative = pull in).
    pub strength: f32,
    /// Inner radius (force is zero inside this radius).
    pub inner_radius: f32,
    /// Outer radius (force is zero outside this radius).
    pub outer_radius: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl RadialField {
    /// Create a new radial field.
    pub fn new(center: Point, strength: f32, radius: f32) -> Self {
        Self {
            center,
            strength,
            inner_radius: 0.0,
            outer_radius: radius,
            enabled: true,
        }
    }

    /// Compute the radial force on a body.
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        if !self.enabled {
            return Vector2D::ZERO;
        }
        let dx = body.position.x - self.center.x;
        let dy = body.position.y - self.center.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < self.inner_radius || dist > self.outer_radius || dist < 0.001 {
            return Vector2D::ZERO;
        }

        // Falloff: full strength at inner radius, zero at outer radius
        let t = (dist - self.inner_radius) / (self.outer_radius - self.inner_radius);
        let falloff = 1.0 - t;
        let magnitude = self.strength * falloff;

        Vector2D::new(dx / dist * magnitude, dy / dist * magnitude)
    }

    /// Apply the radial force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }
}

// ---------------------------------------------------------------------------
// Directional Field
// ---------------------------------------------------------------------------

/// A directional field that pushes bodies along a fixed direction within a region.
#[derive(Debug, Clone, Copy)]
pub struct DirectionalField {
    /// Origin point of the field.
    pub origin: Point,
    /// Direction of the force.
    pub direction: Vector2D,
    /// Strength of the force.
    pub strength: f32,
    /// Radius of influence.
    pub radius: f32,
    /// Whether the field is active.
    pub enabled: bool,
}

impl DirectionalField {
    /// Create a new directional field.
    pub fn new(origin: Point, direction: Vector2D, strength: f32, radius: f32) -> Self {
        Self {
            origin,
            direction,
            strength,
            radius,
            enabled: true,
        }
    }

    /// Compute the directional force on a body.
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        if !self.enabled {
            return Vector2D::ZERO;
        }
        let dx = body.position.x - self.origin.x;
        let dy = body.position.y - self.origin.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist > self.radius {
            return Vector2D::ZERO;
        }

        // Linear falloff
        let falloff = 1.0 - (dist / self.radius).min(1.0);
        let magnitude = self.strength * falloff;

        Vector2D::new(self.direction.x * magnitude, self.direction.y * magnitude)
    }

    /// Apply the directional force to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }
}

// ---------------------------------------------------------------------------
// ForceField — a collection of field sources
// ---------------------------------------------------------------------------

/// A collection of force fields that can be applied to bodies.
#[derive(Debug, Clone)]
pub struct ForceField {
    /// List of field sources.
    pub sources: Vec<FieldSource>,
    /// Optional magnetic field overlay.
    pub magnetic: Option<MagneticField>,
    /// Optional cohesion field for groups.
    pub cohesion: Option<CohesionField>,
    /// Optional separation field for groups.
    pub separation: Option<SeparationField>,
    /// Optional radial field.
    pub radial: Option<RadialField>,
    /// Optional directional field.
    pub directional: Option<DirectionalField>,
}

impl ForceField {
    /// Create a new empty force field.
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            magnetic: None,
            cohesion: None,
            separation: None,
            radial: None,
            directional: None,
        }
    }

    /// Add a field source.
    pub fn add_source(&mut self, source: FieldSource) {
        self.sources.push(source);
    }

    /// Set a magnetic field.
    pub fn set_magnetic(&mut self, field: MagneticField) {
        self.magnetic = Some(field);
    }

    /// Set a cohesion field.
    pub fn set_cohesion(&mut self, field: CohesionField) {
        self.cohesion = Some(field);
    }

    /// Set a separation field.
    pub fn set_separation(&mut self, field: SeparationField) {
        self.separation = Some(field);
    }

    /// Set a radial field.
    pub fn set_radial(&mut self, field: RadialField) {
        self.radial = Some(field);
    }

    /// Set a directional field.
    pub fn set_directional(&mut self, field: DirectionalField) {
        self.directional = Some(field);
    }

    /// Compute the total force on a body from all sources.
    pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
        let mut total = Vector2D::ZERO;

        for source in &self.sources {
            total.x += source.compute_force(body).x;
            total.y += source.compute_force(body).y;
        }

        if let Some(ref magnetic) = self.magnetic {
            let f = magnetic.compute_force(body);
            total.x += f.x;
            total.y += f.y;
        }

        if let Some(ref radial) = self.radial {
            let f = radial.compute_force(body);
            total.x += f.x;
            total.y += f.y;
        }

        if let Some(ref directional) = self.directional {
            let f = directional.compute_force(body);
            total.x += f.x;
            total.y += f.y;
        }

        total
    }

    /// Apply all field forces to a body.
    pub fn apply(&self, body: &mut PhysicsBody, dt: f32) {
        let force = self.compute_force(body);
        body.apply_force(force, dt);
    }

    /// Apply all field forces to a group of bodies.
    pub fn apply_to_group(&self, bodies: &mut [PhysicsBody], dt: f32) {
        // Point sources and magnetic/radial/directional fields
        let len = bodies.len();
        let mut forces: Vec<Vector2D> = vec![Vector2D::ZERO; len];
        for (i, body) in bodies.iter().enumerate() {
            forces[i] = self.compute_force(body);
        }
        for (i, body) in bodies.iter_mut().enumerate() {
            body.apply_force(forces[i], dt);
        }

        // Cohesion (needs group context)
        if let Some(ref cohesion) = self.cohesion {
            cohesion.apply_to_group(bodies, dt);
        }

        // Separation (needs group context)
        if let Some(ref separation) = self.separation {
            separation.apply_to_group(bodies, dt);
        }
    }
}

impl Default for ForceField {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_magnetic_field_attracts() {
        let field = MagneticField::attractive(Point::new(100.0, 100.0), 500.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Force should pull toward (100, 100)
        assert!(force.x > 0.0);
        assert!(force.y > 0.0);
    }

    #[test]
    fn test_magnetic_field_repels() {
        let field = MagneticField::repulsive(Point::new(100.0, 100.0), 500.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Force should push away from (100, 100)
        assert!(force.x < 0.0);
        assert!(force.y < 0.0);
    }

    #[test]
    fn test_magnetic_field_zero_at_center() {
        let field = MagneticField::attractive(Point::new(50.0, 50.0), 500.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // At center, force should be near zero
        assert!(force.length() < 1.0);
    }

    #[test]
    fn test_magnetic_field_max_distance() {
        let mut field = MagneticField::attractive(Point::new(0.0, 0.0), 500.0);
        field.max_distance = 50.0;
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Outside max distance, force should be zero
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_cohesion_field() {
        let cohesion = CohesionField::new(10.0);
        let bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(100.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        ];
        let center = CohesionField::center_of_mass(&bodies);
        assert!((center.x - 50.0).abs() < 0.01);
        let force = cohesion.compute_force(&bodies[0], center);
        // Body 0 at (0,0) should be pulled toward center (50, 0)
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_separation_field() {
        let separation = SeparationField::new(100.0, 50.0);
        let bodies = vec![
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
        let force = separation.compute_force(&bodies[0], &bodies);
        // Body 0 should be pushed away from body 1
        assert!(force.x < 0.0);
    }

    #[test]
    fn test_separation_no_force_when_far() {
        let separation = SeparationField::new(100.0, 50.0);
        let bodies = vec![
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(200.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        ];
        let force = separation.compute_force(&bodies[0], &bodies);
        // Bodies are far apart, no separation force
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_radial_field_push_out() {
        let field = RadialField::new(Point::ORIGIN, 100.0, 100.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Positive strength pushes outward
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_radial_field_outside_range() {
        let field = RadialField::new(Point::ORIGIN, 100.0, 50.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Outside outer radius, no force
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_directional_field() {
        let field = DirectionalField::new(Point::ORIGIN, Vector2D::new(1.0, 0.0), 100.0, 100.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(10.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Force should be in the +x direction
        assert!(force.x > 0.0);
        assert!((force.y - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_field_source_attraction() {
        let source = FieldSource::attraction(Point::new(100.0, 0.0), 500.0, 200.0);
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = source.compute_force(&body);
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_force_field_combined() {
        let mut field = ForceField::new();
        field.set_magnetic(MagneticField::attractive(Point::new(100.0, 0.0), 500.0));
        field.add_source(FieldSource::repulsion(Point::new(50.0, 0.0), 200.0, 100.0));

        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        // Magnetic pulls right, repulsion pushes left — net should be right
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_disabled_field() {
        let mut field = MagneticField::attractive(Point::new(100.0, 0.0), 500.0);
        field.enabled = false;
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let force = field.compute_force(&body);
        assert!(force.length() < 0.01);
    }

    #[test]
    fn test_falloff_exponent() {
        let mut field = MagneticField::new(Point::new(100.0, 0.0), 500.0);
        field.falloff = 0.0; // No falloff
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(90.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let f1 = field.compute_force(&body1);
        let f2 = field.compute_force(&body2);
        // With no falloff, closer body gets stronger force
        assert!(f1.length() > f2.length());
    }
}
