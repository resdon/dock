//! Interaction command system.
//!
//! Makes interaction commands a first-class API rather than having mouse
//! handlers directly manipulate physics.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │                    Application Layer                         │
//! │                                                              │
//! │  Mouse events  ─┐                                           │
//! │  Keyboard ───────┤                                           │
//! │  Touch ──────────┤                                           │
//! │  Gestures ───────┤                                           │
//! │  Scripted ───────┘                                           │
//! │            ↓                                                 │
//! │  InteractionCommand                                          │
//! │            ↓                                                 │
//! │  InteractionManager  (interprets commands)                   │
//! │            ↓                                                 │
//! │  PhysicsWorld / Layout / Dock  (execute operations)          │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Why Commands?
//!
//! 1. **Decoupling**: Application code sends commands without knowing about
//!    physics internals.
//! 2. **Extensibility**: Adding keyboard, touch, gesture, or scripted
//!    interactions requires only new command sources, not new physics code.
//! 3. **Replayability**: Commands can be recorded, logged, and replayed.
//! 4. **Testability**: Tests can send commands directly without simulating
//!    input devices.
//! 5. **Batching**: Multiple commands can be queued and processed together.
//!
//! ## Usage
//!
//! ```ignore
//! use wgpu_api::physics::{InteractionManager, InteractionCommand};
//!
//! let mut manager = InteractionManager::new();
//!
//! // Send a command
//! manager.queue(InteractionCommand::BeginDrag {
//!     target: NodeId::new(1),
//!     pointer: Point::new(100.0, 100.0),
//! });
//!
//! // Process all queued commands
//! manager.process(&mut world);
//! ```

use std::collections::VecDeque;

use super::body::{BodyId, CollisionShape};
use super::groups::GroupId;
use super::interaction::{InteractionApi, ProjectionAxis};
use super::world::PhysicsWorld;
use crate::geometry::{NodeId, Point, Rect, Vector2D};

// ---------------------------------------------------------------------------
// Interaction Command Enum
// ---------------------------------------------------------------------------

/// A high-level interaction command that describes user intent.
///
/// Commands are the primary interface between application code and the
/// physics/layout systems.  Each command is interpreted by the
/// [`InteractionManager`] and translated into physics operations.
#[derive(Debug, Clone)]
pub enum InteractionCommand {
    // ---------------------------------------------------------------------
    // Pointer interactions
    // ---------------------------------------------------------------------
    /// Start dragging a body at the given pointer position.
    BeginDrag { target: NodeId, pointer: Point },

    /// Continue dragging to a new pointer position.
    MoveDrag { pointer: Point },

    /// End the current drag operation.
    EndDrag,

    // ---------------------------------------------------------------------
    // Force-based interactions
    // ---------------------------------------------------------------------
    /// Push a body in a direction with a given strength.
    Push {
        target: NodeId,
        direction: Vector2D,
        strength: f32,
    },

    /// Pull a body toward a destination with a given strength.
    Pull {
        target: NodeId,
        destination: Point,
        strength: f32,
    },

    /// Apply a raw force vector to a body.
    ApplyForce { target: NodeId, force: Vector2D },

    /// Apply a raw impulse (instantaneous force) to a body.
    ApplyImpulse { target: NodeId, impulse: Vector2D },

    /// Set a body's velocity directly.
    SetVelocity { target: NodeId, velocity: Vector2D },

    /// Attract all bodies in a region toward the region's center.
    Attract { region: Rect, strength: f32 },

    /// Repel all bodies in a region away from the region's center.
    Repel { region: Rect, strength: f32 },

    // ---------------------------------------------------------------------
    // Structural interactions
    // ---------------------------------------------------------------------
    /// Insert a new body at the given position with the given shape.
    Insert {
        target: NodeId,
        position: Point,
        shape: CollisionShape,
    },

    /// Remove a body from the world.
    Remove { target: NodeId },

    /// Cut a group at the given position, splitting bodies to the right.
    Cut { group: GroupId, position: Point },

    /// Merge two groups into one.
    Merge { first: GroupId, second: GroupId },

    // ---------------------------------------------------------------------
    // Group operations
    // ---------------------------------------------------------------------
    /// Concentrate bodies in a region toward a center point.
    Concentrate {
        region: Rect,
        center: Point,
        strength: f32,
    },

    /// Disperse bodies in a region, pushing them apart.
    Disperse { region: Rect, strength: f32 },

    /// Project a body's target position onto an axis.
    Project {
        target: NodeId,
        axis: ProjectionAxis,
        target_position: Point,
    },

    /// Pin a body to a fixed position.
    Anchor { target: NodeId, position: Point },

    /// Release a previously anchored body.
    ReleaseAnchor { target: NodeId },

    // ---------------------------------------------------------------------
    // Simulation control
    // ---------------------------------------------------------------------
    /// Pause or resume the physics simulation.
    SetSimulationActive { active: bool },

    /// Step the physics simulation by the given amount of time.
    StepSimulation { dt: f32 },

    /// Clear all bodies and reset the world.
    ResetWorld,

    // ---------------------------------------------------------------------
    // Input source commands (for keyboard, touch, gestures)
    // ---------------------------------------------------------------------
    /// A keyboard interaction command.
    Keyboard {
        key: KeyboardKey,
        modifiers: KeyboardModifiers,
        is_press: bool,
    },

    /// A touch interaction command.
    Touch {
        touch_id: u64,
        position: Point,
        phase: TouchPhase,
    },

    /// A gesture interaction command.
    Gesture {
        gesture_type: PhysicsGestureType,
        position: Point,
        delta: Vector2D,
        scale: f32,
        rotation: f32,
    },

    /// A scripted interaction command (for automated sequences).
    Scripted {
        name: String,
        parameters: Vec<ScriptParameter>,
    },
}

// ---------------------------------------------------------------------------
// Input Source Types
// ---------------------------------------------------------------------------

/// Keyboard keys for interaction commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardKey {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Space,
    Enter,
    Escape,
    Delete,
    Backspace,
    Tab,
    Key(char),
}

/// Keyboard modifiers for interaction commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyboardModifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyboardModifiers {
    pub fn is_empty(&self) -> bool {
        !self.shift && !self.control && !self.alt && !self.meta
    }
}

/// Touch phase for touch interaction commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TouchPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// Gesture types for gesture interaction commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsGestureType {
    /// Pinch-to-zoom gesture.
    Pinch,
    /// Two-finger pan gesture.
    Pan,
    /// Rotation gesture.
    Rotate,
    /// Long press gesture.
    LongPress,
    /// Swipe gesture.
    Swipe,
}

/// Parameters for scripted interaction commands.
#[derive(Debug, Clone)]
pub enum ScriptParameter {
    Float(String, f32),
    Point(String, Point),
    Vector(String, Vector2D),
    NodeId(String, NodeId),
    Rect(String, Rect),
    Bool(String, bool),
    String(String, String),
}

// ---------------------------------------------------------------------------
// Command Result
// ---------------------------------------------------------------------------

/// Result of processing an interaction command.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// The command was successfully executed.
    Success,
    /// The command was ignored (e.g., target not found).
    Ignored(String),
    /// The command failed with an error.
    Error(String),
}

// ---------------------------------------------------------------------------
// Interaction Manager
// ---------------------------------------------------------------------------

/// Manages interaction commands and delegates them to the physics system.
///
/// The manager maintains a command queue and processes commands in order.
/// It tracks the current drag state so that `MoveDrag` and `EndDrag`
/// commands know which body to operate on.
pub struct InteractionManager {
    /// Queue of pending commands.
    command_queue: VecDeque<InteractionCommand>,

    /// Currently dragged body (if any).
    dragged_body: Option<BodyId>,

    /// Whether commands are enabled.
    enabled: bool,

    /// History of processed commands (for debugging/replay).
    command_history: Vec<InteractionCommand>,

    /// Maximum history length (0 = unlimited).
    max_history: usize,
}

impl InteractionManager {
    /// Create a new interaction manager.
    pub fn new() -> Self {
        Self {
            command_queue: VecDeque::new(),
            dragged_body: None,
            enabled: true,
            command_history: Vec::new(),
            max_history: 0,
        }
    }

    /// Set the maximum command history length.
    pub fn set_max_history(&mut self, max: usize) {
        self.max_history = max;
    }

    /// Check if commands are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable command processing.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Get the currently dragged body ID, if any.
    pub fn dragged_body(&self) -> Option<BodyId> {
        self.dragged_body
    }

    /// Check if a drag is in progress.
    pub fn is_dragging(&self) -> bool {
        self.dragged_body.is_some()
    }

    /// Get the number of pending commands.
    pub fn pending_count(&self) -> usize {
        self.command_queue.len()
    }

    /// Get the command history.
    pub fn history(&self) -> &[InteractionCommand] {
        &self.command_history
    }

    /// Queue a command for processing.
    pub fn queue(&mut self, command: InteractionCommand) {
        self.command_queue.push_back(command);
    }

    /// Queue multiple commands for processing.
    pub fn queue_batch(&mut self, commands: impl IntoIterator<Item = InteractionCommand>) {
        for cmd in commands {
            self.command_queue.push_back(cmd);
        }
    }

    /// Process all queued commands against the physics world.
    ///
    /// Returns a list of results for each processed command.
    pub fn process(&mut self, world: &mut PhysicsWorld) -> Vec<CommandResult> {
        if !self.enabled {
            return Vec::new();
        }

        let mut results = Vec::new();
        while let Some(command) = self.command_queue.pop_front() {
            let result = self.execute_command(world, &command);
            self.push_to_history(&command);
            results.push(result);
        }
        results
    }

    /// Execute a single command immediately (without queuing).
    pub fn execute(
        &mut self,
        world: &mut PhysicsWorld,
        command: &InteractionCommand,
    ) -> CommandResult {
        if !self.enabled {
            return CommandResult::Ignored("Commands are disabled".to_string());
        }
        let result = self.execute_command(world, command);
        self.push_to_history(command);
        result
    }

    /// Internal command execution.
    fn execute_command(
        &mut self,
        world: &mut PhysicsWorld,
        command: &InteractionCommand,
    ) -> CommandResult {
        match command {
            // ---------------------------------------------------------------------
            // Pointer interactions
            // ---------------------------------------------------------------------
            InteractionCommand::BeginDrag { target, pointer } => {
                let body_id = BodyId::from(target.as_u64());
                match world.drag(body_id, *pointer) {
                    Some(_) => {
                        self.dragged_body = Some(body_id);
                        CommandResult::Success
                    }
                    None => CommandResult::Ignored(format!(
                        "Cannot drag body {:?} (not dynamic or not found)",
                        body_id
                    )),
                }
            }

            InteractionCommand::MoveDrag { pointer } => {
                if let Some(body_id) = self.dragged_body {
                    if world.drag_move(body_id, *pointer) {
                        CommandResult::Success
                    } else {
                        CommandResult::Ignored("Drag target not found".to_string())
                    }
                } else {
                    CommandResult::Ignored("No active drag".to_string())
                }
            }

            InteractionCommand::EndDrag => {
                if let Some(body_id) = self.dragged_body.take() {
                    if world.drag_release(body_id) {
                        CommandResult::Success
                    } else {
                        CommandResult::Ignored("Drag target not found".to_string())
                    }
                } else {
                    CommandResult::Ignored("No active drag".to_string())
                }
            }

            // ---------------------------------------------------------------------
            // Force-based interactions
            // ---------------------------------------------------------------------
            InteractionCommand::Push {
                target,
                direction,
                strength,
            } => {
                let body_id = BodyId::from(target.as_u64());
                if world.push(body_id, *direction, *strength) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::Pull {
                target,
                destination,
                strength,
            } => {
                let body_id = BodyId::from(target.as_u64());
                if world.pull(body_id, *destination, *strength) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::ApplyForce { target, force } => {
                let body_id = BodyId::from(target.as_u64());
                if world.apply_force(body_id, *force) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::ApplyImpulse { target, impulse } => {
                let body_id = BodyId::from(target.as_u64());
                if world.apply_impulse(body_id, *impulse) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::SetVelocity { target, velocity } => {
                let body_id = BodyId::from(target.as_u64());
                if world.set_velocity(body_id, *velocity) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::Attract { region, strength } => {
                world.attract(region, *strength);
                CommandResult::Success
            }

            InteractionCommand::Repel { region, strength } => {
                world.repel(region, *strength);
                CommandResult::Success
            }

            // ---------------------------------------------------------------------
            // Structural interactions
            // ---------------------------------------------------------------------
            InteractionCommand::Insert {
                target,
                position,
                shape,
            } => {
                let _result = world.insert(*target, *position, *shape);
                CommandResult::Success
            }

            InteractionCommand::Remove { target } => {
                let body_id = BodyId::from(target.as_u64());
                if world.remove(body_id) {
                    // If we removed the dragged body, clear the drag state
                    if self.dragged_body == Some(body_id) {
                        self.dragged_body = None;
                    }
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::Cut { group, position } => match world.cut(*group, *position) {
                Some(_) => CommandResult::Success,
                None => CommandResult::Ignored(format!("Group {:?} not found or empty", group)),
            },

            InteractionCommand::Merge { first, second } => match world.merge(*first, *second) {
                Some(_) => CommandResult::Success,
                None => CommandResult::Ignored("One or both groups not found".to_string()),
            },

            // ---------------------------------------------------------------------
            // Group operations
            // ---------------------------------------------------------------------
            InteractionCommand::Concentrate {
                region,
                center,
                strength,
            } => {
                world.concentrate(region, *center, *strength);
                CommandResult::Success
            }

            InteractionCommand::Disperse { region, strength } => {
                world.disperse(region, *strength);
                CommandResult::Success
            }

            InteractionCommand::Project {
                target,
                axis,
                target_position,
            } => {
                let body_id = BodyId::from(target.as_u64());
                if world.project(body_id, *axis, *target_position) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::Anchor { target, position } => {
                let body_id = BodyId::from(target.as_u64());
                if world.anchor(body_id, *position) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found", target))
                }
            }

            InteractionCommand::ReleaseAnchor { target } => {
                let body_id = BodyId::from(target.as_u64());
                if world.release_anchor(body_id) {
                    CommandResult::Success
                } else {
                    CommandResult::Ignored(format!("Target {:?} not found or not anchored", target))
                }
            }

            // ---------------------------------------------------------------------
            // Simulation control
            // ---------------------------------------------------------------------
            InteractionCommand::SetSimulationActive { active: _ } => {
                // This would typically be handled by the FixedTimestepSimulator
                // For now, we just return success
                CommandResult::Success
            }

            InteractionCommand::StepSimulation { dt } => {
                let original_dt = world.config.timestep;
                world.config.timestep = *dt;
                world.step();
                world.config.timestep = original_dt;
                CommandResult::Success
            }

            InteractionCommand::ResetWorld => {
                world.clear();
                self.dragged_body = None;
                CommandResult::Success
            }

            // ---------------------------------------------------------------------
            // Input source commands
            // ---------------------------------------------------------------------
            InteractionCommand::Keyboard {
                key,
                modifiers,
                is_press,
            } => {
                // Keyboard commands are translated to physics operations
                // based on the key and current context
                self.handle_keyboard(world, *key, *modifiers, *is_press)
            }

            InteractionCommand::Touch {
                touch_id: _touch_id,
                position,
                phase,
            } => {
                // Touch commands are translated to drag/pointer operations
                self.handle_touch(world, *position, *phase)
            }

            InteractionCommand::Gesture {
                gesture_type,
                position,
                delta,
                scale,
                rotation,
            } => {
                // Gesture commands are translated to physics operations
                self.handle_gesture(world, *gesture_type, *position, *delta, *scale, *rotation)
            }

            InteractionCommand::Scripted { name, parameters } => {
                // Scripted commands are interpreted based on name and parameters
                self.handle_scripted(world, name, parameters)
            }
        }
    }

    // ---------------------------------------------------------------------
    // Keyboard Input Handler
    // ---------------------------------------------------------------------

    /// Handle keyboard input commands.
    ///
    /// Translates keyboard events into physics operations based on the
    /// currently selected or focused body.
    fn handle_keyboard(
        &mut self,
        world: &mut PhysicsWorld,
        key: KeyboardKey,
        modifiers: KeyboardModifiers,
        is_press: bool,
    ) -> CommandResult {
        if !is_press {
            return CommandResult::Ignored("Key release ignored".to_string());
        }

        // Find the first dynamic body as the target
        let target = world
            .iter_bodies()
            .find(|(_, body)| body.enabled)
            .map(|(id, _)| *id);

        match key {
            KeyboardKey::ArrowLeft => {
                if let Some(body_id) = target {
                    let strength = if modifiers.shift { 200.0 } else { 50.0 };
                    world.push(body_id, Vector2D::new(-1.0, 0.0), strength);
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No target body".to_string())
                }
            }
            KeyboardKey::ArrowRight => {
                if let Some(body_id) = target {
                    let strength = if modifiers.shift { 200.0 } else { 50.0 };
                    world.push(body_id, Vector2D::new(1.0, 0.0), strength);
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No target body".to_string())
                }
            }
            KeyboardKey::ArrowUp => {
                if let Some(body_id) = target {
                    let strength = if modifiers.shift { 200.0 } else { 50.0 };
                    world.push(body_id, Vector2D::new(0.0, -1.0), strength);
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No target body".to_string())
                }
            }
            KeyboardKey::ArrowDown => {
                if let Some(body_id) = target {
                    let strength = if modifiers.shift { 200.0 } else { 50.0 };
                    world.push(body_id, Vector2D::new(0.0, 1.0), strength);
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No target body".to_string())
                }
            }
            KeyboardKey::Delete | KeyboardKey::Backspace => {
                if let Some(body_id) = target {
                    if modifiers.control {
                        // Delete all bodies
                        world.clear();
                    } else {
                        world.remove(body_id);
                    }
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No target body".to_string())
                }
            }
            KeyboardKey::Escape => {
                // Cancel any active drag
                if let Some(body_id) = self.dragged_body.take() {
                    world.drag_release(body_id);
                }
                CommandResult::Success
            }
            KeyboardKey::Space => {
                // Toggle simulation pause (would need external state)
                CommandResult::Success
            }
            KeyboardKey::Enter => {
                // Confirm/commit operation
                CommandResult::Success
            }
            KeyboardKey::Tab => {
                // Cycle focus (would need external focus state)
                CommandResult::Success
            }
            KeyboardKey::Key(_) => CommandResult::Ignored("Unmapped key".to_string()),
        }
    }

    // ---------------------------------------------------------------------
    // Touch Input Handler
    // ---------------------------------------------------------------------

    /// Handle touch input commands.
    ///
    /// Translates touch events into drag operations.
    fn handle_touch(
        &mut self,
        world: &mut PhysicsWorld,
        position: Point,
        phase: TouchPhase,
    ) -> CommandResult {
        match phase {
            TouchPhase::Started => {
                // Hit-test to find the body under the touch point
                if let Some(body_id) = world.hit_test(&position) {
                    if world.drag(body_id, position).is_some() {
                        self.dragged_body = Some(body_id);
                        CommandResult::Success
                    } else {
                        CommandResult::Ignored("Cannot drag touched body".to_string())
                    }
                } else {
                    CommandResult::Ignored("No body at touch position".to_string())
                }
            }
            TouchPhase::Moved => {
                if let Some(body_id) = self.dragged_body {
                    if world.drag_move(body_id, position) {
                        CommandResult::Success
                    } else {
                        CommandResult::Ignored("Drag target not found".to_string())
                    }
                } else {
                    CommandResult::Ignored("No active drag".to_string())
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if let Some(body_id) = self.dragged_body.take() {
                    if phase == TouchPhase::Ended {
                        world.drag_release(body_id);
                    }
                    CommandResult::Success
                } else {
                    CommandResult::Ignored("No active drag".to_string())
                }
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Gesture Input Handler
    // ---------------------------------------------------------------------------

    /// Handle gesture input commands.
    ///
    /// Translates gestures into physics operations.
    fn handle_gesture(
        &mut self,
        world: &mut PhysicsWorld,
        gesture_type: PhysicsGestureType,
        position: Point,
        delta: Vector2D,
        scale: f32,
        rotation: f32,
    ) -> CommandResult {
        match gesture_type {
            PhysicsGestureType::Pan => {
                // Pan: push bodies in the pan direction
                let region = Rect::new(position.x - 100.0, position.y - 100.0, 200.0, 200.0);
                let bodies = world.bodies_in_region(&region);
                let neg_delta = Vector2D::new(-delta.x, -delta.y);
                for body_id in bodies {
                    world.push(body_id, neg_delta, delta.length() * 0.1);
                }
                CommandResult::Success
            }
            PhysicsGestureType::Pinch => {
                // Pinch: concentrate or disperse based on scale
                let region = Rect::new(position.x - 100.0, position.y - 100.0, 200.0, 200.0);
                if scale > 1.0 {
                    // Zoom in: concentrate
                    world.concentrate(&region, position, (scale - 1.0) * 100.0);
                } else {
                    // Zoom out: disperse
                    world.disperse(&region, (1.0 - scale) * 100.0);
                }
                CommandResult::Success
            }
            PhysicsGestureType::Rotate => {
                // Rotate: apply rotational force around the gesture center
                let region = Rect::new(position.x - 100.0, position.y - 100.0, 200.0, 200.0);
                let bodies = world.bodies_in_region(&region);
                for body_id in bodies {
                    if let Some(body) = world.get_body(body_id) {
                        let dx = body.position.x - position.x;
                        let dy = body.position.y - position.y;
                        // Perpendicular direction for rotation
                        let rx = -dy;
                        let ry = dx;
                        let len = (rx * rx + ry * ry).sqrt().max(0.01);
                        world.push(
                            body_id,
                            Vector2D::new(rx / len, ry / len),
                            rotation.abs() * 50.0,
                        );
                    }
                }
                CommandResult::Success
            }
            PhysicsGestureType::LongPress => {
                // Long press: start drag at the press position
                if let Some(body_id) = world.hit_test(&position) {
                    if world.drag(body_id, position).is_some() {
                        self.dragged_body = Some(body_id);
                        CommandResult::Success
                    } else {
                        CommandResult::Ignored("Cannot drag body".to_string())
                    }
                } else {
                    CommandResult::Ignored("No body at position".to_string())
                }
            }
            PhysicsGestureType::Swipe => {
                // Swipe: push bodies in the swipe direction
                let region = Rect::new(position.x - 100.0, position.y - 100.0, 200.0, 200.0);
                let bodies = world.bodies_in_region(&region);
                for body_id in bodies {
                    world.push(body_id, delta, delta.length() * 0.5);
                }
                CommandResult::Success
            }
        }
    }

    // ---------------------------------------------------------------------------
    // Scripted Interaction Handler
    // ---------------------------------------------------------------------------

    /// Handle scripted interaction commands.
    ///
    /// Interprets named scripts with parameters.
    fn handle_scripted(
        &mut self,
        world: &mut PhysicsWorld,
        name: &str,
        parameters: &[ScriptParameter],
    ) -> CommandResult {
        match name {
            "push_all" => {
                let direction = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::Vector(_, v) = p {
                            Some(*v)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Vector2D::new(1.0, 0.0));
                let strength = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::Float(_, f) = p {
                            Some(*f)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(100.0);

                let ids: Vec<BodyId> = world.iter_bodies().map(|(id, _)| *id).collect();
                for body_id in ids {
                    world.push(body_id, direction, strength);
                }
                CommandResult::Success
            }
            "attract_to" => {
                let center = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::Point(_, p) = p {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Point::ORIGIN);
                let strength = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::Float(_, f) = p {
                            Some(*f)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(100.0);

                let region = Rect::new(-1000.0, -1000.0, 2000.0, 2000.0);
                world.concentrate(&region, center, strength);
                CommandResult::Success
            }
            "reset" => {
                world.clear();
                self.dragged_body = None;
                CommandResult::Success
            }
            "animate_insert" => {
                let node = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::NodeId(_, n) = p {
                            Some(*n)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(NodeId::new(0));
                let position = parameters
                    .iter()
                    .find_map(|p| {
                        if let ScriptParameter::Point(_, p) = p {
                            Some(*p)
                        } else {
                            None
                        }
                    })
                    .unwrap_or(Point::ORIGIN);

                let shape = CollisionShape::circle(25.0);
                world.insert(node, position, shape);
                CommandResult::Success
            }
            _ => CommandResult::Ignored(format!("Unknown script: {}", name)),
        }
    }

    // ---------------------------------------------------------------------------
    // History Management
    // ---------------------------------------------------------------------------

    /// Add a command to the history.
    fn push_to_history(&mut self, command: &InteractionCommand) {
        if self.max_history > 0 && self.command_history.len() >= self.max_history {
            self.command_history.remove(0);
        }
        self.command_history.push(command.clone());
    }

    /// Clear the command history.
    pub fn clear_history(&mut self) {
        self.command_history.clear();
    }

    /// Clear the command queue without processing.
    pub fn clear_queue(&mut self) {
        self.command_queue.clear();
    }

    /// Replay a sequence of commands.
    pub fn replay(
        &mut self,
        world: &mut PhysicsWorld,
        commands: &[InteractionCommand],
    ) -> Vec<CommandResult> {
        let mut results = Vec::new();
        for command in commands {
            let result = self.execute_command(world, command);
            results.push(result);
        }
        results
    }
}

impl Default for InteractionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_world() -> PhysicsWorld {
        let mut world = PhysicsWorld::new();
        let body = super::super::body::PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);
        world
    }

    // ---------------------------------------------------------------------
    // Command enum tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_command_begin_drag() {
        let cmd = InteractionCommand::BeginDrag {
            target: NodeId::new(1),
            pointer: Point::new(100.0, 100.0),
        };
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert!(manager.is_dragging());
    }

    #[test]
    fn test_command_move_drag() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::BeginDrag {
                target: NodeId::new(1),
                pointer: Point::new(100.0, 100.0),
            },
        );
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::MoveDrag {
                    pointer: Point::new(200.0, 200.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_end_drag() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::BeginDrag {
                target: NodeId::new(1),
                pointer: Point::new(100.0, 100.0),
            },
        );
        assert_eq!(
            manager.execute(&mut world, &InteractionCommand::EndDrag),
            CommandResult::Success
        );
        assert!(!manager.is_dragging());
    }

    #[test]
    fn test_command_end_drag_no_active() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert!(matches!(
            manager.execute(&mut world, &InteractionCommand::EndDrag),
            CommandResult::Ignored(_)
        ));
    }

    #[test]
    fn test_command_push() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Push {
                    target: NodeId::new(1),
                    direction: Vector2D::new(1.0, 0.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_pull() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Pull {
                    target: NodeId::new(1),
                    destination: Point::new(200.0, 200.0),
                    strength: 50.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_insert() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Insert {
                    target: NodeId::new(1),
                    position: Point::new(100.0, 100.0),
                    shape: CollisionShape::circle(25.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_remove() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Remove {
                    target: NodeId::new(1),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_concentrate() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Concentrate {
                    region: Rect::new(0.0, 0.0, 200.0, 200.0),
                    center: Point::new(100.0, 100.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_disperse() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Disperse {
                    region: Rect::new(0.0, 0.0, 200.0, 200.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_anchor() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Anchor {
                    target: NodeId::new(1),
                    position: Point::new(100.0, 100.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_release_anchor() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        manager.execute(
            &mut world,
            &InteractionCommand::Anchor {
                target: NodeId::new(1),
                position: Point::new(100.0, 100.0),
            },
        );
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::ReleaseAnchor {
                    target: NodeId::new(1),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_step_simulation() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::StepSimulation { dt: 1.0 / 60.0 }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_command_reset_world() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert!(world.iter_bodies().count() > 0);
        assert_eq!(
            manager.execute(&mut world, &InteractionCommand::ResetWorld),
            CommandResult::Success
        );
        assert_eq!(world.iter_bodies().count(), 0);
    }

    // ---------------------------------------------------------------------
    // Queue and batch tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_queue_and_process() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();

        manager.queue(InteractionCommand::Insert {
            target: NodeId::new(1),
            position: Point::new(100.0, 100.0),
            shape: CollisionShape::circle(25.0),
        });
        manager.queue(InteractionCommand::Insert {
            target: NodeId::new(2),
            position: Point::new(200.0, 100.0),
            shape: CollisionShape::circle(25.0),
        });

        assert_eq!(manager.pending_count(), 2);
        let results = manager.process(&mut world);
        assert_eq!(results.len(), 2);
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_queue_batch() {
        let mut manager = InteractionManager::new();
        manager.queue_batch([
            InteractionCommand::ResetWorld,
            InteractionCommand::StepSimulation { dt: 1.0 / 60.0 },
        ]);
        assert_eq!(manager.pending_count(), 2);
    }

    #[test]
    fn test_clear_queue() {
        let mut manager = InteractionManager::new();
        manager.queue(InteractionCommand::ResetWorld);
        manager.clear_queue();
        assert_eq!(manager.pending_count(), 0);
    }

    // ---------------------------------------------------------------------
    // Enabled/disabled tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_disabled_manager() {
        let mut manager = InteractionManager::new();
        manager.set_enabled(false);
        let mut world = create_test_world();

        manager.queue(InteractionCommand::ResetWorld);
        let results = manager.process(&mut world);
        assert!(results.is_empty());
        assert_eq!(manager.pending_count(), 1); // Queue not drained when disabled
    }

    // ---------------------------------------------------------------------
    // History tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_command_history() {
        let mut manager = InteractionManager::new();
        manager.set_max_history(10);
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::Push {
                target: NodeId::new(1),
                direction: Vector2D::new(1.0, 0.0),
                strength: 100.0,
            },
        );
        assert_eq!(manager.history().len(), 1);
    }

    #[test]
    fn test_history_limit() {
        let mut manager = InteractionManager::new();
        manager.set_max_history(2);
        let mut world = create_test_world();

        for i in 0..5 {
            manager.execute(
                &mut world,
                &InteractionCommand::Push {
                    target: NodeId::new(1),
                    direction: Vector2D::new(i as f32, 0.0),
                    strength: 100.0,
                },
            );
        }
        assert_eq!(manager.history().len(), 2);
    }

    #[test]
    fn test_clear_history() {
        let mut manager = InteractionManager::new();
        manager.set_max_history(10);
        let mut world = create_test_world();

        manager.execute(&mut world, &InteractionCommand::ResetWorld);
        assert_eq!(manager.history().len(), 1);
        manager.clear_history();
        assert_eq!(manager.history().len(), 0);
    }

    // ---------------------------------------------------------------------
    // Replay tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_replay_commands() {
        let commands = vec![
            InteractionCommand::Insert {
                target: NodeId::new(1),
                position: Point::new(100.0, 100.0),
                shape: CollisionShape::circle(25.0),
            },
            InteractionCommand::Insert {
                target: NodeId::new(2),
                position: Point::new(200.0, 100.0),
                shape: CollisionShape::circle(25.0),
            },
        ];

        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();
        let results = manager.replay(&mut world, &commands);
        assert_eq!(results.len(), 2);
    }

    // ---------------------------------------------------------------------
    // Keyboard input tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_keyboard_arrow_push() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::ArrowRight,
            modifiers: KeyboardModifiers::default(),
            is_press: true,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_keyboard_delete() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::Delete,
            modifiers: KeyboardModifiers::default(),
            is_press: true,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert_eq!(world.iter_bodies().count(), 0);
    }

    #[test]
    fn test_keyboard_escape_cancel_drag() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::BeginDrag {
                target: NodeId::new(1),
                pointer: Point::new(100.0, 100.0),
            },
        );
        assert!(manager.is_dragging());

        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::Escape,
            modifiers: KeyboardModifiers::default(),
            is_press: true,
        };
        manager.execute(&mut world, &cmd);
        assert!(!manager.is_dragging());
    }

    #[test]
    fn test_keyboard_release_ignored() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::ArrowRight,
            modifiers: KeyboardModifiers::default(),
            is_press: false,
        };
        assert!(matches!(
            manager.execute(&mut world, &cmd),
            CommandResult::Ignored(_)
        ));
    }

    // ---------------------------------------------------------------------
    // Touch input tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_touch_start_drag() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Touch {
            touch_id: 1,
            position: Point::new(100.0, 100.0),
            phase: TouchPhase::Started,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert!(manager.is_dragging());
    }

    #[test]
    fn test_touch_move() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::Touch {
                touch_id: 1,
                position: Point::new(100.0, 100.0),
                phase: TouchPhase::Started,
            },
        );
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Touch {
                    touch_id: 1,
                    position: Point::new(200.0, 200.0),
                    phase: TouchPhase::Moved,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_touch_end() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::Touch {
                touch_id: 1,
                position: Point::new(100.0, 100.0),
                phase: TouchPhase::Started,
            },
        );
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Touch {
                    touch_id: 1,
                    position: Point::new(200.0, 200.0),
                    phase: TouchPhase::Ended,
                }
            ),
            CommandResult::Success
        );
        assert!(!manager.is_dragging());
    }

    // ---------------------------------------------------------------------
    // Gesture input tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_gesture_pan() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::Pan,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::new(10.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_gesture_pinch_concentrate() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::Pinch,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::ZERO,
            scale: 1.5,
            rotation: 0.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_gesture_swipe() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::Swipe,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::new(50.0, 0.0),
            scale: 1.0,
            rotation: 0.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    // ---------------------------------------------------------------------
    // Scripted interaction tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_scripted_push_all() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Scripted {
            name: "push_all".to_string(),
            parameters: vec![
                ScriptParameter::Vector("direction".to_string(), Vector2D::new(1.0, 0.0)),
                ScriptParameter::Float("strength".to_string(), 100.0),
            ],
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_scripted_reset() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Scripted {
            name: "reset".to_string(),
            parameters: vec![],
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert_eq!(world.iter_bodies().count(), 0);
    }

    #[test]
    fn test_scripted_unknown() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Scripted {
            name: "unknown_script".to_string(),
            parameters: vec![],
        };
        assert!(matches!(
            manager.execute(&mut world, &cmd),
            CommandResult::Ignored(_)
        ));
    }

    #[test]
    fn test_scripted_animate_insert() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();

        let cmd = InteractionCommand::Scripted {
            name: "animate_insert".to_string(),
            parameters: vec![
                ScriptParameter::NodeId("node".to_string(), NodeId::new(42)),
                ScriptParameter::Point("position".to_string(), Point::new(100.0, 100.0)),
            ],
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    // ---------------------------------------------------------------------
    // Drag state tracking tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_dragged_body_tracking() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        assert_eq!(manager.dragged_body(), None);

        manager.execute(
            &mut world,
            &InteractionCommand::BeginDrag {
                target: NodeId::new(1),
                pointer: Point::new(100.0, 100.0),
            },
        );
        assert_eq!(manager.dragged_body(), Some(BodyId::new(1)));

        manager.execute(&mut world, &InteractionCommand::EndDrag);
        assert_eq!(manager.dragged_body(), None);
    }

    #[test]
    fn test_remove_clears_drag() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::BeginDrag {
                target: NodeId::new(1),
                pointer: Point::new(100.0, 100.0),
            },
        );
        assert!(manager.is_dragging());

        manager.execute(
            &mut world,
            &InteractionCommand::Remove {
                target: NodeId::new(1),
            },
        );
        assert!(!manager.is_dragging());
    }

    // ---------------------------------------------------------------------
    // Keyboard modifiers tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_keyboard_modifiers() {
        let mods = KeyboardModifiers {
            shift: true,
            control: false,
            alt: false,
            meta: false,
        };
        assert!(!mods.is_empty());

        let empty = KeyboardModifiers::default();
        assert!(empty.is_empty());
    }

    // ---------------------------------------------------------------------
    // Command result tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_command_result_success() {
        let result = CommandResult::Success;
        assert!(matches!(result, CommandResult::Success));
    }

    #[test]
    fn test_command_result_ignored() {
        let result = CommandResult::Ignored("test".to_string());
        assert!(matches!(result, CommandResult::Ignored(_)));
    }

    #[test]
    fn test_command_result_error() {
        let result = CommandResult::Error("test".to_string());
        assert!(matches!(result, CommandResult::Error(_)));
    }

    // ---------------------------------------------------------------------
    // Non-existent target tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_push_nonexistent_target() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();

        assert!(matches!(
            manager.execute(
                &mut world,
                &InteractionCommand::Push {
                    target: NodeId::new(999),
                    direction: Vector2D::new(1.0, 0.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Ignored(_)
        ));
    }

    #[test]
    fn test_remove_nonexistent_target() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();

        assert!(matches!(
            manager.execute(
                &mut world,
                &InteractionCommand::Remove {
                    target: NodeId::new(999),
                }
            ),
            CommandResult::Ignored(_)
        ));
    }

    #[test]
    fn test_begin_drag_nonexistent() {
        let mut manager = InteractionManager::new();
        let mut world = PhysicsWorld::new();

        assert!(matches!(
            manager.execute(
                &mut world,
                &InteractionCommand::BeginDrag {
                    target: NodeId::new(999),
                    pointer: Point::new(100.0, 100.0),
                }
            ),
            CommandResult::Ignored(_)
        ));
    }

    // ---------------------------------------------------------------------
    // Full drag cycle test
    // ---------------------------------------------------------------------

    #[test]
    fn test_full_drag_cycle() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        // Begin drag
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::BeginDrag {
                    target: NodeId::new(1),
                    pointer: Point::new(100.0, 100.0),
                }
            ),
            CommandResult::Success
        );

        // Move drag
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::MoveDrag {
                    pointer: Point::new(200.0, 200.0),
                }
            ),
            CommandResult::Success
        );

        // End drag
        assert_eq!(
            manager.execute(&mut world, &InteractionCommand::EndDrag),
            CommandResult::Success
        );

        assert!(!manager.is_dragging());
    }

    #[test]
    fn test_attract_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Attract {
                    region: Rect::new(0.0, 0.0, 200.0, 200.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_repel_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Repel {
                    region: Rect::new(0.0, 0.0, 200.0, 200.0),
                    strength: 100.0,
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_apply_force_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::ApplyForce {
                    target: NodeId::new(1),
                    force: Vector2D::new(10.0, 0.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_apply_impulse_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::ApplyImpulse {
                    target: NodeId::new(1),
                    impulse: Vector2D::new(5.0, 3.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_set_velocity_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::SetVelocity {
                    target: NodeId::new(1),
                    velocity: Vector2D::new(100.0, -50.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_project_command() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Project {
                    target: NodeId::new(1),
                    axis: ProjectionAxis::X,
                    target_position: Point::new(300.0, 0.0),
                }
            ),
            CommandResult::Success
        );
    }

    #[test]
    fn test_gesture_rotate() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::Rotate,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::ZERO,
            scale: 1.0,
            rotation: 45.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_gesture_long_press() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::LongPress,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::ZERO,
            scale: 1.0,
            rotation: 0.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert!(manager.is_dragging());
    }

    #[test]
    fn test_touch_cancelled() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        manager.execute(
            &mut world,
            &InteractionCommand::Touch {
                touch_id: 1,
                position: Point::new(100.0, 100.0),
                phase: TouchPhase::Started,
            },
        );
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::Touch {
                    touch_id: 1,
                    position: Point::new(200.0, 200.0),
                    phase: TouchPhase::Cancelled,
                }
            ),
            CommandResult::Success
        );
        assert!(!manager.is_dragging());
    }

    #[test]
    fn test_scripted_attract_to() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Scripted {
            name: "attract_to".to_string(),
            parameters: vec![
                ScriptParameter::Point("center".to_string(), Point::new(100.0, 100.0)),
                ScriptParameter::Float("strength".to_string(), 50.0),
            ],
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_keyboard_shift_modifier() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let mods = KeyboardModifiers {
            shift: true,
            ..Default::default()
        };
        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::ArrowRight,
            modifiers: mods,
            is_press: true,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_keyboard_control_delete() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        // Add another body
        let body2 = super::super::body::PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(200.0, 200.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body2);

        let mods = KeyboardModifiers {
            control: true,
            ..Default::default()
        };
        let cmd = InteractionCommand::Keyboard {
            key: KeyboardKey::Delete,
            modifiers: mods,
            is_press: true,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
        assert_eq!(world.iter_bodies().count(), 0);
    }

    #[test]
    fn test_touch_no_body_at_position() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Touch {
            touch_id: 1,
            position: Point::new(9999.0, 9999.0),
            phase: TouchPhase::Started,
        };
        assert!(matches!(
            manager.execute(&mut world, &cmd),
            CommandResult::Ignored(_)
        ));
    }

    #[test]
    fn test_gesture_pinch_disperse() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();

        let cmd = InteractionCommand::Gesture {
            gesture_type: PhysicsGestureType::Pinch,
            position: Point::new(100.0, 100.0),
            delta: Vector2D::ZERO,
            scale: 0.5,
            rotation: 0.0,
        };
        assert_eq!(manager.execute(&mut world, &cmd), CommandResult::Success);
    }

    #[test]
    fn test_command_clone() {
        let cmd = InteractionCommand::BeginDrag {
            target: NodeId::new(1),
            pointer: Point::new(100.0, 100.0),
        };
        let cloned = cmd.clone();
        assert!(matches!(cloned, InteractionCommand::BeginDrag { .. }));
    }

    #[test]
    fn test_command_result_clone() {
        let result = CommandResult::Success;
        let cloned = result.clone();
        assert!(matches!(cloned, CommandResult::Success));
    }

    #[test]
    fn test_interaction_manager_default() {
        let manager = InteractionManager::default();
        assert!(manager.is_enabled());
        assert!(!manager.is_dragging());
        assert_eq!(manager.pending_count(), 0);
    }

    #[test]
    fn test_set_simulation_active() {
        let mut manager = InteractionManager::new();
        let mut world = create_test_world();
        assert_eq!(
            manager.execute(
                &mut world,
                &InteractionCommand::SetSimulationActive { active: true }
            ),
            CommandResult::Success
        );
    }
}
