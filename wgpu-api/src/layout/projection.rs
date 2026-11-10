//! Physics-to-scene projection.
//!
//! Bridges the physics engine with the scene graph by projecting physics body
//! positions, rotations, and velocities into scene node transforms.

use std::collections::HashMap;

use crate::geometry::{Point, Size, NodeId};
use crate::physics::{BodyId, PhysicsBody};
use crate::scene::{Node, NodeStyle, NodeContent};

/// Projection mode for physics-to-scene mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionMode {
    /// Direct mapping: body position = node position.
    Direct,
    /// Offset mapping: body position + offset = node position.
    Offset,
    /// Center mapping: body position is the center of the node.
    Center,
}

impl Default for ProjectionMode {
    fn default() -> Self {
        Self::Center
    }
}

/// Configuration for physics-to-scene projection.
#[derive(Debug, Clone, Copy)]
pub struct ProjectionConfig {
    /// Projection mode.
    pub mode: ProjectionMode,
    /// Scale factor for physics to scene coordinates.
    pub scale: f32,
    /// Whether to project rotation.
    pub project_rotation: bool,
    /// Whether to project velocity as visual deformation.
    pub project_velocity: bool,
    /// Maximum visual deformation from velocity.
    pub max_deformation: f32,
}

impl Default for ProjectionConfig {
    fn default() -> Self {
        Self {
            mode: ProjectionMode::Center,
            scale: 1.0,
            project_rotation: true,
            project_velocity: true,
            max_deformation: 0.1,
        }
    }
}

/// Projection data for a single body-node pair.
#[derive(Debug, Clone)]
pub struct ProjectionData {
    /// Body ID.
    pub body_id: BodyId,
    /// Node style updated from physics.
    pub style: NodeStyle,
    /// Visual deformation from velocity.
    pub deformation: f32,
}

/// Physics-to-scene projector.
pub struct Projector {
    /// Projection configuration.
    pub config: ProjectionConfig,
    /// Mapping from body ID to node ID.
    pub body_to_node: HashMap<BodyId, u64>,
}

impl Projector {
    /// Create a new projector.
    pub fn new() -> Self {
        Self {
            config: ProjectionConfig::default(),
            body_to_node: HashMap::new(),
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: ProjectionConfig) -> Self {
        Self {
            config,
            body_to_node: HashMap::new(),
        }
    }

    /// Register a body-node mapping.
    pub fn register(&mut self, body_id: BodyId, node_id: u64) {
        self.body_to_node.insert(body_id, node_id);
    }

    /// Unregister a body-node mapping.
    pub fn unregister(&mut self, body_id: BodyId) {
        self.body_to_node.remove(&body_id);
    }

    /// Project a single body's state to a NodeStyle.
    pub fn project_body(&self, body: &PhysicsBody, size: Size) -> ProjectionData {
        let position = match self.config.mode {
            ProjectionMode::Direct => body.position,
            ProjectionMode::Offset => Point::new(
                body.position.x - size.width / 2.0,
                body.position.y - size.height / 2.0,
            ),
            ProjectionMode::Center => body.position,
        };

        // Compute visual deformation from velocity
        let deformation = if self.config.project_velocity {
            let speed = body.velocity.length();
            (speed / 1000.0 * self.config.max_deformation * 1000.0).min(self.config.max_deformation)
        } else {
            0.0
        };

        // Scale factor: slight scale up when moving fast
        let scale_factor = 1.0 + deformation;

        let style = NodeStyle {
            position,
            size,
            opacity: 1.0,
            scale: Point::new(scale_factor, scale_factor),
            rotation: if self.config.project_rotation {
                body.rotation
            } else {
                0.0
            },
            visible: true,
            z_index: 0,
            anchor: Point::new(0.5, 0.5),
            enabled: body.enabled,
        };

        ProjectionData {
            body_id: body.id,
            style,
            deformation,
        }
    }

    /// Project all bodies to scene nodes.
    pub fn project_all(
        &self,
        bodies: &HashMap<BodyId, PhysicsBody>,
        node_size: Size,
    ) -> Vec<ProjectionData> {
        bodies
            .values()
            .filter(|b| b.enabled)
            .map(|body| self.project_body(body, node_size))
            .collect()
    }


    /// Create scene nodes from physics bodies.
    pub fn create_nodes(
        &mut self,
        bodies: &HashMap<BodyId, PhysicsBody>,
        node_size: Size,
        content: NodeContent,
    ) -> Vec<Node> {
        bodies
            .values()
            .filter(|b| b.enabled)
            .enumerate()
            .map(|(i, body)| {
                let projection = self.project_body(body, node_size);
                let mut node = Node::new(NodeId::new(i as u64), content.clone());
                node.style = projection.style;
                node.data = Some(format!("body_{}", body.id.as_u64()));
                self.body_to_node.insert(body.id, i as u64);
                node
            })
            .collect()
    }

    /// Compute the visual scale for a body based on its velocity.
    pub fn velocity_scale(&self, body: &PhysicsBody) -> f32 {
        if !self.config.project_velocity {
            return 1.0;
        }
        let speed = body.velocity.length();
        let deformation = (speed / 1000.0 * self.config.max_deformation * 1000.0)
            .min(self.config.max_deformation);
        1.0 + deformation
    }

    /// Compute the visual opacity based on body state.
    pub fn body_opacity(&self, body: &PhysicsBody) -> f32 {
        if body.is_sleeping {
            1.0
        } else if body.velocity.length() > 100.0 {
            0.9
        } else {
            1.0
        }
    }
}

impl Default for Projector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size, Vector2D};
    use crate::physics::{BodyId, PhysicsBody, CollisionShape};
    use crate::scene::{NodeContent, Rectangle};

    #[test]
    fn test_project_body_center() {
        let projector = Projector::with_config(ProjectionConfig {
            mode: ProjectionMode::Center,
            ..Default::default()
        });
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(20.0),
            1.0,
        );
        let data = projector.project_body(&body, Size::new(40.0, 40.0));
        assert!((data.style.position.x - 100.0).abs() < f32::EPSILON);
        assert!((data.style.position.y - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_project_body_offset() {
        let projector = Projector::with_config(ProjectionConfig {
            mode: ProjectionMode::Offset,
            ..Default::default()
        });
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(20.0),
            1.0,
        );
        let data = projector.project_body(&body, Size::new(40.0, 40.0));
        assert!((data.style.position.x - 80.0).abs() < f32::EPSILON);
        assert!((data.style.position.y - 80.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_velocity_deformation() {
        let projector = Projector::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(500.0, 0.0);
        let data = projector.project_body(&body, Size::new(40.0, 40.0));
        assert!(data.deformation > 0.0);
        assert!(data.deformation <= projector.config.max_deformation);
    }

    #[test]
    fn test_no_deformation_when_still() {
        let projector = Projector::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        let data = projector.project_body(&body, Size::new(40.0, 40.0));
        assert!((data.deformation - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_register_unregister() {
        let mut projector = Projector::new();
        projector.register(BodyId::new(1), 100);
        assert_eq!(projector.body_to_node.get(&BodyId::new(1)), Some(&100));
        projector.unregister(BodyId::new(1));
        assert!(!projector.body_to_node.contains_key(&BodyId::new(1)));
    }

    #[test]
    fn test_project_all() {
        let projector = Projector::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 50.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        let data = projector.project_all(&bodies, Size::new(40.0, 40.0));
        assert_eq!(data.len(), 1);
    }

    #[test]
    fn test_velocity_scale() {
        let projector = Projector::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.velocity = Vector2D::new(1000.0, 0.0);
        let scale = projector.velocity_scale(&body);
        assert!(scale > 1.0);
        assert!(scale <= 1.0 + projector.config.max_deformation);
    }

    #[test]
    fn test_body_opacity() {
        let projector = Projector::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        assert!((projector.body_opacity(&body) - 1.0).abs() < f32::EPSILON);
        body.velocity = Vector2D::new(200.0, 0.0);
        assert!((projector.body_opacity(&body) - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_create_nodes() {
        let mut projector = Projector::new();
        let mut bodies = HashMap::new();
        bodies.insert(
            BodyId::new(1),
            PhysicsBody::dynamic_body(
                BodyId::new(1),
                Point::new(50.0, 50.0),
                CollisionShape::circle(10.0),
                1.0,
            ),
        );
        let nodes = projector.create_nodes(
            &bodies,
            Size::new(40.0, 40.0),
            NodeContent::Rectangle(Rectangle {
                color: crate::color::Color::WHITE,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
                shadow: None,
            }),
        );
        assert_eq!(nodes.len(), 1);
        assert!(projector.body_to_node.contains_key(&BodyId::new(1)));
    }

    #[test]
    fn test_rotation_projection() {
        let projector = Projector::new();
        let mut body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::ORIGIN,
            CollisionShape::circle(10.0),
            1.0,
        );
        body.rotation = 1.57; // 90 degrees
        let data = projector.project_body(&body, Size::new(40.0, 40.0));
        assert!((data.style.rotation - 1.57).abs() < 0.01);
    }
}
