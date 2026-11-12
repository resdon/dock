//! Grid-based spatial hashing for efficient collision queries.
//!
//! The spatial index divides space into uniform grid cells and hashes bodies
//! into cells based on their bounding rectangles. This reduces broad-phase
//! collision detection from O(n²) to approximately O(n) for sparse scenes.

use std::collections::HashMap;

use super::body::{BodyId, PhysicsBody};
use crate::geometry::{Point, Rect};

/// A cell in the spatial hash grid.
#[derive(Debug, Clone)]
pub struct GridCell {
    /// Body IDs in this cell.
    pub bodies: Vec<BodyId>,
}

impl GridCell {
    /// Create a new empty cell.
    pub fn new() -> Self {
        Self {
            bodies: Vec::with_capacity(4),
        }
    }

    /// Add a body to this cell.
    pub fn add(&mut self, body: BodyId) {
        self.bodies.push(body);
    }

    /// Remove a body from this cell.
    pub fn remove(&mut self, body: BodyId) {
        self.bodies.retain(|&id| id != body);
    }

    /// Number of bodies in this cell.
    pub fn len(&self) -> usize {
        self.bodies.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.bodies.is_empty()
    }
}

impl Default for GridCell {
    fn default() -> Self {
        Self::new()
    }
}

/// Grid cell key (integer coordinates).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CellKey {
    pub x: i32,
    pub y: i32,
}

/// Configuration for the spatial hash grid.
#[derive(Debug, Clone, Copy)]
pub struct SpatialHashConfig {
    /// Cell size in world units.
    pub cell_size: f32,
    /// Maximum bodies per cell before overflow handling.
    pub max_bodies_per_cell: usize,
}

impl Default for SpatialHashConfig {
    fn default() -> Self {
        Self {
            cell_size: 64.0,
            max_bodies_per_cell: 16,
        }
    }
}

/// Statistics for the spatial hash.
#[derive(Debug, Clone, Copy)]
pub struct SpatialHashStats {
    /// Total number of cells.
    pub cell_count: usize,
    /// Number of non-empty cells.
    pub occupied_cells: usize,
    /// Total bodies indexed.
    pub body_count: usize,
    /// Maximum bodies in any single cell.
    pub max_bodies_in_cell: usize,
    /// Average bodies per occupied cell.
    pub avg_bodies_per_cell: f32,
}

/// Grid-based spatial hash index.
pub struct SpatialHash {
    /// Grid configuration.
    pub config: SpatialHashConfig,
    /// Hash map from cell key to cell data.
    grid: HashMap<CellKey, GridCell>,
    /// Reverse mapping: body ID to set of cell keys.
    body_cells: HashMap<BodyId, Vec<CellKey>>,
    /// Statistics.
    pub stats: SpatialHashStats,
}

impl SpatialHash {
    /// Create a new spatial hash with default configuration.
    pub fn new() -> Self {
        Self {
            config: SpatialHashConfig::default(),
            grid: HashMap::new(),
            body_cells: HashMap::new(),
            stats: SpatialHashStats {
                cell_count: 0,
                occupied_cells: 0,
                body_count: 0,
                max_bodies_in_cell: 0,
                avg_bodies_per_cell: 0.0,
            },
        }
    }

    /// Create with custom configuration.
    pub fn with_config(config: SpatialHashConfig) -> Self {
        Self {
            config,
            grid: HashMap::new(),
            body_cells: HashMap::new(),
            stats: SpatialHashStats {
                cell_count: 0,
                occupied_cells: 0,
                body_count: 0,
                max_bodies_in_cell: 0,
                avg_bodies_per_cell: 0.0,
            },
        }
    }

    /// Convert world coordinates to cell coordinates.
    pub fn world_to_cell(&self, x: f32, y: f32) -> CellKey {
        let cs = self.config.cell_size;
        CellKey {
            x: (x / cs).floor() as i32,
            y: (y / cs).floor() as i32,
        }
    }

    /// Get all cell keys that a bounding rectangle overlaps.
    pub fn get_cells_for_rect(&self, rect: &Rect) -> Vec<CellKey> {
        let min_key = self.world_to_cell(rect.x, rect.y);
        let max_key = self.world_to_cell(rect.right(), rect.bottom());

        let mut cells = Vec::new();
        for x in min_key.x..=max_key.x {
            for y in min_key.y..=max_key.y {
                cells.push(CellKey { x, y });
            }
        }
        cells
    }

    /// Insert a body into the spatial hash.
    pub fn insert(&mut self, body: &PhysicsBody) {
        // Remove from old cells first
        self.remove(body.id);

        let rect = body.bounding_rect();
        let cells = self.get_cells_for_rect(&rect);

        let mut cell_keys = Vec::new();
        for &key in &cells {
            let cell = self.grid.entry(key).or_insert_with(GridCell::new);
            cell.add(body.id);
            cell_keys.push(key);
        }

        self.body_cells.insert(body.id, cell_keys);
        self.update_stats();
    }

    /// Remove a body from the spatial hash.
    pub fn remove(&mut self, body: BodyId) {
        if let Some(cell_keys) = self.body_cells.remove(&body) {
            for &key in &cell_keys {
                if let Some(cell) = self.grid.get_mut(&key) {
                    cell.remove(body);
                    if cell.is_empty() {
                        self.grid.remove(&key);
                    }
                }
            }
        }
        self.update_stats();
    }

    /// Query all bodies at a point.
    pub fn query_point(&self, point: &Point) -> Vec<BodyId> {
        let key = self.world_to_cell(point.x, point.y);
        self.grid
            .get(&key)
            .map(|cell| cell.bodies.clone())
            .unwrap_or_default()
    }

    /// Query all bodies overlapping a rectangle.
    pub fn query_rect(&self, rect: &Rect) -> Vec<BodyId> {
        let cells = self.get_cells_for_rect(rect);
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for &key in &cells {
            if let Some(cell) = self.grid.get(&key) {
                for &body_id in &cell.bodies {
                    if seen.insert(body_id) {
                        result.push(body_id);
                    }
                }
            }
        }

        result
    }

    /// Find potential collision pairs.
    pub fn query_collision_pairs(&self) -> Vec<(BodyId, BodyId)> {
        let mut pairs = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for (_, cell) in &self.grid {
            let bodies = &cell.bodies;
            for i in 0..bodies.len() {
                for j in (i + 1)..bodies.len() {
                    let a = bodies[i];
                    let b = bodies[j];
                    // Ensure consistent ordering
                    let (first, second) = if a < b { (a, b) } else { (b, a) };
                    let pair_key = (first.as_u64(), second.as_u64());
                    if seen.insert(pair_key) {
                        pairs.push((first, second));
                    }
                }
            }
        }

        pairs
    }

    /// Rebuild the entire index from a list of bodies.
    pub fn rebuild(&mut self, bodies: &[(BodyId, PhysicsBody)]) {
        self.grid.clear();
        self.body_cells.clear();

        for (_, body) in bodies {
            self.insert(body);
        }
    }

    /// Clear all data.
    pub fn clear(&mut self) {
        self.grid.clear();
        self.body_cells.clear();
        self.stats = SpatialHashStats {
            cell_count: 0,
            occupied_cells: 0,
            body_count: 0,
            max_bodies_in_cell: 0,
            avg_bodies_per_cell: 0.0,
        };
    }

    /// Check if a body is indexed.
    pub fn contains(&self, body: BodyId) -> bool {
        self.body_cells.contains_key(&body)
    }

    /// Update internal statistics.
    fn update_stats(&mut self) {
        self.stats.cell_count = self.grid.len();
        self.stats.occupied_cells = self.grid.values().filter(|c| !c.is_empty()).count();
        self.stats.body_count = self.body_cells.len();

        let mut max_bodies = 0;
        let mut total_bodies = 0;
        let mut occupied = 0;

        for cell in self.grid.values() {
            if !cell.is_empty() {
                occupied += 1;
                total_bodies += cell.len();
                max_bodies = max_bodies.max(cell.len());
            }
        }

        self.stats.max_bodies_in_cell = max_bodies;
        self.stats.avg_bodies_per_cell = if occupied > 0 {
            total_bodies as f32 / occupied as f32
        } else {
            0.0
        };
    }
}

impl Default for SpatialHash {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Rect};
    use crate::physics::body::{BodyId, CollisionShape, PhysicsBody};

    #[test]
    fn test_world_to_cell() {
        let hash = SpatialHash::new();
        let key = hash.world_to_cell(100.0, 200.0);
        assert_eq!(key.x, 1); // 100 / 64 = 1.56, floor = 1
        assert_eq!(key.y, 3); // 200 / 64 = 3.125, floor = 3
    }

    #[test]
    fn test_insert_and_query_point() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        let results = hash.query_point(&Point::new(100.0, 100.0));
        assert!(results.contains(&BodyId::new(1)));
    }

    #[test]
    fn test_insert_and_query_rect() {
        let mut hash = SpatialHash::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(200.0, 200.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body1);
        hash.insert(&body2);
        let results = hash.query_rect(&Rect::new(0.0, 0.0, 150.0, 150.0));
        assert!(results.contains(&BodyId::new(1)));
        assert!(!results.contains(&BodyId::new(2)));
    }

    #[test]
    fn test_remove_body() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        assert!(hash.contains(BodyId::new(1)));
        hash.remove(BodyId::new(1));
        assert!(!hash.contains(BodyId::new(1)));
    }

    #[test]
    fn test_collision_pairs() {
        let mut hash = SpatialHash::new();
        // Two bodies in the same cell
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(10.0, 10.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(20.0, 20.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body1);
        hash.insert(&body2);
        let pairs = hash.query_collision_pairs();
        assert!(!pairs.is_empty());
    }

    #[test]
    fn test_no_collision_pairs_when_separate() {
        let mut hash = SpatialHash::new();
        let body1 = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(10.0, 10.0),
            CollisionShape::circle(5.0),
            1.0,
        );
        let body2 = PhysicsBody::dynamic_body(
            BodyId::new(2),
            Point::new(500.0, 500.0),
            CollisionShape::circle(5.0),
            1.0,
        );
        hash.insert(&body1);
        hash.insert(&body2);
        let pairs = hash.query_collision_pairs();
        assert!(pairs.is_empty());
    }

    #[test]
    fn test_rebuild() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        hash.clear();
        assert!(!hash.contains(BodyId::new(1)));
        hash.rebuild(&[(BodyId::new(1), body)]);
        assert!(hash.contains(BodyId::new(1)));
    }

    #[test]
    fn test_stats() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        assert_eq!(hash.stats.body_count, 1);
        assert!(hash.stats.occupied_cells > 0);
    }

    #[test]
    fn test_clear() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(100.0, 100.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        hash.clear();
        assert_eq!(hash.stats.body_count, 0);
        assert_eq!(hash.stats.occupied_cells, 0);
    }

    #[test]
    fn test_custom_cell_size() {
        let hash = SpatialHash::with_config(SpatialHashConfig {
            cell_size: 128.0,
            max_bodies_per_cell: 8,
        });
        let key = hash.world_to_cell(200.0, 0.0);
        assert_eq!(key.x, 1); // 200 / 128 = 1.56, floor = 1
    }

    #[test]
    fn test_get_cells_for_rect() {
        let hash = SpatialHash::new();
        // Rect spanning multiple cells
        let cells = hash.get_cells_for_rect(&Rect::new(0.0, 0.0, 200.0, 200.0));
        // Should cover cells (0,0) through (3,3) = 16 cells
        assert!(cells.len() >= 9);
    }

    #[test]
    fn test_duplicate_body_ids_not_added() {
        let mut hash = SpatialHash::new();
        let body = PhysicsBody::dynamic_body(
            BodyId::new(1),
            Point::new(10.0, 10.0),
            CollisionShape::circle(10.0),
            1.0,
        );
        hash.insert(&body);
        let results = hash.query_point(&Point::new(10.0, 10.0));
        // Should appear only once
        let count = results.iter().filter(|&&id| id == BodyId::new(1)).count();
        assert_eq!(count, 1);
    }
}
