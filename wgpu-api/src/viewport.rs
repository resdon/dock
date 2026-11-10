//! Viewport and coordinate system management for HiDPI rendering.
//!
//! This module provides the `Viewport` type that separates logical coordinates
//! from physical pixel coordinates, making HiDPI handling straightforward.
//!
//! ## Coordinate Systems
//!
//! Application code uses **logical coordinates** (e.g., 400×300). The viewport
//! tracks the **scale factor** (e.g., 1.5) and computes the **physical
//! framebuffer size** (e.g., 600×450). Conversion happens only at render time.
//!
//! ```text
//! Logical width: 400
//! Logical height: 300
//! Scale factor: 1.5
//! ─────────────────────────
//! Physical framebuffer: 600 × 450
//! ```
//!
//! ## Resize Handling
//!
//! When a surface resizes, only the viewport and layout are updated. Icon
//! textures, glyph atlases, static images, animation definitions, and unrelated
//! GPU buffers are **not** recreated. Only resources whose dimensions or formats
//! actually changed are recreated.

use crate::geometry::{Size, Rect, Point};

// ---------------------------------------------------------------------------
// Viewport
// ---------------------------------------------------------------------------

/// A viewport defines the relationship between logical and physical coordinates.
///
/// Use logical coordinates in application code. The viewport converts to
/// physical pixels only when preparing the rendering pass.
///
/// # Example
///
/// ```
/// use wgpu_api::{Viewport, Size};
///
/// let viewport = Viewport::new(Size::new(400.0, 300.0), 1.5);
/// assert!((viewport.physical_size().width - 600.0).abs() < f32::EPSILON);
/// assert!((viewport.physical_size().height - 450.0).abs() < f32::EPSILON);
/// ```
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Logical size in application pixels.
    logical_size: Size,
    /// Physical size in framebuffer pixels (logical × scale).
    physical_size: Size,
    /// Device pixel ratio (DPI scale factor).
    scale_factor: f32,
}

impl Viewport {
    /// Create a new viewport with the given logical size and scale factor.
    ///
    /// The physical size is computed as `logical_size * scale_factor`, rounded
    /// to integer pixels.
    pub fn new(logical_size: Size, scale_factor: f32) -> Self {
        let sf = scale_factor.max(0.1);
        let physical_size = Size::new(
            (logical_size.width * sf).round(),
            (logical_size.height * sf).round(),
        );
        Self {
            logical_size,
            physical_size,
            scale_factor: sf,
        }
    }

    /// Create a viewport with a physical size, deriving the logical size.
    ///
    /// Useful when the OS reports the framebuffer size directly.
    pub fn from_physical(physical_size: Size, scale_factor: f32) -> Self {
        let sf = scale_factor.max(0.1);
        let logical_size = Size::new(
            physical_size.width / sf,
            physical_size.height / sf,
        );
        Self {
            logical_size,
            physical_size,
            scale_factor: sf,
        }
    }

    /// Get the logical size in application pixels.
    pub fn logical_size(&self) -> Size {
        self.logical_size
    }

    /// Get the physical size in framebuffer pixels.
    pub fn physical_size(&self) -> Size {
        self.physical_size
    }

    /// Get the device pixel ratio.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Convert a logical point to physical coordinates.
    pub fn logical_to_physical(&self, point: Point) -> Point {
        Point::new(
            (point.x * self.scale_factor).round(),
            (point.y * self.scale_factor).round(),
        )
    }

    /// Convert a physical point to logical coordinates.
    pub fn physical_to_logical(&self, point: Point) -> Point {
        Point::new(
            point.x / self.scale_factor,
            point.y / self.scale_factor,
        )
    }

    /// Convert a logical rectangle to physical coordinates.
    pub fn logical_to_physical_rect(&self, rect: Rect) -> Rect {
        let origin = self.logical_to_physical(rect.origin());
        let size = self.logical_size_to_physical(rect.size());
        Rect::from_origin_size(&origin, &size)
    }

    /// Convert a physical rectangle to logical coordinates.
    pub fn physical_to_logical_rect(&self, rect: Rect) -> Rect {
        let origin = self.physical_to_logical(rect.origin());
        let size = Size::new(
            rect.width / self.scale_factor,
            rect.height / self.scale_factor,
        );
        Rect::from_origin_size(&origin, &size)
    }

    /// Convert a logical size to physical dimensions.
    pub fn logical_size_to_physical(&self, size: Size) -> Size {
        Size::new(
            (size.width * self.scale_factor).round(),
            (size.height * self.scale_factor).round(),
        )
    }

    /// Convert a physical size to logical dimensions.
    pub fn physical_size_to_logical(&self, size: Size) -> Size {
        Size::new(
            size.width / self.scale_factor,
            size.height / self.scale_factor,
        )
    }

    /// Resize the viewport to a new logical size.
    ///
    /// Updates the physical size based on the current scale factor.
    pub fn resize(&mut self, logical_size: Size) {
        self.logical_size = logical_size;
        self.physical_size = Size::new(
            (logical_size.width * self.scale_factor).round(),
            (logical_size.height * self.scale_factor).round(),
        );
    }

    /// Resize the viewport to new physical dimensions.
    ///
    /// Updates the logical size based on the current scale factor.
    pub fn resize_physical(&mut self, physical_size: Size) {
        self.physical_size = physical_size;
        self.logical_size = Size::new(
            physical_size.width / self.scale_factor,
            physical_size.height / self.scale_factor,
        );
    }

    /// Update the scale factor, recomputing physical size.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.1);
        self.physical_size = Size::new(
            (self.logical_size.width * self.scale_factor).round(),
            (self.logical_size.height * self.scale_factor).round(),
        );
    }

    /// Check if the viewport is in HiDPI mode (scale > 1.0).
    pub fn is_hidpi(&self) -> bool {
        self.scale_factor > 1.0
    }

    /// Get the aspect ratio of the logical size.
    pub fn aspect_ratio(&self) -> f32 {
        self.logical_size.aspect_ratio()
    }

    /// Check if the viewport is empty.
    pub fn is_empty(&self) -> bool {
        self.logical_size.is_empty()
    }
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new(Size::new(800.0, 600.0), 1.0)
    }
}

impl From<Size> for Viewport {
    fn from(size: Size) -> Self {
        Self::new(size, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Resize event
// ---------------------------------------------------------------------------

/// Describes a resize event from the window system.
///
/// Carries both the new logical and physical dimensions so the renderer
/// can update the viewport without guessing which changed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ResizeEvent {
    /// New logical size.
    pub logical_size: Size,
    /// New physical size.
    pub physical_size: Size,
    /// New scale factor (may differ from previous).
    pub scale_factor: f32,
}

impl ResizeEvent {
    /// Create a resize event.
    pub fn new(logical_size: Size, physical_size: Size, scale_factor: f32) -> Self {
        Self {
            logical_size,
            physical_size,
            scale_factor,
        }
    }

    /// Create from a physical size and scale factor (derive logical).
    pub fn from_physical(physical_size: Size, scale_factor: f32) -> Self {
        let sf = scale_factor.max(0.1);
        let logical_size = Size::new(
            physical_size.width / sf,
            physical_size.height / sf,
        );
        Self {
            logical_size,
            physical_size,
            scale_factor: sf,
        }
    }

    /// Check if this is a meaningful resize (size actually changed).
    pub fn is_meaningful(&self) -> bool {
        !self.logical_size.is_empty() && self.scale_factor > 0.0
    }
}

// ---------------------------------------------------------------------------
// Resource change tracking
// ---------------------------------------------------------------------------

/// Tracks which GPU resources need to be recreated after a resize.
///
/// The goal is to **minimize** recreation: only resources whose dimensions
/// or formats actually changed should be recreated.
#[derive(Debug, Clone, PartialEq)]
#[derive(Default)]
pub struct ResourceChanges {
    /// The surface framebuffer needs to be resized.
    surface_framebuffer: bool,
    /// The MSAA render target needs to be resized.
    msaa_target: bool,
    /// The glyph atlas needs to be resized (HiDPI change).
    glyph_atlas: bool,
    /// The scissor rectangles need to be recalculated.
    scissor_rects: bool,
    /// The transform matrices need to be recalculated.
    transforms: bool,
}

impl ResourceChanges {
    /// Create a new empty change set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the surface framebuffer as needing resize.
    pub fn mark_surface_framebuffer(&mut self) {
        self.surface_framebuffer = true;
    }

    /// Mark the MSAA target as needing resize.
    pub fn mark_msaa_target(&mut self) {
        self.msaa_target = true;
    }

    /// Mark the glyph atlas as needing resize.
    pub fn mark_glyph_atlas(&mut self) {
        self.glyph_atlas = true;
    }

    /// Mark scissor rectangles as needing recalculation.
    pub fn mark_scissor_rects(&mut self) {
        self.scissor_rects = true;
    }

    /// Mark transforms as needing recalculation.
    pub fn mark_transforms(&mut self) {
        self.transforms = true;
    }

    /// Check if the surface framebuffer needs to be resized.
    pub fn needs_surface_framebuffer(&self) -> bool {
        self.surface_framebuffer
    }

    /// Check if the MSAA target needs to be resized.
    pub fn needs_msaa_target(&self) -> bool {
        self.msaa_target
    }

    /// Check if the glyph atlas needs to be resized.
    pub fn needs_glyph_atlas(&self) -> bool {
        self.glyph_atlas
    }

    /// Check if scissor rectangles need to be recalculated.
    pub fn needs_scissor_rects(&self) -> bool {
        self.scissor_rects
    }

    /// Check if transforms need to be recalculated.
    pub fn needs_transforms(&self) -> bool {
        self.transforms
    }

    /// Check if any resources need to be changed.
    pub fn has_changes(&self) -> bool {
        self.surface_framebuffer
            || self.msaa_target
            || self.glyph_atlas
            || self.scissor_rects
            || self.transforms
    }

    /// Clear all change flags.
    pub fn clear(&mut self) {
        *self = Self::new();
    }
}

/// Compute which resources need to change after a resize.
///
/// Compares the old and new viewports to determine the minimal set of
/// GPU resources that must be recreated.
pub fn compute_resource_changes(old_vp: &Viewport, new_vp: &Viewport) -> ResourceChanges {
    let mut changes = ResourceChanges::new();

    // Physical size changed → framebuffer and MSAA target
    if old_vp.physical_size() != new_vp.physical_size() {
        changes.mark_surface_framebuffer();
        changes.mark_msaa_target();
    }

    // Scale factor changed → glyph atlas may need rerasterization
    if (old_vp.scale_factor() - new_vp.scale_factor()).abs() > f32::EPSILON {
        changes.mark_glyph_atlas();
    }

    // Any size change → scissor rects and transforms
    if old_vp.logical_size() != new_vp.logical_size()
        || old_vp.physical_size() != new_vp.physical_size()
    {
        changes.mark_scissor_rects();
        changes.mark_transforms();
    }

    changes
}

// ---------------------------------------------------------------------------
// HiDPI-aware text scaling
// ---------------------------------------------------------------------------

/// Ensures text remains readable at different scale factors.
///
/// On HiDPI displays, text that is too small in logical pixels may still
/// appear blurry if the glyph atlas is not rasterized at the correct
/// physical resolution.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextScalePolicy {
    /// Minimum font size in logical pixels.
    pub min_font_size: f32,
    /// Whether to round font sizes to integer physical pixels.
    pub round_to_pixel: bool,
    /// Whether to use subpixel rendering.
    pub subpixel: bool,
}

impl TextScalePolicy {
    /// Create a new text scale policy.
    pub fn new() -> Self {
        Self {
            min_font_size: 8.0,
            round_to_pixel: true,
            subpixel: true,
        }
    }

    /// Compute the rasterization size for a font at the given scale factor.
    ///
    /// Returns the physical pixel size to use when rasterizing glyphs.
    pub fn raster_size(&self, font_size: f32, scale_factor: f32) -> f32 {
        let physical = font_size * scale_factor;
        if self.round_to_pixel {
            physical.round().max(self.min_font_size)
        } else {
            physical.max(self.min_font_size)
        }
    }

    /// Adjust a font size to ensure readability at the given scale.
    pub fn adjust_font_size(&self, font_size: f32, scale_factor: f32) -> f32 {
        let min_physical = self.min_font_size * scale_factor;
        let adjusted = font_size.max(min_physical / scale_factor);
        if self.round_to_pixel {
            (adjusted * scale_factor).round() / scale_factor
        } else {
            adjusted
        }
    }
}

impl Default for TextScalePolicy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_viewport_creation() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 1.5);
        assert!((vp.logical_size().width - 400.0).abs() < f32::EPSILON);
        assert!((vp.physical_size().width - 600.0).abs() < f32::EPSILON);
        assert!((vp.physical_size().height - 450.0).abs() < f32::EPSILON);
        assert!((vp.scale_factor() - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_from_physical() {
        let vp = Viewport::from_physical(Size::new(600.0, 450.0), 1.5);
        assert!((vp.logical_size().width - 400.0).abs() < f32::EPSILON);
        assert!((vp.physical_size().width - 600.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_logical_to_physical() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        let point = Point::new(100.0, 50.0);
        let physical = vp.logical_to_physical(point);
        assert!((physical.x - 200.0).abs() < f32::EPSILON);
        assert!((physical.y - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_physical_to_logical() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        let point = Point::new(200.0, 100.0);
        let logical = vp.physical_to_logical(point);
        assert!((logical.x - 100.0).abs() < f32::EPSILON);
        assert!((logical.y - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_rect_conversion() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        let rect = Rect::new(10.0, 20.0, 100.0, 50.0);
        let physical = vp.logical_to_physical_rect(rect);
        assert!((physical.x - 20.0).abs() < f32::EPSILON);
        assert!((physical.y - 40.0).abs() < f32::EPSILON);
        assert!((physical.width - 200.0).abs() < f32::EPSILON);
        assert!((physical.height - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_resize() {
        let mut vp = Viewport::new(Size::new(400.0, 300.0), 1.5);
        vp.resize(Size::new(800.0, 600.0));
        assert!((vp.logical_size().width - 800.0).abs() < f32::EPSILON);
        assert!((vp.physical_size().width - 1200.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_resize_physical() {
        let mut vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        vp.resize_physical(Size::new(1600.0, 1200.0));
        assert!((vp.physical_size().width - 1600.0).abs() < f32::EPSILON);
        assert!((vp.logical_size().width - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_scale_factor_change() {
        let mut vp = Viewport::new(Size::new(400.0, 300.0), 1.0);
        vp.set_scale_factor(2.0);
        assert!((vp.scale_factor() - 2.0).abs() < f32::EPSILON);
        assert!((vp.physical_size().width - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_hidpi() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        assert!(vp.is_hidpi());

        let vp = Viewport::new(Size::new(400.0, 300.0), 1.0);
        assert!(!vp.is_hidpi());
    }

    #[test]
    fn test_viewport_default() {
        let vp = Viewport::default();
        assert!((vp.logical_size().width - 800.0).abs() < f32::EPSILON);
        assert!((vp.scale_factor() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_from_size() {
        let vp = Viewport::from(Size::new(1024.0, 768.0));
        assert!((vp.logical_size().width - 1024.0).abs() < f32::EPSILON);
        assert!((vp.scale_factor() - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_resize_event() {
        let event = ResizeEvent::new(
            Size::new(800.0, 600.0),
            Size::new(1600.0, 1200.0),
            2.0,
        );
        assert!(event.is_meaningful());
    }

    #[test]
    fn test_resize_event_from_physical() {
        let event = ResizeEvent::from_physical(Size::new(1600.0, 1200.0), 2.0);
        assert!((event.logical_size.width - 800.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_resource_changes() {
        let old_vp = Viewport::new(Size::new(400.0, 300.0), 1.0);
        let new_vp = Viewport::new(Size::new(800.0, 600.0), 2.0);
        let changes = compute_resource_changes(&old_vp, &new_vp);

        assert!(changes.needs_surface_framebuffer());
        assert!(changes.needs_msaa_target());
        assert!(changes.needs_glyph_atlas());
        assert!(changes.needs_scissor_rects());
        assert!(changes.needs_transforms());
        assert!(changes.has_changes());
    }

    #[test]
    fn test_resource_changes_same_size() {
        let vp = Viewport::new(Size::new(400.0, 300.0), 1.0);
        let changes = compute_resource_changes(&vp, &vp);
        assert!(!changes.has_changes());
    }

    #[test]
    fn test_resource_changes_scale_only() {
        let old_vp = Viewport::new(Size::new(400.0, 300.0), 1.0);
        let new_vp = Viewport::new(Size::new(400.0, 300.0), 2.0);
        let changes = compute_resource_changes(&old_vp, &new_vp);

        assert!(changes.needs_surface_framebuffer());
        assert!(changes.needs_glyph_atlas());
        // Physical size changed, so scissor rects and transforms need recalculation
        assert!(changes.needs_scissor_rects());
        assert!(changes.needs_transforms());
    }

    #[test]
    fn test_text_scale_policy() {
        let policy = TextScalePolicy::new();
        let raster = policy.raster_size(12.0, 2.0);
        assert!((raster - 24.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_text_scale_policy_min_size() {
        let policy = TextScalePolicy::new();
        let raster = policy.raster_size(2.0, 1.0);
        assert!(raster >= policy.min_font_size);
    }

    #[test]
    fn test_text_scale_policy_adjust() {
        let policy = TextScalePolicy::new();
        let adjusted = policy.adjust_font_size(12.0, 2.0);
        assert!(adjusted >= 12.0);
    }

    #[test]
    fn test_viewport_aspect_ratio() {
        let vp = Viewport::new(Size::new(1600.0, 900.0), 1.0);
        let ratio = vp.aspect_ratio();
        assert!((ratio - 16.0 / 9.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_viewport_empty() {
        let vp = Viewport::new(Size::new(0.0, 0.0), 1.0);
        assert!(vp.is_empty());

        let vp = Viewport::new(Size::new(100.0, 100.0), 1.0);
        assert!(!vp.is_empty());
    }

    #[test]
    fn test_viewport_scale_clamp() {
        // Scale factor should be clamped to minimum 0.1
        let vp = Viewport::new(Size::new(400.0, 300.0), 0.0);
        assert!((vp.scale_factor() - 0.1).abs() < f32::EPSILON);
    }
}
