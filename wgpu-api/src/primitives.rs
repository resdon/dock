//! Vector path and shape primitives.
//!
//! Paths are built from line segments, arcs, and bezier curves.
//! Shapes are convenience constructors for common geometric figures.

use crate::geometry::Point;
use crate::color::Color;

/// A vector path composed of drawing commands.
///
/// Paths are immutable once created and can be filled or stroked.
/// The path builder pattern allows fluent construction.
#[derive(Debug, Clone)]
pub struct Path {
    /// Sequence of path commands.
    pub commands: Vec<PathCommand>,
    /// Whether the path is closed.
    pub closed: bool,
}

impl Path {
    /// Create a new empty path.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            closed: false,
        }
    }

    /// Move the current point to (x, y) without drawing.
    pub fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::MoveTo(Point::new(x, y)));
    }

    /// Draw a line from the current point to (x, y).
    pub fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(PathCommand::LineTo(Point::new(x, y)));
    }

    /// Draw a horizontal line from the current point.
    pub fn horizontal_line_to(&mut self, x: f32) {
        let y = match self.commands.last() {
            Some(PathCommand::MoveTo(p)) => p.y,
            Some(PathCommand::LineTo(p)) => p.y,
            Some(PathCommand::CubicBezierTo { end, .. }) => end.y,
            Some(PathCommand::QuadraticBezierTo { end, .. }) => end.y,
            Some(PathCommand::Arc { end, .. }) => end.y,
            _ => return,
        };
        self.commands.push(PathCommand::LineTo(Point::new(x, y)));
    }

    /// Draw a vertical line from the current point.
    pub fn vertical_line_to(&mut self, y: f32) {
        let x = match self.commands.last() {
            Some(PathCommand::MoveTo(p)) => p.x,
            Some(PathCommand::LineTo(p)) => p.x,
            Some(PathCommand::CubicBezierTo { end, .. }) => end.x,
            Some(PathCommand::QuadraticBezierTo { end, .. }) => end.x,
            Some(PathCommand::Arc { end, .. }) => end.x,
            _ => return,
        };
        self.commands.push(PathCommand::LineTo(Point::new(x, y)));
    }

    /// Draw a rectangle.
    pub fn rect(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.move_to(x, y);
        self.line_to(x + width, y);
        self.line_to(x + width, y + height);
        self.line_to(x, y + height);
        self.close();
    }

    /// Draw a rounded rectangle.
    pub fn rounded_rect(&mut self, x: f32, y: f32, width: f32, height: f32, radius: f32) {
        let r = radius.min(width / 2.0).min(height / 2.0);
        if r <= 0.0 {
            self.rect(x, y, width, height);
            return;
        }

        self.move_to(x + r, y);
        self.line_to(x + width - r, y);
        self.arc(x + width - r, y + r, r, -std::f32::consts::PI / 2.0, 0.0, true);
        self.line_to(x + width, y + height - r);
        self.arc(x + width - r, y + height - r, r, 0.0, std::f32::consts::PI / 2.0, true);
        self.line_to(x + r, y + height);
        self.arc(x + r, y + height - r, r, std::f32::consts::PI / 2.0, std::f32::consts::PI, true);
        self.line_to(x, y + r);
        self.arc(x, y + r, r, std::f32::consts::PI, -std::f32::consts::PI / 2.0, true);
        self.close();
    }

    /// Draw an ellipse.
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, _ry: f32) {
        self.move_to(cx + rx, cy);
        self.arc(cx, cy, rx, 0.0, std::f32::consts::PI * 2.0, true);
        self.close();
    }

    /// Draw a circle.
    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32) {
        self.ellipse(cx, cy, radius, radius);
    }

    /// Draw an arc.
    pub fn arc(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        clockwise: bool,
    ) {
        self.commands.push(PathCommand::Arc {
            center: Point::new(cx, cy),
            radius,
            start_angle,
            end_angle,
            clockwise,
            end: Point::new(
                cx + radius * end_angle.cos(),
                cy + radius * end_angle.sin(),
            ),
        });
    }

    /// Draw a cubic bezier curve.
    pub fn cubic_bezier_to(
        &mut self,
        cx1: f32,
        cy1: f32,
        cx2: f32,
        cy2: f32,
        x: f32,
        y: f32,
    ) {
        self.commands.push(PathCommand::CubicBezierTo {
            cp1: Point::new(cx1, cy1),
            cp2: Point::new(cx2, cy2),
            end: Point::new(x, y),
        });
    }

    /// Draw a quadratic bezier curve.
    pub fn quadratic_bezier_to(
        &mut self,
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    ) {
        self.commands.push(PathCommand::QuadraticBezierTo {
            cp: Point::new(cx, cy),
            end: Point::new(x, y),
        });
    }

    /// Close the current subpath.
    pub fn close(&mut self) {
        self.commands.push(PathCommand::Close);
        self.closed = true;
    }

    /// Check if the path is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Append another path to this one.
    pub fn append(&mut self, other: &Self) {
        self.commands.extend(other.commands.iter().cloned());
        self.closed = self.closed || other.closed;
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

/// A single command in a Path.
#[derive(Debug, Clone)]
pub enum PathCommand {
    /// Move to a point (starts a new subpath).
    MoveTo(Point),
    /// Draw a line to a point.
    LineTo(Point),
    /// Draw an arc.
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        clockwise: bool,
        end: Point,
    },
    /// Draw a cubic bezier curve.
    CubicBezierTo {
        cp1: Point,
        cp2: Point,
        end: Point,
    },
    /// Draw a quadratic bezier curve.
    QuadraticBezierTo {
        cp: Point,
        end: Point,
    },
    /// Close the current subpath.
    Close,
}

/// Predefined shape constructors.
pub mod shape {
    use super::*;

    /// Create a rectangle path.
    pub fn rect(x: f32, y: f32, width: f32, height: f32) -> Path {
        let mut path = Path::new();
        path.rect(x, y, width, height);
        path
    }

    /// Create a rounded rectangle path.
    pub fn rounded_rect(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Path {
        let mut path = Path::new();
        path.rounded_rect(x, y, width, height, radius);
        path
    }

    /// Create a circle path.
    pub fn circle(cx: f32, cy: f32, radius: f32) -> Path {
        let mut path = Path::new();
        path.circle(cx, cy, radius);
        path
    }

    /// Create an ellipse path.
    pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Path {
        let mut path = Path::new();
        path.ellipse(cx, cy, rx, ry);
        path
    }

    /// Create a star path.
    pub fn star(cx: f32, cy: f32, points: usize, outer_radius: f32, inner_radius: f32) -> Path {
        let mut path = Path::new();
        let angle_step = std::f32::consts::PI / points as f32;
        for i in 0..points * 2 {
            let angle = i as f32 * angle_step - std::f32::consts::PI / 2.0;
            let r = if i % 2 == 0 { outer_radius } else { inner_radius };
            let x = cx + r * angle.cos();
            let y = cy + r * angle.sin();
            if i == 0 {
                path.move_to(x, y);
            } else {
                path.line_to(x, y);
            }
        }
        path.close();
        path
    }

    /// Create a triangle path.
    pub fn triangle(x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32) -> Path {
        let mut path = Path::new();
        path.move_to(x1, y1);
        path.line_to(x2, y2);
        path.line_to(x3, y3);
        path.close();
        path
    }

    /// Create a diamond path.
    pub fn diamond(cx: f32, cy: f32, width: f32, height: f32) -> Path {
        let mut path = Path::new();
        path.move_to(cx, cy - height / 2.0);
        path.line_to(cx + width / 2.0, cy);
        path.line_to(cx, cy + height / 2.0);
        path.line_to(cx - width / 2.0, cy);
        path.close();
        path
    }
}

/// Style for stroking a path.
#[derive(Debug, Clone)]
pub struct StrokeStyle {
    /// Width of the stroke in logical pixels.
    pub width: f32,
    /// Line cap style for the ends of open subpaths.
    pub line_cap: LineCap,
    /// Line join style for corners.
    pub line_join: LineJoin,
    /// Miter limit for miter joins.
    pub miter_limit: f32,
    /// Dash pattern (empty = solid line).
    pub dash_pattern: DashPattern,
}

impl StrokeStyle {
    /// Create a new stroke style with default values.
    pub fn new(width: f32) -> Self {
        Self {
            width,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            miter_limit: 10.0,
            dash_pattern: DashPattern::Solid,
        }
    }

    /// Set the line cap style.
    pub fn with_line_cap(mut self, cap: LineCap) -> Self {
        self.line_cap = cap;
        self
    }

    /// Set the line join style.
    pub fn with_line_join(mut self, join: LineJoin) -> Self {
        self.line_join = join;
        self
    }

    /// Set the miter limit.
    pub fn with_miter_limit(mut self, limit: f32) -> Self {
        self.miter_limit = limit;
        self
    }

    /// Set a dash pattern.
    pub fn with_dashes(mut self, pattern: DashPattern) -> Self {
        self.dash_pattern = pattern;
        self
    }
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self::new(1.0)
    }
}

/// Line cap style for the ends of open subpaths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineCap {
    /// Flat end (no extension).
    Butt,
    /// Rounded end (semicircle).
    Round,
    /// Square end (extends by half the stroke width).
    Square,
}

/// Line join style for corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LineJoin {
    /// Sharp corner (extends to a point).
    Miter,
    /// Rounded corner.
    Round,
    /// Beveled corner (flat).
    Bevel,
}

/// Dash pattern for stroked paths.
#[derive(Debug, Clone, PartialEq)]
pub enum DashPattern {
    /// Solid line (no dashes).
    Solid,
    /// Custom dash pattern: alternating dash and gap lengths.
    Custom(Vec<f32>),
}

/// Fill rule for determining the interior of a path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FillRule {
    /// Nonzero winding number rule.
    Nonzero,
    /// Even-odd rule.
    EvenOdd,
}

impl Default for FillRule {
    fn default() -> Self {
        Self::Nonzero
    }
}

/// Fill style for a path (solid color or gradient).
#[derive(Debug, Clone)]
pub enum FillStyle {
    /// Solid color fill.
    Solid(Color),
    /// Gradient fill.
    Gradient(crate::color::Gradient),
}

impl From<Color> for FillStyle {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<crate::color::Gradient> for FillStyle {
    fn from(gradient: crate::color::Gradient) -> Self {
        Self::Gradient(gradient)
    }
}

/// A complete shape description: path + fill + stroke.
#[derive(Debug, Clone)]
pub struct Shape {
    /// The vector path.
    pub path: Path,
    /// Fill style (None = no fill).
    pub fill: Option<FillStyle>,
    /// Stroke style (None = no stroke).
    pub stroke: Option<(StrokeStyle, Color)>,
    /// Fill rule.
    pub fill_rule: FillRule,
}

impl Shape {
    /// Create a new shape with a path.
    pub fn new(path: Path) -> Self {
        Self {
            path,
            fill: None,
            stroke: None,
            fill_rule: FillRule::default(),
        }
    }

    /// Set the fill style.
    pub fn with_fill(mut self, fill: impl Into<FillStyle>) -> Self {
        self.fill = Some(fill.into());
        self
    }

    /// Set the stroke style and color.
    pub fn with_stroke(mut self, style: StrokeStyle, color: Color) -> Self {
        self.stroke = Some((style, color));
        self
    }

    /// Set the fill rule.
    pub fn with_fill_rule(mut self, rule: FillRule) -> Self {
        self.fill_rule = rule;
        self
    }
}

impl From<Path> for Shape {
    fn from(path: Path) -> Self {
        Self::new(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_rect() {
        let mut path = Path::new();
        path.rect(10.0, 20.0, 100.0, 50.0);
        assert_eq!(path.commands.len(), 5); // move + 4 lines + close
        assert!(path.closed);
    }

    #[test]
    fn test_shape_convenience() {
        let s = Shape::from(shape::circle(50.0, 50.0, 25.0))
            .with_fill(Color::BLUE)
            .with_stroke(StrokeStyle::new(2.0), Color::BLACK);
        assert!(s.fill.is_some());
        assert!(s.stroke.is_some());
    }
}
