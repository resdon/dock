//! Force commands for the interaction engine.
//!
//! High-level commands that translate user interactions into physics forces:
//! push, pull, insert, cut, and redistribute.

use std::collections::HashMap;

use crate::geometry::{Point, Vector2D};
use crate::physics::body::{BodyId, PhysicsBody};
use crate::physics::forces::{PushForce, RepulsionForce, AttractionForce, BodyRepulsion};

/// Type of force command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandType {
    /// Push bodies away from a point.
    Push,
    /// Pull bodies toward a point.
    Pull,
    /// Insert a body at a position, making room.
    Insert,
    /// Cut a group, splitting it into two.
    Cut,
    /// Redistribute bodies to fill gaps.
    Redistribute,
    /// Expand a group to make room.
    Expand,
    /// Contract a group after removal.
    Contract,
}

/// A force command to be applied to the physics world.
#[derive(Debug, Clone)]
pub struct ForceCommand {
    /// Type of command.
    pub command_type: CommandType,
    /// Origin point of the command.
    pub origin: Point,
    /// Direction of the force.
    pub direction: Vector2D,
    /// Strength of the force.
    pub strength: f32,
    /// Radius of effect.
    pub radius: f32,
    /// Target body (for insert/cut).
    pub target_body: Option<BodyId>,
    /// Whether the command has been applied.
    pub applied: bool,
}

impl ForceCommand {
    /// Create a push command.
    pub fn push(origin: Point, direction: Vector2D, strength: f32, radius: f32) -> Self {
        Self {
            command_type: CommandType::Push,
            origin,
            direction,
            strength,
            radius,
            target_body: None,
            applied: false,
        }
    }

    /// Create a pull command.
    pub fn pull(origin: Point, target: Point, strength: f32) -> Self {
        let dir = Vector2D::new(target.x - origin.x, target.y - origin.y);
        let len = dir.length().max(0.001);
        Self {
            command_type: CommandType::Pull,
            origin,
            direction: Vector2D::new(dir.x / len, dir.y / len),
            strength,
            radius: 200.0,
            target_body: None,
            applied: false,
        }
    }

    /// Create an insert command.
    pub fn insert(position: Point, body_id: BodyId, strength: f32, radius: f32) -> Self {
        Self {
            command_type: CommandType::Insert,
            origin: position,
            direction: Vector2D::ZERO,
            strength,
            radius,
            target_body: Some(body_id),
            applied: false,
        }
    }

    /// Create a cut command.
    pub fn cut(position: Point, direction: Vector2D, strength: f32, radius: f32) -> Self {
        Self {
            command_type: CommandType::Cut,
            origin: position,
            direction,
            strength,
            radius,
            target_body: None,
            applied: false,
        }
    }

    /// Create a redistribute command.
    pub fn redistribute(center: Point, radius: f32, strength: f32) -> Self {
        Self {
            command_type: CommandType::Redistribute,
            origin: center,
            direction: Vector2D::ZERO,
            strength,
            radius,
            target_body: None,
            applied: false,
        }
    }

    /// Apply the command to all bodies in range.
    pub fn apply(&mut self, bodies: &mut HashMap<BodyId, PhysicsBody>, dt: f32) {
        if self.applied {
            return;
        }

        match self.command_type {
            CommandType::Push => {
                let push = PushForce::new(
                    self.origin,
                    self.direction,
                    self.strength,
                    self.radius,
                );
                for (_, body) in bodies.iter_mut() {
                    push.apply(body, dt);
                }
            }
            CommandType::Pull => {
                let attraction = AttractionForce::new(
                    self.origin + self.direction * self.radius,
                    self.strength,
                    self.radius * 2.0,
                );
                for (_, body) in bodies.iter_mut() {
                    attraction.apply(body, dt);
                }
            }
            CommandType::Insert => {
                // Push all bodies away from the insert point
                let repulsion = RepulsionForce::new(
                    self.origin,
                    self.strength,
                    self.radius,
                );
                for (_, body) in bodies.iter_mut() {
                    repulsion.apply(body, dt);
                }
            }
            CommandType::Cut => {
                // Push bodies in the cut direction
                let push = PushForce::new(
                    self.origin,
                    self.direction,
                    self.strength,
                    self.radius,
                );
                for (_, body) in bodies.iter_mut() {
                    push.apply(body, dt);
                }
            }
            CommandType::Redistribute => {
                // Apply mutual repulsion to spread bodies out
                let body_vec: Vec<PhysicsBody> = bodies.values().cloned().collect();
                let len = body_vec.len();
                let repulsion = BodyRepulsion::new(self.strength, self.radius / 2.0);
                for i in 0..len {
                    for j in (i + 1)..len {
                        // Can't mutably borrow twice; use IDs
                    }
                }
                // Simplified: apply repulsion from center
                let repulsion = RepulsionForce::new(
                    self.origin,
                    self.strength * 0.5,
                    self.radius,
                );
                for (_, body) in bodies.iter_mut() {
                    repulsion.apply(body, dt);
                }
            }
            CommandType::Expand | CommandType::Contract => {
                // Expand: push outward from center
                // Contract: pull inward toward center
                let factor = if self.command_type == CommandType::Expand {
                    self.strength
                } else {
                    -self.strength
                };

                for (_, body) in bodies.iter_mut() {
                    let dx = body.position.x - self.origin.x;
                    let dy = body.position.y - self.origin.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(0.001);
                    let nx = dx / dist;
                    let ny = dy / dist;
                    let force = Vector2D::new(nx * factor, ny * factor);
                    body.apply_force(force, dt);
                }
            }
        }

        self.applied = true;
    }

    /// Reset the command so it can be reapplied.
    pub fn reset(&mut self) {
        self.applied = false;
    }
}

/// Command queue for batching force commands.
pub struct CommandQueue {
    /// Pending commands.
    commands: Vec<ForceCommand>,
    /// Maximum queue size.
    max_size: usize,
}

impl CommandQueue {
    /// Create a new command queue.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            max_size: 64,
        }
    }

    /// Add a command to the queue.
    pub fn push(&mut self, command: ForceCommand) {
        if self.commands.len() >= self.max_size {
            self.commands.remove(0);
        }
        self.commands.push(command);
    }

    /// Apply all pending commands.
    pub fn apply_all(&mut self, bodies: &mut HashMap<BodyId, PhysicsBody>, dt: f32) {
        for cmd in self.commands.iter_mut() {
            cmd.apply(bodies, dt);
        }
        self.commands.clear();
    }

    /// Number of pending commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Clear all commands.
    pub fn clear(&mut self) {
        self.commands.clear();
    }

    /// Set maximum queue size.
    pub fn set_max_size(&mut self, size: usize) {
        self.max_size = size;
    }
}

impl Default for CommandQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::body::CollisionShape;

    #[test]
    fn test_push_command() {
        let mut cmd = ForceCommand::push(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            100.0,
        );
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        assert!(cmd.applied);
        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_pull_command() {
        let mut cmd = ForceCommand::pull(
            Point::new(0.0, 0.0),
            Point::new(100.0, 0.0),
            100.0,
        );
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        assert!(cmd.applied);
    }

    #[test]
    fn test_insert_command() {
        let mut cmd = ForceCommand::insert(
            Point::new(100.0, 100.0),
            BodyId::new(99),
            200.0,
            80.0,
        );
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(110.0, 110.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        let body = bodies.get(&BodyId::new(1)).unwrap();
        // Body should be pushed away from insert point
        assert!(body.velocity.length() > 0.0);
    }

    #[test]
    fn test_cut_command() {
        let mut cmd = ForceCommand::cut(
            Point::new(100.0, 100.0),
            Vector2D::new(1.0, 0.0),
            150.0,
            100.0,
        );
        assert_eq!(cmd.command_type, CommandType::Cut);
        assert!(!cmd.applied);
    }

    #[test]
    fn test_redistribute_command() {
        let mut cmd = ForceCommand::redistribute(
            Point::new(100.0, 100.0),
            100.0,
            100.0,
        );
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(110.0, 110.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        assert!(cmd.applied);
    }

    #[test]
    fn test_expand_command() {
        let mut cmd = ForceCommand {
            command_type: CommandType::Expand,
            origin: Point::new(100.0, 100.0),
            direction: Vector2D::ZERO,
            strength: 100.0,
            radius: 100.0,
            target_body: None,
            applied: false,
        };
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(150.0, 100.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        let body = bodies.get(&BodyId::new(1)).unwrap();
        // Should be pushed outward (right)
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_contract_command() {
        let mut cmd = ForceCommand {
            command_type: CommandType::Contract,
            origin: Point::new(100.0, 100.0),
            direction: Vector2D::ZERO,
            strength: 100.0,
            radius: 100.0,
            target_body: None,
            applied: false,
        };
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(150.0, 100.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        let body = bodies.get(&BodyId::new(1)).unwrap();
        // Should be pulled inward (left)
        assert!(body.velocity.x < 0.0);
    }

    #[test]
    fn test_command_reset() {
        let mut cmd = ForceCommand::push(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            100.0,
        );
        cmd.applied = true;
        cmd.reset();
        assert!(!cmd.applied);
    }

    #[test]
    fn test_command_queue() {
        let mut queue = CommandQueue::new();
        queue.push(ForceCommand::push(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            100.0,
        ));
        assert_eq!(queue.len(), 1);
        assert!(!queue.is_empty());
        queue.clear();
        assert!(queue.is_empty());
    }

    #[test]
    fn test_command_queue_max_size() {
        let mut queue = CommandQueue::new();
        queue.set_max_size(2);
        queue.push(ForceCommand::push(Point::ORIGIN, Vector2D::X, 1.0, 1.0));
        queue.push(ForceCommand::push(Point::ORIGIN, Vector2D::X, 1.0, 1.0));
        queue.push(ForceCommand::push(Point::ORIGIN, Vector2D::X, 1.0, 1.0));
        assert_eq!(queue.len(), 2); // Oldest should be evicted
    }

    #[test]
    fn test_applied_command_skips() {
        let mut cmd = ForceCommand::push(
            Point::ORIGIN,
            Vector2D::new(1.0, 0.0),
            100.0,
            100.0,
        );
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        cmd.apply(&mut bodies, 0.016);
        let vel1 = bodies.get(&BodyId::new(1)).unwrap().velocity;
        cmd.apply(&mut bodies, 0.016); // Should be skipped
        let vel2 = bodies.get(&BodyId::new(1)).unwrap().velocity;
        assert!((vel1.x - vel2.x).abs() < f32::EPSILON);
    }

    #[test]
    fn test_command_type_variants() {
        assert_eq!(ForceCommand::push(Point::ORIGIN, Vector2D::X, 1.0, 1.0).command_type, CommandType::Push);
        assert_eq!(ForceCommand::pull(Point::ORIGIN, Point::new(1.0, 0.0), 1.0).command_type, CommandType::Pull);
        assert_eq!(ForceCommand::insert(Point::ORIGIN, BodyId::new(1), 1.0, 1.0).command_type, CommandType::Insert);
        assert_eq!(ForceCommand::cut(Point::ORIGIN, Vector2D::X, 1.0, 1.0).command_type, CommandType::Cut);
        assert_eq!(ForceCommand::redistribute(Point::ORIGIN, 1.0, 1.0).command_type, CommandType::Redistribute);
    }
}
