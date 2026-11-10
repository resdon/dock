//! Body groups for dock layout behavior.
//!
//! Groups manage collections of bodies that behave together, such as dock icons
//! that redistribute when one is dragged. Groups provide:
//! - Collective spring attraction to rest positions
//! - Mutual repulsion between members
//! - Group-level constraints and boundaries
//! - Split and merge operations

use std::collections::HashMap;

use super::body::{BodyId, PhysicsBody};
use crate::geometry::{Point, Rect, Size};

/// Opaque identifier for a body group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GroupId(u64);

impl GroupId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for GroupId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

/// Behavior mode for a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupBehavior {
    /// Members attract to rest positions and repel each other.
    /// Default dock icon behavior.
    DockIcons,
    /// Members move together as a rigid unit.
    Rigid,
    /// Members are connected by springs in a chain.
    Chain,
    /// Members freely interact with no group constraints.
    Free,
}

impl Default for GroupBehavior {
    fn default() -> Self {
        Self::DockIcons
    }
}

/// Configuration for a body group.
#[derive(Debug, Clone)]
pub struct GroupConfig {
    /// Spring stiffness for rest position attraction.
    pub spring_stiffness: f32,
    /// Spring damping.
    pub spring_damping: f32,
    /// Mutual repulsion strength.
    pub repulsion_strength: f32,
    /// Minimum distance between members.
    pub min_distance: f32,
    /// Group behavior mode.
    pub behavior: GroupBehavior,
    /// Optional boundary rectangle.
    pub boundary: Option<Rect>,
}

impl Default for GroupConfig {
    fn default() -> Self {
        Self {
            spring_stiffness: 500.0,
            spring_damping: 0.8,
            repulsion_strength: 100.0,
            min_distance: 60.0,
            behavior: GroupBehavior::DockIcons,
            boundary: None,
        }
    }
}

/// A group of physics bodies that behave together.
#[derive(Debug, Clone)]
pub struct BodyGroup {
    /// Group ID.
    pub id: GroupId,
    /// Group name for debugging.
    pub name: String,
    /// Member body IDs.
    pub members: Vec<BodyId>,
    /// Rest positions for each member (same index as members).
    pub rest_positions: Vec<Point>,
    /// Group configuration.
    pub config: GroupConfig,
    /// Whether the group is active.
    pub enabled: bool,
    /// Reverse index from body to its position within the member vectors.
    member_index: HashMap<BodyId, usize>,
}

impl BodyGroup {
    /// Create a new group.
    pub fn new(id: GroupId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            members: Vec::new(),
            rest_positions: Vec::new(),
            config: GroupConfig::default(),
            enabled: true,
            member_index: HashMap::new(),
        }
    }

    /// Create a group with custom configuration.
    pub fn with_config(id: GroupId, name: impl Into<String>, config: GroupConfig) -> Self {
        Self {
            id,
            name: name.into(),
            members: Vec::new(),
            rest_positions: Vec::new(),
            config,
            enabled: true,
            member_index: HashMap::new(),
        }
    }

    fn reindex_members(&mut self) {
        self.member_index.clear();
        for (idx, &body_id) in self.members.iter().enumerate() {
            self.member_index.insert(body_id, idx);
        }
    }

    /// Add a member with a rest position.
    pub fn add_member(&mut self, body: BodyId, rest_position: Point) {
        if let Some(idx) = self.member_index.get(&body).copied() {
            self.rest_positions[idx] = rest_position;
            return;
        }

        self.members.push(body);
        self.rest_positions.push(rest_position);
        self.member_index.insert(body, self.members.len() - 1);
    }

    /// Remove a member by body ID.
    pub fn remove_member(&mut self, body: BodyId) {
        let Some(idx) = self.member_index.remove(&body) else {
            return;
        };

        self.members.swap_remove(idx);
        self.rest_positions.swap_remove(idx);
        self.reindex_members();
    }

    /// Check if a body is a member.
    pub fn contains(&self, body: BodyId) -> bool {
        self.member_index.contains_key(&body)
    }

    /// Number of members.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }

    /// Get the rest position for a body.
    pub fn rest_position(&self, body: BodyId) -> Option<Point> {
        self.member_index
            .get(&body)
            .and_then(|idx| self.rest_positions.get(*idx).copied())
    }

    /// Set the rest position for a body.
    pub fn set_rest_position(&mut self, body: BodyId, position: Point) {
        if let Some(idx) = self.member_index.get(&body).copied() {
            self.rest_positions[idx] = position;
        }
    }

    /// Compute the group's center of mass.
    pub fn center_of_mass(&self, bodies: &HashMap<BodyId, PhysicsBody>) -> Point {
        let mut total_mass = 0.0;
        let mut center = Point::ORIGIN;

        for &body_id in &self.members {
            if let Some(body) = bodies.get(&body_id) {
                if body.mass > 0.0 && body.mass.is_finite() {
                    center.x += body.position.x * body.mass;
                    center.y += body.position.y * body.mass;
                    total_mass += body.mass;
                }
            }
        }

        if total_mass > 0.0 {
            Point::new(center.x / total_mass, center.y / total_mass)
        } else {
            let mut position_sum = Point::ORIGIN;
            let mut count = 0usize;
            for &body_id in &self.members {
                if let Some(body) = bodies.get(&body_id) {
                    if body.position.x.is_finite() && body.position.y.is_finite() {
                        position_sum.x += body.position.x;
                        position_sum.y += body.position.y;
                        count += 1;
                    }
                }
            }
            if count == 0 {
                Point::ORIGIN
            } else {
                Point::new(position_sum.x / count as f32, position_sum.y / count as f32)
            }
        }
    }

    /// Compute the group's bounding rectangle.
    pub fn bounding_rect(&self, bodies: &HashMap<BodyId, PhysicsBody>) -> Option<Rect> {
        let mut bounds: Option<Rect> = None;
        for &body_id in &self.members {
            if let Some(body) = bodies.get(&body_id) {
                let b = body.bounding_rect();
                bounds = Some(match bounds {
                    Some(existing) => existing.union(&b),
                    None => b,
                });
            }
        }
        bounds
    }

    /// Check if all members are settled (near rest positions with low velocity).
    pub fn is_settled(&self, bodies: &HashMap<BodyId, PhysicsBody>, threshold: f32) -> bool {
        for (&body_id, &rest_pos) in self.members.iter().zip(&self.rest_positions) {
            if let Some(body) = bodies.get(&body_id) {
                let dx = body.position.x - rest_pos.x;
                let dy = body.position.y - rest_pos.y;
                let dist_sq = dx * dx + dy * dy;
                let vel_sq = body.velocity.length_squared();
                if dist_sq > threshold * threshold || vel_sq > threshold * threshold {
                    return false;
                }
            }
        }
        true
    }

    /// Apply boundary constraint to all members.
    pub fn apply_boundary(&self, bodies: &mut HashMap<BodyId, PhysicsBody>) {
        if let Some(boundary) = self.config.boundary {
            for &body_id in &self.members {
                if let Some(body) = bodies.get_mut(&body_id) {
                    if body.inv_mass == 0.0 {
                        continue;
                    }
                    let mut dx = 0.0;
                    let mut dy = 0.0;

                    if body.position.x < boundary.x {
                        dx = boundary.x - body.position.x;
                    } else if body.position.x > boundary.right() {
                        dx = boundary.right() - body.position.x;
                    }

                    if body.position.y < boundary.y {
                        dy = boundary.y - body.position.y;
                    } else if body.position.y > boundary.bottom() {
                        dy = boundary.bottom() - body.position.y;
                    }

                    body.position.x += dx;
                    body.position.y += dy;
                }
            }
        }
    }

    /// Split this group into two at the given body index.
    pub fn split_at(&self, index: usize) -> (Option<Self>, Option<Self>) {
        if index == 0 || index >= self.members.len() {
            return (None, None);
        }

        let mut left = Self {
            id: self.id,
            name: format!("{}_left", self.name),
            members: self.members[..index].to_vec(),
            rest_positions: self.rest_positions[..index].to_vec(),
            config: self.config.clone(),
            enabled: self.enabled,
            member_index: HashMap::new(),
        };
        left.reindex_members();

        let mut right = Self {
            id: GroupId::new(self.id.as_u64() + 1),
            name: format!("{}_right", self.name),
            members: self.members[index..].to_vec(),
            rest_positions: self.rest_positions[index..].to_vec(),
            config: self.config.clone(),
            enabled: self.enabled,
            member_index: HashMap::new(),
        };
        right.reindex_members();

        (Some(left), Some(right))
    }

    /// Merge another group into this one.
    pub fn merge(&mut self, other: &Self) {
        for (&body_id, &rest_pos) in other.members.iter().zip(&other.rest_positions) {
            if !self.members.contains(&body_id) {
                self.add_member(body_id, rest_pos);
            }
        }
    }

    /// Arrange existing members in a horizontal row by updating rest positions.
    pub fn arrange_horizontal(&mut self, start: Point, spacing: f32, member_size: Size) {
        self.rest_positions =
            Self::compute_horizontal_layout(self.members.len(), start, spacing, member_size);
    }

    /// Compute rest positions for a horizontal arrangement.
    pub fn compute_horizontal_layout(
        member_count: usize,
        start: Point,
        spacing: f32,
        member_size: Size,
    ) -> Vec<Point> {
        (0..member_count)
            .map(|i| {
                Point::new(
                    start.x + i as f32 * (member_size.width + spacing) + member_size.width / 2.0,
                    start.y + member_size.height / 2.0,
                )
            })
            .collect()
    }

    /// Compute rest positions for a vertical arrangement.
    pub fn compute_vertical_layout(
        member_count: usize,
        start: Point,
        spacing: f32,
        member_size: Size,
    ) -> Vec<Point> {
        (0..member_count)
            .map(|i| {
                Point::new(
                    start.x + member_size.width / 2.0,
                    start.y + i as f32 * (member_size.height + spacing) + member_size.height / 2.0,
                )
            })
            .collect()
    }
}

/// Manager for body groups.
pub struct GroupManager {
    /// All groups.
    groups: HashMap<GroupId, BodyGroup>,
    /// Next group ID.
    next_id: u64,
    /// Reverse index from a body ID to the group currently holding it.
    body_to_group: HashMap<BodyId, GroupId>,
    /// Rebuild the reverse index the next time a body lookup occurs.
    body_index_dirty: bool,
}

impl GroupManager {
    /// Create a new group manager.
    pub fn new() -> Self {
        Self {
            groups: HashMap::new(),
            next_id: 1,
            body_to_group: HashMap::new(),
            body_index_dirty: true,
        }
    }

    fn rebuild_body_index(&mut self) {
        self.body_to_group.clear();
        for (&group_id, group) in &self.groups {
            for &body_id in &group.members {
                self.body_to_group.insert(body_id, group_id);
            }
        }
        self.body_index_dirty = false;
    }

    /// Create a new group.
    pub fn create_group(&mut self, name: impl Into<String>) -> GroupId {
        let id = GroupId::new(self.next_id);
        self.next_id += 1;
        let group = BodyGroup::new(id, name);
        self.groups.insert(id, group);
        self.body_index_dirty = true;
        id
    }

    /// Create a group with custom configuration.
    pub fn create_group_with_config(
        &mut self,
        name: impl Into<String>,
        config: GroupConfig,
    ) -> GroupId {
        let id = GroupId::new(self.next_id);
        self.next_id += 1;
        let group = BodyGroup::with_config(id, name, config);
        self.groups.insert(id, group);
        self.body_index_dirty = true;
        id
    }

    /// Get a group.
    pub fn get_group(&self, id: GroupId) -> Option<&BodyGroup> {
        self.groups.get(&id)
    }

    /// Get a mutable group.
    pub fn get_group_mut(&mut self, id: GroupId) -> Option<&mut BodyGroup> {
        self.body_index_dirty = true;
        self.groups.get_mut(&id)
    }

    /// Remove a group.
    pub fn remove_group(&mut self, id: GroupId) -> Option<BodyGroup> {
        let removed = self.groups.remove(&id);
        self.body_index_dirty = true;
        removed
    }

    /// Find the group containing a body.
    pub fn find_group_for_body(&mut self, body: BodyId) -> Option<GroupId> {
        if self.body_index_dirty {
            self.rebuild_body_index();
        }
        self.body_to_group.get(&body).copied()
    }

    /// Number of groups.
    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Iterate over all groups.
    pub fn iter(&self) -> impl Iterator<Item = (&GroupId, &BodyGroup)> {
        self.groups.iter()
    }

    /// Clear all groups.
    pub fn clear(&mut self) {
        self.groups.clear();
        self.body_to_group.clear();
        self.body_index_dirty = false;
    }
}

impl Default for GroupManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_create_group() {
        let group = BodyGroup::new(GroupId::new(1), "dock");
        assert_eq!(group.id, GroupId::new(1));
        assert_eq!(group.name, "dock");
        assert_eq!(group.member_count(), 0);
    }

    #[test]
    fn test_add_member() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        group.add_member(BodyId::new(2), Point::new(110.0, 50.0));
        assert_eq!(group.member_count(), 2);
        assert!(group.contains(BodyId::new(1)));
        assert!(group.contains(BodyId::new(2)));
    }

    #[test]
    fn zero_mass_center_of_mass_uses_geometric_center() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        let mut bodies = HashMap::new();
        for (id, x) in [(1, 10.0), (2, 30.0)] {
            let mut body = PhysicsBody::dynamic_body(
                BodyId::new(id),
                Point::new(x, 20.0),
                CollisionShape::circle(5.0),
                1.0,
            );
            body.mass = 0.0;
            body.inv_mass = 0.0;
            group.add_member(BodyId::new(id), Point::new(x, 20.0));
            bodies.insert(BodyId::new(id), body);
        }

        assert_eq!(group.center_of_mass(&bodies), Point::new(20.0, 20.0));
    }

    #[test]
    fn test_remove_member() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        group.add_member(BodyId::new(2), Point::new(110.0, 50.0));
        group.remove_member(BodyId::new(1));
        assert_eq!(group.member_count(), 1);
        assert!(!group.contains(BodyId::new(1)));
        assert!(group.contains(BodyId::new(2)));
    }

    #[test]
    fn test_rest_position() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        let rest = group.rest_position(BodyId::new(1)).unwrap();
        assert!((rest.x - 50.0).abs() < f32::EPSILON);
        assert!(group.rest_position(BodyId::new(999)).is_none());
    }

    #[test]
    fn test_set_rest_position() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        group.set_rest_position(BodyId::new(1), Point::new(100.0, 100.0));
        let rest = group.rest_position(BodyId::new(1)).unwrap();
        assert!((rest.x - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_split_group() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        group.add_member(BodyId::new(2), Point::new(110.0, 50.0));
        group.add_member(BodyId::new(3), Point::new(170.0, 50.0));
        let (left, right) = group.split_at(2);
        assert!(left.is_some());
        assert!(right.is_some());
        let left = left.unwrap();
        let right = right.unwrap();
        assert_eq!(left.member_count(), 2);
        assert_eq!(right.member_count(), 1);
    }

    #[test]
    fn test_merge_groups() {
        let mut group1 = BodyGroup::new(GroupId::new(1), "dock1");
        group1.add_member(BodyId::new(1), Point::new(50.0, 50.0));
        let mut group2 = BodyGroup::new(GroupId::new(2), "dock2");
        group2.add_member(BodyId::new(2), Point::new(110.0, 50.0));
        group1.merge(&group2);
        assert_eq!(group1.member_count(), 2);
        assert!(group1.contains(BodyId::new(1)));
        assert!(group1.contains(BodyId::new(2)));
    }

    #[test]
    fn test_horizontal_layout() {
        let positions = BodyGroup::compute_horizontal_layout(
            3,
            Point::new(0.0, 40.0),
            10.0,
            Size::new(40.0, 40.0),
        );
        assert_eq!(positions.len(), 3);
        assert!((positions[0].x - 20.0).abs() < f32::EPSILON);
        assert!((positions[1].x - 70.0).abs() < f32::EPSILON);
        assert!((positions[2].x - 120.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_vertical_layout() {
        let positions = BodyGroup::compute_vertical_layout(
            3,
            Point::new(40.0, 0.0),
            10.0,
            Size::new(40.0, 40.0),
        );
        assert_eq!(positions.len(), 3);
        assert!((positions[0].y - 20.0).abs() < f32::EPSILON);
        assert!((positions[1].y - 70.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_group_manager() {
        let mut manager = GroupManager::new();
        let id1 = manager.create_group("dock1");
        let _id2 = manager.create_group("dock2");
        assert_eq!(manager.group_count(), 2);
        assert!(manager.get_group(id1).is_some());
        manager.remove_group(id1);
        assert_eq!(manager.group_count(), 1);
    }

    #[test]
    fn test_find_group_for_body() {
        let mut manager = GroupManager::new();
        let id = manager.create_group("dock");
        if let Some(group) = manager.get_group_mut(id) {
            group.add_member(BodyId::new(1), Point::ORIGIN);
        }
        assert_eq!(manager.find_group_for_body(BodyId::new(1)), Some(id));
        assert_eq!(manager.find_group_for_body(BodyId::new(999)), None);
    }

    #[test]
    fn test_boundary_constraint() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.config.boundary = Some(Rect::new(0.0, 0.0, 200.0, 100.0));
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));

        let mut bodies = HashMap::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(250.0, 50.0),
            super::super::body::CollisionShape::circle(10.0),
            1.0,
        );
        bodies.insert(BodyId::new(1), body);

        group.apply_boundary(&mut bodies);
        let body = bodies.get(&BodyId::new(1)).unwrap();
        assert!(body.position.x <= 200.0);
    }

    #[test]
    fn test_is_settled() {
        let mut group = BodyGroup::new(GroupId::new(1), "dock");
        group.add_member(BodyId::new(1), Point::new(50.0, 50.0));

        let mut bodies = HashMap::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(50.1, 50.1),
            super::super::body::CollisionShape::circle(10.0),
            1.0,
        );
        bodies.insert(BodyId::new(1), body);

        assert!(group.is_settled(&bodies, 1.0));
    }

    #[test]
    fn test_group_config_defaults() {
        let config = GroupConfig::default();
        assert!((config.spring_stiffness - 500.0).abs() < f32::EPSILON);
        assert_eq!(config.behavior, GroupBehavior::DockIcons);
    }
}
