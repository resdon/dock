//! Physics body types for the 2D interaction engine.
//!
//! Each UI element that participates in physics simulation is represented by a
//! `PhysicsBody`. Bodies can be static (immovable backgrounds), kinematic
//! (moved by code, not forces), or dynamic (affected by forces and collisions).
//!
//! Every interactive visual element has a separate physical representation
//! linked by [`NodeId`]. The renderer doesn't need to know about mass, forces,
//! or collision shapes. Likewise, the physics engine doesn't need to know
//! whether the object is an icon, a text label, or a video preview.
//!
//! ## Body Types
//!
//! - **Static**: Infinite mass, never moves. Used for dock backgrounds, walls.
//! - **Kinematic**: Moved explicitly via position/velocity, pushes dynamic bodies.
//!   Used for scroll containers, animated panels.
//! - **Dynamic**: Affected by forces, springs, and collisions. Used for icons,
//!   draggable items, and interactive elements.
//!
//! ## Physical Properties
//!
//! [`PhysicalProperties`] consolidates mass, friction, restitution, damping,
//! and stiffness.  An icon's mass can be a design parameter rather than a
//! physically realistic property — a heavier icon displaces its neighbors
//! more strongly.
//!
//! ## Object Mapping
//!
//! [`PhysicsObject`] bridges the visual scene graph and the physics world.
//! It holds a [`NodeId`] (visual identity) and a [`PhysicsBody`] (physical
//! identity), plus optional [`PhysicalProperties`] for fine-tuned behavior.

use crate::geometry::{Point, Rect, Size, Vector2D};

/// Opaque identifier for a physics body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BodyId(u64);

impl BodyId {
    /// Create a new BodyId.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw u64 value.
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for BodyId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

// Re-export NodeId from geometry for ergonomic imports in physics module
pub use super::super::geometry::NodeId;

/// Consolidated physical properties for an interactive element.
///
/// Not every property is needed on every object.  For example:
///
/// | Object              | Physical behavior          |
/// |---------------------|----------------------------|
/// | Dock icon           | Dynamic, spring-based      |
/// | Dock background     | Static                     |
/// | Popup panel         | Kinematic                  |
/// | Floating notification| Dynamic or spring-based   |
/// | Video preview       | Kinematic                  |
/// | Text label          | Attached to parent         |
/// | Dragged object      | Temporarily controlled by pointer |
///
/// An icon's mass can be a design parameter rather than a physically realistic
/// property — a heavier icon displaces its neighbors more strongly.
#[derive(Debug, Clone, Copy)]
pub struct PhysicalProperties {
    /// Mass of the object (design parameter, not necessarily realistic).
    pub mass: f32,
    /// Coefficient of friction (0.0 = slippery, 1.0 = sticky).
    pub friction: f32,
    /// Coefficient of restitution (0.0 = no bounce, 1.0 = perfect bounce).
    pub restitution: f32,
    /// Linear velocity damping (0.0 = no damping, 1.0 = maximum).
    pub linear_damping: f32,
    /// Angular velocity damping (0.0 = no damping, 1.0 = maximum).
    pub angular_damping: f32,
    /// Spring stiffness for rest-position attraction.
    pub stiffness: f32,
}

impl PhysicalProperties {
    /// Default properties for a dock icon (dynamic, spring-based).
    pub fn dock_icon() -> Self {
        Self {
            mass: 1.0,
            friction: 0.3,
            restitution: 0.2,
            linear_damping: 0.5,
            angular_damping: 0.5,
            stiffness: 500.0,
        }
    }

    /// Properties for a static background element.
    pub fn static_background() -> Self {
        Self {
            mass: f32::INFINITY,
            friction: 1.0,
            restitution: 0.0,
            linear_damping: 1.0,
            angular_damping: 1.0,
            stiffness: 0.0,
        }
    }

    /// Properties for a kinematic panel (moved by code).
    pub fn kinematic_panel() -> Self {
        Self {
            mass: f32::INFINITY,
            friction: 0.5,
            restitution: 0.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            stiffness: 0.0,
        }
    }

    /// Properties for a floating notification (lightweight, bouncy).
    pub fn floating_notification() -> Self {
        Self {
            mass: 0.5,
            friction: 0.2,
            restitution: 0.4,
            linear_damping: 0.3,
            angular_damping: 0.3,
            stiffness: 300.0,
        }
    }

    /// Properties for a dragged object (temporarily pointer-controlled).
    pub fn dragged_object() -> Self {
        Self {
            mass: 2.0,
            friction: 0.1,
            restitution: 0.0,
            linear_damping: 0.8,
            angular_damping: 0.8,
            stiffness: 1000.0,
        }
    }

    /// Apply these properties to a PhysicsBody.
    pub fn apply_to(&self, body: &mut PhysicsBody) {
        body.mass = self.mass;
        body.inv_mass = if self.mass > 0.0 && self.mass < f32::INFINITY {
            1.0 / self.mass
        } else {
            0.0
        };
        body.inertia = PhysicsBody::compute_inertia_from_shape(&body.shape, self.mass);
        body.inv_inertia = if body.inertia > 0.0 && body.inertia < f32::INFINITY {
            1.0 / body.inertia
        } else {
            0.0
        };
        body.damping.linear = self.linear_damping;
        body.damping.angular = self.angular_damping;
        body.material.friction = self.friction;
        body.material.restitution = self.restitution;
    }
}

impl Default for PhysicalProperties {
    fn default() -> Self {
        Self::dock_icon()
    }
}

/// Bridges a visual scene node and its physical representation.
///
/// The `NodeId` connects the physical representation to the visual representation.
/// The renderer doesn't need to know about mass, forces or collision shapes.
/// Likewise, the physics engine doesn't need to know whether the object is an
/// icon, a text label or a video preview.
#[derive(Debug, Clone)]
pub struct PhysicsObject {
    /// Visual scene node identity.
    pub node: NodeId,
    /// Physical body for simulation.
    pub body: PhysicsBody,
    /// Optional fine-tuned physical properties.
    pub properties: Option<PhysicalProperties>,
    /// Whether this object is currently enabled for interaction.
    pub enabled: bool,
}

impl PhysicsObject {
    /// Create a new physics object from a body.
    pub fn new(node: NodeId, body: PhysicsBody) -> Self {
        Self {
            node,
            body,
            properties: None,
            enabled: true,
        }
    }

    /// Create a physics object with custom properties.
    pub fn with_properties(
        node: NodeId,
        body: PhysicsBody,
        properties: PhysicalProperties,
    ) -> Self {
        let mut obj = Self {
            node,
            body,
            properties: Some(properties),
            enabled: true,
        };
        // Apply properties to the body
        if let Some(ref props) = obj.properties {
            props.apply_to(&mut obj.body);
        }
        obj
    }

    /// Create a dynamic dock icon.
    pub fn dock_icon(node: NodeId, position: Point, shape: CollisionShape) -> Self {
        let body = PhysicsBody::dynamic_body(BodyId::from(node.as_u64()), position, shape, 1.0);
        Self::with_properties(node, body, PhysicalProperties::dock_icon())
    }

    /// Create a static background element.
    pub fn static_background(node: NodeId, position: Point, shape: CollisionShape) -> Self {
        let body = PhysicsBody::static_body(BodyId::from(node.as_u64()), position, shape);
        Self::with_properties(node, body, PhysicalProperties::static_background())
    }

    /// Create a kinematic panel.
    pub fn kinematic_panel(node: NodeId, position: Point, shape: CollisionShape) -> Self {
        let body = PhysicsBody::kinematic_body(BodyId::from(node.as_u64()), position, shape);
        Self::with_properties(node, body, PhysicalProperties::kinematic_panel())
    }

    /// Get a reference to the body.
    pub fn body(&self) -> &PhysicsBody {
        &self.body
    }

    /// Get a mutable reference to the body.
    pub fn body_mut(&mut self) -> &mut PhysicsBody {
        &mut self.body
    }

    /// Get the body ID.
    pub fn body_id(&self) -> BodyId {
        self.body.id
    }

    /// Get the current position.
    pub fn position(&self) -> Point {
        self.body.position
    }

    /// Set the rest (preferred) position for spring attraction.
    pub fn set_rest_position(&mut self, position: Point) {
        self.body.set_rest_position(position);
    }

    /// Disable this object for interaction.
    pub fn disable(&mut self) {
        self.enabled = false;
        self.body.enabled = false;
    }

    /// Re-enable this object.
    pub fn enable(&mut self) {
        self.enabled = true;
        self.body.enabled = true;
    }
}

/// Type of physics body determining how it responds to forces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyType {
    /// Immovable body with infinite mass. Never affected by forces.
    Static,
    /// Body moved explicitly by code; pushes dynamic bodies on collision.
    Kinematic,
    /// Body affected by forces, springs, and collisions.
    Dynamic,
}

impl Default for BodyType {
    fn default() -> Self {
        Self::Dynamic
    }
}

/// Collision shape for a physics body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionShape {
    /// Axis-aligned bounding box.
    AABB {
        /// Half-extents (width/2, height/2).
        half_extents: Size,
    },
    /// Circular collision shape.
    Circle {
        /// Radius of the circle.
        radius: f32,
    },
}

impl CollisionShape {
    /// Create an AABB shape from a rectangle.
    pub fn from_rect(rect: &Rect) -> Self {
        Self::AABB {
            half_extents: Size::new(rect.width / 2.0, rect.height / 2.0),
        }
    }

    /// Create a circular shape with the given radius.
    pub fn circle(radius: f32) -> Self {
        Self::Circle { radius }
    }

    /// Get the bounding rectangle of this shape centered at the given point.
    pub fn bounding_rect(&self, center: &Point) -> Rect {
        match self {
            Self::AABB { half_extents } => Rect::new(
                center.x - half_extents.width,
                center.y - half_extents.height,
                half_extents.width * 2.0,
                half_extents.height * 2.0,
            ),
            Self::Circle { radius } => {
                let s = radius * 2.0;
                Rect::new(center.x - radius, center.y - radius, s, s)
            }
        }
    }

    /// Check if a point is inside this shape centered at the given point.
    pub fn contains_point(&self, center: &Point, point: &Point) -> bool {
        match self {
            Self::AABB { half_extents } => {
                let dx = (point.x - center.x).abs();
                let dy = (point.y - center.y).abs();
                dx <= half_extents.width && dy <= half_extents.height
            }
            Self::Circle { radius } => {
                let dx = point.x - center.x;
                let dy = point.y - center.y;
                dx * dx + dy * dy <= radius * radius
            }
        }
    }
}

/// Damping configuration for a body.
#[derive(Debug, Clone, Copy)]
pub struct DampingConfig {
    /// Linear velocity damping (0.0 = no damping, 1.0 = maximum).
    pub linear: f32,
    /// Angular velocity damping (0.0 = no damping, 1.0 = maximum).
    pub angular: f32,
}

impl Default for DampingConfig {
    fn default() -> Self {
        Self {
            linear: 0.5,
            angular: 0.5,
        }
    }
}

/// Restitution (bounciness) and friction for collision response.
#[derive(Debug, Clone, Copy)]
pub struct MaterialProperties {
    /// Coefficient of restitution (0.0 = no bounce, 1.0 = perfect bounce).
    pub restitution: f32,
    /// Coefficient of friction (0.0 = slippery, 1.0 = sticky).
    pub friction: f32,
}

impl Default for MaterialProperties {
    fn default() -> Self {
        Self {
            restitution: 0.3,
            friction: 0.5,
        }
    }
}

/// A physics body representing a physical object in the 2D world.
#[derive(Debug, Clone)]
pub struct PhysicsBody {
    /// Unique identifier.
    pub id: BodyId,
    /// Body type determining force response.
    pub body_type: BodyType,
    /// Current position (center of mass).
    pub position: Point,
    /// Current linear velocity.
    pub velocity: Vector2D,
    /// Current linear acceleration (from accumulated forces).
    pub acceleration: Vector2D,
    /// Current rotation angle in radians.
    pub rotation: f32,
    /// Current angular velocity.
    pub angular_velocity: f32,
    /// Mass (ignored for Static bodies).
    pub mass: f32,
    /// Inverse mass for faster calculations.
    pub inv_mass: f32,
    /// Moment of inertia (rotational mass).
    pub inertia: f32,
    /// Inverse inertia.
    pub inv_inertia: f32,
    /// Collision shape.
    pub shape: CollisionShape,
    /// Damping configuration.
    pub damping: DampingConfig,
    /// Material properties for collision response.
    pub material: MaterialProperties,
    /// Whether this body participates in collision detection.
    pub enabled: bool,
    /// Whether this body can be put to sleep when idle.
    pub allow_sleep: bool,
    /// Whether this body is currently sleeping.
    pub is_sleeping: bool,
    /// Optional preferred/rest position for spring attraction.
    pub rest_position: Option<Point>,
    /// Optional anchor point for rotation (relative to position).
    pub anchor: Point,
    /// User data tag for application-level identification.
    pub tag: Option<String>,
    /// Original body type saved when anchored (for restoration on release).
    pub anchored_type: Option<BodyType>,
}

impl PhysicsBody {
    /// Create a new static body at the given position.
    pub fn static_body(id: BodyId, position: Point, shape: CollisionShape) -> Self {
        Self {
            id,
            body_type: BodyType::Static,
            position,
            velocity: Vector2D::ZERO,
            acceleration: Vector2D::ZERO,
            rotation: 0.0,
            angular_velocity: 0.0,
            mass: f32::INFINITY,
            inv_mass: 0.0,
            inertia: f32::INFINITY,
            inv_inertia: 0.0,
            shape,
            damping: DampingConfig::default(),
            material: MaterialProperties::default(),
            enabled: true,
            allow_sleep: false,
            is_sleeping: false,
            rest_position: None,
            anchor: Point::ORIGIN,
            tag: None,
            anchored_type: None,
        }
    }

    /// Create a new kinematic body at the given position.
    pub fn kinematic_body(id: BodyId, position: Point, shape: CollisionShape) -> Self {
        Self {
            id,
            body_type: BodyType::Kinematic,
            position,
            velocity: Vector2D::ZERO,
            acceleration: Vector2D::ZERO,
            rotation: 0.0,
            angular_velocity: 0.0,
            mass: f32::INFINITY,
            inv_mass: 0.0,
            inertia: f32::INFINITY,
            inv_inertia: 0.0,
            shape,
            damping: DampingConfig::default(),
            material: MaterialProperties::default(),
            enabled: true,
            allow_sleep: false,
            is_sleeping: false,
            rest_position: None,
            anchor: Point::ORIGIN,
            tag: None,
            anchored_type: None,
        }
    }

    /// Create a new dynamic body at the given position with the given mass.
    pub fn dynamic_body(id: BodyId, position: Point, shape: CollisionShape, mass: f32) -> Self {
        let inv_mass = if mass > 0.0 { 1.0 / mass } else { 0.0 };
        let inertia = Self::compute_inertia_from_shape(&shape, mass);
        let inv_inertia = if inertia > 0.0 { 1.0 / inertia } else { 0.0 };

        Self {
            id,
            body_type: BodyType::Dynamic,
            position,
            velocity: Vector2D::ZERO,
            acceleration: Vector2D::ZERO,
            rotation: 0.0,
            angular_velocity: 0.0,
            mass,
            inv_mass,
            inertia,
            inv_inertia,
            shape,
            damping: DampingConfig::default(),
            material: MaterialProperties::default(),
            enabled: true,
            allow_sleep: true,
            is_sleeping: false,
            rest_position: None,
            anchor: Point::ORIGIN,
            tag: None,
            anchored_type: None,
        }
    }

    /// Create a dynamic body from a rectangle.
    pub fn dynamic_from_rect(id: BodyId, rect: &Rect, mass: f32) -> Self {
        Self::dynamic_body(id, rect.center(), CollisionShape::from_rect(rect), mass)
    }

    /// Compute the moment of inertia for a shape with the given mass.
    pub fn compute_inertia_from_shape(shape: &CollisionShape, mass: f32) -> f32 {
        match shape {
            CollisionShape::AABB { half_extents } => {
                // Inertia of a rectangle about its center
                let w = half_extents.width * 2.0;
                let h = half_extents.height * 2.0;
                mass * (w * w + h * h) / 12.0
            }
            CollisionShape::Circle { radius } => {
                // Inertia of a circle about its center
                0.5 * mass * radius * radius
            }
        }
    }

    /// Get the bounding rectangle of this body.
    pub fn bounding_rect(&self) -> Rect {
        self.shape.bounding_rect(&self.position)
    }

    /// Check if a point is inside this body's collision shape.
    pub fn contains_point(&self, point: &Point) -> bool {
        self.shape.contains_point(&self.position, point)
    }

    /// Apply an impulse (instantaneous force) at the body's center.
    pub fn apply_impulse(&mut self, impulse: Vector2D) {
        if self.body_type != BodyType::Dynamic || self.is_sleeping {
            return;
        }
        self.velocity.x += impulse.x * self.inv_mass;
        self.velocity.y += impulse.y * self.inv_mass;
        self.is_sleeping = false;
    }

    /// Apply an impulse at a specific point (also creates angular impulse).
    pub fn apply_impulse_at_point(&mut self, impulse: Vector2D, point: Point) {
        if self.body_type != BodyType::Dynamic || self.is_sleeping {
            return;
        }
        self.apply_impulse(impulse);
        // Angular impulse = r × F
        let r = Vector2D::new(point.x - self.position.x, point.y - self.position.y);
        let angular_impulse = r.cross(&impulse);
        self.angular_velocity += angular_impulse * self.inv_inertia;
    }

    /// Apply a continuous force at the body's center.
    ///
    /// The acceleration is accumulated for diagnostics, while each force
    /// contributes its own `F / m * dt` velocity change exactly once.
    pub fn apply_force(&mut self, force: Vector2D, dt: f32) {
        if self.body_type != BodyType::Dynamic || self.is_sleeping {
            return;
        }
        let ax = force.x * self.inv_mass;
        let ay = force.y * self.inv_mass;
        self.acceleration.x += ax;
        self.acceleration.y += ay;
        self.velocity.x += ax * dt;
        self.velocity.y += ay * dt;
        self.is_sleeping = false;
    }

    /// Reset accumulated acceleration to zero.
    ///
    /// Call this at the end of each physics step after forces have been
    /// integrated into velocity, so that the next step starts fresh.
    pub fn reset_acceleration(&mut self) {
        self.acceleration = Vector2D::ZERO;
    }

    /// Wake up a sleeping body.
    pub fn wake_up(&mut self) {
        self.is_sleeping = false;
    }

    /// Put a body to sleep.
    pub fn put_to_sleep(&mut self) {
        if self.allow_sleep {
            self.is_sleeping = true;
            self.velocity = Vector2D::ZERO;
            self.angular_velocity = 0.0;
        }
    }

    /// Check if this body should sleep based on velocity threshold.
    pub fn should_sleep(&self, velocity_threshold: f32) -> bool {
        if !self.allow_sleep || self.body_type != BodyType::Dynamic {
            return false;
        }
        self.velocity.length_squared() < velocity_threshold
            && self.angular_velocity.abs() < velocity_threshold
    }

    /// Integrate position and velocity for one timestep.
    pub fn integrate(&mut self, dt: f32) {
        if self.body_type != BodyType::Dynamic || self.is_sleeping {
            return;
        }

        // Apply linear damping
        let linear_damp = (1.0 - self.damping.linear * dt).max(0.0);
        self.velocity.x *= linear_damp;
        self.velocity.y *= linear_damp;

        // Apply angular damping
        let angular_damp = (1.0 - self.damping.angular * dt).max(0.0);
        self.angular_velocity *= angular_damp;

        // Integrate position
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;

        // Integrate rotation
        self.rotation += self.angular_velocity * dt;

        // Check sleep condition
        if self.should_sleep(0.001) {
            self.put_to_sleep();
        }
    }

    /// Set a rest position that the body will be attracted to by springs.
    pub fn set_rest_position(&mut self, position: Point) {
        self.rest_position = Some(position);
    }

    /// Clear the rest position.
    pub fn clear_rest_position(&mut self) {
        self.rest_position = None;
    }

    /// Set a tag for application-level identification.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Set the anchor point for rotation.
    pub fn with_anchor(mut self, anchor: Point) -> Self {
        self.anchor = anchor;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_body_has_zero_inv_mass() {
        let body = PhysicsBody::static_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::from_rect(&Rect::new(0.0, 0.0, 50.0, 50.0)),
        );
        assert_eq!(body.body_type, BodyType::Static);
        assert_eq!(body.inv_mass, 0.0);
        assert_eq!(body.mass, f32::INFINITY);
    }

    #[test]
    fn test_dynamic_body_has_correct_inv_mass() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            2.0,
        );
        assert_eq!(body.body_type, BodyType::Dynamic);
        assert!((body.inv_mass - 0.5).abs() < f32::EPSILON);
        assert!(body.inv_inertia > 0.0);
    }

    #[test]
    fn test_impulse_affects_velocity() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(3),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.apply_impulse(Vector2D::new(10.0, 0.0));
        assert!((body.velocity.x - 10.0).abs() < f32::EPSILON);
        assert!((body.velocity.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_impulse_ignored_on_static() {
        let mut body =
            PhysicsBody::static_body(BodyId::new(4), Point::ORIGIN, CollisionShape::circle(10.0));
        body.apply_impulse(Vector2D::new(100.0, 0.0));
        assert!((body.velocity.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_impulse_at_point_creates_angular_velocity() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(5),
            Point::ORIGIN,
            CollisionShape::AABB {
                half_extents: Size::new(10.0, 10.0),
            },
            1.0,
        );
        // Apply impulse at top-right corner
        body.apply_impulse_at_point(Vector2D::new(0.0, 10.0), Point::new(10.0, -10.0));
        assert!(body.angular_velocity.abs() > 0.0);
    }

    #[test]
    fn test_integration_updates_position() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(6),
            Point::new(0.0, 0.0),
            CollisionShape::circle(5.0),
            1.0,
        );
        body.velocity = Vector2D::new(10.0, 20.0);
        body.integrate(0.016); // ~60fps timestep
        assert!((body.position.x - 0.16).abs() < 0.01);
        assert!((body.position.y - 0.32).abs() < 0.01);
    }

    #[test]
    fn test_damping_reduces_velocity() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(7),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        body.damping.linear = 0.5;
        body.integrate(1.0);
        assert!(body.velocity.x < 100.0);
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_sleeping_body_ignores_integration() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(8),
            Point::new(0.0, 0.0),
            CollisionShape::circle(5.0),
            1.0,
        );
        body.velocity = Vector2D::new(100.0, 0.0);
        body.is_sleeping = true;
        body.integrate(1.0);
        assert!((body.position.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_wake_up_enables_integration() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(9),
            Point::new(0.0, 0.0),
            CollisionShape::circle(5.0),
            1.0,
        );
        body.is_sleeping = true;
        body.velocity = Vector2D::new(10.0, 0.0);
        body.wake_up();
        body.integrate(1.0);
        assert!(body.position.x > 0.0);
    }

    #[test]
    fn test_circle_contains_point() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(10),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        assert!(body.contains_point(&Point::new(50.0, 50.0)));
        assert!(body.contains_point(&Point::new(60.0, 60.0)));
        assert!(!body.contains_point(&Point::new(100.0, 100.0)));
    }

    #[test]
    fn test_aabb_contains_point() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(11),
            Point::new(50.0, 50.0),
            CollisionShape::AABB {
                half_extents: Size::new(25.0, 25.0),
            },
            1.0,
        );
        assert!(body.contains_point(&Point::new(50.0, 50.0)));
        assert!(body.contains_point(&Point::new(26.0, 26.0)));
        assert!(!body.contains_point(&Point::new(0.0, 0.0)));
        assert!(!body.contains_point(&Point::new(100.0, 100.0)));
    }

    #[test]
    fn test_bounding_rect() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(12),
            Point::new(50.0, 50.0),
            CollisionShape::AABB {
                half_extents: Size::new(25.0, 25.0),
            },
            1.0,
        );
        let rect = body.bounding_rect();
        assert!((rect.x - 25.0).abs() < f32::EPSILON);
        assert!((rect.y - 25.0).abs() < f32::EPSILON);
        assert!((rect.width - 50.0).abs() < f32::EPSILON);
        assert!((rect.height - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rest_position() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(13),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        );
        body.set_rest_position(Point::new(100.0, 200.0));
        assert!(body.rest_position.is_some());
        let rest = body.rest_position.unwrap();
        assert!((rest.x - 100.0).abs() < f32::EPSILON);
        assert!((rest.y - 200.0).abs() < f32::EPSILON);
        body.clear_rest_position();
        assert!(body.rest_position.is_none());
    }

    #[test]
    fn test_kinematic_body() {
        let body = PhysicsBody::kinematic_body(
            BodyId::new(14),
            Point::new(100.0, 100.0),
            CollisionShape::from_rect(&Rect::new(0.0, 0.0, 100.0, 100.0)),
        );
        assert_eq!(body.body_type, BodyType::Kinematic);
        assert_eq!(body.inv_mass, 0.0);
    }

    #[test]
    fn test_dynamic_from_rect() {
        let body = PhysicsBody::dynamic_from_rect(
            BodyId::new(15),
            &Rect::new(10.0, 20.0, 40.0, 60.0),
            1.0,
        );
        assert!((body.position.x - 30.0).abs() < f32::EPSILON);
        assert!((body.position.y - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_tag_and_anchor() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(16),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        )
        .with_tag("icon-1")
        .with_anchor(Point::new(0.5, 0.5));
        assert_eq!(body.tag.as_deref(), Some("icon-1"));
        assert!((body.anchor.x - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_force_application() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(17),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        );
        body.apply_force(Vector2D::new(10.0, 0.0), 1.0);
        assert!((body.velocity.x - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_multiple_forces_are_not_reintegrated() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(19),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        );
        body.apply_force(Vector2D::new(10.0, 0.0), 0.5);
        body.apply_force(Vector2D::new(6.0, 0.0), 0.5);

        assert!((body.velocity.x - 8.0).abs() < f32::EPSILON);
        assert!((body.acceleration.x - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_should_sleep() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(18),
            Point::ORIGIN,
            CollisionShape::circle(5.0),
            1.0,
        );
        assert!(body.should_sleep(1.0)); // zero velocity should sleep

        let mut fast_body = body.clone();
        fast_body.velocity = Vector2D::new(100.0, 0.0);
        assert!(!fast_body.should_sleep(1.0));
    }
}
