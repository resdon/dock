//! Color, gradient, and blending types.
//!
//! Colors use normalized floating-point components (0.0–1.0) for R, G, B, A.
//! This matches the GPU shader convention and avoids precision loss from
//! 8-bit integer arithmetic during interpolation and blending.

/// An RGBA color with normalized floating-point components.
///
/// All components are in the range `0.0..=1.0`. The alpha channel represents
/// opacity (0.0 = fully transparent, 1.0 = fully opaque).
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct Color {
    /// Red component (0.0–1.0).
    pub r: f32,
    /// Green component (0.0–1.0).
    pub g: f32,
    /// Blue component (0.0–1.0).
    pub b: f32,
    /// Alpha/opacity component (0.0–1.0).
    pub a: f32,
}

impl Color {
    /// Fully transparent black.
    pub const TRANSPARENT: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 0.0,
    };

    /// Fully opaque black.
    pub const BLACK: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// Fully opaque white.
    pub const WHITE: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// Fully opaque red.
    pub const RED: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 0.0,
        a: 1.0,
    };

    /// Fully opaque green.
    pub const GREEN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };

    /// Fully opaque blue.
    pub const BLUE: Self = Self {
        r: 0.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    /// Fully opaque yellow.
    pub const YELLOW: Self = Self {
        r: 1.0,
        g: 1.0,
        b: 0.0,
        a: 1.0,
    };

    /// Fully opaque cyan.
    pub const CYAN: Self = Self {
        r: 0.0,
        g: 1.0,
        b: 1.0,
        a: 1.0,
    };

    /// Fully opaque magenta.
    pub const MAGENTA: Self = Self {
        r: 1.0,
        g: 0.0,
        b: 1.0,
        a: 1.0,
    };

    /// Create a new color from RGBA components (0.0–1.0).
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self {
            r: r.clamp(0.0, 1.0),
            g: g.clamp(0.0, 1.0),
            b: b.clamp(0.0, 1.0),
            a: a.clamp(0.0, 1.0),
        }
    }

    /// Create a fully opaque color from RGB components (0.0–1.0).
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    /// Create from 8-bit RGBA values (0–255).
    pub fn from_u8_rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self {
            r: r as f32 / 255.0,
            g: g as f32 / 255.0,
            b: b as f32 / 255.0,
            a: a as f32 / 255.0,
        }
    }

    /// Create from 8-bit RGB values (0–255), fully opaque.
    pub fn from_u8_rgb(r: u8, g: u8, b: u8) -> Self {
        Self::from_u8_rgba(r, g, b, 255)
    }

    /// Create from a hex string (e.g., "#FF0000", "FF0000", "#FF0000FF").
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        let bytes = u32::from_str_radix(hex, 16).ok()?;

        if hex.len() == 6 {
            // #RRGGBB
            let r = (bytes >> 16) & 0xFF;
            let g = (bytes >> 8) & 0xFF;
            let b = bytes & 0xFF;
            Some(Self::from_u8_rgb(r as u8, g as u8, b as u8))
        } else if hex.len() == 8 {
            // #RRGGBBAA
            let a = (bytes >> 24) & 0xFF;
            let r = (bytes >> 16) & 0xFF;
            let g = (bytes >> 8) & 0xFF;
            let b = bytes & 0xFF;
            Some(Self::from_u8_rgba(r as u8, g as u8, b as u8, a as u8))
        } else {
            None
        }
    }

    /// Convert to 8-bit RGBA array [R, G, B, A].
    pub fn to_u8_rgba(&self) -> [u8; 4] {
        [
            (self.r * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.b * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    /// Convert to 8-bit BGRA array [B, G, R, A] (BGRA8888 format).
    pub fn to_u8_bgra(&self) -> [u8; 4] {
        [
            (self.b * 255.0).round() as u8,
            (self.g * 255.0).round() as u8,
            (self.r * 255.0).round() as u8,
            (self.a * 255.0).round() as u8,
        ]
    }

    /// Linear interpolation between two colors.
    pub fn lerp(a: &Self, b: &Self, t: f32) -> Self {
        Self {
            r: a.r + (b.r - a.r) * t,
            g: a.g + (b.g - a.g) * t,
            b: a.b + (b.b - a.b) * t,
            a: a.a + (b.a - a.a) * t,
        }
    }

    /// Multiply this color by another (component-wise).
    pub fn multiply(&self, other: &Self) -> Self {
        Self {
            r: self.r * other.r,
            g: self.g * other.g,
            b: self.b * other.b,
            a: self.a * other.a,
        }
    }

    /// Apply alpha blending: this color over `background`.
    pub fn over(&self, background: &Self) -> Self {
        let out_a = self.a + background.a * (1.0 - self.a);
        if out_a < f32::EPSILON {
            return Self::TRANSPARENT;
        }
        Self {
            r: (self.r * self.a + background.r * background.a * (1.0 - self.a)) / out_a,
            g: (self.g * self.a + background.g * background.a * (1.0 - self.a)) / out_a,
            b: (self.b * self.a + background.b * background.a * (1.0 - self.a)) / out_a,
            a: out_a,
        }
    }

    /// Scale the alpha channel.
    pub fn with_alpha(&self, alpha: f32) -> Self {
        Self {
            r: self.r,
            g: self.g,
            b: self.b,
            a: alpha.clamp(0.0, 1.0),
        }
    }

    /// Compute the relative luminance (WCAG 2.0 formula).
    pub fn luminance(&self) -> f32 {
        let r = if self.r <= 0.04045 {
            self.r / 12.92
        } else {
            ((self.r + 0.055) / 1.055).powf(2.4)
        };
        let g = if self.g <= 0.04045 {
            self.g / 12.92
        } else {
            ((self.g + 0.055) / 1.055).powf(2.4)
        };
        let b = if self.b <= 0.04045 {
            self.b / 12.92
        } else {
            ((self.b + 0.055) / 1.055).powf(2.4)
        };
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    /// Compute the contrast ratio between two colors (WCAG 2.0).
    pub fn contrast_ratio(a: &Self, b: &Self) -> f32 {
        let l1 = a.luminance();
        let l2 = b.luminance();
        let lighter = l1.max(l2);
        let darker = l1.min(l2);
        (lighter + 0.05) / (darker + 0.05)
    }

    /// Convert to grayscale (luminance-based).
    pub fn to_grayscale(&self) -> Self {
        let l = self.luminance();
        Self {
            r: l,
            g: l,
            b: l,
            a: self.a,
        }
    }

    /// Invert the color (negate RGB, keep alpha).
    pub fn invert(&self) -> Self {
        Self {
            r: 1.0 - self.r,
            g: 1.0 - self.g,
            b: 1.0 - self.b,
            a: self.a,
        }
    }

    /// Adjust brightness (-1.0 = darker, 0.0 = unchanged, 1.0 = brighter).
    pub fn adjust_brightness(&self, amount: f32) -> Self {
        Self {
            r: (self.r + amount).clamp(0.0, 1.0),
            g: (self.g + amount).clamp(0.0, 1.0),
            b: (self.b + amount).clamp(0.0, 1.0),
            a: self.a,
        }
    }

    /// Adjust saturation (0.0 = grayscale, 1.0 = unchanged, >1.0 = more saturated).
    pub fn adjust_saturation(&self, amount: f32) -> Self {
        let gray = self.luminance();
        Self {
            r: gray + (self.r - gray) * amount,
            g: gray + (self.g - gray) * amount,
            b: gray + (self.b - gray) * amount,
            a: self.a,
        }
    }
}

/// Type of gradient fill.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GradientType {
    /// Linear gradient along a line from start to end point.
    Linear {
        /// Start point of the gradient line.
        start_x: f32,
        /// Start point of the gradient line.
        start_y: f32,
        /// End point of the gradient line.
        end_x: f32,
        /// End point of the gradient line.
        end_y: f32,
    },
    /// Radial gradient from a center point outward.
    Radial {
        /// Center X of the inner circle.
        center_x: f32,
        /// Center Y of the inner circle.
        center_y: f32,
        /// Radius of the inner circle.
        inner_radius: f32,
        /// Center X of the outer circle (can differ for elliptical gradients).
        focal_x: f32,
        /// Center Y of the outer circle.
        focal_y: f32,
        /// Radius of the outer circle.
        outer_radius: f32,
    },
}

/// A color stop in a gradient.
#[derive(Debug, Clone, PartialEq)]
pub struct ColorStop {
    /// Position along the gradient (0.0 = start, 1.0 = end).
    pub offset: f32,
    /// Color at this position.
    pub color: Color,
}

impl ColorStop {
    /// Create a new color stop.
    pub fn new(offset: f32, color: Color) -> Self {
        Self {
            offset: offset.clamp(0.0, 1.0),
            color,
        }
    }
}

/// A gradient fill (linear or radial).
///
/// Used as a fill or stroke style instead of a solid color.
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    /// Type of gradient (linear or radial).
    pub gradient_type: GradientType,
    /// Color stops defining the gradient.
    pub stops: Vec<ColorStop>,
    /// Spread mode when the gradient extends beyond its bounds.
    pub spread: SpreadMode,
}

impl Gradient {
    /// Create a new linear gradient.
    pub fn linear(start_x: f32, start_y: f32, end_x: f32, end_y: f32) -> Self {
        Self {
            gradient_type: GradientType::Linear {
                start_x,
                start_y,
                end_x,
                end_y,
            },
            stops: Vec::new(),
            spread: SpreadMode::Pad,
        }
    }

    /// Create a new radial gradient.
    pub fn radial(
        center_x: f32,
        center_y: f32,
        inner_radius: f32,
        focal_x: f32,
        focal_y: f32,
        outer_radius: f32,
    ) -> Self {
        Self {
            gradient_type: GradientType::Radial {
                center_x,
                center_y,
                inner_radius,
                focal_x,
                focal_y,
                outer_radius,
            },
            stops: Vec::new(),
            spread: SpreadMode::Pad,
        }
    }

    /// Add a color stop to the gradient.
    pub fn add_stop(&mut self, offset: f32, color: Color) {
        self.stops.push(ColorStop::new(offset, color));
    }

    /// Set the spread mode.
    pub fn with_spread(mut self, spread: SpreadMode) -> Self {
        self.spread = spread;
        self
    }

    /// Sample the gradient at a given position (0.0–1.0 along the gradient).
    pub fn sample(&self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let stops = &self.stops;

        if stops.is_empty() {
            return Color::TRANSPARENT;
        }
        if stops.len() == 1 {
            return stops[0].color;
        }

        // Find the two surrounding stops
        for window in stops.windows(2) {
            let a = &window[0];
            let b = &window[1];
            if t >= a.offset && t <= b.offset {
                let local_t = if (b.offset - a.offset).abs() > f32::EPSILON {
                    (t - a.offset) / (b.offset - a.offset)
                } else {
                    0.0
                };
                return Color::lerp(&a.color, &b.color, local_t);
            }
        }

        // Fallback: clamp to first or last stop
        if t <= stops[0].offset {
            stops[0].color
        } else {
            stops[stops.len() - 1].color
        }
    }
}

/// Spread mode for gradients extending beyond their bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpreadMode {
    /// Repeat the gradient pattern.
    Repeat,
    /// Repeat the gradient pattern, reversing direction each time.
    Reflect,
    /// Extend the edge colors beyond the gradient bounds.
    Pad,
}

/// Blend mode for compositing operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlendMode {
    /// Standard alpha blending (source over destination).
    SourceOver,
    /// No blending; source replaces destination.
    SourceIn,
    /// Source only where destination exists.
    SourceOut,
    /// Source where destination doesn't exist.
    SourceAtop,
    /// Destination over source.
    DestinationOver,
    /// Multiply the source and destination colors.
    Multiply,
    /// Screen blend mode (inverted multiply).
    Screen,
    /// Overlay blend mode.
    Overlay,
    /// Darken blend mode.
    Darken,
    /// Lighten blend mode.
    Lighten,
    /// Color dodge blend mode.
    ColorDodge,
    /// Color burn blend mode.
    ColorBurn,
    /// Hard light blend mode.
    HardLight,
    /// Soft light blend mode.
    SoftLight,
    /// Difference blend mode.
    Difference,
    /// Exclusion blend mode.
    Exclusion,
    /// Extract the alpha channel as a mask.
    AlphaMask,
    /// Additive blending (light adds up).
    Additive,
    /// Subtractive blending.
    Subtractive,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::SourceOver
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_hex() {
        let c = Color::from_hex("#FF0000").unwrap();
        assert!((c.r - 1.0).abs() < f32::EPSILON);
        assert!(c.g < f32::EPSILON);
        assert!(c.b < f32::EPSILON);
        assert!((c.a - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_color_lerp() {
        let a = Color::RED;
        let b = Color::BLUE;
        let mid = Color::lerp(&a, &b, 0.5);
        assert!((mid.r - 0.5).abs() < f32::EPSILON);
        assert!(mid.g < f32::EPSILON);
        assert!((mid.b - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_gradient_sample() {
        let mut grad = Gradient::linear(0.0, 0.0, 100.0, 0.0);
        grad.add_stop(0.0, Color::RED);
        grad.add_stop(1.0, Color::BLUE);
        let mid = grad.sample(0.5);
        assert!((mid.r - 0.5).abs() < f32::EPSILON);
        assert!((mid.b - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_color_to_u8_bgra() {
        let c = Color::from_u8_rgb(255, 128, 64);
        let bgra = c.to_u8_bgra();
        assert_eq!(bgra, [64, 128, 255, 255]);
    }
}
