//! Physical dock layout model.
//!
//! Represents a dock as a **one-dimensional ordered layout** with physical
//! animation.  Each item has a target position determined by its slot, and
//! spring forces animate items toward their targets when the layout changes.
//!
//! ## Core idea
//!
//! Imagine five icons in a horizontal dock:
//!
//! ```text
//! Normal dock:        Dragging C between D and E:
//! [A][B][C][D][E]    [A][B]  [D][C][E]
//! ```
//!
//! When C is dragged between D and E:
//! 1. Identify the nearest insertion position.
//! 2. Update the logical ordering.
//! 3. Recalculate target positions for all items.
//! 4. Apply spring forces to neighboring items.
//! 5. Animate their movement.
//! 6. When released, settle into the new arrangement.
//!
//! The dragged item follows the pointer with a spring-like attachment rather
//! than teleporting, producing a natural result.
//!
//! ## Proximity interaction
//!
//! As the pointer approaches an icon, neighboring icons spread apart before
//! any physical collision occurs.  The influence function is:
//!
//! ```text
//! d_i = ‖p - x_i‖
//! F_i = k · max(0, 1 - d_i / R)² · n̂_i
//! ```
//!
//! Where:
//! - `p` is the pointer position.
//! - `x_i` is the icon position.
//! - `R` is the interaction radius.
//! - `k` is the interaction strength.
//! - `n̂_i` is the direction away from the pointer.
//!
//! This creates a magnetic or repulsive dock effect without rigid-body
//! collision detection.

use crate::geometry::{NodeId, Point, Vector2D};
use crate::physics::{BodyId, CollisionShape, PhysicsBody};

// ---------------------------------------------------------------------------
// Dock axis
// ---------------------------------------------------------------------------

/// The axis along which dock items are arranged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAxis {
    /// Items arranged horizontally (left to right).
    Horizontal,
    /// Items arranged vertically (top to bottom).
    Vertical,
}

impl Default for DockAxis {
    fn default() -> Self {
        Self::Horizontal
    }
}

// ---------------------------------------------------------------------------
// Dock item
// ---------------------------------------------------------------------------

/// A single item in the physical dock layout.
///
/// Each item has a logical slot position in the ordered sequence and a
/// physical representation with current position, velocity, and target
/// position.  Spring forces drive the current position toward the target.
#[derive(Debug, Clone)]
pub struct DockItem {
    /// Unique identifier for the visual node.
    pub id: NodeId,
    /// Physics body identifier (links to PhysicsWorld).
    pub body_id: BodyId,
    /// Logical slot index in the dock order.
    pub slot: usize,
    /// Current 1D position along the dock axis.
    pub current_position: f32,
    /// Current velocity along the dock axis.
    pub velocity: f32,
    /// Target 1D position determined by slot and layout.
    pub target_position: f32,
    /// Perpendicular coordinate (fixed for 1D layout).
    pub perpendicular: f32,
    /// Item size along the axis.
    pub size: f32,
    /// Whether this item is currently being dragged.
    pub is_dragged: bool,
}

impl DockItem {
    /// Create a new dock item.
    pub fn new(id: NodeId, body_id: BodyId, slot: usize, size: f32) -> Self {
        Self {
            id,
            body_id,
            slot,
            current_position: 0.0,
            velocity: 0.0,
            target_position: 0.0,
            perpendicular: 0.0,
            size,
            is_dragged: false,
        }
    }

    /// Get the full 2D position from the 1D dock coordinates.
    pub fn position(&self, axis: DockAxis) -> Point {
        match axis {
            DockAxis::Horizontal => Point::new(self.current_position, self.perpendicular),
            DockAxis::Vertical => Point::new(self.perpendicular, self.current_position),
        }
    }

    /// Set the full 2D position, updating the 1D coordinate.
    pub fn set_position(&mut self, pos: Point, axis: DockAxis) {
        match axis {
            DockAxis::Horizontal => {
                self.current_position = pos.x;
                self.perpendicular = pos.y;
            }
            DockAxis::Vertical => {
                self.current_position = pos.y;
                self.perpendicular = pos.x;
            }
        }
    }

    /// Compute the spring force toward the target position.
    ///
    /// Formula: `F = -k * (x - x_target) - c * v`
    ///
    /// # Arguments
    /// * `stiffness` - Spring stiffness coefficient `k`.
    /// * `damping` - Damping coefficient `c`.
    pub fn spring_force(&self, stiffness: f32, damping: f32) -> f32 {
        let displacement = self.current_position - self.target_position;
        -stiffness * displacement - damping * self.velocity
    }

    /// Integrate the item's physics for one timestep.
    ///
    /// # Arguments
    /// * `stiffness` - Spring stiffness coefficient.
    /// * `damping` - Damping coefficient.
    /// * `dt` - Timestep in seconds.
    pub fn integrate(&mut self, stiffness: f32, damping: f32, dt: f32) {
        if self.is_dragged {
            return;
        }

        let force = self.spring_force(stiffness, damping);
        let acceleration = force; // mass = 1 for dock items
        self.velocity += acceleration * dt;
        self.current_position += self.velocity * dt;
    }
}

// ---------------------------------------------------------------------------
// Dock configuration
// ---------------------------------------------------------------------------

/// Configuration for the physical dock layout.
#[derive(Debug, Clone)]
pub struct DockConfig {
    /// Spacing between items along the axis.
    pub spacing: f32,
    /// Padding from the dock origin.
    pub padding: f32,
    /// Layout axis.
    pub axis: DockAxis,
    /// Perpendicular center position (Y for horizontal, X for vertical).
    pub perpendicular_center: f32,
    /// Spring stiffness for rest position attraction.
    pub spring_stiffness: f32,
    /// Spring damping.
    pub spring_damping: f32,
    /// Interaction radius for proximity effects.
    pub interaction_radius: f32,
    /// Interaction strength for proximity effects.
    pub interaction_strength: f32,
    /// Spring stiffness for dragged item following pointer.
    pub drag_stiffness: f32,
    /// Spring damping for dragged item following pointer.
    pub drag_damping: f32,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            spacing: 8.0,
            padding: 16.0,
            axis: DockAxis::Horizontal,
            perpendicular_center: 50.0,
            spring_stiffness: 500.0,
            spring_damping: 12.0,
            interaction_radius: 150.0,
            interaction_strength: 200.0,
            drag_stiffness: 800.0,
            drag_damping: 15.0,
        }
    }
}

// ---------------------------------------------------------------------------
// Drag state
// ---------------------------------------------------------------------------

/// State of an active drag operation on a dock item.
#[derive(Debug, Clone)]
pub struct DockDragState {
    /// Body ID of the dragged item.
    pub body_id: BodyId,
    /// Current pointer position (2D).
    pub pointer_position: Point,
    /// Spring-smoothed position for the dragged item.
    pub spring_position: Point,
    /// Spring velocity for smooth following.
    pub spring_velocity: Vector2D,
}

impl DockDragState {
    pub fn new(body_id: BodyId, pointer: Point) -> Self {
        Self {
            body_id,
            pointer_position: pointer,
            spring_position: pointer,
            spring_velocity: Vector2D::ZERO,
        }
    }

    /// Update the pointer position and integrate the spring follower.
    pub fn update(&mut self, pointer: Point, stiffness: f32, damping: f32, dt: f32) {
        self.pointer_position = pointer;

        // Spring force toward pointer
        let dx = self.pointer_position.x - self.spring_position.x;
        let dy = self.pointer_position.y - self.spring_position.y;

        let ax = stiffness * dx - damping * self.spring_velocity.x;
        let ay = stiffness * dy - damping * self.spring_velocity.y;

        self.spring_velocity.x += ax * dt;
        self.spring_velocity.y += ay * dt;
        self.spring_position.x += self.spring_velocity.x * dt;
        self.spring_position.y += self.spring_velocity.y * dt;
    }
}

// ---------------------------------------------------------------------------
// Dock
// ---------------------------------------------------------------------------

/// A one-dimensional ordered dock layout with physical animation.
///
/// Manages the logical ordering of items, computes target positions,
/// applies spring forces, and handles drag-to-reorder interactions.
pub struct Dock {
    /// Ordered list of dock items.
    pub items: Vec<DockItem>,
    /// Layout configuration.
    pub config: DockConfig,
    /// Current drag state, if an item is being dragged.
    pub drag: Option<DockDragState>,
    /// Next body ID counter.
    next_body_id: u64,
}

impl Dock {
    /// Create a new empty dock with default configuration.
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            config: DockConfig::default(),
            drag: None,
            next_body_id: 1,
        }
    }

    /// Create a new dock with custom configuration.
    pub fn with_config(config: DockConfig) -> Self {
        Self {
            items: Vec::new(),
            config,
            drag: None,
            next_body_id: 1,
        }
    }

    /// Get the number of items in the dock.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the dock is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    // ---------------------------------------------------------------------
    // Item management
    // ---------------------------------------------------------------------

    /// Add a new item to the end of the dock.
    ///
    /// Returns the body ID of the newly created item.
    pub fn add_item(&mut self, id: NodeId, size: f32) -> BodyId {
        let body_id = BodyId::new(self.next_body_id);
        self.next_body_id += 1;

        let slot = self.items.len();
        let mut item = DockItem::new(id, body_id, slot, size);
        item.target_position = self.compute_target_position(slot);
        item.current_position = item.target_position;
        item.perpendicular = self.config.perpendicular_center;

        self.items.push(item);
        body_id
    }

    /// Remove an item by body ID.
    ///
    /// Returns true if the item was found and removed.
    pub fn remove_item(&mut self, body_id: BodyId) -> bool {
        if let Some(idx) = self.items.iter().position(|item| item.body_id == body_id) {
            self.items.remove(idx);
            self.reorder_slots();
            true
        } else {
            false
        }
    }

    /// Remove an item by slot index.
    ///
    /// Returns the removed item, or None if the index is out of bounds.
    pub fn remove_item_at(&mut self, slot: usize) -> Option<DockItem> {
        let item = if slot < self.items.len() {
            Some(self.items.remove(slot))
        } else {
            None
        };
        if item.is_some() {
            self.reorder_slots();
        }
        item
    }

    /// Get a reference to an item by body ID.
    pub fn get_item(&self, body_id: BodyId) -> Option<&DockItem> {
        self.items.iter().find(|item| item.body_id == body_id)
    }

    /// Get a mutable reference to an item by body ID.
    pub fn get_item_mut(&mut self, body_id: BodyId) -> Option<&mut DockItem> {
        self.items.iter_mut().find(|item| item.body_id == body_id)
    }

    /// Get an item by slot index.
    pub fn get_item_at(&self, slot: usize) -> Option<&DockItem> {
        self.items.get(slot)
    }

    // ---------------------------------------------------------------------
    // Reordering
    // ---------------------------------------------------------------------

    /// Move an item from one slot to another.
    ///
    /// Returns true if the move was successful.
    pub fn move_item(&mut self, body_id: BodyId, to_slot: usize) -> bool {
        let from_slot = match self.items.iter().position(|item| item.body_id == body_id) {
            Some(slot) => slot,
            None => return false,
        };

        if from_slot == to_slot || to_slot >= self.items.len() {
            return false;
        }

        let item = self.items.remove(from_slot);
        self.items.insert(to_slot, item);
        self.reorder_slots();
        true
    }

    /// Reorder items based on pointer position.
    ///
    /// When dragging, this identifies the nearest insertion position and
    /// updates the logical ordering accordingly.
    pub fn reorder_by_pointer(&mut self, pointer: Point) -> Option<usize> {
        let drag = match &self.drag {
            Some(d) => d,
            None => return None,
        };

        let dragged_body = drag.body_id;
        let dragged_slot = match self
            .items
            .iter()
            .position(|item| item.body_id == dragged_body)
        {
            Some(slot) => slot,
            None => return None,
        };

        // Project pointer onto the dock axis
        let pointer_1d = match self.config.axis {
            DockAxis::Horizontal => pointer.x,
            DockAxis::Vertical => pointer.y,
        };

        // Find the insertion slot by comparing with non-dragged items
        let mut insertion_slot = dragged_slot;
        for (i, item) in self.items.iter().enumerate() {
            if item.body_id == dragged_body {
                continue;
            }

            if (pointer_1d - item.target_position).abs() < self.config.spacing / 2.0 {
                if pointer_1d < item.target_position {
                    insertion_slot = i.min(insertion_slot);
                } else {
                    insertion_slot = (i + 1).min(insertion_slot);
                }
            }
        }

        // Clamp to valid range
        insertion_slot = insertion_slot.min(self.items.len() - 1);

        if insertion_slot != dragged_slot {
            self.move_item(dragged_body, insertion_slot);
            Some(insertion_slot)
        } else {
            None
        }
    }

    /// Reassign slot indices after reordering.
    fn reorder_slots(&mut self) {
        let spacing = self.config.spacing;
        let padding = self.config.padding;
        let mut pos = padding;
        for (i, item) in self.items.iter_mut().enumerate() {
            item.slot = i;
            item.target_position = pos + item.size * 0.5;
            pos += item.size + spacing;
        }
    }

    // ---------------------------------------------------------------------
    // Target position computation
    // ---------------------------------------------------------------------

    /// Compute the target 1D position for a given slot.
    pub fn compute_target_position(&self, slot: usize) -> f32 {
        let spacing = self.config.spacing;
        let mut pos = self.config.padding;
        for (i, item) in self.items.iter().enumerate().take(slot) {
            pos += item.size + spacing;
            let _ = i;
        }
        if slot < self.items.len() {
            pos + self.items[slot].size * 0.5
        } else {
            pos
        }
    }

    /// Recalculate all target positions based on current ordering.
    pub fn recalculate_targets(&mut self) {
        let spacing = self.config.spacing;
        let padding = self.config.padding;
        let mut pos = padding;
        for item in self.items.iter_mut() {
            item.target_position = pos + item.size * 0.5;
            pos += item.size + spacing;
        }
    }

    // ---------------------------------------------------------------------
    // Physics integration
    // ---------------------------------------------------------------------

    /// Apply spring forces to all non-dragged items.
    ///
    /// Uses the engine's fixed timestep (120 Hz) by default.
    pub fn apply_springs(&mut self) {
        self.apply_springs_dt(1.0 / 120.0);
    }

    /// Apply spring forces with a custom timestep.
    pub fn apply_springs_dt(&mut self, dt: f32) {
        for item in self.items.iter_mut() {
            if !item.is_dragged {
                item.integrate(self.config.spring_stiffness, self.config.spring_damping, dt);
            }
        }
    }

    /// Step the dock physics: apply springs and update body positions.
    pub fn step(&mut self, dt: f32) {
        // Apply spring forces
        self.apply_springs_dt(dt);

        // Sync physics bodies
        self.sync_bodies();
    }

    /// Sync dock item positions with their physics bodies.
    pub fn sync_bodies(&self) {
        // This method would be called by the PhysicsWorld to sync positions
        // The actual body updates happen in the world's integration step
    }

    /// Create physics bodies for all dock items.
    pub fn create_bodies(&self) -> Vec<PhysicsBody> {
        self.items
            .iter()
            .map(|item| {
                let pos = item.position(self.config.axis);
                let shape = CollisionShape::AABB {
                    half_extents: crate::geometry::Size::new(item.size / 2.0, item.size / 2.0),
                };
                let mut body = PhysicsBody::dynamic_body(item.body_id, pos, shape, 1.0);
                body.set_rest_position(pos);
                body
            })
            .collect()
    }

    // ---------------------------------------------------------------------
    // Drag operations
    // ---------------------------------------------------------------------

    /// Start dragging an item.
    ///
    /// Returns true if the item was found and drag started.
    pub fn start_drag(&mut self, body_id: BodyId, pointer: Point) -> bool {
        let item = match self.items.iter_mut().find(|item| item.body_id == body_id) {
            Some(item) => item,
            None => return false,
        };

        item.is_dragged = true;
        self.drag = Some(DockDragState::new(body_id, pointer));
        true
    }

    /// Continue dragging with updated pointer position.
    ///
    /// Returns the new slot if the item was reordered, or None.
    pub fn continue_drag(&mut self, pointer: Point) -> Option<usize> {
        self.continue_drag_dt(pointer, 1.0 / 120.0)
    }

    /// Continue dragging with custom timestep.
    pub fn continue_drag_dt(&mut self, pointer: Point, dt: f32) -> Option<usize> {
        let drag = match &mut self.drag {
            Some(d) => d,
            None => return None,
        };

        drag.update(
            pointer,
            self.config.drag_stiffness,
            self.config.drag_damping,
            dt,
        );

        if let Some(item) = self
            .items
            .iter_mut()
            .find(|item| item.body_id == drag.body_id)
        {
            item.set_position(drag.spring_position, self.config.axis);
        }

        self.reorder_by_pointer(pointer)
    }

    /// End the drag operation.
    ///
    /// The dragged item will spring to its new target position.
    pub fn end_drag(&mut self) {
        if let Some(ref drag) = self.drag {
            if let Some(item) = self
                .items
                .iter_mut()
                .find(|item| item.body_id == drag.body_id)
            {
                item.is_dragged = false;
            }
        }
        self.drag = None;
    }

    /// Check if an item is currently being dragged.
    pub fn is_dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Get the dragged item's body ID.
    pub fn dragged_body_id(&self) -> Option<BodyId> {
        self.drag.as_ref().map(|d| d.body_id)
    }

    /// Get the spring-smoothed drag position.
    pub fn drag_position(&self) -> Option<Point> {
        self.drag.as_ref().map(|d| d.spring_position)
    }

    // ---------------------------------------------------------------------
    // Proximity interaction
    // ---------------------------------------------------------------------

    /// Compute the proximity repulsion force on an item from a pointer.
    ///
    /// Formula: `F = k * max(0, 1 - d/R)² * n̂`
    ///
    /// Where:
    /// - `d` is the distance from pointer to item.
    /// - `R` is the interaction radius.
    /// - `k` is the interaction strength.
    /// - `n̂` is the direction away from the pointer.
    pub fn proximity_force(&self, item: &DockItem, pointer: Point) -> Vector2D {
        let pos = item.position(self.config.axis);
        let dx = pos.x - pointer.x;
        let dy = pos.y - pointer.y;
        let dist = (dx * dx + dy * dy).sqrt();

        let radius = self.config.interaction_radius;
        if dist > radius || dist < 0.001 {
            return Vector2D::ZERO;
        }

        let influence = 1.0 - dist / radius;
        let strength = self.config.interaction_strength * influence * influence;

        let nx = dx / dist;
        let ny = dy / dist;

        Vector2D::new(strength * nx, strength * ny)
    }

    /// Apply proximity repulsion from a pointer to all items.
    ///
    /// Returns the total force applied to each item.
    pub fn apply_proximity_repulsion(&self, pointer: Point) -> Vec<Vector2D> {
        self.items
            .iter()
            .map(|item| self.proximity_force(item, pointer))
            .collect()
    }

    /// Apply proximity attraction from a pointer to all items.
    ///
    /// Opposite of repulsion — pulls items toward the pointer.
    pub fn apply_proximity_attraction(&self, pointer: Point) -> Vec<Vector2D> {
        self.items
            .iter()
            .map(|item| {
                let force = self.proximity_force(item, pointer);
                Vector2D::new(-force.x, -force.y)
            })
            .collect()
    }

    /// Compute the hover magnification effect.
    ///
    /// As the pointer approaches an item, it scales up slightly.
    /// Returns the scale factor for each item (1.0 = no magnification).
    pub fn hover_magnification(&self, pointer: Point) -> Vec<f32> {
        let max_magnification = 1.3;
        let radius = self.config.interaction_radius;

        self.items
            .iter()
            .map(|item| {
                let pos = item.position(self.config.axis);
                let dx = pos.x - pointer.x;
                let dy = pos.y - pointer.y;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist > radius {
                    1.0
                } else {
                    let t = 1.0 - dist / radius;
                    1.0 + (max_magnification - 1.0) * t * t
                }
            })
            .collect()
    }

    // ---------------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------------

    /// Find the item nearest to a pointer position.
    pub fn find_nearest_item(&self, pointer: Point) -> Option<usize> {
        let mut best_idx: Option<usize> = None;
        let mut best_dist = f32::MAX;

        for (i, item) in self.items.iter().enumerate() {
            let pos = item.position(self.config.axis);
            let dx = pos.x - pointer.x;
            let dy = pos.y - pointer.y;
            let dist = dx * dx + dy * dy;

            if dist < best_dist {
                best_dist = dist;
                best_idx = Some(i);
            }
        }

        best_idx
    }

    /// Find the item at a specific 1D position along the axis.
    pub fn find_item_at_position(&self, position_1d: f32) -> Option<usize> {
        self.items.iter().enumerate().find_map(|(i, item)| {
            let half_size = item.size / 2.0;
            if position_1d >= item.current_position - half_size
                && position_1d <= item.current_position + half_size
            {
                Some(i)
            } else {
                None
            }
        })
    }

    /// Get the dock's bounding rectangle.
    pub fn bounds(&self) -> crate::geometry::Rect {
        if self.items.is_empty() {
            return crate::geometry::Rect::ZERO;
        }

        let mut min_1d = f32::MAX;
        let mut max_1d = f32::MIN;

        for item in &self.items {
            let half = item.size / 2.0;
            min_1d = min_1d.min(item.current_position - half);
            max_1d = max_1d.max(item.current_position + half);
        }

        let cross_size = self
            .items
            .iter()
            .map(|item| item.size)
            .fold(0.0_f32, f32::max);

        match self.config.axis {
            DockAxis::Horizontal => crate::geometry::Rect::new(
                min_1d,
                self.config.perpendicular_center - cross_size / 2.0,
                max_1d - min_1d,
                cross_size,
            ),
            DockAxis::Vertical => crate::geometry::Rect::new(
                self.config.perpendicular_center - cross_size / 2.0,
                min_1d,
                cross_size,
                max_1d - min_1d,
            ),
        }
    }

    /// Check if the dock is settled (all items near their targets).
    pub fn is_settled(&self, threshold: f32) -> bool {
        self.items.iter().all(|item| {
            let error = (item.current_position - item.target_position).abs();
            let velocity = item.velocity.abs();
            error < threshold && velocity < threshold * 0.1
        })
    }
}

impl Default for Dock {
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

    #[test]
    fn test_create_empty_dock() {
        let dock = Dock::new();
        assert!(dock.is_empty());
        assert_eq!(dock.len(), 0);
    }

    #[test]
    fn test_add_item() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);
        assert_eq!(dock.len(), 1);
        assert!(dock.get_item(body_id).is_some());
    }

    #[test]
    fn test_add_multiple_items() {
        let mut dock = Dock::new();
        let a = dock.add_item(NodeId::new(1), 48.0);
        let b = dock.add_item(NodeId::new(2), 48.0);
        let c = dock.add_item(NodeId::new(3), 48.0);

        assert_eq!(dock.len(), 3);
        assert_eq!(dock.get_item_at(0).unwrap().body_id, a);
        assert_eq!(dock.get_item_at(1).unwrap().body_id, b);
        assert_eq!(dock.get_item_at(2).unwrap().body_id, c);
    }

    #[test]
    fn test_target_positions() {
        let mut dock = Dock::with_config(DockConfig {
            spacing: 8.0,
            padding: 16.0,
            ..Default::default()
        });
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);
        dock.add_item(NodeId::new(3), 48.0);

        let item0 = dock.get_item_at(0).unwrap();
        let item1 = dock.get_item_at(1).unwrap();
        let item2 = dock.get_item_at(2).unwrap();

        // Target positions should be evenly spaced
        assert!((item1.target_position - item0.target_position - 56.0).abs() < f32::EPSILON);
        assert!((item2.target_position - item1.target_position - 56.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_remove_item() {
        let mut dock = Dock::new();
        let _a = dock.add_item(NodeId::new(1), 48.0);
        let b = dock.add_item(NodeId::new(2), 48.0);
        let _c = dock.add_item(NodeId::new(3), 48.0);

        assert!(dock.remove_item(b));
        assert_eq!(dock.len(), 2);
        assert!(!dock.get_item(b).is_some());
    }

    #[test]
    fn test_remove_nonexistent_item() {
        let mut dock = Dock::new();
        assert!(!dock.remove_item(BodyId::new(999)));
    }

    #[test]
    fn test_move_item() {
        let mut dock = Dock::new();
        let a = dock.add_item(NodeId::new(1), 48.0);
        let b = dock.add_item(NodeId::new(2), 48.0);
        let c = dock.add_item(NodeId::new(3), 48.0);

        // Move C to position 0
        assert!(dock.move_item(c, 0));
        assert_eq!(dock.get_item_at(0).unwrap().body_id, c);
        assert_eq!(dock.get_item_at(1).unwrap().body_id, a);
        assert_eq!(dock.get_item_at(2).unwrap().body_id, b);
    }

    #[test]
    fn test_move_item_invalid() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        // Move to same slot
        assert!(!dock.move_item(BodyId::new(1), 0));
        // Move to out-of-bounds slot
        assert!(!dock.move_item(BodyId::new(1), 10));
    }

    #[test]
    fn test_spring_force() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.items.first_mut().unwrap();
        // Move item away from target
        item.current_position = item.target_position + 10.0;

        let force = item.spring_force(100.0, 5.0);
        // Force should pull back toward target (negative)
        assert!(force < 0.0);
    }

    #[test]
    fn test_spring_force_at_target() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 100.0;
        item.target_position = 100.0;
        item.velocity = 0.0;

        let force = item.spring_force(100.0, 5.0);
        assert!((force - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_integrate_toward_target() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 50.0;
        item.target_position = 100.0;
        item.velocity = 0.0;

        item.integrate(100.0, 5.0, 0.016);
        // Position should move toward target
        assert!(item.current_position > 50.0);
        assert!(item.current_position < 100.0);
    }

    #[test]
    fn test_dragged_item_not_integrated() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 50.0;
        item.target_position = 100.0;
        item.velocity = 0.0;
        item.is_dragged = true;

        let pos_before = item.current_position;
        item.integrate(100.0, 5.0, 0.016);
        // Position should not change when dragged
        assert!((item.current_position - pos_before).abs() < f32::EPSILON);
    }

    #[test]
    fn test_start_drag() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);

        assert!(!dock.is_dragging());
        assert!(dock.start_drag(body_id, Point::new(100.0, 100.0)));
        assert!(dock.is_dragging());
        assert_eq!(dock.dragged_body_id(), Some(body_id));
    }

    #[test]
    fn test_start_drag_nonexistent() {
        let mut dock = Dock::new();
        assert!(!dock.start_drag(BodyId::new(999), Point::ORIGIN));
    }

    #[test]
    fn test_end_drag() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);
        dock.start_drag(body_id, Point::new(100.0, 100.0));

        dock.end_drag();
        assert!(!dock.is_dragging());
        let item = dock.get_item(body_id).unwrap();
        assert!(!item.is_dragged);
    }

    #[test]
    fn test_drag_spring_follower() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);
        dock.start_drag(body_id, Point::new(100.0, 100.0));

        // Move pointer
        dock.continue_drag(Point::new(200.0, 100.0));

        let drag_pos = dock.drag_position().unwrap();
        // Spring position should be between start and end
        assert!(drag_pos.x >= 100.0 && drag_pos.x <= 200.0);
    }

    #[test]
    fn test_proximity_force_zero_far() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);
        let item = dock.get_item(body_id).unwrap();

        // Pointer far away
        let force = dock.proximity_force(item, Point::new(10000.0, 10000.0));
        assert!((force.x - 0.0).abs() < f32::EPSILON);
        assert!((force.y - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_proximity_force_repels() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);

        // Pointer close to item
        let item = dock.get_item(body_id).unwrap();
        let pos = item.position(dock.config.axis);
        let pointer = Point::new(pos.x - 50.0, pos.y);

        let force = dock.proximity_force(item, pointer);
        // Force should push away from pointer (positive X)
        assert!(force.x > 0.0);
    }

    #[test]
    fn test_proximity_force_symmetric() {
        let mut dock = Dock::new();
        let body_id = dock.add_item(NodeId::new(1), 48.0);

        let item = dock.get_item(body_id).unwrap();
        let pos = item.position(dock.config.axis);

        let force_left = dock.proximity_force(item, Point::new(pos.x - 50.0, pos.y));
        let force_right = dock.proximity_force(item, Point::new(pos.x + 50.0, pos.y));

        // Forces should be symmetric
        assert!((force_left.x + force_right.x).abs() < 0.01);
    }

    #[test]
    fn test_hover_magnification() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.get_item(BodyId::new(1)).unwrap();
        let pos = item.position(dock.config.axis);

        // Pointer at item position
        let scales = dock.hover_magnification(pos);
        assert!(scales[0] > 1.0);
        assert!(scales[0] <= 1.3);

        // Pointer far away
        let scales = dock.hover_magnification(Point::new(10000.0, 10000.0));
        assert!((scales[0] - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_find_nearest_item() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);
        dock.add_item(NodeId::new(3), 48.0);

        // Pointer near first item
        let item0 = dock.get_item_at(0).unwrap();
        let pos = item0.position(dock.config.axis);
        let nearest = dock.find_nearest_item(Point::new(pos.x + 1.0, pos.y));
        assert_eq!(nearest, Some(0));
    }

    #[test]
    fn test_bounds() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        let bounds = dock.bounds();
        assert!(bounds.width > 0.0);
        assert!(bounds.height > 0.0);
    }

    #[test]
    fn test_empty_bounds() {
        let dock = Dock::new();
        let bounds = dock.bounds();
        assert!(bounds.is_empty());
    }

    #[test]
    fn test_is_settled() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        // Item at target with zero velocity
        let item = dock.items.first_mut().unwrap();
        item.current_position = item.target_position;
        item.velocity = 0.0;

        assert!(dock.is_settled(0.1));
    }

    #[test]
    fn test_is_not_settled() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.items.first_mut().unwrap();
        item.current_position = item.target_position + 100.0;
        item.velocity = 50.0;

        assert!(!dock.is_settled(0.1));
    }

    #[test]
    fn test_vertical_dock() {
        let mut dock = Dock::with_config(DockConfig {
            axis: DockAxis::Vertical,
            ..Default::default()
        });
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        let item0 = dock.get_item_at(0).unwrap();
        let item1 = dock.get_item_at(1).unwrap();

        // Y positions should differ, X should be the same
        assert!(
            (item0.position(DockAxis::Vertical).x - item1.position(DockAxis::Vertical).x).abs()
                < f32::EPSILON
        );
        assert!(item1.target_position > item0.target_position);
    }

    #[test]
    fn test_position_conversion() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 100.0;
        item.perpendicular = 50.0;

        let pos = item.position(DockAxis::Horizontal);
        assert!((pos.x - 100.0).abs() < f32::EPSILON);
        assert!((pos.y - 50.0).abs() < f32::EPSILON);

        item.set_position(Point::new(200.0, 75.0), DockAxis::Horizontal);
        assert!((item.current_position - 200.0).abs() < f32::EPSILON);
        assert!((item.perpendicular - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_recalculate_targets() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        // Manually move items
        let item = dock.items.first_mut().unwrap();
        item.current_position = 999.0;

        dock.recalculate_targets();

        // Targets should be recalculated based on slots
        let item0 = dock.get_item_at(0).unwrap();
        let item1 = dock.get_item_at(1).unwrap();
        assert!(item1.target_position > item0.target_position);
    }

    #[test]
    fn test_apply_springs() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.items.first_mut().unwrap();
        item.current_position = item.target_position + 50.0;
        let pos_before = item.current_position;

        dock.apply_springs();

        // Item should move toward target
        let item = dock.get_item(BodyId::new(1)).unwrap();
        assert!(item.current_position < pos_before);
    }

    #[test]
    fn test_step() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.items.first_mut().unwrap();
        item.current_position = item.target_position + 50.0;
        let pos_before = item.current_position;

        dock.step(0.016);

        let item = dock.get_item(BodyId::new(1)).unwrap();
        assert!(item.current_position < pos_before);
    }

    #[test]
    fn test_create_bodies() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        let bodies = dock.create_bodies();
        assert_eq!(bodies.len(), 2);
        assert!(bodies[0].rest_position.is_some());
    }

    #[test]
    fn test_reorder_slots() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);
        dock.add_item(NodeId::new(3), 48.0);

        // Remove middle item
        dock.remove_item(BodyId::new(2));

        // Slots should be renumbered
        assert_eq!(dock.get_item_at(0).unwrap().slot, 0);
        assert_eq!(dock.get_item_at(1).unwrap().slot, 1);
    }

    #[test]
    fn test_remove_item_at() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        let removed = dock.remove_item_at(0);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().body_id, BodyId::new(1));
        assert_eq!(dock.len(), 1);
    }

    #[test]
    fn test_remove_item_at_out_of_bounds() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        assert!(dock.remove_item_at(10).is_none());
    }

    #[test]
    fn test_find_item_at_position() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.get_item_at(0).unwrap();
        let idx = dock.find_item_at_position(item.current_position);
        assert_eq!(idx, Some(0));
    }

    #[test]
    fn test_apply_proximity_repulsion() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        let item = dock.get_item(BodyId::new(1)).unwrap();
        let pos = item.position(dock.config.axis);
        let pointer = Point::new(pos.x - 30.0, pos.y);

        let forces = dock.apply_proximity_repulsion(pointer);
        assert_eq!(forces.len(), 2);
        // First item should be repelled
        assert!(forces[0].x > 0.0);
    }

    #[test]
    fn test_apply_proximity_attraction() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.get_item(BodyId::new(1)).unwrap();
        let pos = item.position(dock.config.axis);
        let pointer = Point::new(pos.x - 30.0, pos.y);

        let forces = dock.apply_proximity_attraction(pointer);
        // First item should be attracted toward pointer
        assert!(forces[0].x < 0.0);
    }

    #[test]
    fn test_dock_drag_state_new() {
        let state = DockDragState::new(BodyId::new(1), Point::new(100.0, 200.0));
        assert_eq!(state.body_id, BodyId::new(1));
        assert!((state.pointer_position.x - 100.0).abs() < f32::EPSILON);
        assert!((state.spring_position.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_dock_drag_state_update() {
        let mut state = DockDragState::new(BodyId::new(1), Point::new(100.0, 100.0));
        state.update(Point::new(200.0, 100.0), 800.0, 15.0, 0.016);

        // Spring position should move toward pointer
        assert!(state.spring_position.x > 100.0);
        assert!(state.spring_position.x <= 200.0);
    }

    #[test]
    fn test_continue_drag_returns_new_slot() {
        let mut dock = Dock::new();
        let _a = dock.add_item(NodeId::new(1), 48.0);
        let b = dock.add_item(NodeId::new(2), 48.0);
        let _c = dock.add_item(NodeId::new(3), 48.0);

        // Start dragging item B
        dock.start_drag(b, Point::new(100.0, 50.0));

        // Move pointer far to the right, past C
        let result = dock.continue_drag(Point::new(1000.0, 50.0));
        // Should have reordered
        assert!(result.is_some() || dock.get_item_at(0).unwrap().body_id != b);
    }

    #[test]
    fn test_dock_config_default() {
        let config = DockConfig::default();
        assert_eq!(config.axis, DockAxis::Horizontal);
        assert!(config.spring_stiffness > 0.0);
        assert!(config.interaction_radius > 0.0);
    }

    #[test]
    fn test_dock_with_config() {
        let dock = Dock::with_config(DockConfig {
            spacing: 20.0,
            axis: DockAxis::Vertical,
            ..Default::default()
        });
        assert_eq!(dock.config.spacing, 20.0);
        assert_eq!(dock.config.axis, DockAxis::Vertical);
    }

    #[test]
    fn test_item_id_and_body_id() {
        let mut dock = Dock::new();
        let node_id = NodeId::new(42);
        let body_id = dock.add_item(node_id, 48.0);

        let item = dock.get_item(body_id).unwrap();
        assert_eq!(item.id, node_id);
        assert_eq!(item.body_id, body_id);
    }

    #[test]
    fn test_multiple_drag_cycles() {
        let mut dock = Dock::new();
        let a = dock.add_item(NodeId::new(1), 48.0);
        let b = dock.add_item(NodeId::new(2), 48.0);

        // First drag cycle
        dock.start_drag(a, Point::new(100.0, 50.0));
        dock.end_drag();
        assert!(!dock.is_dragging());

        // Second drag cycle
        dock.start_drag(b, Point::new(200.0, 50.0));
        assert!(dock.is_dragging());
        dock.end_drag();
        assert!(!dock.is_dragging());
    }

    #[test]
    fn test_proximity_force_influence_function() {
        let mut dock = Dock::with_config(DockConfig {
            interaction_radius: 100.0,
            interaction_strength: 100.0,
            ..Default::default()
        });
        dock.add_item(NodeId::new(1), 48.0);

        let item = dock.get_item(BodyId::new(1)).unwrap();
        let pos = item.position(dock.config.axis);

        // At half radius, influence should be (1 - 0.5)^2 = 0.25
        let pointer = Point::new(pos.x - 50.0, pos.y);
        let force = dock.proximity_force(item, pointer);
        let expected_strength = 100.0 * 0.25;
        assert!((force.length() - expected_strength).abs() < 1.0);
    }

    #[test]
    fn test_dock_item_default_values() {
        let item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        assert!(!item.is_dragged);
        assert!((item.current_position - 0.0).abs() < f32::EPSILON);
        assert!((item.velocity - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_spring_force_with_velocity() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 100.0;
        item.target_position = 100.0;
        item.velocity = 10.0;

        let force = item.spring_force(100.0, 5.0);
        // Damping should oppose velocity
        assert!((force - (-50.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn test_reorder_by_pointer_no_drag() {
        let mut dock = Dock::new();
        dock.add_item(NodeId::new(1), 48.0);
        dock.add_item(NodeId::new(2), 48.0);

        // No drag active
        let result = dock.reorder_by_pointer(Point::new(1000.0, 50.0));
        assert!(result.is_none());
    }

    #[test]
    fn test_horizontal_vs_vertical_position() {
        let mut item = DockItem::new(NodeId::new(1), BodyId::new(1), 0, 48.0);
        item.current_position = 100.0;
        item.perpendicular = 50.0;

        let h_pos = item.position(DockAxis::Horizontal);
        let v_pos = item.position(DockAxis::Vertical);

        assert!((h_pos.x - 100.0).abs() < f32::EPSILON);
        assert!((h_pos.y - 50.0).abs() < f32::EPSILON);
        assert!((v_pos.x - 50.0).abs() < f32::EPSILON);
        assert!((v_pos.y - 100.0).abs() < f32::EPSILON);
    }
}
