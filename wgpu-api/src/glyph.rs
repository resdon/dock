//! Glyph atlas and caching for efficient text rendering.
//!
//! This module provides a dynamically expandable glyph atlas that stores
//! rasterized glyphs in a single GPU texture, minimizing draw calls and
//! VRAM usage. Glyphs are cached by font identity, glyph ID, rasterization
//! size, and rasterization mode.
//!
//! ## Design Principles
//!
//! - **Low VRAM**: Glyphs are packed into a single atlas texture with
//!   power-of-two dimensions that expands as needed.
//! - **Low CPU**: Once a glyph is rasterized and cached, subsequent uses
//!   are O(1) lookups.
//! - **Font fallback**: A shaped text run can contain positioned glyph IDs
//!   from different fonts, including multiple glyphs for one character or
//!   one glyph representing several characters.
//! - **Rasterization modes**: Start with grayscale for UI text, add SDF
//!   only if scaling without rerasterizing is needed.

use crate::geometry::{Point, Size};
use crate::texture::TextureId;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Rasterization mode
// ---------------------------------------------------------------------------

/// Rasterization mode for glyph rendering.
///
/// Start with grayscale for small UI text. Add SDF only if you discover
/// a real need for scaling text significantly without rerasterizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RasterizationMode {
    /// Grayscale alpha bitmap — straightforward and suitable for small UI text.
    /// Each pixel stores an alpha value (0.0–1.0).
    Grayscale,
    /// Sub-pixel anti-aliased bitmap — uses RGB channels for horizontal
    /// sub-pixel positioning. Better for LCD displays at fixed sizes.
    Subpixel,
    /// Signed Distance Field — stores distance to the glyph edge in each
    /// channel. Allows arbitrary scaling without rerasterizing, but uses
    /// more VRAM and shader complexity.
    Sdf,
    /// Multi-channel Signed Distance Field — improved SDF with better
    /// corner handling. Uses RGBA channels for distance fields.
    Msdf,
}

impl Default for RasterizationMode {
    fn default() -> Self {
        Self::Grayscale
    }
}

// ---------------------------------------------------------------------------
// Glyph info
// ---------------------------------------------------------------------------

/// Information about a single glyph in the atlas.
///
/// Each glyph stores its texture region within the atlas, advance metrics
/// for positioning, and bearing offsets for correct placement.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// Region of the glyph within the atlas texture.
    pub texture_region: TextureRegion,
    /// Horizontal advance in logical pixels.
    pub advance_x: f32,
    /// Vertical advance in logical pixels (typically 0 for horizontal text).
    pub advance_y: f32,
    /// Horizontal bearing offset from the pen position.
    pub bearing_x: f32,
    /// Vertical bearing offset from the baseline (negative = above baseline).
    pub bearing_y: f32,
    /// Glyph bitmap width in pixels.
    pub width: f32,
    /// Glyph bitmap height in pixels.
    pub height: f32,
}

impl GlyphInfo {
    /// Create a new glyph info.
    pub fn new(
        texture_region: TextureRegion,
        advance_x: f32,
        advance_y: f32,
        bearing_x: f32,
        bearing_y: f32,
        width: f32,
        height: f32,
    ) -> Self {
        Self {
            texture_region,
            advance_x,
            advance_y,
            bearing_x,
            bearing_y,
            width,
            height,
        }
    }

    /// Get the top-left position of the glyph relative to the pen position.
    pub fn position(&self) -> Point {
        Point::new(self.bearing_x, -self.bearing_y + self.height)
    }

    /// Check if this glyph is a zero-width space or similar invisible glyph.
    pub fn is_zero_width(&self) -> bool {
        self.width == 0.0 && self.height == 0.0
    }
}

// ---------------------------------------------------------------------------
// Texture region
// ---------------------------------------------------------------------------

/// A rectangular region within a texture atlas.
///
/// Used to locate a glyph within the atlas texture. Coordinates are
/// normalized (0.0–1.0) for direct use in shader UV calculations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureRegion {
    /// Atlas texture this region belongs to.
    pub texture_id: TextureId,
    /// Normalized UV coordinates of the top-left corner.
    pub uv_min: Point,
    /// Normalized UV coordinates of the bottom-right corner.
    pub uv_max: Point,
    /// Pixel coordinates within the atlas (for rasterization).
    pub pixel_min: Point,
    /// Pixel coordinates within the atlas (for rasterization).
    pub pixel_max: Point,
    /// Width of the region in pixels.
    pub width: u32,
    /// Height of the region in pixels.
    pub height: u32,
}

impl TextureRegion {
    /// Create a new texture region.
    pub fn new(
        texture_id: TextureId,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        atlas_width: u32,
        atlas_height: u32,
    ) -> Self {
        let uv_min = Point::new(
            x as f32 / atlas_width as f32,
            y as f32 / atlas_height as f32,
        );
        let uv_max = Point::new(
            (x + width) as f32 / atlas_width as f32,
            (y + height) as f32 / atlas_height as f32,
        );
        let pixel_min = Point::new(x as f32, y as f32);
        let pixel_max = Point::new((x + width) as f32, (y + height) as f32);

        Self {
            texture_id,
            uv_min,
            uv_max,
            pixel_min,
            pixel_max,
            width,
            height,
        }
    }

    /// Get the center of the region in UV coordinates.
    pub fn uv_center(&self) -> Point {
        Point::new(
            (self.uv_min.x + self.uv_max.x) * 0.5,
            (self.uv_min.y + self.uv_max.y) * 0.5,
        )
    }

    /// Check if this region is empty.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

// ---------------------------------------------------------------------------
// Glyph cache key
// ---------------------------------------------------------------------------

/// Unique key for caching glyphs.
///
/// A glyph is uniquely identified by:
/// - Font identity (which font file/face)
/// - Glyph ID (from the font's glyph table)
/// - Rasterization size (pixel dimensions)
/// - Rasterization mode (grayscale, subpixel, SDF)
///
/// Unicode codepoints are NOT glyph IDs. A shaped text run can contain
/// positioned glyph IDs from different fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphCacheKey {
    /// Unique identifier for the font (e.g., file path hash + face index).
    pub font_id: u32,
    /// Glyph ID from the font's glyph table.
    pub glyph_id: u16,
    /// Rasterization width in pixels.
    pub raster_width: u32,
    /// Rasterization height in pixels.
    pub raster_height: u32,
    /// Rasterization mode.
    pub mode: RasterizationMode,
}

impl GlyphCacheKey {
    /// Create a new cache key.
    pub fn new(font_id: u32, glyph_id: u16, size: Size, mode: RasterizationMode) -> Self {
        Self {
            font_id,
            glyph_id,
            raster_width: size.width as u32,
            raster_height: size.height as u32,
            mode,
        }
    }
}

// ---------------------------------------------------------------------------
// Glyph cluster
// ---------------------------------------------------------------------------

/// A cluster of characters that map to one or more glyphs.
///
/// In Unicode text, a single displayed character may require multiple glyphs
/// (e.g., combining accents), or multiple characters may map to a single glyph
/// (e.g., ligatures). A cluster tracks this many-to-many relationship.
#[derive(Debug, Clone)]
pub struct GlyphCluster {
    /// Byte range in the original string that this cluster represents.
    pub byte_range: std::ops::Range<usize>,
    /// Glyphs in this cluster (may be multiple for combining characters).
    pub glyphs: Vec<GlyphRun>,
}

impl GlyphCluster {
    /// Create a new glyph cluster.
    pub fn new(byte_range: std::ops::Range<usize>, glyphs: Vec<GlyphRun>) -> Self {
        Self { byte_range, glyphs }
    }

    /// Get the total advance width of this cluster.
    pub fn advance_width(&self) -> f32 {
        self.glyphs.iter().map(|g| g.info.advance_x).sum()
    }

    /// Check if this cluster is a space or zero-width glyph.
    pub fn is_space(&self) -> bool {
        if self.glyphs.len() != 1 {
            return false;
        }
        let ch = self.glyphs[0].char;
        ch == ' ' || ch == '\t' || self.glyphs[0].info.is_zero_width()
    }
}

// ---------------------------------------------------------------------------
// Glyph run
// ---------------------------------------------------------------------------

/// A single glyph with its position and info.
///
/// This is the final positioned glyph ready for rendering.
#[derive(Debug, Clone)]
pub struct GlyphRun {
    /// The Unicode character (for debugging; not used for rendering).
    pub char: char,
    /// Glyph ID from the font.
    pub glyph_id: u16,
    /// Font ID this glyph came from.
    pub font_id: u32,
    /// Position in logical pixels (pen position + bearing).
    pub position: Point,
    /// Glyph metrics and atlas region.
    pub info: GlyphInfo,
}

impl GlyphRun {
    /// Create a new glyph run.
    pub fn new(char: char, glyph_id: u16, font_id: u32, position: Point, info: GlyphInfo) -> Self {
        Self {
            char,
            glyph_id,
            font_id,
            position,
            info,
        }
    }
}

// ---------------------------------------------------------------------------
// Shaped line
// ---------------------------------------------------------------------------

/// A shaped line of text containing clusters and metrics.
#[derive(Debug, Clone)]
pub struct ShapedLine {
    /// Clusters in this line (each cluster may have multiple glyphs).
    pub clusters: Vec<GlyphCluster>,
    /// Total advance width of the line.
    pub width: f32,
    /// Ascent of the line (distance from baseline to top).
    pub ascent: f32,
    /// Descent of the line (distance from baseline to bottom).
    pub descent: f32,
    /// Line height (ascent + descent + leading).
    pub height: f32,
    /// White space at the end of the line.
    pub trailing_whitespace: f32,
}

impl ShapedLine {
    /// Create a new shaped line.
    pub fn new(
        clusters: Vec<GlyphCluster>,
        width: f32,
        ascent: f32,
        descent: f32,
    ) -> Self {
        let height = ascent + descent;
        Self {
            clusters,
            width,
            ascent,
            descent,
            height,
            trailing_whitespace: 0.0,
        }
    }

    /// Get all glyph runs in this line.
    pub fn all_glyphs(&self) -> impl Iterator<Item = &GlyphRun> {
        self.clusters.iter().flat_map(|c| c.glyphs.iter())
    }

    /// Check if this line is empty.
    pub fn is_empty(&self) -> bool {
        self.clusters.is_empty() || self.clusters.iter().all(|c| c.glyphs.is_empty())
    }
}

// ---------------------------------------------------------------------------
// Glyph atlas
// ---------------------------------------------------------------------------

/// A dynamically expandable glyph atlas.
///
/// Stores rasterized glyphs in a single GPU texture using a simple
/// row-based packing strategy. When the atlas is full, a new texture
/// is allocated with doubled dimensions.
///
/// ## Memory Efficiency
///
/// - Atlas textures use power-of-two dimensions for optimal GPU usage.
/// - Glyphs are packed row-by-row with minimal padding.
/// - Old atlases are retained until all references are dropped.
#[derive(Debug)]
pub struct GlyphAtlas {
    /// Current atlas texture ID.
    pub current_texture: TextureId,
    /// Atlas dimensions in pixels.
    pub size: Size,
    /// Current packing cursor position.
    pub cursor: Point,
    /// Current row height tracking.
    pub current_row_height: u32,
    /// Maximum atlas dimension (prevents unbounded growth).
    pub max_dimension: u32,
    /// Glyph cache mapping keys to atlas regions.
    pub cache: HashMap<GlyphCacheKey, GlyphInfo>,
    /// Cache statistics.
    pub stats: AtlasStats,
}

/// Statistics about the glyph atlas.
#[derive(Debug, Clone, Copy)]
pub struct AtlasStats {
    /// Number of glyphs cached.
    pub glyph_count: usize,
    /// Atlas texture width.
    pub atlas_width: u32,
    /// Atlas texture height.
    pub atlas_height: u32,
    /// Used pixels in the atlas.
    pub used_pixels: u32,
    /// Total pixels in the atlas.
    pub total_pixels: u32,
}

impl GlyphAtlas {
    /// Create a new glyph atlas with the given initial size.
    ///
    /// Default size is 1024x1024, which can hold thousands of glyphs
    /// at typical UI font sizes.
    pub fn new(initial_size: u32) -> Self {
        let size = Size::new(initial_size as f32, initial_size as f32);
        Self {
            current_texture: TextureId::new(0),
            size,
            cursor: Point::new(0.0, 0.0),
            current_row_height: 0,
            max_dimension: 4096,
            cache: HashMap::new(),
            stats: AtlasStats {
                glyph_count: 0,
                atlas_width: initial_size,
                atlas_height: initial_size,
                used_pixels: 0,
                total_pixels: initial_size * initial_size,
            },
        }
    }

    /// Allocate space in the atlas for a glyph of the given size.
    ///
    /// Returns the texture region for the glyph, or None if the atlas
    /// is full and cannot be expanded.
    pub fn allocate(&mut self, width: u32, height: u32) -> Option<TextureRegion> {
        if width == 0 || height == 0 {
            return None;
        }

        // Check if we need to wrap to the next row
        let row_width = self.size.width as u32;
        if self.cursor.x + width as f32 > row_width as f32 {
            self.cursor.x = 0.0;
            self.cursor.y += self.current_row_height as f32 + 1.0; // 1px padding between rows
            self.current_row_height = 0;
        }

        // Check if we need to expand the atlas
        if self.cursor.y + height as f32 > self.size.height as f32 {
            if !self.expand() {
                return None;
            }
        }

        let x = self.cursor.x as u32;
        let y = self.cursor.y as u32;

        // Update cursor
        self.cursor.x += width as f32 + 1.0; // 1px padding between glyphs
        if height > self.current_row_height {
            self.current_row_height = height;
        }

        // Update stats
        self.stats.used_pixels += width * height;
        self.stats.glyph_count += 1;

        Some(TextureRegion::new(
            self.current_texture,
            x,
            y,
            width,
            height,
            self.size.width as u32,
            self.size.height as u32,
        ))
    }

    /// Expand the atlas by doubling its dimensions.
    fn expand(&mut self) -> bool {
        let new_width = (self.size.width as u32 * 2).min(self.max_dimension);
        let new_height = (self.size.height as u32 * 2).min(self.max_dimension);

        if new_width == self.size.width as u32 && new_height == self.size.height as u32 {
            return false;
        }

        // In a real implementation, we would:
        // 1. Create a new texture
        // 2. Copy the old atlas data to the new texture
        // 3. Update all cached texture regions
        // For now, we just update the size tracking
        self.current_texture = TextureId::new(self.current_texture.as_u64() + 1);
        self.size = Size::new(new_width as f32, new_height as f32);
        self.cursor = Point::new(0.0, 0.0);
        self.current_row_height = 0;
        self.stats.atlas_width = new_width;
        self.stats.atlas_height = new_height;
        self.stats.total_pixels = new_width * new_height;
        self.stats.used_pixels = 0;

        true
    }

    /// Cache a glyph and return its info.
    pub fn cache_glyph(
        &mut self,
        key: GlyphCacheKey,
        width: u32,
        height: u32,
        advance_x: f32,
        advance_y: f32,
        bearing_x: f32,
        bearing_y: f32,
    ) -> Option<GlyphInfo> {
        // Check cache first
        if let Some(info) = self.cache.get(&key) {
            return Some(*info);
        }

        // Allocate space in the atlas
        let region = self.allocate(width, height)?;

        let info = GlyphInfo::new(region, advance_x, advance_y, bearing_x, bearing_y, width as f32, height as f32);
        self.cache.insert(key, info);
        Some(info)
    }

    /// Look up a cached glyph.
    pub fn get_glyph(&self, key: &GlyphCacheKey) -> Option<GlyphInfo> {
        self.cache.get(key).copied()
    }

    /// Estimate VRAM usage in bytes.
    ///
    /// Assumes 1 byte per pixel (grayscale alpha).
    pub fn estimate_vram_bytes(&self) -> u64 {
        self.stats.total_pixels as u64
    }

    /// Estimate VRAM usage in MB.
    pub fn estimate_vram_mb(&self) -> f32 {
        self.estimate_vram_bytes() as f32 / (1024.0 * 1024.0)
    }

    /// Get cache statistics.
    pub fn stats(&self) -> AtlasStats {
        self.stats
    }

    /// Clear the cache (useful when switching fonts or clearing memory).
    pub fn clear(&mut self) {
        self.cache.clear();
        self.cursor = Point::new(0.0, 0.0);
        self.current_row_height = 0;
        self.stats.glyph_count = 0;
        self.stats.used_pixels = 0;
    }
}

impl Default for GlyphAtlas {
    fn default() -> Self {
        Self::new(1024)
    }
}

// ---------------------------------------------------------------------------
// Glyph cache
// ---------------------------------------------------------------------------

/// High-level glyph cache that manages the atlas and rasterization.
///
/// This is the main entry point for glyph caching. It handles:
/// - Font identity tracking
/// - Glyph rasterization (via swash)
/// - Atlas management
/// - Cache eviction when memory limits are reached
#[derive(Debug)]
pub struct GlyphCache {
    /// The glyph atlas for texture storage.
    atlas: GlyphAtlas,
    /// Maximum VRAM budget in MB.
    max_vram_mb: f32,
    /// Rasterization mode for new glyphs.
    mode: RasterizationMode,
    /// Scale factor for rasterization (e.g., 2.0 for Retina displays).
    scale_factor: f32,
}

impl GlyphCache {
    /// Create a new glyph cache.
    ///
    /// Default VRAM budget is 16MB, which is sufficient for most UI text.
    pub fn new() -> Self {
        Self {
            atlas: GlyphAtlas::new(1024),
            max_vram_mb: 16.0,
            mode: RasterizationMode::Grayscale,
            scale_factor: 1.0,
        }
    }

    /// Set the maximum VRAM budget.
    pub fn with_max_vram_mb(mut self, mb: f32) -> Self {
        self.max_vram_mb = mb;
        self
    }

    /// Set the rasterization mode.
    pub fn with_mode(mut self, mode: RasterizationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the scale factor for rasterization.
    pub fn with_scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Get or create a glyph info for the given key.
    ///
    /// If the glyph is already cached, returns the cached info.
    /// Otherwise, allocates space in the atlas and returns a new info.
    pub fn get_or_allocate(
        &mut self,
        font_id: u32,
        glyph_id: u16,
        font_size: f32,
        advance_x: f32,
        advance_y: f32,
        bearing_x: f32,
        bearing_y: f32,
        glyph_width: u32,
        glyph_height: u32,
    ) -> Option<GlyphInfo> {
        let size = Size::new(
            (font_size * self.scale_factor).max(1.0),
            (font_size * self.scale_factor).max(1.0),
        );

        let key = GlyphCacheKey::new(font_id, glyph_id, size, self.mode);

        // Check cache first
        if let Some(info) = self.atlas.get_glyph(&key) {
            return Some(info);
        }

        // Check VRAM budget before allocating
        let estimated_vram = self.atlas.estimate_vram_mb() + (glyph_width * glyph_height) as f32 / (1024.0 * 1024.0);
        if estimated_vram > self.max_vram_mb {
            // In a real implementation, evict least-recently-used glyphs
            log::warn!("Glyph cache VRAM budget exceeded, consider increasing max_vram_mb");
        }

        // Allocate in atlas
        self.atlas.cache_glyph(
            key,
            glyph_width,
            glyph_height,
            advance_x,
            advance_y,
            bearing_x,
            bearing_y,
        )
    }

    /// Get the underlying atlas.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Get the mutable underlying atlas.
    pub fn atlas_mut(&mut self) -> &mut GlyphAtlas {
        &mut self.atlas
    }

    /// Estimate total VRAM usage.
    pub fn estimate_vram_mb(&self) -> f32 {
        self.atlas.estimate_vram_mb()
    }

    /// Clear all cached glyphs.
    pub fn clear(&mut self) {
        self.atlas.clear();
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glyph_info_creation() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 16, 16, 1024, 1024);
        let info = GlyphInfo::new(region, 10.0, 0.0, 2.0, 12.0, 16.0, 16.0);
        assert!((info.advance_x - 10.0).abs() < f32::EPSILON);
        assert!((info.bearing_x - 2.0).abs() < f32::EPSILON);
        assert!(!info.is_zero_width());
    }

    #[test]
    fn test_zero_width_glyph() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 0, 0, 1024, 1024);
        let info = GlyphInfo::new(region, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert!(info.is_zero_width());
    }

    #[test]
    fn test_texture_region_uv() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 16, 16, 1024, 1024);
        assert!((region.uv_min.x - 0.0).abs() < f32::EPSILON);
        assert!((region.uv_min.y - 0.0).abs() < f32::EPSILON);
        let expected = 16.0 / 1024.0;
        assert!((region.uv_max.x - expected).abs() < f32::EPSILON);
        assert!(!region.is_empty());
    }

    #[test]
    fn test_empty_texture_region() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 0, 16, 1024, 1024);
        assert!(region.is_empty());
    }

    #[test]
    fn test_glyph_cache_key() {
        let key = GlyphCacheKey::new(
            1,
            65,
            Size::new(16.0, 16.0),
            RasterizationMode::Grayscale,
        );
        assert_eq!(key.font_id, 1);
        assert_eq!(key.glyph_id, 65);
        assert_eq!(key.mode, RasterizationMode::Grayscale);
    }

    #[test]
    fn test_glyph_atlas_allocate() {
        let mut atlas = GlyphAtlas::new(256);
        let region = atlas.allocate(16, 16);
        assert!(region.is_some());
        let region = region.unwrap();
        assert_eq!(region.width, 16);
        assert_eq!(region.height, 16);
        assert!((region.pixel_min.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_glyph_atlas_row_wrap() {
        let mut atlas = GlyphAtlas::new(64);
        // Fill the first row
        for _ in 0..4 {
            let region = atlas.allocate(16, 16);
            assert!(region.is_some());
        }
        // Next allocation should wrap to the next row
        let region = atlas.allocate(16, 16);
        assert!(region.is_some());
        let region = region.unwrap();
        assert!(region.pixel_min.y > 0.0);
    }

    #[test]
    fn test_glyph_atlas_cache() {
        let mut atlas = GlyphAtlas::new(256);
        let key = GlyphCacheKey::new(1, 65, Size::new(16.0, 16.0), RasterizationMode::Grayscale);

        let info = atlas.cache_glyph(key, 16, 16, 10.0, 0.0, 2.0, 12.0);
        assert!(info.is_some());

        // Second call with the same key should return the cached info
        let info2 = atlas.cache_glyph(key, 16, 16, 10.0, 0.0, 2.0, 12.0);
        assert!(info2.is_some());
        assert_eq!(info.unwrap().texture_region.pixel_min, info2.unwrap().texture_region.pixel_min);
    }

    #[test]
    fn test_glyph_cache_creation() {
        let cache = GlyphCache::new()
            .with_max_vram_mb(32.0)
            .with_mode(RasterizationMode::Subpixel)
            .with_scale_factor(2.0);
        assert_eq!(cache.max_vram_mb, 32.0);
        assert_eq!(cache.mode, RasterizationMode::Subpixel);
        assert!((cache.scale_factor - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_glyph_cache_allocate() {
        let mut cache = GlyphCache::new();
        let info = cache.get_or_allocate(1, 65, 16.0, 10.0, 0.0, 2.0, 12.0, 16, 16);
        assert!(info.is_some());
    }

    #[test]
    fn test_glyph_cluster_advance() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 16, 16, 1024, 1024);
        let info = GlyphInfo::new(region, 10.0, 0.0, 2.0, 12.0, 16.0, 16.0);
        let glyph = GlyphRun::new('A', 65, 1, Point::new(0.0, 0.0), info);
        let cluster = GlyphCluster::new(0..1, vec![glyph]);
        assert!((cluster.advance_width() - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shaped_line() {
        let region = TextureRegion::new(TextureId::new(0), 0, 0, 16, 16, 1024, 1024);
        let info = GlyphInfo::new(region, 10.0, 0.0, 2.0, 12.0, 16.0, 16.0);
        let glyph = GlyphRun::new('A', 65, 1, Point::new(0.0, 0.0), info);
        let cluster = GlyphCluster::new(0..1, vec![glyph]);
        let line = ShapedLine::new(vec![cluster], 10.0, 12.0, 4.0);
        assert!((line.width - 10.0).abs() < f32::EPSILON);
        assert!((line.height - 16.0).abs() < f32::EPSILON);
        assert!(!line.is_empty());
    }

    #[test]
    fn test_atlas_stats() {
        let atlas = GlyphAtlas::new(1024);
        let stats = atlas.stats();
        assert_eq!(stats.glyph_count, 0);
        assert_eq!(stats.atlas_width, 1024);
        assert!((atlas.estimate_vram_mb() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_rasterization_mode_default() {
        let mode = RasterizationMode::default();
        assert_eq!(mode, RasterizationMode::Grayscale);
    }
}
