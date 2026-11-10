//! Dock layout arrangement for physics-based icon positioning.
//!
//! Computes rest positions for dock icons and manages layout transitions
//! when icons are added, removed, or rearranged.

use crate::geometry::{Point, Rect, Size};
use crate::physics::{BodyId, PhysicsBody, CollisionShape, BodyGroup, GroupConfig, GroupId};

/// Layout direction for dock icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockLayoutDirection {
    /// Icons arranged horizontally.
    Horizontal,
    /// Icons arranged vertically.
    Vertical,
}

impl Default for DockLayoutDirection {
    fn default() -> Self {
        Self::Horizontal
    }
}

/// Alignment within the dock container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockAlignment {
    /// Align to the start (left/top).
    Start,
    /// Center within the container.
    Center,
    /// Align to the end (right/bottom).
    End,
    /// Space evenly across the container.
    SpaceEvenly,
}

impl Default for DockAlignment {
    fn default() -> Self {
        Self::Center
    }
}

/// Configuration for dock layout.
#[derive(Debug, Clone, Copy)]
pub struct DockLayoutConfig {
    /// Size of each icon slot.
    pub icon_size: Size,
    /// Spacing between icons.
    pub spacing: f32,
    /// Padding from container edges.
    pub padding: f32,
    /// Layout direction.
    pub direction: DockLayoutDirection,
    /// Alignment within container.
    pub alignment: DockAlignment,
    /// Container size.
    pub container_size: Size,
    /// Spring stiffness for rest position attraction.
    pub spring_stiffness: f32,
    /// Spring damping.
    pub spring_damping: f32,
    /// Mutual repulsion strength.
    pub repulsion_strength: f32,
}

impl Default for DockLayoutConfig {
    fn default() -> Self {
        Self {
            icon_size: Size::new(48.0, 48.0),
            spacing: 8.0,
            padding: 16.0,
            direction: DockLayoutDirection::Horizontal,
            alignment: DockAlignment::Center,
            container_size: Size::new(800.0, 100.0),
            spring_stiffness: 500.0,
            spring_damping: 0.8,
            repulsion_strength: 100.0,
        }
    }
}

/// Dock layout manager.
pub struct DockLayout {
    /// Layout configuration.
    pub config: DockLayoutConfig,
    /// Associated physics group.
    pub group: Option<GroupId>,
    /// Number of icons.
    pub icon_count: usize,
}

impl DockLayout {
    /// Create a new dock layout.
    pub fn new() -> Self {
        Self {
            config: DockLayoutConfig::default(),
            group: None,
            icon_count: 0,
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: DockLayoutConfig) -> Self {
        Self {
            config,
            group: None,
            icon_count: 0,
        }
    }

    /// Compute rest positions for all icons.
    pub fn compute_rest_positions(&self) -> Vec<Point> {
        self.compute_rest_positions_with_count(self.icon_count)
    }

    /// Compute rest positions for a given number of icons.
    pub fn compute_rest_positions_with_count(&self, count: usize) -> Vec<Point> {
        if count == 0 {
            return Vec::new();
        }

        match self.config.direction {
            DockLayoutDirection::Horizontal => {
                self.compute_horizontal_positions(count)
            }
            DockLayoutDirection::Vertical => {
                self.compute_vertical_positions(count)
            }
        }
    }

    /// Compute horizontal rest positions.
    fn compute_horizontal_positions(&self, count: usize) -> Vec<Point> {
        let icon_w = self.config.icon_size.width;
        let icon_h = self.config.icon_size.height;
        let spacing = self.config.spacing;
        let padding = self.config.padding;
        let container_w = self.config.container_size.width;
        let container_h = self.config.container_size.height;

        // Total width of all icons + spacing
        let total_width = count as f32 * icon_w + (count as f32 - 1.0) * spacing;

        // Starting X based on alignment
        let start_x = match self.config.alignment {
            DockAlignment::Start => padding,
            DockAlignment::Center => {
                (container_w - total_width) / 2.0
            }
            DockAlignment::End => {
                container_w - total_width - padding
            }
            DockAlignment::SpaceEvenly => {
                let available = container_w - 2.0 * padding;
                let total_with_spacing = count as f32 * icon_w + (count as f32 - 1.0) * spacing;
                if count > 1 && total_with_spacing < available {
                    let extra_spacing = (available - total_with_spacing) / (count as f32 - 1.0);
                    padding
                } else {
                    padding
                }
            }
        };

        let center_y = container_h / 2.0;

        // Calculate actual spacing for SpaceEvenly
        let actual_spacing = match self.config.alignment {
            DockAlignment::SpaceEvenly if count > 1 => {
                let available = container_w - 2.0 * padding;
                (available - count as f32 * icon_w) / (count as f32 - 1.0)
            }
            _ => spacing,
        };

        (0..count)
            .map(|i| {
                Point::new(
                    start_x + i as f32 * (icon_w + actual_spacing) + icon_w / 2.0,
                    center_y,
                )
            })
            .collect()
    }

    /// Compute vertical rest positions.
    fn compute_vertical_positions(&self, count: usize) -> Vec<Point> {
        let icon_w = self.config.icon_size.width;
        let icon_h = self.config.icon_size.height;
        let spacing = self.config.spacing;
        let padding = self.config.padding;
        let container_w = self.config.container_size.width;
        let container_h = self.config.container_size.height;

        let total_height = count as f32 * icon_h + (count as f32 - 1.0) * spacing;
        let start_y = match self.config.alignment {
            DockAlignment::Start => padding,
            DockAlignment::Center => (container_h - total_height) / 2.0,
            DockAlignment::End => container_h - total_height - padding,
            DockAlignment::SpaceEvenly => padding,
        };

        let center_x = container_w / 2.0;

        (0..count)
            .map(|i| {
                Point::new(
                    center_x,
                    start_y + i as f32 * (icon_h + spacing) + icon_h / 2.0,
                )
            })
            .collect()
    }

    /// Create a BodyGroup for the dock.
    pub fn create_group(&self) -> BodyGroup {
        let positions = self.compute_rest_positions();
        let group_id = GroupId::new(1);

        let mut group = BodyGroup::new(group_id, "dock");
        group.config = GroupConfig {
            spring_stiffness: self.config.spring_stiffness,
            spring_damping: self.config.spring_damping,
            repulsion_strength: self.config.repulsion_strength,
            min_distance: self.config.icon_size.width + self.config.spacing,
            boundary: Some(Rect::new(
                0.0,
                0.0,
                self.config.container_size.width,
                self.config.container_size.height,
            )),
            ..Default::default()
        };

        for (i, pos) in positions.iter().enumerate() {
            group.add_member(BodyId::new(i as u64 + 1), *pos);
        }

        group
    }

    /// Add an icon and recompute layout.
    pub fn add_icon(&mut self) {
        self.icon_count += 1;
    }

    /// Remove an icon and recompute layout.
    pub fn remove_icon(&mut self) {
        if self.icon_count > 0 {
            self.icon_count -= 1;
        }
    }

    /// Set the number of icons.
    pub fn set_icon_count(&mut self, count: usize) {
        self.icon_count = count;
    }

    /// Get the container bounds.
    pub fn container_bounds(&self) -> Rect {
        Rect::new(
            0.0,
            0.0,
            self.config.container_size.width,
            self.config.container_size.height,
        )
    }

    /// Check if a point is within the dock container.
    pub fn contains_point(&self, point: &Point) -> bool {
        let bounds = self.container_bounds();
        point.x >= bounds.x
            && point.x <= bounds.right()
            && point.y >= bounds.y
            && point.y <= bounds.bottom()
    }

    /// Find which icon slot a point is closest to.
    pub fn find_nearest_slot(&self, point: &Point) -> Option<usize> {
        let positions = self.compute_rest_positions();
        let mut best_idx = 0;
        let mut best_dist = f32::MAX;

        for (i, pos) in positions.iter().enumerate() {
            let dx = point.x - pos.x;
            let dy = point.y - pos.y;
            let dist = dx * dx + dy * dy;
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }

        if best_dist < (self.config.icon_size.width * 2.0) * (self.config.icon_size.width * 2.0) {
            Some(best_idx)
        } else {
            None
        }
    }

    /// Create physics bodies for all icons.
    pub fn create_bodies(&self) -> Vec<PhysicsBody> {
        let positions = self.compute_rest_positions();
        let half_size = Size::new(
            self.config.icon_size.width / 2.0,
            self.config.icon_size.height / 2.0,
        );

        positions
            .iter()
            .enumerate()
            .map(|(i, pos)| {
                let shape = CollisionShape::AABB {
                    half_extents: half_size,
                };
                let mut body = PhysicsBody::dynamic_body(
                    BodyId::new(i as u64 + 1),
                    *pos,
                    shape,
                    1.0,
                );
                body.set_rest_position(*pos);
                body
            })
            .collect()
    }
}

impl Default for DockLayout {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::physics::{BodyId, PhysicsBody, CollisionShape};
    use super::DockLayoutConfig;

    #[test]
    fn test_horizontal_layout() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            spacing: 10.0,
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;
        let positions = layout.compute_rest_positions();
        assert_eq!(positions.len(), 3);
        // Icons should be centered horizontally
        assert!(positions[0].x < positions[1].x);
        assert!(positions[1].x < positions[2].x);
    }

    #[test]
    fn test_vertical_layout() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            direction: DockLayoutDirection::Vertical,
            icon_size: Size::new(40.0, 40.0),
            spacing: 10.0,
            container_size: Size::new(100.0, 300.0),
            ..Default::default()
        });
        layout.icon_count = 3;
        let positions = layout.compute_rest_positions();
        assert_eq!(positions.len(), 3);
        assert!(positions[0].y < positions[1].y);
        assert!(positions[1].y < positions[2].y);
    }

    #[test]
    fn test_empty_layout() {
        let layout = DockLayout::new();
        let positions = layout.compute_rest_positions();
        assert!(positions.is_empty());
    }

    #[test]
    fn test_add_remove_icon() {
        let mut layout = DockLayout::new();
        layout.add_icon();
        layout.add_icon();
        assert_eq!(layout.icon_count, 2);
        layout.remove_icon();
        assert_eq!(layout.icon_count, 1);
        layout.remove_icon();
        assert_eq!(layout.icon_count, 0);
    }

    #[test]
    fn test_contains_point() {
        let layout = DockLayout::with_config(DockLayoutConfig {
            container_size: Size::new(200.0, 100.0),
            ..Default::default()
        });
        assert!(layout.contains_point(&Point::new(100.0, 50.0)));
        assert!(!layout.contains_point(&Point::new(300.0, 50.0)));
    }

    #[test]
    fn test_find_nearest_slot() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            spacing: 10.0,
            container_size: Size::new(300.0, 100.0),
            alignment: DockAlignment::Start,
            padding: 0.0,
            ..Default::default()
        });
        layout.icon_count = 3;
        let slot = layout.find_nearest_slot(&Point::new(20.0, 50.0));
        assert_eq!(slot, Some(0));
    }

    #[test]
    fn test_create_bodies() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;
        let bodies = layout.create_bodies();
        assert_eq!(bodies.len(), 3);
        assert!(bodies[0].rest_position.is_some());
    }

    #[test]
    fn test_create_group() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;
        let group = layout.create_group();
        assert_eq!(group.member_count(), 3);
        assert!(group.config.boundary.is_some());
    }

    #[test]
    fn test_center_alignment() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            spacing: 10.0,
            container_size: Size::new(300.0, 100.0),
            alignment: DockAlignment::Center,
            padding: 0.0,
            ..Default::default()
        });
        layout.icon_count = 3;
        let positions = layout.compute_rest_positions();
        // Middle icon should be near center
        assert!((positions[1].x - 150.0).abs() < 1.0);
    }

    #[test]
    fn test_container_bounds() {
        let layout = DockLayout::with_config(DockLayoutConfig {
            container_size: Size::new(200.0, 100.0),
            ..Default::default()
        });
        let bounds = layout.container_bounds();
        assert!((bounds.width - 200.0).abs() < f32::EPSILON);
        assert!((bounds.height - 100.0).abs() < f32::EPSILON);
    }
}
