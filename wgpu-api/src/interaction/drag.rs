//! Drag and drop mechanics for the interaction engine.
//!
//! Manages drag state, tracking which body is being dragged, computing drag
//! offsets, and applying drag forces to the physics world.

use std::collections::HashMap;

use crate::geometry::{Point, Vector2D};
use crate::physics::body::{BodyId, PhysicsBody};

/// State of a drag operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragState {
    /// No drag in progress.
    None,
    /// Drag has started but hasn't moved enough to commit.
    Pending,
    /// Drag is active and moving.
    Active,
    /// Drag has ended, drop is being processed.
    Dropping,
}

/// Configuration for drag behavior.
#[derive(Debug, Clone, Copy)]
pub struct DragConfig {
    /// Minimum distance to start a drag (pixels).
    pub drag_threshold: f32,
    /// Whether to apply velocity on release.
    pub apply_velocity_on_release: bool,
    /// Velocity multiplier on release.
    pub velocity_multiplier: f32,
    /// Whether dragged body follows pointer exactly.
    pub direct_follow: bool,
    /// Inertia damping after release.
    pub inertia_damping: f32,
    /// Maximum drag velocity.
    pub max_velocity: f32,
}

impl Default for DragConfig {
    fn default() -> Self {
        Self {
            drag_threshold: 5.0,
            apply_velocity_on_release: true,
            velocity_multiplier: 0.5,
            direct_follow: true,
            inertia_damping: 0.9,
            max_velocity: 5000.0,
        }
    }
}

/// Information about the current drag operation.
#[derive(Debug, Clone)]
pub struct DragInfo {
    /// Body being dragged.
    pub body_id: BodyId,
    /// Current state.
    pub state: DragState,
    /// Position where drag started.
    pub start_position: Point,
    /// Current pointer position.
    pub current_position: Point,
    /// Offset from pointer to body center at drag start.
    pub offset: Vector2D,
    /// Total drag distance.
    pub drag_distance: f32,
    /// Drag velocity (pixels/second).
    pub velocity: Vector2D,
    /// Previous pointer position for velocity calculation.
    pub previous_position: Point,
    /// Previous timestamp.
    pub previous_time: f64,
    /// Whether the drag has committed (moved past threshold).
    pub committed: bool,
}

impl DragInfo {
    /// Create a new drag info.
    pub fn new(body_id: BodyId, start_position: Point, offset: Vector2D) -> Self {
        Self {
            body_id,
            state: DragState::Pending,
            start_position,
            current_position: start_position,
            offset,
            drag_distance: 0.0,
            velocity: Vector2D::ZERO,
            previous_position: start_position,
            previous_time: 0.0,
            committed: false,
        }
    }

    /// Update with new pointer position.
    pub fn update(&mut self, position: Point, timestamp: f64) {
        self.current_position = position;

        // Calculate velocity
        if timestamp > self.previous_time && (timestamp - self.previous_time) > 0.001 {
            let dt = (timestamp - self.previous_time) as f32;
            let dx = (position.x - self.previous_position.x) / dt;
            let dy = (position.y - self.previous_position.y) / dt;
            self.velocity = Vector2D::new(dx, dy);

            // Clamp velocity
            let speed = self.velocity.length();
            if speed > 5000.0 {
                let scale = 5000.0 / speed;
                self.velocity.x *= scale;
                self.velocity.y *= scale;
            }
        }

        self.previous_position = position;
        self.previous_time = timestamp;

        // Calculate drag distance
        let dx = position.x - self.start_position.x;
        let dy = position.y - self.start_position.y;
        self.drag_distance = (dx * dx + dy * dy).sqrt();

        // Check if drag has committed
        if !self.committed && self.drag_distance > 5.0 {
            self.committed = true;
            self.state = DragState::Active;
        }
    }

    /// Get the target position for the dragged body.
    pub fn target_position(&self) -> Point {
        Point::new(
            self.current_position.x - self.offset.x,
            self.current_position.y - self.offset.y,
        )
    }
}

/// Drag handler that manages drag operations.
pub struct DragHandler {
    /// Drag configuration.
    pub config: DragConfig,
    /// Current drag info.
    pub drag_info: Option<DragInfo>,
    /// Velocity history for smooth release.
    pub velocity_history: Vec<Vector2D>,
}

impl DragHandler {
    /// Create a new drag handler.
    pub fn new() -> Self {
        Self {
            config: DragConfig::default(),
            drag_info: None,
            velocity_history: Vec::with_capacity(10),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: DragConfig) -> Self {
        Self {
            config,
            drag_info: None,
            velocity_history: Vec::with_capacity(10),
        }
    }

    /// Try to start a drag on a body.
    pub fn start_drag(
        &mut self,
        body_id: BodyId,
        pointer_position: Point,
        body_position: Point,
        timestamp: f64,
    ) {
        let offset = Vector2D::new(
            body_position.x - pointer_position.x,
            body_position.y - pointer_position.y,
        );

        self.drag_info = Some(DragInfo::new(body_id, pointer_position, offset));
        if let Some(ref mut info) = self.drag_info {
            info.previous_time = timestamp;
        }
        self.velocity_history.clear();
    }

    /// Update the current drag with new pointer position.
    pub fn update_drag(&mut self, position: Point, timestamp: f64) -> Option<&DragInfo> {
        if let Some(ref mut info) = self.drag_info {
            info.update(position, timestamp);

            if info.state == DragState::Active {
                self.velocity_history.push(info.velocity);
                if self.velocity_history.len() > 10 {
                    self.velocity_history.remove(0);
                }
            }
        }
        self.drag_info.as_ref()
    }

    /// End the current drag and optionally apply velocity.
    pub fn end_drag(
        &mut self,
        bodies: &mut HashMap<BodyId, PhysicsBody>,
    ) -> Option<BodyId> {
        let info = self.drag_info.take()?;
        let body_id = info.body_id;

        if let Some(body) = bodies.get_mut(&body_id) {
            if info.committed {
                // Set body position to final drag position
                let target = info.target_position();
                body.position = target;

                // Apply velocity on release
                if self.config.apply_velocity_on_release {
                    // Use average of recent velocities
                    if !self.velocity_history.is_empty() {
                        let avg_x: f32 = self.velocity_history.iter().map(|v| v.x).sum::<f32>()
                            / self.velocity_history.len() as f32;
                        let avg_y: f32 = self.velocity_history.iter().map(|v| v.y).sum::<f32>()
                            / self.velocity_history.len() as f32;

                        body.velocity = Vector2D::new(
                            avg_x * self.config.velocity_multiplier,
                            avg_y * self.config.velocity_multiplier,
                        );
                    }
                }

                body.is_sleeping = false;
            }

            info.state = DragState::Dropping;
        }

        self.velocity_history.clear();
        Some(body_id)
    }

    /// Apply direct follow: move dragged body to pointer position.
    pub fn apply_direct_follow(
        &mut self,
        bodies: &mut HashMap<BodyId, PhysicsBody>,
    ) {
        if let Some(ref info) = self.drag_info {
            if info.state == DragState::Active {
                if let Some(body) = bodies.get_mut(&info.body_id) {
                    let target = info.target_position();
                    if self.config.direct_follow {
                        body.position = target;
                        body.is_sleeping = false;
                    }
                }
            }
        }
    }

    /// Cancel the current drag.
    pub fn cancel_drag(&mut self) {
        self.drag_info = None;
        self.velocity_history.clear();
    }

    /// Check if a drag is currently active.
    pub fn is_dragging(&self) -> bool {
        self.drag_info.as_ref().map_or(false, |info| {
            info.state == DragState::Active || info.state == DragState::Pending
        })
    }

    /// Get the currently dragged body ID.
    pub fn dragged_body(&self) -> Option<BodyId> {
        self.drag_info.as_ref().map(|info| info.body_id)
    }

    /// Get the current drag info.
    pub fn drag_info(&self) -> Option<&DragInfo> {
        self.drag_info.as_ref()
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.drag_info = None;
        self.velocity_history.clear();
    }
}

impl Default for DragHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::body::CollisionShape;

    #[test]
    fn test_start_drag() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(110.0, 110.0),
            0.0,
        );
        assert!(handler.is_dragging());
        assert_eq!(handler.dragged_body(), Some(BodyId::new(1)));
    }

    #[test]
    fn test_drag_commit() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(110.0, 110.0),
            0.0,
        );
        // Move past threshold
        handler.update_drag(Point::new(120.0, 120.0), 0.016);
        let info = handler.drag_info().unwrap();
        assert!(info.committed);
        assert_eq!(info.state, DragState::Active);
    }

    #[test]
    fn test_drag_no_commit() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(110.0, 110.0),
            0.0,
        );
        // Move less than threshold
        handler.update_drag(Point::new(102.0, 102.0), 0.016);
        let info = handler.drag_info().unwrap();
        assert!(!info.committed);
    }

    #[test]
    fn test_end_drag_applies_velocity() {
        let mut handler = DragHandler::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(100.0, 100.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );

        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(100.0, 100.0),
            0.0,
        );
        // Drag with velocity
        handler.update_drag(Point::new(150.0, 100.0), 0.016);
        handler.update_drag(Point::new(200.0, 100.0), 0.032);

        let dropped = handler.end_drag(&mut bodies).unwrap();
        assert_eq!(dropped, BodyId::new(1));
        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!(body.velocity.x > 0.0);
    }

    #[test]
    fn test_cancel_drag() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(100.0, 100.0),
            0.0,
        );
        handler.cancel_drag();
        assert!(!handler.is_dragging());
    }

    #[test]
    fn test_target_position() {
        let info = DragInfo::new(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Vector2D::new(10.0, 10.0),
        );
        let mut info = info;
        info.update(Point::new(200.0, 200.0), 0.016);
        let target = info.target_position();
        assert!((target.x - 190.0).abs() < f32::EPSILON);
        assert!((target.y - 190.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_direct_follow() {
        let mut handler = DragHandler::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(0.0, 0.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );

        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(0.0, 0.0),
            0.0,
        );
        handler.update_drag(Point::new(200.0, 200.0), 0.016);
        handler.apply_direct_follow(&mut bodies);

        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!((body.position.x - 200.0).abs() < f32::EPSILON);
        assert!((body.position.y - 200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_drag_config_defaults() {
        let config = DragConfig::default();
        assert!((config.drag_threshold - 5.0).abs() < f32::EPSILON);
        assert!(config.apply_velocity_on_release);
    }

    #[test]
    fn test_clear() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            Point::new(100.0, 100.0),
            0.0,
        );
        handler.clear();
        assert!(!handler.is_dragging());
        assert!(handler.dragged_body().is_none());
    }

    #[test]
    fn test_velocity_clamping() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            Point::new(0.0, 0.0),
            0.0,
        );
        // Very fast movement
        handler.update_drag(Point::new(10000.0, 0.0), 0.016);
        let info = handler.drag_info().unwrap();
        assert!(info.velocity.length() <= 5000.0);
    }

    #[test]
    fn test_drag_distance() {
        let mut handler = DragHandler::new();
        handler.start_drag(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            Point::new(0.0, 0.0),
            0.0,
        );
        handler.update_drag(Point::new(3.0, 4.0), 0.016);
        let info = handler.drag_info().unwrap();
        assert!((info.drag_distance - 5.0).abs() < 0.01);
    }
}
