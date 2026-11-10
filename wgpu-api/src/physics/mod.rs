//! # Physics Engine
//!
//! A modular 2D physics system with three distinct subsystems:
//!
//! ## A. Spring-based UI Physics (`springs`, `forces::SpringForce`)
//!
//! Damped springs for natural UI behavior:
//! - Dock icons returning to preferred positions
//! - Smooth dragging and magnetic snapping
//! - Elastic resizing and window expansion
//! - Pulling and stretching groups
//!
//! Formula: `F = -k(x - x_target) - c·v`
//!
//! ## B. Collision Detection (`collision`, `solver`, `body`)
//!
//! Lightweight collision detection for dock icon separation:
//! - AABB and circle collision detection
//! - Simple impulse-based resolution
//! - No rotating bodies, joints, or CCD
//!
//! Per ADR 005, general-purpose physics engines (Rapier2D) are not used.
//! Analytical springs and simple collision detection are sufficient for dock UI.
//!
//! ## C. Field-based Interaction Physics (`fields`)
//!
//! Expressive force fields for group-level behavior:
//! - Attraction and repulsion fields
//! - Magnetic fields centered at interaction points
//! - Group cohesion and separation
//! - Radial pushing and directional pulling
//!
//! Formula (magnetic field): `F_i = k · (p - x_i) / (‖p - x_i‖ + ε)^n`
//!
//! ## Interaction Command System (`commands`)
//!
//! A first-class command API that decouples application input from physics:
//! - `InteractionCommand` enum for all interaction types
//! - `InteractionManager` for command dispatch and state tracking
//! - Keyboard, touch, gesture, and scripted interaction support
//! - Command history and replay for debugging
//!
//! ## Fixed Timestep Architecture (`timestep`)
//!
//! Decouples physics simulation from rendering frame rate:
//! - Fixed 120 Hz simulation (configurable)
//! - Interpolated visual state between steps
//! - Spiral-of-death prevention via max substeps
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Application Layer                         │
//! │                                                              │
//! │  Mouse / Keyboard / Touch / Gestures / Scripts               │
//! │            ↓                                                 │
//! │  InteractionCommand  →  InteractionManager                   │
//! │            ↓                                                 │
//! │  PhysicsWorld (springs + collisions + fields)                │
//! │            ↓                                                 │
//! │  FixedTimestepSimulator (120 Hz)                             │
//! │            ↓                                                 │
//! │  VisualEffects (squash/stretch/glow)                         │
//! │            ↓                                                 │
//! │  wgpu rendering                                              │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Libraries Considered (Issue 11)
//!
//! The current implementation uses a custom spring/field engine.  The
//! following libraries were evaluated for potential integration:
//!
//! ### Rapier2D (Not Used)
//!
//! Per ADR 005, Rapier2D is explicitly not used.  The dock's physics needs
//! are well-defined and simple: damped springs for icon positioning and
//! proximity fields for interaction effects.  A general-purpose physics
//! engine would increase CPU, RAM, and VRAM usage without benefit.
//!
//! ### impetus
//!
//! [impetus](https://crates.io/crates/impetus) is worth investigating for UI
//! physics.  It provides:
//! - Damped harmonic oscillators
//! - 1D, 2D and 3D springs
//! - Window animations and drag-and-release behavior
//!
//! Integration point: could enhance `springs.rs` with more sophisticated
//! spring models.
//!
//! ### Blinc
//!
//! [Blinc](https://github.com/kpreisser/blinc) is a wgpu-based reactive UI
//! framework with spring physics animations and GPU-rendered effects.
//! Worth examining for ideas, but not a replacement for the custom renderer.
//!
//! ## Usage
//!
//! ```ignore
//! use wgpu_api::physics::{
//!     PhysicsWorld, PhysicsConfig, FixedTimestepSimulator,
//!     InteractionManager, InteractionCommand,
//! };
//!
//! let mut world = PhysicsWorld::new();
//! let mut sim = FixedTimestepSimulator::default_config();
//! let mut manager = InteractionManager::new();
//!
//! // Send a command
//! manager.queue(InteractionCommand::BeginDrag {
//!     target: NodeId::new(1),
//!     pointer: Point::new(100.0, 100.0),
//! });
//! manager.process(&mut world);
//!
//! // Step physics at fixed timestep
//! sim.step(&mut world, 1.0 / 60.0);
//!
//! // Query interpolated state for rendering
//! let state = sim.interpolated_state(&world, body_id);
//! ```

// ---------------------------------------------------------------------------
// Submodule declarations
// ---------------------------------------------------------------------------

pub mod body;
pub mod collision;
pub mod constraints;
pub mod forces;
pub mod groups;
pub mod solver;
pub mod spatial_index;
pub mod springs;
pub mod fields;
pub mod interaction;
pub mod commands;
pub mod dock;
pub mod visual_effects;
pub mod timestep;
pub mod world;

// ---------------------------------------------------------------------------
// Re-exports for ergonomic imports
// ---------------------------------------------------------------------------

// Core body types
pub use body::{
    BodyId, BodyType, PhysicsBody, CollisionShape, DampingConfig, MaterialProperties,
    NodeId, PhysicalProperties, PhysicsObject,
};

// World and simulation
pub use world::{PhysicsWorld, PhysicsWorldConfig, PhysicsStats};

// Spring system (A)
pub use springs::{SpringConstraint, DistanceConstraint, SpringChain, RadialSpringSystem};
pub use forces::{
    SpringForce, DragForce, RepulsionForce, AttractionForce,
    WindForce, PushForce, BodyRepulsion, ForceAccumulator,
    apply_rest_springs, apply_mutual_repulsion,
};

// Rigid body system (B)
pub use collision::{Collision, CollisionPair, broad_phase, detect_collision, resolve_collision, aabb_vs_aabb, circle_vs_circle, aabb_vs_circle};
pub use solver::{PhysicsSolver, SolverConfig, SolverStats, ContactManifold, ContactPoint};
pub use constraints::{
    AnchorConstraint, HingeJoint, WeldJoint, PrismaticJoint,
    RegionConstraint, SliderConstraint, ConstraintSet,
    // New constraint types
    SpringConstraint as ConstraintSpring, FixedConstraint, DistanceConstraint as ConstraintsDistanceConstraint,
    AxisConstraint, PathConstraint, AlignmentConstraint, AlignmentAxis,
    SeparationConstraint, AttractionConstraint, GroupConstraint,
    Constraint, ConstraintSystem,
};

// Field system (C)
pub use fields::{
    ForceField, FieldType, FieldConfig, FieldSource,
    MagneticField, CohesionField, SeparationField,
    RadialField, DirectionalField,
};

// Groups and spatial indexing
pub use groups::{BodyGroup, GroupId, GroupConfig, GroupBehavior, GroupManager};
pub use spatial_index::{SpatialHash, SpatialHashConfig, SpatialHashStats, GridCell, CellKey};

// High-level interaction API
pub use interaction::{
    InteractionApi, PhysicsDragState, DragConfig, ProjectionAxis,
    InsertResult, CutResult, MergeResult,
};

// Physical dock layout model
pub use dock::{
    Dock, DockItem, DockConfig, DockAxis, DockDragState,
};

// Visual effects system
pub use visual_effects::{
    MotionState, VisualTransform, VisualEffectsConfig,
    CollisionEffect, DragVisualState, VisualEffects,
    GroupVisualEffects,
};

// Fixed timestep simulation
pub use timestep::{
    PhysicsConfig, FixedTimestepSimulator, InterpolatedState,
};

// Interaction command system
pub use commands::{
    InteractionCommand, InteractionManager, CommandResult,
    KeyboardKey, KeyboardModifiers, TouchPhase, PhysicsGestureType,
    ScriptParameter,
};

// Rapier2D removed per ADR 005: analytical springs are sufficient for dock physics.
