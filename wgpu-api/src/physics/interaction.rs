//! High-level intent-based interaction API.
//!
//! Instead of exposing low-level physics operations everywhere, this module
//! provides semantic operations that describe *what* the user wants to happen,
//! not *how* to make it happen.
//!
//! Each operation internally creates forces, constraints, layout changes or
//! topology changes.  This lets you change the implementation later without
//! changing application code.
//!
//! ## API Design
//!
//! ```ignore
//! world.drag(icon);
//! world.push(icon, direction, strength);
//! world.pull(icon, target, strength);
//! world.attract(region, strength);
//! world.repel(region, strength);
//! world.insert(icon, position);
//! world.remove(icon);
//! world.cut(group, position);
//! world.merge(group_a, group_b);
//! world.concentrate(region, center);
//! world.disperse(region, strength);
//! world.project(icon, axis);
//! world.apply_force(icon, force);
//! world.apply_impulse(icon, impulse);
//! world.set_velocity(icon, velocity);
//! world.anchor(icon, position);
//! world.release_anchor(icon);
//! ```
//!
//! ## Force vs. Structural Operations
//!
//! Not all interactions are forces.  The API contains two categories:
//!
//! **Force-based operations** change an object's motion through physics:
//! - `push` — applies an impulse (instantaneous force)
//! - `pull` — applies a continuous attraction force
//! - `apply_force` — applies a raw force vector
//! - `apply_impulse` — applies a raw impulse vector
//! - `attract` / `repel` — field-based forces on a region
//! - `concentrate` / `disperse` — group-level field forces
//!
//! **Structural operations** change relationships and layout:
//! - `insert` — creates a new slot, shifts neighbors via springs
//! - `remove` — deletes a body and its constraints
//! - `cut` — splits a group into two at a boundary
//! - `merge` — combines two groups into one
//! - `project` — constrains movement along an axis
//! - `anchor` / `release_anchor` — pins or unpins a body
//!
//! This separation means the system needs both a physics engine (for forces)
//! and a relationship/layout engine (for structural changes).
//!
//! ## Architecture
//!
//! The interaction API sits on top of the three physics subsystems:
//!
//! - **System A (Springs)** — drag, pull, project use spring constraints
//! - **System B (Rigid bodies)** — insert, cut, merge modify collision topology
//! - **System C (Fields)** — attract, repel, concentrate, disperse use force fields

use super::body::{
    BodyId, BodyType, CollisionShape, PhysicalProperties, PhysicsBody, PhysicsObject,
};
use super::fields::{FieldSource, ForceField};
use super::groups::GroupId;
use super::world::PhysicsWorld;
use crate::geometry::{NodeId, Point, Rect, Vector2D};

/// State of a physics drag operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsDragState {
    /// No drag in progress.
    Idle,
    /// Dragging with pointer tracking.
    Dragging,
    /// Drag released, object returning to rest position.
    Released,
}

/// Configuration for drag behavior.
#[derive(Debug, Clone, Copy)]
pub struct DragConfig {
    /// Spring stiffness when dragging (higher = tighter follow).
    pub stiffness: f32,
    /// Damping when dragging (higher = less oscillation).
    pub damping: f32,
    /// Whether to allow velocity buildup during drag.
    pub allow_velocity: bool,
    /// Maximum drag velocity.
    pub max_velocity: f32,
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            stiffness: 1000.0,
            damping: 0.8,
            allow_velocity: true,
            max_velocity: 5000.0,
        }
    }
}

/// Axis for projection operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionAxis {
    /// Project along the X axis (horizontal alignment).
    X,
    /// Project along the Y axis (vertical alignment).
    Y,
    /// Project along both axes.
    Both,
}

/// Result of an insert operation.
#[derive(Debug, Clone, Copy)]
pub struct InsertResult {
    /// The body ID that was inserted.
    pub body_id: BodyId,
    /// The rest position assigned to the inserted body.
    pub rest_position: Point,
    /// Whether the insert caused a layout shift.
    pub layout_shifted: bool,
}

/// Result of a cut operation.
#[derive(Debug, Clone)]
pub struct CutResult {
    /// Bodies that were cut from the group.
    pub cut_bodies: Vec<BodyId>,
    /// The group that was cut from.
    pub source_group: GroupId,
    /// Whether a new group was created for the cut bodies.
    pub new_group_created: bool,
}

/// Result of a merge operation.
#[derive(Debug, Clone)]
pub struct MergeResult {
    /// The resulting group after merge.
    pub merged_group: GroupId,
    /// Number of bodies in the merged group.
    pub body_count: usize,
    /// Combined bounding rect of the merged group.
    pub bounds: Rect,
}

// ---------------------------------------------------------------------------
// Interaction API extension trait for PhysicsWorld
// ---------------------------------------------------------------------------

/// High-level interaction operations for PhysicsWorld.
///
/// This trait extends `PhysicsWorld` with semantic operations that describe
/// user intent rather than low-level physics manipulation.
pub trait InteractionApi {
    // ---------------------------------------------------------------------
    // Pointer interactions
    // ---------------------------------------------------------------------

    /// Start dragging a body.
    fn drag(&mut self, body_id: BodyId, pointer_position: Point) -> Option<PhysicsDragState>;

    /// Continue dragging a body to a new pointer position.
    fn drag_move(&mut self, body_id: BodyId, pointer_position: Point) -> bool;

    /// Release a dragged body.
    fn drag_release(&mut self, body_id: BodyId) -> bool;

    /// Check if a body is currently being dragged.
    fn is_dragging(&self, body_id: BodyId) -> bool;

    // ---------------------------------------------------------------------
    // Force-based interactions
    // ---------------------------------------------------------------------

    fn push(&mut self, body_id: BodyId, direction: Vector2D, strength: f32) -> bool;
    fn pull(&mut self, body_id: BodyId, target: Point, strength: f32) -> bool;
    fn attract(&mut self, region: &Rect, strength: f32);
    fn repel(&mut self, region: &Rect, strength: f32);

    // ---------------------------------------------------------------------
    // Direct physics control
    // ---------------------------------------------------------------------

    /// Apply a raw force vector to a body, changing its acceleration.
    ///
    /// Forces are accumulated over time steps, so a continuous force produces
    /// continuous acceleration.  This is the most direct way to control
    /// an object's motion through physics.
    ///
    /// API: `apply_force(target, force)`
    fn apply_force(&mut self, body_id: BodyId, force: Vector2D) -> bool;

    /// Apply a raw impulse (instantaneous force) to a body.
    ///
    /// Unlike forces, impulses are applied immediately and do not accumulate
    /// over time steps.  Use impulses for sudden changes in motion, such as
    /// collisions, explosions, or quick pushes.
    fn apply_impulse(&mut self, body_id: BodyId, impulse: Vector2D) -> bool;

    /// Set a body's velocity directly, bypassing forces.
    ///
    /// This is useful for kinematic control, teleportation effects, or
    /// resetting motion state.
    fn set_velocity(&mut self, body_id: BodyId, velocity: Vector2D) -> bool;

    /// Pin a body to a fixed position, preventing it from moving.
    ///
    /// An anchored body behaves like a static object — it cannot be moved
    /// by forces, collisions, or springs.  Use `release_anchor` to restore
    /// normal behavior.
    fn anchor(&mut self, body_id: BodyId, position: Point) -> bool;

    /// Release a previously anchored body, restoring its normal body type.
    fn release_anchor(&mut self, body_id: BodyId) -> bool;

    /// Check whether a body is currently anchored.
    fn is_anchored(&self, body_id: BodyId) -> bool;

    // ---------------------------------------------------------------------
    // Layout operations
    // ---------------------------------------------------------------------

    fn insert(&mut self, node: NodeId, position: Point, shape: CollisionShape) -> InsertResult;
    fn remove(&mut self, body_id: BodyId) -> bool;
    fn cut(&mut self, group: GroupId, position: Point) -> Option<CutResult>;
    fn merge(&mut self, group_a: GroupId, group_b: GroupId) -> Option<MergeResult>;

    // ---------------------------------------------------------------------
    // Group operations
    // ---------------------------------------------------------------------

    fn concentrate(&mut self, region: &Rect, center: Point, strength: f32);
    fn disperse(&mut self, region: &Rect, strength: f32);
    fn project(&mut self, body_id: BodyId, axis: ProjectionAxis, target: Point) -> bool;

    // ---------------------------------------------------------------------
    // Query operations
    // ---------------------------------------------------------------------

    fn hit_test(&self, point: &Point) -> Option<BodyId>;
    fn bodies_in_region(&self, region: &Rect) -> Vec<BodyId>;
    fn drag_state(&self, body_id: BodyId) -> PhysicsDragState;
}

impl InteractionApi for PhysicsWorld {
    // ---------------------------------------------------------------------
    // Pointer interactions
    // ---------------------------------------------------------------------

    fn drag(&mut self, body_id: BodyId, pointer_position: Point) -> Option<PhysicsDragState> {
        let body = self.get_body_mut(body_id)?;

        if body.body_type != BodyType::Dynamic {
            return None;
        }

        body.wake_up();
        body.rest_position = Some(pointer_position);
        self.spring_stiffness = 1000.0;
        self.spring_damping = 0.8;

        Some(PhysicsDragState::Dragging)
    }

    fn drag_move(&mut self, body_id: BodyId, pointer_position: Point) -> bool {
        let dt = self.config.timestep;
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };

        body.rest_position = Some(pointer_position);
        let prev = body.position;
        body.position = pointer_position;

        if dt > 0.0 {
            body.velocity.x = (body.position.x - prev.x) / dt;
            body.velocity.y = (body.position.y - prev.y) / dt;
        }

        true
    }

    fn drag_release(&mut self, body_id: BodyId) -> bool {
        let max_vel = self.config.max_velocity;

        // Clamp velocity first while we have mutable access
        {
            let body = match self.get_body_mut(body_id) {
                Some(b) => b,
                None => return false,
            };
            let vel_sq = body.velocity.length_squared();
            if vel_sq > max_vel * max_vel {
                let scale = max_vel / vel_sq.sqrt();
                body.velocity.x *= scale;
                body.velocity.y *= scale;
            }
            // body dropped here
        };

        // Now safe to modify spring params
        self.spring_stiffness = 500.0;
        self.spring_damping = 0.8;

        true
    }

    fn is_dragging(&self, body_id: BodyId) -> bool {
        if let Some(body) = self.get_body(body_id) {
            body.rest_position.is_some() && self.spring_stiffness > 500.0
        } else {
            false
        }
    }

    // ---------------------------------------------------------------------
    // Force-based interactions
    // ---------------------------------------------------------------------

    fn push(&mut self, body_id: BodyId, direction: Vector2D, strength: f32) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();
        body.apply_impulse(Vector2D::new(
            direction.x * strength,
            direction.y * strength,
        ));
        true
    }

    fn pull(&mut self, body_id: BodyId, target: Point, strength: f32) -> bool {
        let dt = self.config.timestep;
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();
        body.rest_position = Some(target);

        let dx = target.x - body.position.x;
        let dy = target.y - body.position.y;
        body.apply_force(Vector2D::new(dx * strength, dy * strength), dt);
        true
    }

    fn attract(&mut self, region: &Rect, strength: f32) {
        let center = region.center();
        let source = FieldSource::attraction(center, strength, region.width.max(region.height));
        let dt = self.config.timestep;

        let ids: Vec<BodyId> = self.query_overlap(region);
        for id in ids {
            if let Some(body) = self.get_body_mut(id) {
                body.wake_up();
                let force = source.compute_force(body);
                body.apply_force(force, dt);
            }
        }
    }

    fn repel(&mut self, region: &Rect, strength: f32) {
        let center = region.center();
        let source = FieldSource::repulsion(center, strength, region.width.max(region.height));
        let dt = self.config.timestep;

        let ids: Vec<BodyId> = self.query_overlap(region);
        for id in ids {
            if let Some(body) = self.get_body_mut(id) {
                body.wake_up();
                let force = source.compute_force(body);
                body.apply_force(force, dt);
            }
        }
    }

    // ---------------------------------------------------------------------
    // Direct physics control
    // ---------------------------------------------------------------------

    fn apply_force(&mut self, body_id: BodyId, force: Vector2D) -> bool {
        let dt = self.config.timestep;
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();
        body.apply_force(force, dt);
        true
    }

    fn apply_impulse(&mut self, body_id: BodyId, impulse: Vector2D) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();
        body.apply_impulse(impulse);
        true
    }

    fn set_velocity(&mut self, body_id: BodyId, velocity: Vector2D) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();
        body.velocity = velocity;
        true
    }

    fn anchor(&mut self, body_id: BodyId, position: Point) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        // Save the original body type so we can restore it on release
        body.anchored_type = Some(body.body_type);
        body.body_type = BodyType::Static;
        body.position = position;
        body.velocity = Vector2D::ZERO;
        body.acceleration = Vector2D::ZERO;
        body.put_to_sleep();
        true
    }

    fn release_anchor(&mut self, body_id: BodyId) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        if let Some(original_type) = body.anchored_type.take() {
            body.body_type = original_type;
            body.wake_up();
            true
        } else {
            false
        }
    }

    fn is_anchored(&self, body_id: BodyId) -> bool {
        if let Some(body) = self.get_body(body_id) {
            body.anchored_type.is_some()
        } else {
            false
        }
    }

    // ---------------------------------------------------------------------
    // Layout operations
    // ---------------------------------------------------------------------

    fn insert(&mut self, node: NodeId, position: Point, shape: CollisionShape) -> InsertResult {
        let body_id = BodyId::from(node.as_u64());
        let mut body = PhysicsBody::dynamic_body(body_id, position, shape, 1.0);
        body.rest_position = Some(position);
        self.add_body(body);

        // Push neighbors away
        let repulsion_strength = 50.0;
        let bounds = shape.bounding_rect(&position);
        let repel_radius = bounds.width.max(bounds.height) * 2.0;

        let repel_region = Rect::new(
            position.x - repel_radius,
            position.y - repel_radius,
            repel_radius * 2.0,
            repel_radius * 2.0,
        );

        let neighbors: Vec<BodyId> = self.query_overlap(&repel_region);
        let mut layout_shifted = false;
        for neighbor_id in neighbors {
            if neighbor_id == body_id {
                continue;
            }
            if let Some(neighbor) = self.get_body_mut(neighbor_id) {
                neighbor.wake_up();
                let dx = neighbor.position.x - position.x;
                let dy = neighbor.position.y - position.y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                neighbor.apply_impulse(Vector2D::new(
                    dx / dist * repulsion_strength,
                    dy / dist * repulsion_strength,
                ));
                layout_shifted = true;
            }
        }

        InsertResult {
            body_id,
            rest_position: position,
            layout_shifted,
        }
    }

    fn remove(&mut self, body_id: BodyId) -> bool {
        self.remove_body(body_id).is_some()
    }

    fn cut(&mut self, group: GroupId, position: Point) -> Option<CutResult> {
        // Find bodies tagged with this group
        let group_tag = format!("group:{}", group.as_u64());
        let bodies: Vec<BodyId> = self
            .iter_bodies()
            .filter(|(_, body)| body.enabled && body.tag.as_deref() == Some(&group_tag))
            .map(|(id, _)| *id)
            .collect();

        if bodies.is_empty() {
            return None;
        }

        let mut cut_bodies = Vec::new();
        for id in &bodies {
            if let Some(body) = self.get_body(*id) {
                if body.position.x > position.x {
                    cut_bodies.push(*id);
                }
            }
        }

        if cut_bodies.is_empty() {
            return None;
        }

        for id in &cut_bodies {
            if let Some(body) = self.get_body_mut(*id) {
                body.wake_up();
            }
        }

        Some(CutResult {
            cut_bodies,
            source_group: group,
            new_group_created: false,
        })
    }

    fn merge(&mut self, group_a: GroupId, group_b: GroupId) -> Option<MergeResult> {
        let tag_a = format!("group:{}", group_a.as_u64());
        let tag_b = format!("group:{}", group_b.as_u64());

        let all_bodies: Vec<(BodyId, Rect)> = self
            .iter_bodies()
            .filter(|(_, body)| {
                body.enabled
                    && (body.tag.as_deref() == Some(&tag_a) || body.tag.as_deref() == Some(&tag_b))
            })
            .map(|(id, body)| (*id, body.bounding_rect()))
            .collect();

        if all_bodies.is_empty() {
            return None;
        }

        let mut first = true;
        let mut bounds = Rect::ZERO;
        for (_, rect) in &all_bodies {
            if first {
                bounds = *rect;
                first = false;
            } else {
                bounds = bounds.union(rect);
            }
        }

        let merged_tag = format!("group:{}", group_a.as_u64());
        for (id, _) in &all_bodies {
            if let Some(body) = self.get_body_mut(*id) {
                body.tag = Some(merged_tag.clone());
            }
        }

        Some(MergeResult {
            merged_group: group_a,
            body_count: all_bodies.len(),
            bounds,
        })
    }

    // ---------------------------------------------------------------------
    // Group operations
    // ---------------------------------------------------------------------

    fn concentrate(&mut self, region: &Rect, center: Point, strength: f32) {
        let source = FieldSource::attraction(center, strength, region.width.max(region.height));
        let dt = self.config.timestep;

        let ids: Vec<BodyId> = self.query_overlap(region);
        for id in ids {
            if let Some(body) = self.get_body_mut(id) {
                body.wake_up();
                let force = source.compute_force(body);
                body.apply_force(force, dt);
            }
        }
    }

    fn disperse(&mut self, region: &Rect, strength: f32) {
        let ids: Vec<BodyId> = self.query_overlap(region);

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                let id_a = ids[i];
                let id_b = ids[j];

                let (pos_a, pos_b) = match (self.get_body(id_a), self.get_body(id_b)) {
                    (Some(a), Some(b)) => (a.position, b.position),
                    _ => continue,
                };

                let dx = pos_b.x - pos_a.x;
                let dy = pos_b.y - pos_a.y;
                let dist = (dx * dx + dy * dy).sqrt().max(0.01);

                let force_mag = strength / (dist + 0.01);
                let fx = dx / dist * force_mag;
                let fy = dy / dist * force_mag;

                if let Some(mut_a) = self.get_body_mut(id_a) {
                    mut_a.wake_up();
                    mut_a.apply_impulse(Vector2D::new(-fx, -fy));
                }
                if let Some(mut_b) = self.get_body_mut(id_b) {
                    mut_b.wake_up();
                    mut_b.apply_impulse(Vector2D::new(fx, fy));
                }
            }
        }
    }

    fn project(&mut self, body_id: BodyId, axis: ProjectionAxis, target: Point) -> bool {
        let body = match self.get_body_mut(body_id) {
            Some(b) => b,
            None => return false,
        };
        body.wake_up();

        match axis {
            ProjectionAxis::X => {
                body.rest_position = Some(Point::new(target.x, body.position.y));
            }
            ProjectionAxis::Y => {
                body.rest_position = Some(Point::new(body.position.x, target.y));
            }
            ProjectionAxis::Both => {
                body.rest_position = Some(target);
            }
        }

        true
    }

    // ---------------------------------------------------------------------
    // Query operations
    // ---------------------------------------------------------------------

    fn hit_test(&self, point: &Point) -> Option<BodyId> {
        self.query_point(point)
    }

    fn bodies_in_region(&self, region: &Rect) -> Vec<BodyId> {
        self.query_overlap(region)
    }

    fn drag_state(&self, body_id: BodyId) -> PhysicsDragState {
        if self.is_dragging(body_id) {
            PhysicsDragState::Dragging
        } else if let Some(body) = self.get_body(body_id) {
            if body.rest_position.is_some() {
                PhysicsDragState::Released
            } else {
                PhysicsDragState::Idle
            }
        } else {
            PhysicsDragState::Idle
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drag_start() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);

        let state = world.drag(BodyId::new(1), Point::new(150.0, 150.0));
        assert_eq!(state, Some(PhysicsDragState::Dragging));
        assert!(world.is_dragging(BodyId::new(1)));
    }

    #[test]
    fn test_drag_move() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);

        world.drag(BodyId::new(1), Point::new(100.0, 100.0));
        world.drag_move(BodyId::new(1), Point::new(200.0, 200.0));

        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.position.x - 200.0).abs() < f32::EPSILON);
        assert!((body.position.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_drag_release() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);

        world.drag(BodyId::new(1), Point::new(100.0, 100.0));
        world.drag_move(BodyId::new(1), Point::new(200.0, 200.0));
        world.drag_release(BodyId::new(1));

        // After release, spring stiffness should be restored
        assert!((world.spring_stiffness - 500.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_push() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        world.push(BodyId::new(1), Vector2D::new(1.0, 0.0), 100.0);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.velocity.x > 0.0);
        assert!((body.velocity.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pull() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        world.pull(BodyId::new(1), Point::new(100.0, 0.0), 10.0);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_attract() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        let region = Rect::new(0.0, 0.0, 100.0, 100.0);
        world.attract(&region, 100.0);

        let body = world.get_body(BodyId::new(1)).unwrap();
        // Body should have been attracted toward center (50, 50)
        // Since it's at center, velocity should be near zero
        assert!(body.velocity.length() < 1.0);
    }

    #[test]
    fn test_repel() {
        let mut world = PhysicsWorld::new();
        // Place body off-center so repulsion force is non-zero
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(70.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        // Region center is (50,50); body at (70,50) should be pushed right
        let region = Rect::new(0.0, 0.0, 100.0, 100.0);
        world.repel(&region, 100.0);

        let body = world.get_body(BodyId::new(1)).unwrap();
        // Body should be pushed away from center (positive X velocity)
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_insert() {
        let mut world = PhysicsWorld::new();
        let result = world.insert(
            NodeId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
        );

        assert_eq!(result.body_id, BodyId::new(1));
        assert!((result.rest_position.x - 100.0).abs() < f32::EPSILON);
        assert!(world.has_body(BodyId::new(1)));
    }

    #[test]
    fn test_insert_pushes_neighbors() {
        let mut world = PhysicsWorld::new();
        // Add a neighbor near the insertion point
        let neighbor = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(120.0, 100.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(neighbor);

        let result = world.insert(
            NodeId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(25.0),
        );

        assert!(result.layout_shifted);
    }

    #[test]
    fn test_remove() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        assert!(world.remove(BodyId::new(1)));
        assert!(!world.has_body(BodyId::new(1)));
    }

    #[test]
    fn test_concentrate() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(100.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body1);
        world.add_body(body2);

        let region = Rect::new(-50.0, -50.0, 200.0, 100.0);
        let center = Point::new(50.0, 0.0);
        world.concentrate(&region, center, 100.0);

        // Body 1 should be pulled toward center (positive X velocity)
        let b1 = world.get_body(BodyId::new(1)).unwrap();
        assert!(b1.velocity.x > 0.0);

        // Body 2 should be pulled toward center (negative X velocity)
        let b2 = world.get_body(BodyId::new(2)).unwrap();
        assert!(b2.velocity.x < 0.0);
    }

    #[test]
    fn test_disperse() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(60.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body1);
        world.add_body(body2);

        let region = Rect::new(0.0, 0.0, 100.0, 100.0);
        world.disperse(&region, 100.0);

        // Bodies should be pushed apart
        let b1 = world.get_body(BodyId::new(1)).unwrap();
        let b2 = world.get_body(BodyId::new(2)).unwrap();

        // Body 1 should move left, body 2 should move right
        assert!(b1.velocity.x < 0.0);
        assert!(b2.velocity.x > 0.0);
    }

    #[test]
    fn test_project_x() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 200.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        world.project(BodyId::new(1), ProjectionAxis::X, Point::new(300.0, 0.0));
        let body = world.get_body(BodyId::new(1)).unwrap();
        let rest = body.rest_position.unwrap();
        assert!((rest.x - 300.0).abs() < f32::EPSILON);
        assert!((rest.y - 200.0).abs() < f32::EPSILON); // Y unchanged
    }

    #[test]
    fn test_project_y() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 200.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        world.project(BodyId::new(1), ProjectionAxis::Y, Point::new(0.0, 400.0));
        let body = world.get_body(BodyId::new(1)).unwrap();
        let rest = body.rest_position.unwrap();
        assert!((rest.x - 100.0).abs() < f32::EPSILON); // X unchanged
        assert!((rest.y - 400.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_hit_test() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        world.add_body(body);

        assert_eq!(
            world.hit_test(&Point::new(50.0, 50.0)),
            Some(BodyId::new(1))
        );
        assert_eq!(world.hit_test(&Point::new(200.0, 200.0)), None);
    }

    #[test]
    fn test_bodies_in_region() {
        let mut world = PhysicsWorld::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(200.0, 200.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body1);
        world.add_body(body2);

        let region = Rect::new(0.0, 0.0, 100.0, 100.0);
        let bodies = world.bodies_in_region(&region);
        assert_eq!(bodies.len(), 1);
        assert!(bodies.contains(&BodyId::new(1)));
    }

    #[test]
    fn test_drag_state() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        // Initially idle
        assert_eq!(world.drag_state(BodyId::new(1)), PhysicsDragState::Idle);

        // After drag, should be dragging
        world.drag(BodyId::new(1), Point::new(100.0, 100.0));
        assert_eq!(world.drag_state(BodyId::new(1)), PhysicsDragState::Dragging);

        // After release, should be released
        world.drag_release(BodyId::new(1));
        assert_eq!(world.drag_state(BodyId::new(1)), PhysicsDragState::Released);
    }

    #[test]
    fn test_static_body_cannot_drag() {
        let mut world = PhysicsWorld::new();
        let body =
            PhysicsBody::static_body(BodyId::new(1), Point::ORIGIN, CollisionShape::circle(10.0));
        world.add_body(body);

        let state = world.drag(BodyId::new(1), Point::new(100.0, 100.0));
        assert_eq!(state, None);
    }

    #[test]
    fn test_physical_properties_presets() {
        let icon_props = PhysicalProperties::dock_icon();
        assert!(icon_props.mass > 0.0 && icon_props.mass < f32::INFINITY);
        assert!(icon_props.stiffness > 0.0);

        let bg_props = PhysicalProperties::static_background();
        assert_eq!(bg_props.mass, f32::INFINITY);

        let panel_props = PhysicalProperties::kinematic_panel();
        assert_eq!(panel_props.mass, f32::INFINITY);

        let notif_props = PhysicalProperties::floating_notification();
        assert!(notif_props.mass < icon_props.mass);

        let drag_props = PhysicalProperties::dragged_object();
        assert!(drag_props.stiffness > icon_props.stiffness);
    }

    #[test]
    fn test_physics_object_creation() {
        let node = NodeId::new(42);
        let obj =
            PhysicsObject::dock_icon(node, Point::new(100.0, 100.0), CollisionShape::circle(25.0));

        assert_eq!(obj.node, node);
        assert_eq!(obj.body_id(), BodyId::new(42));
        assert!(obj.enabled);
        assert!(obj.properties.is_some());
    }

    #[test]
    fn test_physics_object_disable() {
        let node = NodeId::new(1);
        let mut obj = PhysicsObject::dock_icon(node, Point::ORIGIN, CollisionShape::circle(10.0));

        obj.disable();
        assert!(!obj.enabled);
        assert!(!obj.body.enabled);

        obj.enable();
        assert!(obj.enabled);
        assert!(obj.body.enabled);
    }

    #[test]
    fn test_physical_properties_apply_to() {
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );

        let props = PhysicalProperties {
            mass: 5.0,
            friction: 0.8,
            restitution: 0.6,
            linear_damping: 0.9,
            angular_damping: 0.7,
            stiffness: 1000.0,
        };

        props.apply_to(&mut body);
        assert!((body.mass - 5.0).abs() < f32::EPSILON);
        assert!((body.inv_mass - 0.2).abs() < f32::EPSILON);
        assert!((body.material.friction - 0.8).abs() < f32::EPSILON);
        assert!((body.material.restitution - 0.6).abs() < f32::EPSILON);
        assert!((body.damping.linear - 0.9).abs() < f32::EPSILON);
        assert!((body.damping.angular - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_node_id() {
        let node = NodeId::new(123);
        assert_eq!(node.as_u64(), 123);

        let node2: NodeId = 456.into();
        assert_eq!(node2.as_u64(), 456);
    }

    #[test]
    fn test_merge() {
        let mut world = PhysicsWorld::new();
        let mut body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(100.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body1.tag = Some("group:1".to_string());
        body2.tag = Some("group:2".to_string());
        world.add_body(body1);
        world.add_body(body2);

        let result = world.merge(GroupId::new(1), GroupId::new(2));
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.merged_group, GroupId::new(1));
        assert_eq!(result.body_count, 2);
    }

    #[test]
    fn test_cut() {
        let mut world = PhysicsWorld::new();
        let mut body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let mut body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(150.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body1.tag = Some("group:1".to_string());
        body2.tag = Some("group:1".to_string());
        world.add_body(body1);
        world.add_body(body2);

        let result = world.cut(GroupId::new(1), Point::new(100.0, 0.0));
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.cut_bodies.len(), 1);
        assert!(result.cut_bodies.contains(&BodyId::new(2)));
    }

    // ---------------------------------------------------------------------
    // Direct physics control tests
    // ---------------------------------------------------------------------

    #[test]
    fn test_apply_force() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        let force = Vector2D::new(10.0, 0.0);
        assert!(world.apply_force(BodyId::new(1), force));
        let body = world.get_body(BodyId::new(1)).unwrap();
        // Velocity should increase in the direction of the force
        assert!(body.velocity.x > 0.0);
        assert!((body.velocity.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_apply_force_nonexistent() {
        let mut world = PhysicsWorld::new();
        assert!(!world.apply_force(BodyId::new(999), Vector2D::new(1.0, 0.0)));
    }

    #[test]
    fn test_apply_impulse() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        let impulse = Vector2D::new(5.0, 3.0);
        assert!(world.apply_impulse(BodyId::new(1), impulse));
        let body = world.get_body(BodyId::new(1)).unwrap();
        // With mass=1, velocity should equal impulse
        assert!((body.velocity.x - 5.0).abs() < f32::EPSILON);
        assert!((body.velocity.y - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_set_velocity() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        let velocity = Vector2D::new(100.0, -50.0);
        assert!(world.set_velocity(BodyId::new(1), velocity));
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.velocity.x - 100.0).abs() < f32::EPSILON);
        assert!((body.velocity.y - (-50.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_anchor_and_release() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        // Initially not anchored
        assert!(!world.is_anchored(BodyId::new(1)));

        // Anchor the body
        assert!(world.anchor(BodyId::new(1), Point::new(200.0, 200.0)));
        assert!(world.is_anchored(BodyId::new(1)));

        let body = world.get_body(BodyId::new(1)).unwrap();
        assert_eq!(body.body_type, BodyType::Static);
        assert!((body.position.x - 200.0).abs() < f32::EPSILON);
        assert!((body.position.y - 200.0).abs() < f32::EPSILON);
        assert_eq!(body.velocity, Vector2D::ZERO);

        // Release the anchor
        assert!(world.release_anchor(BodyId::new(1)));
        assert!(!world.is_anchored(BodyId::new(1)));

        let body = world.get_body(BodyId::new(1)).unwrap();
        assert_eq!(body.body_type, BodyType::Dynamic);
    }

    #[test]
    fn test_anchor_nonexistent() {
        let mut world = PhysicsWorld::new();
        assert!(!world.anchor(BodyId::new(999), Point::ORIGIN));
    }

    #[test]
    fn test_release_anchor_nonexistent() {
        let mut world = PhysicsWorld::new();
        assert!(!world.release_anchor(BodyId::new(999)));
    }

    #[test]
    fn test_release_unanchored_body() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        // Releasing a body that was never anchored should return false
        assert!(!world.release_anchor(BodyId::new(1)));
    }

    #[test]
    fn test_acceleration_tracking() {
        let mut world = PhysicsWorld::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);

        // Apply force
        world.apply_force(BodyId::new(1), Vector2D::new(10.0, 0.0));
        let body = world.get_body(BodyId::new(1)).unwrap();
        // Acceleration should be tracked
        assert!(body.acceleration.x > 0.0);

        // After a step, acceleration should be reset
        world.step();
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!((body.acceleration.x - 0.0).abs() < f32::EPSILON);
        assert!((body.acceleration.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_force_vs_structural_operations() {
        // Demonstrate the distinction between force-based and structural operations.
        // Force-based: push changes velocity
        // Structural: insert creates a new body with layout shift
        let mut world = PhysicsWorld::new();

        // Force-based: push
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        world.add_body(body);
        world.push(BodyId::new(1), Vector2D::new(1.0, 0.0), 100.0);
        let body = world.get_body(BodyId::new(1)).unwrap();
        assert!(body.velocity.x > 0.0);

        // Structural: insert
        let result = world.insert(
            NodeId::new(2),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
        );
        assert!(world.has_body(result.body_id));
        assert!(result.rest_position.x > 0.0);
    }
}
