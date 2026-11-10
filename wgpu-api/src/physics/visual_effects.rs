//! Visual effects system for the physics engine.
//!
//! Translates physical state data into rendering properties so that the
//! graphical appearance responds to the physical state of objects.
//!
//! ## Core idea
//!
//! The physics engine provides data such as position, velocity, acceleration,
//! and collision intensity.  The visual system translates this into rendering
//! properties like squash/stretch, glow, shadow, and elastic deformation.
//!
//! ## Physical → Visual Mapping
//!
//! | Physical property    | Visual response                    |
//! |----------------------|------------------------------------|
//! | Velocity             | Directional stretch                |
//! | Acceleration         | Increasing scale or glow           |
//! | Collision intensity  | Brief squash                       |
//! | Spring tension       | Elastic deformation                |
//! | Attraction           | Subtle visual pull                 |
//! | Group cohesion       | Connecting lines or shared motion  |
//! | Separation           | Increasing visual spacing          |
//! | Dragging             | Shadow and elevation               |
//! | High angular velocity| Rotation                           |
//!
//! ## Squash and Stretch
//!
//! An icon moving horizontally stretches along its movement direction:
//!
//! ```text
//! s_x = 1 + α * |v|
//! s_y = 1 - β * |v|
//! ```
//!
//! Results are clamped to reasonable ranges so the icon doesn't become
//! excessively distorted.
//!
//! ## Collision Response
//!
//! When two icons collide:
//! 1. Briefly squash both.
//! 2. Add a small overshoot.
//! 3. Increase their separation.
//! 4. Apply a short-lived glow or highlight.
//!
//! Derive from collision intensity and use a short decay function.

use super::body::{BodyId, PhysicsBody};
use std::collections::HashMap;

use crate::geometry::{Point, Size, Vector2D};

// ---------------------------------------------------------------------------
// Motion state
// ---------------------------------------------------------------------------

/// Snapshot of a body's motion state for visual effects computation.
///
/// The physics engine provides this data each frame, and the visual system
/// translates it into rendering properties.
#[derive(Debug, Clone, Copy)]
pub struct MotionState {
    /// Current position.
    pub position: Point,
    /// Current linear velocity.
    pub velocity: Vector2D,
    /// Current linear acceleration.
    pub acceleration: Vector2D,
    /// Current angular velocity (radians/s).
    pub angular_velocity: f32,
    /// Collision contact intensity (0.0 = no contact, 1.0 = hard impact).
    pub contact_intensity: f32,
    /// Spring deformation amount (0.0 = at rest, 1.0 = fully stretched).
    pub deformation: f32,
}

impl Default for MotionState {
    fn default() -> Self {
        Self {
            position: Point::ORIGIN,
            velocity: Vector2D::ZERO,
            acceleration: Vector2D::ZERO,
            angular_velocity: 0.0,
            contact_intensity: 0.0,
            deformation: 0.0,
        }
    }
}

impl MotionState {
    /// Create a new motion state from a physics body.
    pub fn from_body(body: &PhysicsBody) -> Self {
        Self {
            position: body.position,
            velocity: body.velocity,
            acceleration: body.acceleration,
            angular_velocity: body.angular_velocity,
            contact_intensity: 0.0,
            deformation: 0.0,
        }
    }

    /// Compute the speed (magnitude of velocity).
    pub fn speed(&self) -> f32 {
        self.velocity.length()
    }

    /// Compute the acceleration magnitude.
    pub fn acceleration_magnitude(&self) -> f32 {
        self.acceleration.length()
    }

    /// Check if the body is approximately at rest.
    pub fn is_at_rest(&self, threshold: f32) -> bool {
        self.velocity.length() < threshold
            && self.acceleration.length() < threshold
            && self.angular_velocity.abs() < threshold
    }
}

// ---------------------------------------------------------------------------
// Visual transform
// ---------------------------------------------------------------------------

/// A 2D transform with scale, rotation, and translation for rendering.
#[derive(Debug, Clone, Copy)]
pub struct VisualTransform {
    /// Translation offset.
    pub translation: Point,
    /// Scale factors (1.0 = no scaling).
    pub scale: Size,
    /// Rotation angle in radians.
    pub rotation: f32,
}

impl VisualTransform {
    /// Create an identity transform.
    pub fn identity() -> Self {
        Self {
            translation: Point::ORIGIN,
            scale: Size::new(1.0, 1.0),
            rotation: 0.0,
        }
    }

    /// Apply this transform to a base position.
    pub fn apply_position(&self, base: Point) -> Point {
        let scaled_x = base.x * self.scale.width;
        let scaled_y = base.y * self.scale.height;
        let (sin, cos) = self.rotation.sin_cos();
        Point::new(
            scaled_x * cos - scaled_y * sin + self.translation.x,
            scaled_x * sin + scaled_y * cos + self.translation.y,
        )
    }

    /// Apply `self` followed by `other`.
    ///
    /// Translation composition includes rotation. The compact transform
    /// representation cannot encode shear, so composing rotated non-uniform
    /// scales is an approximation.
    pub fn compose(&self, other: &VisualTransform) -> Self {
        let scaled_x = self.translation.x * other.scale.width;
        let scaled_y = self.translation.y * other.scale.height;
        let (sin, cos) = other.rotation.sin_cos();
        Self {
            translation: Point::new(
                scaled_x * cos - scaled_y * sin + other.translation.x,
                scaled_x * sin + scaled_y * cos + other.translation.y,
            ),
            scale: Size::new(
                self.scale.width * other.scale.width,
                self.scale.height * other.scale.height,
            ),
            rotation: self.rotation + other.rotation,
        }
    }
}

// ---------------------------------------------------------------------------
// Visual effects configuration
// ---------------------------------------------------------------------------

/// Configuration for visual effects parameters.
#[derive(Debug, Clone, Copy)]
pub struct VisualEffectsConfig {
    /// Squash and stretch coefficient for velocity-based stretch.
    /// Controls how much an object stretches along its movement direction.
    pub stretch_coefficient: f32,
    /// Squash coefficient perpendicular to movement.
    pub squash_coefficient: f32,
    /// Maximum stretch factor (clamps the scale).
    pub max_stretch: f32,
    /// Maximum squash factor (clamps the scale).
    pub max_squash: f32,
    /// Collision squash intensity multiplier.
    pub collision_squash: f32,
    /// Collision squash decay rate (per second).
    pub collision_decay: f32,
    /// Glow intensity multiplier for acceleration.
    pub glow_intensity: f32,
    /// Maximum glow alpha.
    pub max_glow_alpha: f32,
    /// Shadow elevation multiplier for dragged objects.
    pub shadow_elevation: f32,
    /// Angular rotation multiplier.
    pub angular_rotation: f32,
    /// Elastic overshoot factor for collision response.
    pub elastic_overshoot: f32,
    /// Deformation visual multiplier for spring tension.
    pub deformation_multiplier: f32,
}

impl Default for VisualEffectsConfig {
    fn default() -> Self {
        Self {
            stretch_coefficient: 0.002,
            squash_coefficient: 0.001,
            max_stretch: 1.3,
            max_squash: 0.7,
            collision_squash: 0.15,
            collision_decay: 5.0,
            glow_intensity: 0.001,
            max_glow_alpha: 0.5,
            shadow_elevation: 0.02,
            angular_rotation: 1.0,
            elastic_overshoot: 0.1,
            deformation_multiplier: 0.2,
        }
    }
}

// ---------------------------------------------------------------------------
// Collision effect state
// ---------------------------------------------------------------------------

/// Tracks the visual effect of a recent collision.
#[derive(Debug, Clone, Copy)]
pub struct CollisionEffect {
    /// Remaining intensity of the collision effect.
    pub intensity: f32,
    /// Whether the effect is active.
    pub active: bool,
    /// Squash direction (normalized).
    pub squash_direction: Vector2D,
}

impl CollisionEffect {
    /// Create a new collision effect.
    pub fn new(intensity: f32, direction: Vector2D) -> Self {
        Self {
            intensity: intensity.clamp(0.0, 1.0),
            active: true,
            squash_direction: if direction.length() > 0.001 {
                Vector2D::new(
                    direction.x / direction.length(),
                    direction.y / direction.length(),
                )
            } else {
                Vector2D::new(1.0, 0.0)
            },
        }
    }

    /// Decay the effect over time.
    pub fn decay(&mut self, dt: f32, decay_rate: f32) {
        if !self.active {
            return;
        }
        self.intensity *= (-decay_rate.max(0.0) * dt.max(0.0)).exp();
        if self.intensity < 0.01 {
            self.active = false;
            self.intensity = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// Drag visual state
// ---------------------------------------------------------------------------

/// Visual state for a dragged object.
#[derive(Debug, Clone, Copy)]
pub struct DragVisualState {
    /// Whether the object is being dragged.
    pub is_dragging: bool,
    /// Shadow elevation (0.0 = no shadow, 1.0 = maximum).
    pub shadow_elevation: f32,
    /// Scale increase while dragging.
    pub scale_increase: f32,
    /// Glow alpha while dragging.
    pub glow_alpha: f32,
}

impl DragVisualState {
    /// Create a new drag visual state.
    pub fn new() -> Self {
        Self {
            is_dragging: false,
            shadow_elevation: 0.0,
            scale_increase: 0.0,
            glow_alpha: 0.0,
        }
    }

    /// Activate dragging visual effects.
    pub fn start_dragging(&mut self) {
        self.is_dragging = true;
    }

    /// Deactivate dragging visual effects.
    pub fn stop_dragging(&mut self) {
        self.is_dragging = false;
        self.shadow_elevation = 0.0;
        self.scale_increase = 0.0;
        self.glow_alpha = 0.0;
    }

    /// Update drag visual effects based on velocity.
    pub fn update(&mut self, velocity: Vector2D, config: &VisualEffectsConfig) {
        if !self.is_dragging {
            return;
        }
        let speed = velocity.length();
        self.shadow_elevation = (speed * config.shadow_elevation).min(1.0);
        self.scale_increase = (speed * config.stretch_coefficient * 0.5).min(0.1);
        self.glow_alpha = (speed * config.glow_intensity * 10.0).min(config.max_glow_alpha);
    }
}

// ---------------------------------------------------------------------------
// Visual effects system
// ---------------------------------------------------------------------------

/// The visual effects system translates physical state into rendering properties.
///
/// For each body, it computes:
/// - Squash and stretch based on velocity
/// - Collision response (squash, glow)
/// - Elastic deformation from spring tension
/// - Drag effects (shadow, elevation)
/// - Rotation from angular velocity
pub struct VisualEffects {
    /// Configuration parameters.
    pub config: VisualEffectsConfig,
    /// Per-body collision effects.
    collision_effects: HashMap<BodyId, CollisionEffect>,
    /// Per-body drag visual state.
    drag_states: HashMap<BodyId, DragVisualState>,
}

impl VisualEffects {
    /// Create a new visual effects system with default configuration.
    pub fn new() -> Self {
        Self {
            config: VisualEffectsConfig::default(),
            collision_effects: HashMap::new(),
            drag_states: HashMap::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: VisualEffectsConfig) -> Self {
        Self {
            config,
            collision_effects: HashMap::new(),
            drag_states: HashMap::new(),
        }
    }

    // ---------------------------------------------------------------------
    // Squash and stretch
    // ---------------------------------------------------------------------

    /// Compute squash and stretch transform from velocity.
    ///
    /// An icon moving horizontally stretches along its movement direction:
    ///
    /// ```text
    /// s_x = 1 + α * |v_x|
    /// s_y = 1 - β * |v_x|
    /// ```
    ///
    /// Results are clamped to reasonable ranges.
    pub fn compute_squash_stretch(&self, motion: &MotionState) -> VisualTransform {
        let speed = motion.speed();
        if speed < 0.01 {
            return VisualTransform::identity();
        }

        // Direction of movement
        let dir_x = motion.velocity.x / speed;
        let dir_y = motion.velocity.y / speed;

        // Stretch along movement direction
        let stretch =
            (1.0 + speed * self.config.stretch_coefficient).clamp(1.0, self.config.max_stretch);

        // Squash perpendicular to movement
        let squash =
            (1.0 - speed * self.config.squash_coefficient).clamp(self.config.max_squash, 1.0);

        // Compute scale in the movement direction
        // For horizontal movement: stretch X, squash Y
        // For vertical movement: squash X, stretch Y
        // For diagonal: blend based on direction
        let abs_dir_x = dir_x.abs();
        let abs_dir_y = dir_y.abs();

        let scale_x;
        let scale_y;

        if abs_dir_x > abs_dir_y {
            // Primarily horizontal
            scale_x = stretch;
            scale_y = squash;
        } else {
            // Primarily vertical
            scale_x = squash;
            scale_y = stretch;
        }

        VisualTransform {
            translation: Point::ORIGIN,
            scale: Size::new(scale_x, scale_y),
            rotation: 0.0,
        }
    }

    /// Compute squash and stretch from acceleration.
    ///
    /// Acceleration can indicate the start of movement, so we use it to
    /// create a more responsive visual effect.
    pub fn compute_acceleration_squash(&self, motion: &MotionState) -> VisualTransform {
        let accel = motion.acceleration_magnitude();
        if accel < 0.01 {
            return VisualTransform::identity();
        }

        let stretch = (1.0 + accel * self.config.stretch_coefficient * 0.5)
            .clamp(1.0, self.config.max_stretch);
        let squash =
            (1.0 - accel * self.config.squash_coefficient * 0.5).clamp(self.config.max_squash, 1.0);

        VisualTransform {
            translation: Point::ORIGIN,
            scale: Size::new(stretch, squash),
            rotation: 0.0,
        }
    }

    // ---------------------------------------------------------------------
    // Collision response
    // ---------------------------------------------------------------------

    /// Register a collision event for visual effects.
    pub fn register_collision(&mut self, body: BodyId, intensity: f32, direction: Vector2D) {
        self.collision_effects
            .insert(body, CollisionEffect::new(intensity, direction));
    }

    /// Compute the collision squash transform for a body.
    pub fn compute_collision_squash(&self, body: BodyId, dt: f32) -> VisualTransform {
        let Some(effect) = self.collision_effects.get(&body) else {
            return VisualTransform::identity();
        };

        if !effect.active || effect.intensity < 0.01 {
            return VisualTransform::identity();
        }

        let squash_amount = effect.intensity * self.config.collision_squash;

        // Squash along the collision normal
        let dir = effect.squash_direction;
        let abs_dir_x = dir.x.abs();
        let abs_dir_y = dir.y.abs();

        let scale_x;
        let scale_y;

        if abs_dir_x > abs_dir_y {
            // Horizontal collision: squash X, stretch Y
            scale_x = 1.0 - squash_amount;
            scale_y = 1.0 + squash_amount * self.config.elastic_overshoot;
        } else {
            // Vertical collision: squash Y, stretch X
            scale_x = 1.0 + squash_amount * self.config.elastic_overshoot;
            scale_y = 1.0 - squash_amount;
        }

        VisualTransform {
            translation: Point::ORIGIN,
            scale: Size::new(scale_x.max(0.5), scale_y.max(0.5)),
            rotation: 0.0,
        }
    }

    /// Get the glow alpha for a body based on collision intensity.
    pub fn get_collision_glow(&self, body: BodyId) -> f32 {
        let Some(effect) = self.collision_effects.get(&body) else {
            return 0.0;
        };

        if !effect.active {
            return 0.0;
        }

        effect.intensity * self.config.max_glow_alpha
    }

    /// Decay all collision effects.
    pub fn decay_collision_effects(&mut self, dt: f32) {
        for effect in self.collision_effects.values_mut() {
            effect.decay(dt, self.config.collision_decay);
        }
        self.collision_effects.retain(|_, effect| effect.active);
    }

    // ---------------------------------------------------------------------
    // Elastic deformation
    // ---------------------------------------------------------------------

    /// Compute elastic deformation from spring tension.
    ///
    /// When a body is under spring tension, it deforms elastically.
    /// The deformation is proportional to the spring displacement.
    pub fn compute_elastic_deformation(&self, motion: &MotionState) -> VisualTransform {
        let deformation = motion.deformation;
        if deformation < 0.01 {
            return VisualTransform::identity();
        }

        let amount = deformation * self.config.deformation_multiplier;

        // Elastic deformation: stretch in one direction, squash in the other
        let stretch = (1.0 + amount).clamp(1.0, self.config.max_stretch);
        let squash = (1.0 - amount * 0.5).clamp(self.config.max_squash, 1.0);

        VisualTransform {
            translation: Point::ORIGIN,
            scale: Size::new(stretch, squash),
            rotation: 0.0,
        }
    }

    // ---------------------------------------------------------------------
    // Rotation
    // ---------------------------------------------------------------------

    /// Compute rotation transform from angular velocity.
    pub fn compute_rotation(&self, motion: &MotionState) -> VisualTransform {
        let rotation = motion.angular_velocity * self.config.angular_rotation;
        if rotation.abs() < 0.001 {
            return VisualTransform::identity();
        }

        VisualTransform {
            translation: Point::ORIGIN,
            scale: Size::new(1.0, 1.0),
            rotation,
        }
    }

    // ---------------------------------------------------------------------
    // Drag effects
    // ---------------------------------------------------------------------

    /// Set a body as being dragged.
    pub fn set_dragging(&mut self, body: BodyId, is_dragging: bool) {
        if is_dragging {
            if let Some(state) = self.drag_states.get_mut(&body) {
                state.start_dragging();
            } else {
                let mut state = DragVisualState::new();
                state.start_dragging();
                self.drag_states.insert(body, state);
            }
        } else {
            if let Some(state) = self.drag_states.get_mut(&body) {
                state.stop_dragging();
            }
            self.drag_states.remove(&body);
        }
    }

    /// Update drag visual effects for all bodies.
    pub fn update_drag_effects(&mut self, bodies: &[(BodyId, MotionState)]) {
        for (body, motion) in bodies {
            if let Some(state) = self.drag_states.get_mut(body) {
                state.update(motion.velocity, &self.config);
            }
        }
    }

    /// Get the drag visual state for a body.
    pub fn get_drag_state(&self, body: BodyId) -> Option<DragVisualState> {
        self.drag_states.get(&body).copied()
    }

    // ---------------------------------------------------------------------
    // Combined transform
    // ---------------------------------------------------------------------

    /// Compute the combined visual transform for a body.
    ///
    /// Combines squash/stretch, collision response, elastic deformation,
    /// rotation, and drag effects into a single transform.
    pub fn compute_combined_transform(
        &self,
        body: BodyId,
        motion: &MotionState,
        dt: f32,
    ) -> VisualTransform {
        let mut transform = VisualTransform::identity();

        // 1. Squash and stretch from velocity
        let squash_stretch = self.compute_squash_stretch(motion);
        transform = transform.compose(&squash_stretch);

        // 2. Collision response
        let collision = self.compute_collision_squash(body, dt);
        transform = transform.compose(&collision);

        // 3. Elastic deformation
        let elastic = self.compute_elastic_deformation(motion);
        transform = transform.compose(&elastic);

        // 4. Rotation
        let rotation = self.compute_rotation(motion);
        transform = transform.compose(&rotation);

        // 5. Drag scale increase
        if let Some(drag_state) = self.get_drag_state(body) {
            if drag_state.is_dragging {
                let drag_scale = 1.0 + drag_state.scale_increase;
                transform = transform.compose(&VisualTransform {
                    translation: Point::ORIGIN,
                    scale: Size::new(drag_scale, drag_scale),
                    rotation: 0.0,
                });
            }
        }

        transform
    }

    /// Compute the glow alpha for a body from all sources.
    pub fn compute_glow_alpha(&self, body: BodyId, motion: &MotionState) -> f32 {
        let mut alpha = 0.0;

        // Collision glow
        alpha += self.get_collision_glow(body);

        // Acceleration glow
        let accel_glow = (motion.acceleration_magnitude() * self.config.glow_intensity)
            .min(self.config.max_glow_alpha);
        alpha += accel_glow;

        // Drag glow
        if let Some(drag_state) = self.get_drag_state(body) {
            alpha += drag_state.glow_alpha;
        }

        alpha.min(self.config.max_glow_alpha)
    }

    /// Compute the shadow elevation for a body.
    pub fn compute_shadow_elevation(&self, body: BodyId) -> f32 {
        if let Some(drag_state) = self.get_drag_state(body) {
            return drag_state.shadow_elevation;
        }
        0.0
    }

    /// Update all visual effects for one frame.
    pub fn update(&mut self, dt: f32) {
        self.decay_collision_effects(dt);
    }

    /// Clear all effects.
    pub fn clear(&mut self) {
        self.collision_effects.clear();
        self.drag_states.clear();
    }
}

impl Default for VisualEffects {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Group visual effects
// ---------------------------------------------------------------------------

/// Visual effects for a group of bodies.
///
/// When bodies are in a group, they can share visual effects like
/// connecting lines, shared motion blur, or collective glow.
#[derive(Debug, Clone, Copy)]
pub struct GroupVisualEffects {
    /// Whether connecting lines between group members are visible.
    pub show_connecting_lines: bool,
    /// Line alpha (0.0 = invisible, 1.0 = fully visible).
    pub line_alpha: f32,
    /// Shared motion blur intensity.
    pub shared_motion_blur: f32,
    /// Collective glow alpha.
    pub collective_glow: f32,
}

impl GroupVisualEffects {
    /// Create new group visual effects.
    pub fn new() -> Self {
        Self {
            show_connecting_lines: false,
            line_alpha: 0.0,
            shared_motion_blur: 0.0,
            collective_glow: 0.0,
        }
    }

    /// Compute group visual effects from member motion states.
    pub fn from_motion_states(states: &[MotionState]) -> Self {
        if states.is_empty() {
            return Self::new();
        }

        let mut avg_speed = 0.0;
        let mut max_deformation: f32 = 0.0;

        for state in states {
            avg_speed += state.speed();
            max_deformation = max_deformation.max(state.deformation);
        }

        avg_speed /= states.len() as f32;

        Self {
            show_connecting_lines: avg_speed > 200.0 || max_deformation > 0.4,
            line_alpha: (avg_speed / 500.0).min(0.3),
            shared_motion_blur: (avg_speed / 200.0).min(0.5),
            collective_glow: (max_deformation * 0.5_f32).min(0.3_f32),
        }
    }
}

impl Default for GroupVisualEffects {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_motion_state_from_body() {
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            super::super::body::CollisionShape::circle(10.0),
            1.0,
        );
        let motion = MotionState::from_body(&body);
        assert!((motion.position.x - 100.0).abs() < f32::EPSILON);
        assert!(motion.is_at_rest(0.01));
    }

    #[test]
    fn test_motion_state_speed() {
        let mut motion = MotionState::default();
        motion.velocity = Vector2D::new(3.0, 4.0);
        assert!((motion.speed() - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_squash_stretch_identity_at_rest() {
        let effects = VisualEffects::new();
        let motion = MotionState::default();
        let transform = effects.compute_squash_stretch(&motion);
        assert!((transform.scale.width - 1.0).abs() < f32::EPSILON);
        assert!((transform.scale.height - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_squash_stretch_horizontal() {
        let effects = VisualEffects::new();
        let mut motion = MotionState::default();
        motion.velocity = Vector2D::new(500.0, 0.0);
        let transform = effects.compute_squash_stretch(&motion);
        assert!(transform.scale.width > 1.0);
        assert!(transform.scale.height < 1.0);
        assert!(transform.scale.width <= effects.config.max_stretch);
        assert!(transform.scale.height >= effects.config.max_squash);
    }

    #[test]
    fn test_squash_stretch_vertical() {
        let effects = VisualEffects::new();
        let mut motion = MotionState::default();
        motion.velocity = Vector2D::new(0.0, 500.0);
        let transform = effects.compute_squash_stretch(&motion);
        assert!(transform.scale.width < 1.0);
        assert!(transform.scale.height > 1.0);
    }

    #[test]
    fn test_collision_effect_decay() {
        let mut effect = CollisionEffect::new(1.0, Vector2D::new(1.0, 0.0));
        assert!(effect.active);
        effect.decay(0.1, 5.0);
        assert!(effect.intensity < 1.0);
        assert!(effect.intensity > 0.0);
        effect.decay(1.0, 5.0);
        assert!(!effect.active);
    }

    #[test]
    fn test_collision_squash() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        effects.register_collision(body, 1.0, Vector2D::new(1.0, 0.0));
        let transform = effects.compute_collision_squash(body, 0.0);
        assert!(transform.scale.width < 1.0);
        assert!(transform.scale.height > 1.0);
    }

    #[test]
    fn test_collision_glow() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        effects.register_collision(body, 1.0, Vector2D::new(1.0, 0.0));
        let glow = effects.get_collision_glow(body);
        assert!(glow > 0.0);
        assert!(glow <= effects.config.max_glow_alpha);
    }

    #[test]
    fn test_elastic_deformation() {
        let effects = VisualEffects::new();
        let mut motion = MotionState::default();
        motion.deformation = 0.5;
        let transform = effects.compute_elastic_deformation(&motion);
        assert!(transform.scale.width > 1.0);
        assert!(transform.scale.height < 1.0);
    }

    #[test]
    fn test_rotation() {
        let effects = VisualEffects::new();
        let mut motion = MotionState::default();
        motion.angular_velocity = 1.0;
        let transform = effects.compute_rotation(&motion);
        assert!((transform.rotation - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_drag_visual_state() {
        let mut state = DragVisualState::new();
        assert!(!state.is_dragging);
        state.start_dragging();
        assert!(state.is_dragging);
        state.update(Vector2D::new(100.0, 0.0), &VisualEffectsConfig::default());
        assert!(state.shadow_elevation > 0.0);
        state.stop_dragging();
        assert!(!state.is_dragging);
    }

    #[test]
    fn test_combined_transform() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        let mut motion = MotionState::default();
        motion.velocity = Vector2D::new(200.0, 0.0);
        motion.deformation = 0.3;
        motion.angular_velocity = 0.5;
        effects.register_collision(body, 0.5, Vector2D::new(1.0, 0.0));
        let transform = effects.compute_combined_transform(body, &motion, 0.0);
        assert!(transform.scale.width != 1.0 || transform.scale.height != 1.0);
        assert!((transform.rotation - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_visual_transform_compose() {
        let t1 = VisualTransform {
            translation: Point::new(10.0, 20.0),
            scale: Size::new(2.0, 2.0),
            rotation: 0.5,
        };
        let t2 = VisualTransform {
            translation: Point::new(5.0, 5.0),
            scale: Size::new(1.5, 1.5),
            rotation: 0.3,
        };
        let composed = t1.compose(&t2);
        assert!((composed.scale.width - 3.0).abs() < f32::EPSILON);
        assert!((composed.rotation - 0.8).abs() < f32::EPSILON);
        assert!(
            (composed.translation.x
                - (10.0 * 1.5 * 0.3_f32.cos() - 20.0 * 1.5 * 0.3_f32.sin() + 5.0))
                .abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn test_apply_position_applies_rotation_and_scale_about_origin() {
        let transform = VisualTransform {
            translation: Point::new(5.0, 7.0),
            scale: Size::new(2.0, 1.0),
            rotation: std::f32::consts::FRAC_PI_2,
        };
        let result = transform.apply_position(Point::new(3.0, 4.0));
        assert!((result.x - 1.0).abs() < 0.0001);
        assert!((result.y - 13.0).abs() < 0.0001);
    }

    #[test]
    fn test_motion_rest_threshold_uses_magnitude() {
        let mut motion = MotionState::default();
        motion.velocity = Vector2D::new(0.08, 0.0);
        motion.angular_velocity = 0.08;
        assert!(motion.is_at_rest(0.1));
    }

    #[test]
    fn test_group_visual_effects() {
        let mut states = vec![MotionState::default(), MotionState::default()];
        states[0].velocity = Vector2D::new(100.0, 0.0);
        states[1].velocity = Vector2D::new(100.0, 0.0);
        let effects = GroupVisualEffects::from_motion_states(&states);
        assert!(!effects.show_connecting_lines);
        assert!(effects.shared_motion_blur > 0.0);
    }

    #[test]
    fn test_group_visual_effects_with_deformation() {
        let mut states = vec![MotionState::default()];
        states[0].deformation = 0.5;
        let effects = GroupVisualEffects::from_motion_states(&states);
        assert!(effects.show_connecting_lines);
        assert!(effects.collective_glow > 0.0);
    }

    #[test]
    fn test_acceleration_squash() {
        let effects = VisualEffects::new();
        let mut motion = MotionState::default();
        motion.acceleration = Vector2D::new(1000.0, 0.0);
        let transform = effects.compute_acceleration_squash(&motion);
        assert!(transform.scale.width > 1.0);
        assert!(transform.scale.height < 1.0);
    }

    #[test]
    fn test_glow_alpha_combined() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        effects.register_collision(body, 0.5, Vector2D::new(1.0, 0.0));
        let mut motion = MotionState::default();
        motion.acceleration = Vector2D::new(500.0, 0.0);
        let glow = effects.compute_glow_alpha(body, &motion);
        assert!(glow > 0.0);
        assert!(glow <= effects.config.max_glow_alpha);
    }

    #[test]
    fn test_shadow_elevation() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        effects.set_dragging(body, true);
        effects.update_drag_effects(&[(body, MotionState::default())]);
        let elevation = effects.compute_shadow_elevation(body);
        assert!(elevation >= 0.0);
    }

    #[test]
    fn test_update_decay() {
        let mut effects = VisualEffects::new();
        let body = BodyId::new(1);
        effects.register_collision(body, 1.0, Vector2D::new(1.0, 0.0));
        effects.update(0.1);
        let glow = effects.get_collision_glow(body);
        assert!(glow < effects.config.max_glow_alpha);
    }

    #[test]
    fn test_clear_effects() {
        let mut effects = VisualEffects::new();
        effects.register_collision(BodyId::new(1), 1.0, Vector2D::new(1.0, 0.0));
        effects.set_dragging(BodyId::new(2), true);
        effects.clear();
        assert!(effects.collision_effects.is_empty());
        assert!(effects.drag_states.is_empty());
    }
}
