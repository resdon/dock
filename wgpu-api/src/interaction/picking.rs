//! Hit testing and body selection for the interaction engine.
//!
//! The picker determines which physics body is under the pointer at a given
//! position, enabling click, drag, and hover interactions.

use std::collections::HashMap;

use crate::geometry::Point;
use crate::physics::body::{BodyId, PhysicsBody};

/// Result of a pick query.
#[derive(Debug, Clone)]
pub struct PickResult {
    /// The picked body ID.
    pub body_id: BodyId,
    /// Pick position in world space.
    pub position: Point,
    /// Local position within the body (offset from body center).
    pub local_offset: crate::geometry::Vector2D,
    /// Depth/z-order of the body (higher = on top).
    pub depth: usize,
}

/// Configuration for picking.
#[derive(Debug, Clone, Copy)]
pub struct PickConfig {
    /// Maximum depth to pick (0 = unlimited).
    pub max_depth: usize,
    /// Whether to pick sleeping bodies.
    pub pick_sleeping: bool,
    /// Whether to pick disabled bodies.
    pub pick_disabled: bool,
    /// Hit expansion radius (pixels) for easier picking.
    pub hit_expansion: f32,
}

impl Default for PickConfig {
    fn default() -> Self {
        Self {
            max_depth: 0,
            pick_sleeping: true,
            pick_disabled: false,
            hit_expansion: 5.0,
        }
    }
}

/// The picker performs hit testing against physics bodies.
pub struct Picker {
    /// Pick configuration.
    pub config: PickConfig,
    /// Currently selected body.
    pub selected_body: Option<BodyId>,
    /// Hovered body (not necessarily selected).
    pub hovered_body: Option<BodyId>,
    /// All bodies under the pointer (sorted by depth).
    pub pointer_over: Vec<BodyId>,
}

impl Picker {
    /// Create a new picker.
    pub fn new() -> Self {
        Self {
            config: PickConfig::default(),
            selected_body: None,
            hovered_body: None,
            pointer_over: Vec::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: PickConfig) -> Self {
        Self {
            config,
            selected_body: None,
            hovered_body: None,
            pointer_over: Vec::new(),
        }
    }

    /// Pick the topmost body at a point.
    pub fn pick(
        &mut self,
        point: Point,
        bodies: &HashMap<BodyId, PhysicsBody>,
    ) -> Option<PickResult> {
        let mut best: Option<PickResult> = None;
        let mut best_depth = usize::MAX;

        for (idx, (body_id, body)) in bodies.iter().enumerate() {
            if !self.config.pick_disabled && !body.enabled {
                continue;
            }
            if !self.config.pick_sleeping && body.is_sleeping {
                continue;
            }

            // Check if point is inside body's collision shape
            let expanded = self.config.hit_expansion;
            let in_shape = if expanded > 0.0 {
                // Expanded hit test
                let dx = point.x - body.position.x;
                let dy = point.y - body.position.y;
                body.shape.contains_point(
                    &body.position,
                    &point,
                ) || (dx * dx + dy * dy).sqrt() < expanded + Self::estimate_radius(body)
            } else {
                body.contains_point(&point)
            };

            if in_shape {
                let depth = idx;
                if self.config.max_depth == 0 || depth < self.config.max_depth {
                    if depth >= best_depth {
                        let local_offset = crate::geometry::Vector2D::new(
                            point.x - body.position.x,
                            point.y - body.position.y,
                        );
                        best = Some(PickResult {
                            body_id: *body_id,
                            position: point,
                            local_offset,
                            depth,
                        });
                        best_depth = depth;
                    }
                }
            }
        }

        best
    }

    /// Pick all bodies at a point.
    pub fn pick_all(
        &mut self,
        point: Point,
        bodies: &HashMap<BodyId, PhysicsBody>,
    ) -> Vec<PickResult> {
        let mut results = Vec::new();

        for (idx, (body_id, body)) in bodies.iter().enumerate() {
            if !self.config.pick_disabled && !body.enabled {
                continue;
            }
            if !self.config.pick_sleeping && body.is_sleeping {
                continue;
            }

            if body.contains_point(&point) {
                let local_offset = crate::geometry::Vector2D::new(
                    point.x - body.position.x,
                    point.y - body.position.y,
                );
                results.push(PickResult {
                    body_id: *body_id,
                    position: point,
                    local_offset,
                    depth: idx,
                });
            }
        }

        // Sort by depth (highest first)
        results.sort_by(|a, b| b.depth.cmp(&a.depth));
        results
    }

    /// Update hover state based on pointer position.
    pub fn update_hover(
        &mut self,
        point: Point,
        bodies: &HashMap<BodyId, PhysicsBody>,
    ) {
        self.pointer_over.clear();
        let results = self.pick_all(point, bodies);
        self.pointer_over = results.iter().map(|r| r.body_id).collect();
        self.hovered_body = results.first().map(|r| r.body_id);
    }

    /// Select a body.
    pub fn select(&mut self, body_id: BodyId) {
        self.selected_body = Some(body_id);
    }

    /// Deselect the current body.
    pub fn deselect(&mut self) {
        self.selected_body = None;
    }

    /// Toggle selection of a body.
    pub fn toggle_select(&mut self, body_id: BodyId) {
        if self.selected_body == Some(body_id) {
            self.deselect();
        } else {
            self.select(body_id);
        }
    }

    /// Check if a body is selected.
    pub fn is_selected(&self, body_id: BodyId) -> bool {
        self.selected_body == Some(body_id)
    }

    /// Check if a body is hovered.
    pub fn is_hovered(&self, body_id: BodyId) -> bool {
        self.hovered_body == Some(body_id)
    }

    /// Check if pointer is over a body.
    pub fn is_pointer_over(&self, body_id: BodyId) -> bool {
        self.pointer_over.contains(&body_id)
    }

    /// Clear all state.
    pub fn clear(&mut self) {
        self.selected_body = None;
        self.hovered_body = None;
        self.pointer_over.clear();
    }

    /// Estimate the radius of a body for expanded hit testing.
    fn estimate_radius(body: &PhysicsBody) -> f32 {
        match body.shape {
            crate::physics::body::CollisionShape::Circle { radius } => radius,
            crate::physics::body::CollisionShape::AABB { half_extents } => {
                (half_extents.width * half_extents.width
                    + half_extents.height * half_extents.height)
                    .sqrt()
            }
        }
    }
}

impl Default for Picker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::body::CollisionShape;

    fn make_test_bodies() -> HashMap<BodyId, PhysicsBody> {
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 50.0),
                CollisionShape::circle(25.0),
                1.0,
            ),
        );
        bodies.insert(
            BodyId::new(2),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(150.0, 50.0),
                CollisionShape::circle(25.0),
                1.0,
            ),
        );
        bodies
    }

    #[test]
    fn test_pick_body() {
        let mut picker = Picker::new();
        let bodies = make_test_bodies();
        let result = picker.pick(Point::new(50.0, 50.0), &bodies);
        assert!(result.is_some());
        assert_eq!(result.unwrap().body_id, BodyId::new(1));
    }

    #[test]
    fn test_pick_miss() {
        let mut picker = Picker::new();
        let bodies = make_test_bodies();
        let result = picker.pick(Point::new(300.0, 300.0), &bodies);
        assert!(result.is_none());
    }

    #[test]
    fn test_pick_all() {
        let mut picker = Picker::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 50.0),
                CollisionShape::circle(30.0),
                1.0,
            ),
        );
        bodies.insert(
            BodyId::new(2),
            PhysicsBody::dynamic_body(
                BodyId::new(2),
                Point::new(50.0, 50.0),
                CollisionShape::circle(20.0),
                1.0,
            ),
        );
        let results = picker.pick_all(Point::new(50.0, 50.0), &bodies);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_select_and_deselect() {
        let mut picker = Picker::new();
        picker.select(BodyId::new(1));
        assert!(picker.is_selected(BodyId::new(1)));
        picker.deselect();
        assert!(!picker.is_selected(BodyId::new(1)));
    }

    #[test]
    fn test_toggle_select() {
        let mut picker = Picker::new();
        picker.toggle_select(BodyId::new(1));
        assert!(picker.is_selected(BodyId::new(1)));
        picker.toggle_select(BodyId::new(1));
        assert!(!picker.is_selected(BodyId::new(1)));
    }

    #[test]
    fn test_hover_update() {
        let mut picker = Picker::new();
        let bodies = make_test_bodies();
        picker.update_hover(Point::new(50.0, 50.0), &bodies);
        assert!(picker.is_hovered(BodyId::new(1)));
        assert!(picker.is_pointer_over(BodyId::new(1)));
    }

    #[test]
    fn test_disabled_body_not_picked() {
        let mut picker = Picker::new();
        let mut bodies = HashMap::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(25.0),
            1.0,
        );
        body.enabled = false;
        bodies.insert(BodyId::new(1), body);
        let result = picker.pick(Point::new(50.0, 50.0), &bodies);
        assert!(result.is_none());
    }

    #[test]
    fn test_hit_expansion() {
        let mut picker = Picker::with_config(PickConfig {
            hit_expansion: 20.0,
            ..Default::default()
        });
        let bodies = make_test_bodies();
        // Point just outside the circle but within expansion
        let result = picker.pick(Point::new(80.0, 50.0), &bodies);
        assert!(result.is_some());
    }

    #[test]
    fn test_clear() {
        let mut picker = Picker::new();
        picker.select(BodyId::new(1));
        picker.clear();
        assert!(picker.selected_body.is_none());
        assert!(picker.hovered_body.is_none());
        assert!(picker.pointer_over.is_empty());
    }

    #[test]
    fn test_aabb_pick() {
        let mut picker = Picker::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_from_rect(
                BodyId::new(1),
                &crate::geometry::Rect::new(0.0, 0.0, 100.0, 100.0),
                1.0,
            ),
        );
        let result = picker.pick(Point::new(50.0, 50.0), &bodies);
        assert!(result.is_some());
    }

    #[test]
    fn test_local_offset() {
        let mut picker = Picker::new();
        let bodies = make_test_bodies();
        let result = picker.pick(Point::new(60.0, 70.0), &bodies).unwrap();
        assert!((result.local_offset.x - 10.0).abs() < f32::EPSILON);
        assert!((result.local_offset.y - 20.0).abs() < f32::EPSILON);
    }
}
