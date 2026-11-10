//! Fixed timestep physics architecture.
//!
//! Decouples physics simulation from rendering frame rate so the simulation
//! runs at a constant rate (e.g., 120 Hz) regardless of frame rate variations.
//!
//! ## Why fixed timestep?
//!
//! Variable timestep physics produces different results depending on frame
//! rate, which makes behavior non-deterministic and hard to tune.  A fixed
//! timestep ensures:
//!
//! - **Deterministic results**: Same input always produces the same output.
//! - **Stable simulation**: Springs, collisions, and constraints behave
//!   consistently regardless of rendering performance.
//! - **Interpolated visuals**: Smooth rendering between simulation steps.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Application Event Loop                    │
//! │                                                              │
//! │  Input events / pointer movement                             │
//! │         ↓                                                    │
//! │  Physics accumulator  ←→  Fixed physics steps (120 Hz)       │
//! │         ↓                                                    │
//! │  Interpolate physical state                                  │
//! │         ↓                                                    │
//! │  wgpu rendering                                              │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## The Physics System Should
//!
//! 1. Receive interaction commands.
//! 2. Accumulate forces.
//! 3. Simulate at a fixed timestep.
//! 4. Resolve collisions and constraints.
//! 5. Produce updated positions and velocities.
//! 6. Expose interpolated visual transforms.
//!
//! ## Usage
//!
//! ```ignore
//! use wgpu_api::physics::{PhysicsConfig, FixedTimestepSimulator};
//!
//! let config = PhysicsConfig {
//!     fixed_dt: 1.0 / 120.0,  // 120 Hz
//!     max_substeps: 8,
//!     velocity_iterations: 3,
//!     position_iterations: 3,
//! };
//!
//! let mut sim = FixedTimestepSimulator::new(config);
//!
//! // Each frame:
//! let frame_dt = 1.0 / 60.0;  // variable frame time
//! sim.step(&mut world, frame_dt);
//!
//! // Query interpolated state for rendering
//! let state = sim.interpolated_state(&world, body_id, frame_dt);
//! ```

use std::collections::HashMap;

use super::body::BodyId;
use super::world::PhysicsWorld;
use crate::geometry::Point;

/// Configuration for the fixed timestep physics simulation.
///
/// Controls the simulation rate, substep granularity, and collision iterations.
#[derive(Debug, Clone, Copy)]
pub struct PhysicsConfig {
    /// Fixed simulation timestep in seconds.
    ///
    /// Typical values:
    /// - 1/60 (≈0.0167): Standard 60 Hz
    /// - 1/120 (≈0.0083): High precision 120 Hz (recommended for UI)
    /// - 1/240 (≈0.0042): Very high precision for complex simulations
    pub fixed_dt: f32,

    /// Maximum number of substeps per fixed step.
    ///
    /// When the accumulator falls behind (e.g., during lag spikes), the
    /// simulator will run at most this many substeps to catch up.
    /// Prevents the "spiral of death" where the simulation can never catch up.
    pub max_substeps: u32,

    /// Legacy velocity-iteration setting mirrored into the world config.
    ///
    /// The fixed-step simulator currently has no separate velocity solver, so
    /// this value does not repeat force application or change simulation output.
    pub velocity_iterations: u32,

    /// Number of position solver iterations per step.
    ///
    /// Position iterations correct constraint violations after velocity
    /// resolution.  For UI physics, 3 is usually sufficient.
    pub position_iterations: u32,
}

impl Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            fixed_dt: 1.0 / 120.0,
            max_substeps: 8,
            velocity_iterations: 3,
            position_iterations: 3,
        }
    }
}

impl PhysicsConfig {
    /// Create a configuration for 60 Hz simulation.
    pub fn hz_60() -> Self {
        Self {
            fixed_dt: 1.0 / 60.0,
            ..Default::default()
        }
    }

    /// Create a configuration for 120 Hz simulation (recommended for UI).
    pub fn hz_120() -> Self {
        Self {
            fixed_dt: 1.0 / 120.0,
            ..Default::default()
        }
    }

    /// Create a configuration for 240 Hz simulation (high precision).
    pub fn hz_240() -> Self {
        Self {
            fixed_dt: 1.0 / 240.0,
            ..Default::default()
        }
    }
}

/// Interpolated state of a body for smooth rendering.
///
/// Between fixed simulation steps, the visual state is interpolated between
/// the previous and current simulation state to produce smooth motion.
#[derive(Debug, Clone, Copy)]
pub struct InterpolatedState {
    /// Interpolated position for rendering.
    pub position: Point,
    /// Interpolated velocity for rendering.
    pub velocity: crate::geometry::Vector2D,
    /// Previous (uninterpolated) position, for reference.
    pub previous_position: Point,
    /// Current (uninterpolated) position, for reference.
    pub current_position: Point,
    /// Alpha value used for interpolation (0.0 = previous, 1.0 = current).
    pub alpha: f32,
}

impl InterpolatedState {
    /// Create a state with no interpolation (alpha = 1.0).
    pub fn exact(position: Point, velocity: crate::geometry::Vector2D) -> Self {
        Self {
            previous_position: position,
            current_position: position,
            position,
            velocity,
            alpha: 1.0,
        }
    }

    /// Interpolate between two positions using the given alpha.
    pub fn interpolate(
        prev_pos: Point,
        curr_pos: Point,
        prev_vel: crate::geometry::Vector2D,
        curr_vel: crate::geometry::Vector2D,
        alpha: f32,
    ) -> Self {
        let clamped_alpha = alpha.clamp(0.0, 1.0);
        Self {
            position: Point::new(
                prev_pos.x + (curr_pos.x - prev_pos.x) * clamped_alpha,
                prev_pos.y + (curr_pos.y - prev_pos.y) * clamped_alpha,
            ),
            velocity: crate::geometry::Vector2D::new(
                prev_vel.x + (curr_vel.x - prev_vel.x) * clamped_alpha,
                prev_vel.y + (curr_vel.y - prev_vel.y) * clamped_alpha,
            ),
            previous_position: prev_pos,
            current_position: curr_pos,
            alpha: clamped_alpha,
        }
    }
}

/// Stores the previous simulation state for interpolation.
#[derive(Debug, Clone)]
struct PreviousState {
    /// Position before the last simulation step.
    position: Point,
    /// Velocity before the last simulation step.
    velocity: crate::geometry::Vector2D,
}

/// Fixed timestep physics simulator.
///
/// Manages the accumulator-based simulation loop and provides interpolated
/// visual state between simulation steps.
///
/// ## Algorithm
///
/// Each frame:
/// 1. Add the elapsed time to the accumulator.
/// 2. While the accumulator >= fixed_dt, run a fixed simulation step.
/// 3. Compute interpolation alpha from the remaining accumulator.
/// 4. For rendering, interpolate between previous and current state.
pub struct FixedTimestepSimulator {
    /// Simulation configuration.
    pub config: PhysicsConfig,

    /// Time accumulator: collects elapsed time between frames.
    accumulator: f32,

    /// Previous state for each body, used for interpolation.
    previous_states: HashMap<BodyId, PreviousState>,

    /// Whether the simulator is active (paused when false).
    active: bool,

    /// Number of fixed steps executed in the last frame.
    steps_this_frame: u32,

    /// Total number of fixed steps executed since creation.
    total_steps: u64,
}

impl FixedTimestepSimulator {
    /// Create a new fixed timestep simulator with the given configuration.
    pub fn new(config: PhysicsConfig) -> Self {
        Self {
            config,
            accumulator: 0.0,
            previous_states: HashMap::new(),
            active: true,
            steps_this_frame: 0,
            total_steps: 0,
        }
    }

    /// Create a simulator with default configuration (120 Hz).
    pub fn default_config() -> Self {
        Self::new(PhysicsConfig::default())
    }

    /// Check if the simulator is active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Pause or resume the simulator.
    ///
    /// When paused, the accumulator is not updated and no simulation steps
    /// are executed.  Useful when the dock is idle.
    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    /// Get the number of fixed steps executed in the last frame.
    pub fn steps_last_frame(&self) -> u32 {
        self.steps_this_frame
    }

    /// Get the total number of fixed steps executed.
    pub fn total_steps_executed(&self) -> u64 {
        self.total_steps
    }

    /// Get the current accumulator value.
    pub fn accumulator_value(&self) -> f32 {
        self.accumulator
    }

    /// Save the current state of all bodies as the "previous" state.
    ///
    /// Called before each fixed simulation step so that interpolation can
    /// blend between the previous and current state.
    fn save_previous_states(&mut self, world: &PhysicsWorld) {
        self.previous_states
            .retain(|id, _| world.get_body(*id).is_some());
        for (id, body) in world.iter_bodies() {
            self.previous_states.insert(
                *id,
                PreviousState {
                    position: body.position,
                    velocity: body.velocity,
                },
            );
        }
    }

    /// Run a single fixed simulation step.
    fn run_fixed_step(&mut self, world: &mut PhysicsWorld) {
        // Save current state as previous for interpolation
        self.save_previous_states(world);

        // Accumulate forces once; solver iterations must not apply them again.
        world.apply_springs(self.config.fixed_dt);
        world.apply_fields(self.config.fixed_dt);
        world.apply_gravity(self.config.fixed_dt);

        // Resolve collisions
        for _ in 0..self.config.position_iterations {
            world.resolve_collisions();
        }

        // Clamp velocities and integrate
        world.clamp_velocities();
        world.integrate_all(self.config.fixed_dt);
        world.sleep_idle_bodies();

        self.total_steps += 1;
    }

    /// Advance the simulation by the given amount of time.
    ///
    /// This is the main entry point for the application event loop.
    /// It accumulates time and runs fixed simulation steps.
    ///
    /// # Arguments
    ///
    /// * `world` - The physics world to simulate.
    /// * `dt` - Elapsed time since the last frame (variable, e.g., 1/60).
    pub fn step(&mut self, world: &mut PhysicsWorld, dt: f32) {
        if !self.active {
            return;
        }

        self.steps_this_frame = 0;

        // Clamp dt to prevent spiral of death
        let max_dt = self.config.fixed_dt * self.config.max_substeps as f32;
        let clamped_dt = dt.min(max_dt);

        // Accumulate time
        self.accumulator += clamped_dt;

        // Run fixed steps while we have enough accumulated time
        let fixed_dt = self.config.fixed_dt;
        let mut steps = 0;

        while self.accumulator >= fixed_dt && steps < self.config.max_substeps {
            self.run_fixed_step(world);
            self.accumulator -= fixed_dt;
            steps += 1;
        }

        self.steps_this_frame = steps;
    }

    /// Get the interpolation alpha for smooth rendering.
    ///
    /// Alpha is the ratio of remaining accumulator time to the fixed timestep.
    /// 0.0 means the last step just completed, 1.0 means the next step is
    /// about to run.
    pub fn interpolation_alpha(&self) -> f32 {
        if self.config.fixed_dt <= 0.0 {
            return 1.0;
        }
        (self.accumulator / self.config.fixed_dt).clamp(0.0, 1.0)
    }

    /// Get the interpolated state of a body for smooth rendering.
    ///
    /// Blends between the previous and current simulation state using the
    /// accumulator's remaining time as the interpolation factor.
    pub fn interpolated_state(
        &self,
        world: &PhysicsWorld,
        id: BodyId,
    ) -> Option<InterpolatedState> {
        let current = world.get_body(id)?;
        let alpha = self.interpolation_alpha();

        // If no previous state, return exact current state
        let prev = match self.previous_states.get(&id) {
            Some(p) => p,
            None => {
                return Some(InterpolatedState::exact(current.position, current.velocity));
            }
        };

        Some(InterpolatedState::interpolate(
            prev.position,
            current.position,
            prev.velocity,
            current.velocity,
            alpha,
        ))
    }

    /// Get the interpolated state of all bodies for rendering.
    pub fn all_interpolated_states(
        &self,
        world: &PhysicsWorld,
    ) -> Vec<(BodyId, InterpolatedState)> {
        let mut states = Vec::new();
        for (id, _) in world.iter_bodies() {
            if let Some(state) = self.interpolated_state(world, *id) {
                states.push((*id, state));
            }
        }
        states
    }

    /// Reset the simulator state.
    ///
    /// Clears the accumulator and previous states.  Call this when the
    /// world is cleared or when restarting the simulation.
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
        self.previous_states.clear();
        self.steps_this_frame = 0;
    }

    /// Apply a force to a body (delegates to PhysicsWorld).
    pub fn apply_force(
        &mut self,
        world: &mut PhysicsWorld,
        id: BodyId,
        force: crate::geometry::Vector2D,
    ) {
        if let Some(body) = world.get_body_mut(id) {
            body.apply_force(force, self.config.fixed_dt);
        }
    }

    /// Apply an impulse to a body (delegates to PhysicsWorld).
    pub fn apply_impulse(
        &mut self,
        world: &mut PhysicsWorld,
        id: BodyId,
        impulse: crate::geometry::Vector2D,
    ) {
        if let Some(body) = world.get_body_mut(id) {
            body.apply_impulse(impulse);
        }
    }

    /// Set the velocity of a body directly (delegates to PhysicsWorld).
    pub fn set_velocity(
        &mut self,
        world: &mut PhysicsWorld,
        id: BodyId,
        velocity: crate::geometry::Vector2D,
    ) {
        if let Some(body) = world.get_body_mut(id) {
            body.velocity = velocity;
        }
    }

    /// Get the motion state of a body (delegates to PhysicsWorld).
    ///
    /// Returns the current (non-interpolated) state.  For interpolated
    /// state, use [`interpolated_state`](Self::interpolated_state).
    pub fn state(
        &self,
        world: &PhysicsWorld,
        id: BodyId,
    ) -> Option<super::visual_effects::MotionState> {
        let body = world.get_body(id)?;
        Some(super::visual_effects::MotionState::from_body(body))
    }

    /// Configure the physics world for fixed timestep simulation.
    ///
    /// Sets the world's timestep and substep limit to match this simulator.
    /// The solver-iteration value is mirrored for compatibility; the fixed-step
    /// simulator does not currently run a separate velocity solver.
    pub fn configure_world(&self, world: &mut PhysicsWorld) {
        world.config.timestep = self.config.fixed_dt;
        world.config.max_substeps = self.config.max_substeps;
        world.config.solver_iterations = self.config.velocity_iterations;
    }
}

impl Default for FixedTimestepSimulator {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::body::{CollisionShape, PhysicsBody};

    fn make_test_body(id: u64, pos: Point) -> PhysicsBody {
        PhysicsBody::dynamic_body(BodyId::new(id), pos, CollisionShape::circle(10.0), 1.0)
    }

    #[test]
    fn test_default_config_is_120hz() {
        let config = PhysicsConfig::default();
        assert!((config.fixed_dt - 1.0 / 120.0).abs() < f32::EPSILON);
        assert_eq!(config.max_substeps, 8);
        assert_eq!(config.velocity_iterations, 3);
        assert_eq!(config.position_iterations, 3);
    }

    #[test]
    fn test_config_presets() {
        let c60 = PhysicsConfig::hz_60();
        assert!((c60.fixed_dt - 1.0 / 60.0).abs() < f32::EPSILON);

        let c120 = PhysicsConfig::hz_120();
        assert!((c120.fixed_dt - 1.0 / 120.0).abs() < f32::EPSILON);

        let c240 = PhysicsConfig::hz_240();
        assert!((c240.fixed_dt - 1.0 / 240.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_simulator_starts_inactive_accumulator() {
        let sim = FixedTimestepSimulator::default_config();
        assert_eq!(sim.accumulator_value(), 0.0);
        assert!(sim.is_active());
        assert_eq!(sim.steps_last_frame(), 0);
    }

    #[test]
    fn test_single_fixed_step() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let mut body = make_test_body(1, Point::new(0.0, 0.0));
        body.velocity = crate::geometry::Vector2D::new(100.0, 0.0);
        world.add_body(body);

        // One frame at 1/60 should produce exactly one fixed step
        sim.step(&mut world, 1.0 / 60.0);

        assert_eq!(sim.steps_last_frame(), 1);
        assert_eq!(sim.total_steps_executed(), 1);

        // Body should have moved
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);
    }

    #[test]
    fn test_multiple_fixed_steps_per_frame() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 120.0,
            max_substeps: 8,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        // A 1/60 frame at 120 Hz fixed should produce 2 fixed steps
        sim.step(&mut world, 1.0 / 60.0);

        assert_eq!(sim.steps_last_frame(), 2);
        assert_eq!(sim.total_steps_executed(), 2);
    }

    #[test]
    fn test_accumulator_carries_over() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 8,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        // Two small frames that together exceed one fixed step
        sim.step(&mut world, 1.0 / 120.0);
        assert_eq!(sim.steps_last_frame(), 0);

        sim.step(&mut world, 1.0 / 120.0);
        assert_eq!(sim.steps_last_frame(), 1);
        assert_eq!(sim.total_steps_executed(), 1);
    }

    #[test]
    fn test_max_substeps_prevents_spiral() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        // A huge dt should be clamped to max_substeps * fixed_dt
        sim.step(&mut world, 1.0);

        assert_eq!(sim.steps_last_frame(), 4);
        // Remaining accumulator should be > 0 (clamped)
        assert!(sim.accumulator_value() > 0.0);
    }

    #[test]
    fn test_paused_simulator_does_not_step() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let mut body = make_test_body(1, Point::new(0.0, 0.0));
        body.velocity = crate::geometry::Vector2D::new(100.0, 0.0);
        world.add_body(body);

        sim.set_active(false);
        sim.step(&mut world, 1.0 / 60.0);

        assert_eq!(sim.steps_last_frame(), 0);
        // Body should not have moved
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.position.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_interpolation_alpha() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        // Before any step, alpha should be 0
        assert!((sim.interpolation_alpha() - 0.0).abs() < 0.01);

        // After half a fixed step's worth of time
        sim.step(&mut world, 0.5 / 60.0);
        let alpha = sim.interpolation_alpha();
        assert!((alpha - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_interpolated_state() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let mut body = make_test_body(1, Point::new(0.0, 0.0));
        body.velocity = crate::geometry::Vector2D::new(60.0, 0.0);
        world.add_body(body);

        // One fixed step moves the body
        sim.step(&mut world, 1.0 / 60.0);

        // Body actually moved
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);

        // Interpolated state at alpha=0 returns the previous state (origin)
        let state = sim.interpolated_state(&world, BodyId::new(1)).unwrap();
        assert!((state.alpha - 0.0).abs() < 0.01); // Just completed a step
        assert!((state.position.x - 0.0).abs() < 0.01); // Previous position
        assert!(state.current_position.x > 0.0); // Current position moved
    }

    #[test]
    fn test_interpolated_state_smoothing() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let mut body = make_test_body(1, Point::new(0.0, 0.0));
        body.velocity = crate::geometry::Vector2D::new(60.0, 0.0);
        world.add_body(body);

        // Half a step — no fixed step runs, but we still get interpolated state
        sim.step(&mut world, 0.5 / 60.0);

        let state = sim.interpolated_state(&world, BodyId::new(1));
        // First frame has no previous state, so it returns exact
        assert!(state.is_some());
    }

    #[test]
    fn test_apply_force() {
        let mut sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        sim.apply_force(
            &mut world,
            BodyId::new(1),
            crate::geometry::Vector2D::new(100.0, 0.0),
        );

        let body = world.get_body(BodyId::new(1)).unwrap();
        // Force should have been applied as acceleration
        assert!(body.acceleration.x > 0.0);
    }

    #[test]
    fn test_removed_bodies_are_pruned_from_previous_states() {
        let mut sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();
        let id = world.add_body(make_test_body(1, Point::ORIGIN));

        sim.save_previous_states(&world);
        assert!(sim.previous_states.contains_key(&id));

        world.remove_body(id);
        sim.save_previous_states(&world);
        assert!(!sim.previous_states.contains_key(&id));
    }

    #[test]
    fn test_velocity_iterations_do_not_repeat_force_application() {
        let run_step = |velocity_iterations| {
            let mut sim = FixedTimestepSimulator::new(PhysicsConfig {
                fixed_dt: 0.01,
                max_substeps: 1,
                velocity_iterations,
                position_iterations: 1,
            });
            let mut world = PhysicsWorld::new();
            world.collisions_enabled = false;
            let mut body = make_test_body(1, Point::ORIGIN);
            body.set_rest_position(Point::new(10.0, 0.0));
            let id = world.add_body(body);

            sim.run_fixed_step(&mut world);
            world.get_body(id).unwrap().velocity.x
        };

        assert!((run_step(1) - run_step(3)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_impulse() {
        let mut sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        sim.apply_impulse(
            &mut world,
            BodyId::new(1),
            crate::geometry::Vector2D::new(50.0, 0.0),
        );

        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.velocity.x - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_set_velocity() {
        let mut sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        sim.set_velocity(
            &mut world,
            BodyId::new(1),
            crate::geometry::Vector2D::new(200.0, 100.0),
        );

        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.velocity.x - 200.0).abs() < f32::EPSILON);
        assert!((body.velocity.y - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_state_query() {
        let sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(100.0, 200.0));
        world.add_body(body);

        let state = sim.state(&world, BodyId::new(1)).unwrap();
        assert!((state.position.x - 100.0).abs() < f32::EPSILON);
        assert!((state.position.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_reset_clears_state() {
        let mut sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        let body = make_test_body(1, Point::new(0.0, 0.0));
        world.add_body(body);

        sim.step(&mut world, 1.0 / 60.0);
        assert!(sim.total_steps_executed() > 0);

        sim.reset();
        assert_eq!(sim.accumulator_value(), 0.0);
        assert_eq!(sim.steps_last_frame(), 0);
    }

    #[test]
    fn test_configure_world() {
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 120.0,
            max_substeps: 8,
            velocity_iterations: 5,
            position_iterations: 3,
        };
        let sim = FixedTimestepSimulator::new(config);

        let mut world = PhysicsWorld::new();
        sim.configure_world(&mut world);

        assert!((world.config.timestep - 1.0 / 120.0).abs() < f32::EPSILON);
        assert_eq!(world.config.max_substeps, 8);
        assert_eq!(world.config.solver_iterations, 5);
    }

    #[test]
    fn test_all_interpolated_states() {
        let sim = FixedTimestepSimulator::default_config();
        let mut world = PhysicsWorld::new();

        world.add_body(make_test_body(1, Point::new(0.0, 0.0)));
        world.add_body(make_test_body(2, Point::new(100.0, 0.0)));
        world.add_body(make_test_body(3, Point::new(200.0, 0.0)));

        let states = sim.all_interpolated_states(&world);
        assert_eq!(states.len(), 3);
    }

    #[test]
    fn test_deterministic_fixed_steps() {
        // Running the same sequence of steps should produce identical results
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 60.0,
            max_substeps: 4,
            velocity_iterations: 1,
            position_iterations: 1,
        };

        // Run 1
        let mut sim1 = FixedTimestepSimulator::new(config);
        let mut world1 = PhysicsWorld::new();
        let mut body1 = make_test_body(1, Point::new(0.0, 0.0));
        body1.velocity = crate::geometry::Vector2D::new(100.0, 50.0);
        world1.add_body(body1);

        for _ in 0..10 {
            sim1.step(&mut world1, 1.0 / 60.0);
        }
        let final_pos1 = world1.get_body(BodyId::new(1)).unwrap().position;

        // Run 2
        let mut sim2 = FixedTimestepSimulator::new(config);
        let mut world2 = PhysicsWorld::new();
        let mut body2 = make_test_body(1, Point::new(0.0, 0.0));
        body2.velocity = crate::geometry::Vector2D::new(100.0, 50.0);
        world2.add_body(body2);

        for _ in 0..10 {
            sim2.step(&mut world2, 1.0 / 60.0);
        }
        let final_pos2 = world2.get_body(BodyId::new(1)).unwrap().position;

        assert!((final_pos1.x - final_pos2.x).abs() < f32::EPSILON);
        assert!((final_pos1.y - final_pos2.y).abs() < f32::EPSILON);
    }

    #[test]
    fn test_interpolated_state_exact() {
        let state = InterpolatedState::exact(
            Point::new(100.0, 200.0),
            crate::geometry::Vector2D::new(10.0, 20.0),
        );
        assert!((state.position.x - 100.0).abs() < f32::EPSILON);
        assert!((state.alpha - 1.0).abs() < f32::EPSILON);
        assert_eq!(state.previous_position, state.current_position);
    }

    #[test]
    fn test_interpolated_state_interpolate() {
        let state = InterpolatedState::interpolate(
            Point::new(0.0, 0.0),
            Point::new(100.0, 100.0),
            crate::geometry::Vector2D::ZERO,
            crate::geometry::Vector2D::new(10.0, 10.0),
            0.5,
        );
        assert!((state.position.x - 50.0).abs() < f32::EPSILON);
        assert!((state.position.y - 50.0).abs() < f32::EPSILON);
        assert!((state.alpha - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_interpolation_clamps_alpha() {
        let state = InterpolatedState::interpolate(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            crate::geometry::Vector2D::ZERO,
            crate::geometry::Vector2D::new(10.0, 0.0),
            2.0, // Out of range
        );
        assert!((state.alpha - 1.0).abs() < f32::EPSILON);
        assert!((state.position.x - 100.0).abs() < f32::EPSILON);

        let state2 = InterpolatedState::interpolate(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            crate::geometry::Vector2D::ZERO,
            crate::geometry::Vector2D::new(10.0, 0.0),
            -1.0, // Out of range
        );
        assert!((state2.alpha - 0.0).abs() < f32::EPSILON);
        assert!((state2.position.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_variable_frame_rate_consistency() {
        // Simulate variable frame rates and verify the simulation stays stable
        let config = PhysicsConfig {
            fixed_dt: 1.0 / 120.0,
            max_substeps: 8,
            velocity_iterations: 1,
            position_iterations: 1,
        };
        let mut sim = FixedTimestepSimulator::new(config);
        let mut world = PhysicsWorld::new();

        let mut body = make_test_body(1, Point::new(0.0, 0.0));
        body.velocity = crate::geometry::Vector2D::new(60.0, 0.0);
        world.add_body(body);

        // Simulate variable frame rates: 30, 60, 120, 45, 90 Hz
        let frame_rates = [30.0, 60.0, 120.0, 45.0, 90.0];
        for &rate in &frame_rates {
            let dt = 1.0 / rate;
            sim.step(&mut world, dt);
        }

        // Body should have moved a reasonable distance
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);
        // Position should be finite and reasonable
        assert!(body.position.x.is_finite());
        assert!(body.position.x < 10000.0);
    }
}
