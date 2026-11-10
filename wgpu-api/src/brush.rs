//! Brush types for filling and stroking shapes.
//!
//! A Brush unifies solid colors, gradients, and pattern fills into a single
//! type that can be used for both fill and stroke operations.

use crate::color::{Color, Gradient};
use crate::geometry::Size;
use crate::image::Image;

/// A brush for filling or stroking shapes.
///
/// Unifies solid colors, gradients, and pattern fills so that API methods
/// can accept a single type instead of separate color/gradient parameters.
#[derive(Debug, Clone)]
pub enum Brush {
    /// Solid color fill.
    Solid(Color),
    /// Gradient fill.
    Gradient(Gradient),
    /// Pattern/tile fill using an image.
    Pattern {
        /// The pattern image.
        image: Image,
        /// Tile size in logical pixels.
        tile_size: Size,
    },
}

impl Brush {
    /// Create a solid color brush.
    pub fn solid(color: Color) -> Self {
        Self::Solid(color)
    }

    /// Create a gradient brush.
    pub fn gradient(gradient: Gradient) -> Self {
        Self::Gradient(gradient)
    }

    /// Create a pattern brush from an image.
    pub fn pattern(image: Image, tile_size: Size) -> Self {
        Self::Pattern { image, tile_size }
    }

    /// Get the solid color if this brush is a solid color.
    pub fn as_solid(&self) -> Option<Color> {
        match self {
            Self::Solid(c) => Some(*c),
            _ => None,
        }
    }

    /// Get the gradient if this brush is a gradient.
    pub fn as_gradient(&self) -> Option<&Gradient> {
        match self {
            Self::Gradient(g) => Some(g),
            _ => None,
        }
    }

    /// Check if this brush is transparent.
    pub fn is_transparent(&self) -> bool {
        match self {
            Self::Solid(c) => c.a <= 0.0,
            Self::Gradient(g) => g.stops.iter().all(|s| s.color.a <= 0.0),
            Self::Pattern { .. } => false,
        }
    }

    /// Apply opacity to the brush.
    pub fn with_opacity(&self, opacity: f32) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(c.with_alpha(c.a * opacity)),
            Self::Gradient(g) => {
                let mut g = g.clone();
                for stop in &mut g.stops {
                    stop.color = stop.color.with_alpha(stop.color.a * opacity);
                }
                Self::Gradient(g)
            }
            Self::Pattern { image, tile_size } => {
                // Pattern opacity would be applied at render time
                Self::Pattern {
                    image: image.clone(),
                    tile_size: *tile_size,
                }
            }
        }
    }
}

impl From<Color> for Brush {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<Gradient> for Brush {
    fn from(gradient: Gradient) -> Self {
        Self::Gradient(gradient)
    }
}

impl PartialEq for Brush {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Solid(a), Self::Solid(b)) => a == b,
            (Self::Gradient(a), Self::Gradient(b)) => a == b,
            (Self::Pattern { .. }, Self::Pattern { .. }) => false, // Images are hard to compare
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    #[test]
    fn test_brush_from_color() {
        let brush: Brush = Color::RED.into();
        assert_eq!(brush.as_solid(), Some(Color::RED));
    }

    #[test]
    fn test_brush_opacity() {
        let brush = Brush::solid(Color::WHITE);
        let faded = brush.with_opacity(0.5);
        assert_eq!(faded.as_solid().unwrap().a, 0.5);
    }

    #[test]
    fn test_brush_transparent() {
        let brush = Brush::solid(Color::TRANSPARENT);
        assert!(brush.is_transparent());
    }
}
