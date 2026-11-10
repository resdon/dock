//! Group arrangement and redistribution for dock layout.
//!
//! Handles redistributing icons when the dock is resized, icons are added/removed,
//! or the layout needs to be reorganized.

use std::collections::HashMap;

use super::dock::DockLayout;
use crate::geometry::Size;
use crate::physics::{BodyId, PhysicsBody};

/// Redistribution mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedistributionMode {
    /// Evenly space all icons.
    Even,
    /// Compact icons to the start.
    CompactStart,
    /// Compact icons to the center.
    CompactCenter,
    /// Maintain relative spacing.
    MaintainSpacing,
}

/// Arrangement manager for dock layout.
pub struct ArrangementManager {
    /// Redistribution mode.
    pub mode: RedistributionMode,
    /// Animation speed for redistribution.
    pub animation_speed: f32,
}

impl ArrangementManager {
    /// Create a new arrangement manager.
    pub fn new() -> Self {
        Self {
            mode: RedistributionMode::Even,
            animation_speed: 10.0,
        }
    }

    /// Redistribute icons in the dock.
    pub fn redistribute(&self, layout: &DockLayout, bodies: &mut HashMap<BodyId, PhysicsBody>) {
        let positions = layout.compute_rest_positions();

        for (i, pos) in positions.iter().enumerate() {
            let body_id = BodyId::new((i + 1) as u64);
            if let Some(body) = bodies.get_mut(&body_id) {
                body.set_rest_position(*pos);
                body.wake_up();
            }
        }
    }

    /// Animate bodies toward their rest positions.
    pub fn animate_to_rest(&self, bodies: &mut HashMap<BodyId, PhysicsBody>, dt: f32) {
        let speed = self.animation_speed * dt;
        for (_, body) in bodies.iter_mut() {
            if let Some(rest) = body.rest_position {
                let dx = rest.x - body.position.x;
                let dy = rest.y - body.position.y;
                let dist = (dx * dx + dy * dy).sqrt();

                if dist > 0.1 {
                    let move_amount = speed.min(dist);
                    body.position.x += (dx / dist) * move_amount;
                    body.position.y += (dy / dist) * move_amount;
                } else {
                    body.position = rest;
                    body.velocity = crate::geometry::Vector2D::ZERO;
                }
            }
        }
    }

    /// Handle dock resize.
    pub fn handle_resize(
        &self,
        layout: &mut DockLayout,
        new_size: Size,
        bodies: &mut HashMap<BodyId, PhysicsBody>,
    ) {
        layout.config.container_size = new_size;
        self.redistribute(layout, bodies);
    }

    /// Compact icons after removal.
    pub fn compact_after_removal(
        &self,
        layout: &DockLayout,
        removed_index: usize,
        bodies: &mut HashMap<BodyId, PhysicsBody>,
    ) {
        let positions = layout.compute_rest_positions();

        // Shift bodies after the removed index
        let mut shift = 0;
        for (_, body) in bodies.iter_mut() {
            let body_num = body.id.as_u64() as usize;
            if body_num > removed_index + 1 {
                let new_idx = body_num - 2 - shift;
                if new_idx < positions.len() {
                    body.set_rest_position(positions[new_idx]);
                    body.wake_up();
                }
                shift += 1;
            }
        }
    }

    /// Check if all bodies are at their rest positions.
    pub fn is_arranged(&self, bodies: &HashMap<BodyId, PhysicsBody>, threshold: f32) -> bool {
        for (_, body) in bodies.iter() {
            if let Some(rest) = body.rest_position {
                let dx = body.position.x - rest.x;
                let dy = body.position.y - rest.y;
                let dist_sq = dx * dx + dy * dy;
                if dist_sq > threshold * threshold {
                    return false;
                }
            }
        }
        true
    }

    /// Snap all bodies to their rest positions.
    pub fn snap_to_rest(&self, bodies: &mut HashMap<BodyId, PhysicsBody>) {
        for (_, body) in bodies.iter_mut() {
            if let Some(rest) = body.rest_position {
                body.position = rest;
                body.velocity = crate::geometry::Vector2D::ZERO;
                body.angular_velocity = 0.0;
            }
        }
    }
}

impl Default for ArrangementManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::layout::DockLayoutConfig;
    use crate::physics::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_redistribute() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;

        let mut bodies = HashMap::new();
        for i in 1..=3 {
            bodies.insert(
                BodyId::new(i),
                PhysicsBody::dynamic_body(
                    BodyId::new(i),
                    Point::ORIGIN,
                    CollisionShape::circle(20.0),
                    1.0,
                ),
            );
        }

        let manager = ArrangementManager::new();
        manager.redistribute(&layout, &mut bodies);

        for i in 1..=3 {
            let body = bodies.get(&BodyId::new(i)).unwrap();
            assert!(body.rest_position.is_some());
        }
    }

    #[test]
    fn test_animate_to_rest() {
        let mut bodies = HashMap::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.set_rest_position(Point::new(100.0, 0.0));
        bodies.insert(BodyId::new(1), body);

        let manager = ArrangementManager::new();
        manager.animate_to_rest(&mut bodies, 0.016);

        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!(body.position.x > 0.0);
    }

    #[test]
    fn test_is_arranged() {
        let mut bodies = HashMap::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.0, 50.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.set_rest_position(Point::new(50.0, 50.0));
        bodies.insert(BodyId::new(1), body);

        let manager = ArrangementManager::new();
        assert!(manager.is_arranged(&bodies, 1.0));
    }

    #[test]
    fn test_is_not_arranged() {
        let mut bodies = HashMap::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.set_rest_position(Point::new(100.0, 0.0));
        bodies.insert(BodyId::new(1), body);

        let manager = ArrangementManager::new();
        assert!(!manager.is_arranged(&bodies, 1.0));
    }

    #[test]
    fn test_snap_to_rest() {
        let mut bodies = HashMap::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(0.0, 0.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        body.set_rest_position(Point::new(100.0, 100.0));
        body.velocity = crate::geometry::Vector2D::new(10.0, 0.0);
        bodies.insert(BodyId::new(1), body);

        let manager = ArrangementManager::new();
        manager.snap_to_rest(&mut bodies);

        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!((body.position.x - 100.0).abs() < f32::EPSILON);
        assert!((body.velocity.length() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_handle_resize() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;

        let mut bodies = HashMap::new();
        for i in 1..=3 {
            bodies.insert(
                BodyId::new(i),
                PhysicsBody::dynamic_body(
                    BodyId::new(i),
                    Point::ORIGIN,
                    CollisionShape::circle(20.0),
                    1.0,
                ),
            );
        }

        let manager = ArrangementManager::new();
        manager.handle_resize(&mut layout, Size::new(600.0, 100.0), &mut bodies);
        assert!((layout.config.container_size.width - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_animation_speed() {
        let manager = ArrangementManager::new();
        assert!((manager.animation_speed - 10.0).abs() < f32::EPSILON);
    }
}
