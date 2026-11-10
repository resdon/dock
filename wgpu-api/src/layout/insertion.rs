//! Item insertion logic for dock layout.
//!
//! Handles inserting new items into a dock, computing where they should go,
//! and triggering the physics forces to make room.

use super::dock::DockLayout;
use crate::geometry::Point;
use crate::physics::BodyId;

/// Strategy for inserting a new item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertStrategy {
    /// Insert at the end.
    Append,
    /// Insert at the beginning.
    Prepend,
    /// Insert at a specific index.
    AtIndex(usize),
    /// Insert at the nearest empty slot.
    NearestSlot,
}

/// Result of an insertion operation.
#[derive(Debug, Clone)]
pub struct InsertResult {
    /// Index where the item was inserted.
    pub index: usize,
    /// Rest position for the new item.
    pub rest_position: Point,
    /// New body ID.
    pub body_id: BodyId,
    /// Bodies that were displaced.
    pub displaced: Vec<BodyId>,
}

/// Insertion manager for dock layout.
pub struct InsertionManager {
    /// Next body ID.
    next_id: u64,
}

impl InsertionManager {
    /// Create a new insertion manager.
    pub fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Insert an item into the dock layout.
    pub fn insert(
        &mut self,
        layout: &mut DockLayout,
        strategy: InsertStrategy,
        point: Option<Point>,
    ) -> InsertResult {
        let count = layout.icon_count;
        let icon_size = layout.config.icon_size;

        let index = match strategy {
            InsertStrategy::Append => count,
            InsertStrategy::Prepend => 0,
            InsertStrategy::AtIndex(i) => i.min(count),
            InsertStrategy::NearestSlot => {
                if let Some(p) = point {
                    layout.find_nearest_slot(&p).unwrap_or(count)
                } else {
                    count
                }
            }
        };

        layout.add_icon();
        let body_id = BodyId::new(self.next_id);
        self.next_id += 1;

        // Recompute positions
        let positions = layout.compute_rest_positions();
        let rest_position = if index < positions.len() {
            positions[index]
        } else {
            // Estimate position beyond last icon
            let last = positions.last().copied().unwrap_or(Point::ORIGIN);
            Point::new(last.x + icon_size.width + layout.config.spacing, last.y)
        };

        // Find displaced bodies (those after the insertion point)
        let mut displaced = Vec::new();
        for i in (index + 1)..=count {
            displaced.push(BodyId::new(i as u64));
        }

        InsertResult {
            index,
            rest_position,
            body_id,
            displaced,
        }
    }

    /// Remove an item from the dock layout.
    pub fn remove(&mut self, layout: &mut DockLayout, index: usize) -> Option<BodyId> {
        if index >= layout.icon_count {
            return None;
        }
        let body_id = BodyId::new((index + 1) as u64);
        layout.remove_icon();
        Some(body_id)
    }

    /// Compute the drop position for a dragged item (preview, does not modify layout).
    pub fn compute_drop_position(&self, layout: &DockLayout, drop_point: Point) -> InsertResult {
        let index = layout
            .find_nearest_slot(&drop_point)
            .unwrap_or(layout.icon_count);
        let body_id = BodyId::new(self.next_id);

        // Compute positions with one extra icon
        let positions = layout.compute_rest_positions_with_count(layout.icon_count + 1);
        let rest_position = if index < positions.len() {
            positions[index]
        } else {
            drop_point
        };

        InsertResult {
            index,
            rest_position,
            body_id,
            displaced: Vec::new(),
        }
    }
}

impl Default for InsertionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::layout::DockLayoutConfig;
    use crate::physics::BodyId;

    #[test]
    fn test_insert_append() {
        let mut layout = DockLayout::new();
        layout.icon_count = 2;
        let mut manager = InsertionManager::new();
        let result = manager.insert(&mut layout, InsertStrategy::Append, None);
        assert_eq!(result.index, 2);
        assert_eq!(layout.icon_count, 3);
    }

    #[test]
    fn test_insert_prepend() {
        let mut layout = DockLayout::new();
        layout.icon_count = 2;
        let mut manager = InsertionManager::new();
        let result = manager.insert(&mut layout, InsertStrategy::Prepend, None);
        assert_eq!(result.index, 0);
        assert_eq!(layout.icon_count, 3);
    }

    #[test]
    fn test_insert_at_index() {
        let mut layout = DockLayout::new();
        layout.icon_count = 4;
        let mut manager = InsertionManager::new();
        let result = manager.insert(&mut layout, InsertStrategy::AtIndex(2), None);
        assert_eq!(result.index, 2);
    }

    #[test]
    fn test_remove() {
        let mut layout = DockLayout::new();
        layout.icon_count = 3;
        let mut manager = InsertionManager::new();
        let removed = manager.remove(&mut layout, 1);
        assert!(removed.is_some());
        assert_eq!(layout.icon_count, 2);
    }

    #[test]
    fn test_remove_out_of_bounds() {
        let mut layout = DockLayout::new();
        layout.icon_count = 2;
        let mut manager = InsertionManager::new();
        let removed = manager.remove(&mut layout, 5);
        assert!(removed.is_none());
    }

    #[test]
    fn test_compute_drop_position() {
        let mut layout = DockLayout::with_config(DockLayoutConfig {
            icon_size: Size::new(40.0, 40.0),
            container_size: Size::new(300.0, 100.0),
            ..Default::default()
        });
        layout.icon_count = 3;
        let manager = InsertionManager::new();
        let result = manager.compute_drop_position(&layout, Point::new(50.0, 50.0));
        assert!(result.index <= 3);
    }

    #[test]
    fn test_displaced_bodies() {
        let mut layout = DockLayout::new();
        layout.icon_count = 3;
        let mut manager = InsertionManager::new();
        let result = manager.insert(&mut layout, InsertStrategy::AtIndex(1), None);
        // Bodies at indices 2 and 3 should be displaced
        assert!(!result.displaced.is_empty());
    }
}
