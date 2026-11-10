//! Layout types, dock arrangement, and physics-based icon positioning.
//!
//! This module provides:
//! - [`Insets`], [`CornerRadius`], [`Shadow`] — styling helpers
//! - Dock layout with physics-based icon arrangement
//! - Group redistribution and insertion animation

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

pub mod dock;
pub mod arrangement;
pub mod insertion;
pub mod projection;

// Re-export submodule types
pub use dock::{DockLayout, DockLayoutConfig, DockLayoutDirection, DockAlignment};
pub use arrangement::{ArrangementManager, RedistributionMode};
pub use insertion::{InsertionManager, InsertStrategy, InsertResult};
pub use projection::{ProjectionMode, ProjectionConfig, ProjectionData, Projector};

use crate::geometry::Size;

/// Four-sided insets (top, right, bottom, left).
///
/// Used for padding, margins, border widths, and spacing.
/// Coordinates are in floating-point logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Insets {
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
    /// Left inset.
    pub left: f32,
}

impl Insets {
    /// Zero insets.
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };

    /// Create new insets with individual values.
    pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    /// Create uniform insets (same value on all sides).
    pub const fn uniform(value: f32) -> Self {
        Self {
            top: value,
            right: value,
            bottom: value,
            left: value,
        }
    }

    /// Create vertical and horizontal insets.
    pub const fn vertical_horizontal(vertical: f32, horizontal: f32) -> Self {
        Self {
            top: vertical,
            right: horizontal,
            bottom: vertical,
            left: horizontal,
        }
    }

    /// Create from a size (used as padding).
    pub fn from_size(size: &Size) -> Self {
        Self {
            top: size.height,
            right: size.width,
            bottom: size.height,
            left: size.width,
        }
    }

    /// Get the total horizontal inset.
    pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    /// Get the total vertical inset.
    pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }

    /// Get the total size of the insets.
    pub fn size(&self) -> Size {
        Size::new(self.horizontal(), self.vertical())
    }

    /// Check if all insets are zero.
    pub fn is_empty(&self) -> bool {
        self.top <= 0.0 && self.right <= 0.0 && self.bottom <= 0.0 && self.left <= 0.0
    }

    /// Add two insets together.
    pub fn add(&self, other: &Self) -> Self {
        Self {
            top: self.top + other.top,
            right: self.right + other.right,
            bottom: self.bottom + other.bottom,
            left: self.left + other.left,
        }
    }

    /// Scale all insets by a factor.
    pub fn scale(&self, factor: f32) -> Self {
        Self {
            top: self.top * factor,
            right: self.right * factor,
            bottom: self.bottom * factor,
            left: self.left * factor,
        }
    }

    /// Inset a rectangle by these values (shrink from edges).
    pub fn inset_rect(&self, rect: &crate::geometry::Rect) -> crate::geometry::Rect {
        crate::geometry::Rect::new(
            rect.x + self.left,
            rect.y + self.top,
            rect.width - self.horizontal(),
            rect.height - self.vertical(),
        )
    }

    /// Outset a rectangle by these values (expand from edges).
    pub fn outset_rect(&self, rect: &crate::geometry::Rect) -> crate::geometry::Rect {
        crate::geometry::Rect::new(
            rect.x - self.left,
            rect.y - self.top,
            rect.width + self.horizontal(),
            rect.height + self.vertical(),
        )
    }
}

impl Default for Insets {
    fn default() -> Self {
        Self::ZERO
    }
}

impl std::fmt::Display for Insets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Insets(top={}, right={}, bottom={}, left={})", self.top, self.right, self.bottom, self.left)
    }
}

/// Corner radius for rounded rectangles with per-corner control.
///
/// Allows different radii for each corner of a rounded rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct CornerRadius {
    /// Top-left corner radius.
    pub top_left: f32,
    /// Top-right corner radius.
    pub top_right: f32,
    /// Bottom-right corner radius.
    pub bottom_right: f32,
    /// Bottom-left corner radius.
    pub bottom_left: f32,
}

impl CornerRadius {
    /// Zero corner radius (sharp corners).
    pub const ZERO: Self = Self {
        top_left: 0.0,
        top_right: 0.0,
        bottom_right: 0.0,
        bottom_left: 0.0,
    };

    /// Create new corner radius with individual values.
    pub const fn new(top_left: f32, top_right: f32, bottom_right: f32, bottom_left: f32) -> Self {
        Self {
            top_left,
            top_right,
            bottom_right,
            bottom_left,
        }
    }

    /// Create uniform corner radius (same value on all corners).
    pub const fn uniform(value: f32) -> Self {
        Self {
            top_left: value,
            top_right: value,
            bottom_right: value,
            bottom_left: value,
        }
    }

    /// Create with top and bottom radii.
    pub const fn top_bottom(top: f32, bottom: f32) -> Self {
        Self {
            top_left: top,
            top_right: top,
            bottom_right: bottom,
            bottom_left: bottom,
        }
    }

    /// Check if all corners are sharp (zero radius).
    pub fn is_zero(&self) -> bool {
        self.top_left <= 0.0
            && self.top_right <= 0.0
            && self.bottom_right <= 0.0
            && self.bottom_left <= 0.0
    }

    /// Get the maximum radius across all corners.
    pub fn max(&self) -> f32 {
        self.top_left
            .max(self.top_right)
            .max(self.bottom_right)
            .max(self.bottom_left)
    }

    /// Clamp radii to fit within a rectangle's dimensions.
    pub fn clamp_to_rect(&self, rect: &crate::geometry::Rect) -> Self {
        let half_w = (rect.width / 2.0).max(0.0);
        let half_h = (rect.height / 2.0).max(0.0);
        Self {
            top_left: self.top_left.min(half_h).min(half_w),
            top_right: self.top_right.min(half_h).min(half_w),
            bottom_right: self.bottom_right.min(half_h).min(half_w),
            bottom_left: self.bottom_left.min(half_h).min(half_w),
        }
    }

    /// Scale all radii by a factor.
    pub fn scale(&self, factor: f32) -> Self {
        Self {
            top_left: self.top_left * factor,
            top_right: self.top_right * factor,
            bottom_right: self.bottom_right * factor,
            bottom_left: self.bottom_left * factor,
        }
    }
}

impl Default for CornerRadius {
    fn default() -> Self {
        Self::ZERO
    }
}

impl From<f32> for CornerRadius {
    fn from(value: f32) -> Self {
        Self::uniform(value)
    }
}

/// Drop shadow effect configuration.
///
/// Used to render a shadow behind a shape or rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    /// Horizontal offset of the shadow.
    pub x: f32,
    /// Vertical offset of the shadow.
    pub y: f32,
    /// Blur radius of the shadow.
    pub blur: f32,
    /// Spread radius (positive = expand, negative = shrink).
    pub spread: f32,
    /// Shadow color.
    pub color: crate::color::Color,
}

impl Shadow {
    /// No shadow.
    pub const NONE: Self = Self {
        x: 0.0,
        y: 0.0,
        blur: 0.0,
        spread: 0.0,
        color: crate::color::Color::TRANSPARENT,
    };

    /// Create a new shadow.
    pub fn new(x: f32, y: f32, blur: f32, color: crate::color::Color) -> Self {
        Self {
            x,
            y,
            blur,
            spread: 0.0,
            color,
        }
    }

    /// Create a simple drop shadow (downward, slight blur).
    pub fn drop(blur: f32, opacity: f32) -> Self {
        Self {
            x: 0.0,
            y: 2.0,
            blur,
            spread: 0.0,
            color: crate::color::Color::BLACK.with_alpha(opacity),
        }
    }

    /// Set the spread radius.
    pub fn with_spread(mut self, spread: f32) -> Self {
        self.spread = spread;
        self
    }

    /// Check if this shadow has no effect.
    pub fn is_none(&self) -> bool {
        self.blur <= 0.0 && self.color.a <= 0.0
    }

    /// Estimate the bounding box expansion needed for this shadow.
    pub fn expansion(&self) -> Insets {
        let radius = self.blur + self.spread.max(0.0);
        let dx = self.x.abs() + radius;
        let dy = self.y.abs() + radius;
        Insets::uniform(dx.max(dy))
    }
}

impl Default for Shadow {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Rect;

    #[test]
    fn test_insets_uniform() {
        let insets = Insets::uniform(10.0);
        assert_eq!(insets.horizontal(), 20.0);
        assert_eq!(insets.vertical(), 20.0);
    }

    #[test]
    fn test_insets_inset_rect() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let insets = Insets::uniform(10.0);
        let inner = insets.inset_rect(&rect);
        assert!((inner.x - 10.0).abs() < f32::EPSILON);
        assert!((inner.width - 80.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_corner_radius_uniform() {
        let r: CornerRadius = 12.0.into();
        assert!((r.top_left - 12.0).abs() < f32::EPSILON);
        assert!((r.bottom_right - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shadow_expansion() {
        let shadow = Shadow::new(2.0, 4.0, 8.0, crate::color::Color::BLACK);
        let expansion = shadow.expansion();
        assert!(expansion.top > 8.0);
    }

    #[test]
    fn test_corner_radius_clamp() {
        let r = CornerRadius::uniform(100.0);
        let rect = Rect::new(0.0, 0.0, 40.0, 60.0);
        let clamped = r.clamp_to_rect(&rect);
        assert!(clamped.top_left <= 20.0); // half of width
    }
}
