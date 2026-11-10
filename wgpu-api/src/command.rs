//! Command buffer and drawing API.
//!
//! Application code uses CommandBuffer to describe what to render each frame.
//! The Renderer compiles these commands into GPU command buffers and submits
//! them to the GPU queue.

use crate::color::{BlendMode, Color, Gradient};
use crate::geometry::{Point, Rect, Size};
use crate::image::Image;
use crate::primitives::{FillRule, FillStyle, Path, Shape, StrokeStyle};
use crate::text::{Text, TextStyle};
use crate::texture::TextureId;
use crate::transform::Transform;

/// Trait for frame rendering callbacks.
pub trait FrameCallback: FnOnce(&mut CommandBuffer) {}

impl<F: FnOnce(&mut CommandBuffer)> FrameCallback for F {}

/// A single rendering command.
///
/// Each command describes one drawing operation. The Renderer batches
/// commands by type to minimize GPU state changes.
#[derive(Debug, Clone)]
pub enum Command {
    /// Clear the entire surface to a solid color.
    Clear(Color),
    /// Fill a path with a solid color or gradient.
    Fill {
        path: Path,
        fill: FillStyle,
        fill_rule: FillRule,
    },
    /// Stroke a path with a solid color.
    Stroke {
        path: Path,
        style: StrokeStyle,
        color: Color,
    },
    /// Draw a rectangle.
    Rect {
        rect: Rect,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a rounded rectangle.
    RoundedRect {
        rect: Rect,
        radius: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a circle.
    Circle {
        center: Point,
        radius: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw an ellipse.
    Ellipse {
        center: Point,
        radius_x: f32,
        radius_y: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a line.
    Line {
        start: Point,
        end: Point,
        style: StrokeStyle,
        color: Color,
    },
    /// Draw an image.
    Image {
        image: Image,
        dest: Rect,
        source: Option<Rect>,
        opacity: f32,
    },
    /// Draw a texture.
    Texture {
        texture_id: TextureId,
        dest: Rect,
        source: Option<Rect>,
        opacity: f32,
    },
    /// Draw text.
    Text { text: Text, position: Point },
    /// Draw a shape (path + fill + stroke).
    Shape(Shape),
    /// Draw a pixel buffer (raw BGRA8888 data).
    PixelBuffer {
        data: Vec<u8>,
        size: Size,
        dest: Rect,
    },
    /// Draw a gradient rectangle.
    GradientRect { rect: Rect, gradient: Gradient },
    /// Draw a dashed line.
    DashedLine {
        start: Point,
        end: Point,
        dash_pattern: Vec<f32>,
        width: f32,
        color: Color,
    },
    /// Draw a shadow.
    Shadow {
        path: Path,
        offset: Point,
        blur: f32,
        color: Color,
    },
    /// Draw a clip region.
    Clip { path: Path },
    /// Reset the clip region.
    ResetClip,
    /// Push a transform.
    PushTransform(Transform),
    /// Pop the last transform.
    PopTransform,
    /// Set the blend mode.
    SetBlendMode(BlendMode),
    /// Set the global opacity.
    SetOpacity(f32),
    /// Draw a rounded rectangle with a border.
    RoundedRectWithBorder {
        rect: Rect,
        radius: f32,
        border_width: f32,
        fill: Option<FillStyle>,
        border_color: Color,
    },
    /// Draw an arc.
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
        clockwise: bool,
        style: StrokeStyle,
        color: Color,
    },
    /// Draw a star.
    Star {
        center: Point,
        points: usize,
        outer_radius: f32,
        inner_radius: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a triangle.
    Triangle {
        p1: Point,
        p2: Point,
        p3: Point,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a diamond.
    Diamond {
        center: Point,
        width: f32,
        height: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a polygon.
    Polygon {
        points: Vec<Point>,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a cubic bezier curve.
    CubicBezier {
        start: Point,
        cp1: Point,
        cp2: Point,
        end: Point,
        style: StrokeStyle,
        color: Color,
    },
    /// Draw a quadratic bezier curve.
    QuadraticBezier {
        start: Point,
        cp: Point,
        end: Point,
        style: StrokeStyle,
        color: Color,
    },
    /// Draw a ring/donut.
    Ring {
        center: Point,
        inner_radius: f32,
        outer_radius: f32,
        fill: Option<FillStyle>,
        stroke: Option<(StrokeStyle, Color)>,
    },
    /// Draw a progress bar.
    ProgressBar {
        rect: Rect,
        progress: f32,
        fill_color: Color,
        background_color: Color,
        border_radius: f32,
    },
    /// Draw a separator line.
    Separator {
        start: Point,
        end: Point,
        color: Color,
        thickness: f32,
    },
    /// Draw a dot/circle marker.
    Dot {
        center: Point,
        radius: f32,
        color: Color,
    },
    /// Draw a checkmark.
    Checkmark {
        center: Point,
        size: f32,
        color: Color,
    },
    /// Draw a cross/X mark.
    Cross {
        center: Point,
        size: f32,
        color: Color,
    },
    /// Draw a gradient circle.
    GradientCircle {
        center: Point,
        radius: f32,
        gradient: Gradient,
    },
    /// Draw a pattern fill.
    PatternFill {
        path: Path,
        pattern: Image,
        tile_size: Size,
    },
    /// Draw a glow effect.
    Glow {
        path: Path,
        color: Color,
        spread: f32,
    },
    /// Draw a blur effect.
    Blur {
        source: TextureId,
        dest: Rect,
        radius: f32,
    },
    /// Draw a vignette effect.
    Vignette {
        rect: Rect,
        color: Color,
        intensity: f32,
    },
    /// Draw a noise overlay.
    Noise { rect: Rect, opacity: f32, seed: u64 },
    /// Draw a scanline effect.
    Scanlines {
        rect: Rect,
        spacing: f32,
        opacity: f32,
    },
    /// Draw a pixelate effect.
    Pixelate {
        source: TextureId,
        dest: Rect,
        pixel_size: f32,
    },
    /// Draw a color matrix effect.
    ColorMatrix {
        source: TextureId,
        dest: Rect,
        matrix: [f32; 20],
    },
    /// Draw a hue rotation effect.
    HueRotate {
        source: TextureId,
        dest: Rect,
        angle: f32,
    },
    /// Draw a brightness/contrast effect.
    BrightnessContrast {
        source: TextureId,
        dest: Rect,
        brightness: f32,
        contrast: f32,
    },
    /// Draw a saturate effect.
    Saturate {
        source: TextureId,
        dest: Rect,
        amount: f32,
    },
    /// Draw an invert effect.
    Invert { source: TextureId, dest: Rect },
    /// Draw a grayscale effect.
    Grayscale { source: TextureId, dest: Rect },
    /// Draw a sepia effect.
    Sepia { source: TextureId, dest: Rect },
    /// Draw an opacity mask.
    OpacityMask {
        source: TextureId,
        mask: TextureId,
        dest: Rect,
    },
    /// Draw a composite operation.
    Composite {
        source: TextureId,
        dest: Rect,
        operator: BlendMode,
    },
    /// Draw a drop shadow on a rectangle.
    DropShadow {
        rect: Rect,
        offset: Point,
        blur: f32,
        color: Color,
    },
    /// Draw an inner shadow on a rectangle.
    InnerShadow { rect: Rect, blur: f32, color: Color },
    /// Draw an outline/stroke around a shape.
    Outline {
        path: Path,
        width: f32,
        color: Color,
        outside: bool,
    },
    /// Draw a multi-stroke (multiple outlines).
    MultiStroke {
        path: Path,
        widths: Vec<f32>,
        colors: Vec<Color>,
    },
    /// Draw a gradient stroke.
    GradientStroke {
        path: Path,
        gradient: Gradient,
        width: f32,
    },
    /// Draw a text outline.
    TextOutline {
        text: Text,
        position: Point,
        width: f32,
        color: Color,
    },
    /// Draw a text shadow.
    TextShadow {
        text: Text,
        position: Point,
        offset: Point,
        blur: f32,
        color: Color,
    },
    /// Draw a text gradient.
    TextGradient {
        text: Text,
        position: Point,
        gradient: Gradient,
    },
    /// Draw a text with a background.
    TextWithBackground {
        text: Text,
        position: Point,
        background_color: Color,
        padding: Size,
        border_radius: f32,
    },
    /// Draw a text with a stroke.
    TextWithStroke {
        text: Text,
        position: Point,
        stroke_width: f32,
        stroke_color: Color,
    },
    /// Draw a text with a glow.
    TextGlow {
        text: Text,
        position: Point,
        color: Color,
        spread: f32,
    },
    /// Draw a text with a blur.
    TextBlur {
        text: Text,
        position: Point,
        radius: f32,
    },
    /// Draw a text with a mask.
    TextMask {
        text: Text,
        position: Point,
        mask: TextureId,
    },
    /// Draw a text with a gradient mask.
    TextGradientMask {
        text: Text,
        position: Point,
        gradient: Gradient,
    },
    /// Draw a text with a pattern fill.
    TextPatternFill {
        text: Text,
        position: Point,
        pattern: Image,
        tile_size: Size,
    },
    /// Draw a text with a noise overlay.
    TextNoise {
        text: Text,
        position: Point,
        opacity: f32,
        seed: u64,
    },
    /// Draw a text with a scanline effect.
    TextScanlines {
        text: Text,
        position: Point,
        spacing: f32,
        opacity: f32,
    },
    /// Draw a text with a pixelate effect.
    TextPixelate {
        text: Text,
        position: Point,
        pixel_size: f32,
    },
    /// Draw a text with a color matrix effect.
    TextColorMatrix {
        text: Text,
        position: Point,
        matrix: [f32; 20],
    },
    /// Draw a text with a hue rotation effect.
    TextHueRotate {
        text: Text,
        position: Point,
        angle: f32,
    },
    /// Draw a text with a brightness/contrast effect.
    TextBrightnessContrast {
        text: Text,
        position: Point,
        brightness: f32,
        contrast: f32,
    },
    /// Draw a text with a saturate effect.
    TextSaturate {
        text: Text,
        position: Point,
        amount: f32,
    },
    /// Draw a text with an invert effect.
    TextInvert { text: Text, position: Point },
    /// Draw a text with a grayscale effect.
    TextGrayscale { text: Text, position: Point },
    /// Draw a text with a sepia effect.
    TextSepia { text: Text, position: Point },
    /// Draw a text with an opacity mask.
    TextOpacityMask {
        text: Text,
        position: Point,
        mask: TextureId,
    },
    /// Draw a text with a composite operation.
    TextComposite {
        text: Text,
        position: Point,
        operator: BlendMode,
    },
    /// Draw a text with a drop shadow.
    TextDropShadow {
        text: Text,
        position: Point,
        offset: Point,
        blur: f32,
        color: Color,
    },
    /// Draw a text with an inner shadow.
    TextInnerShadow {
        text: Text,
        position: Point,
        blur: f32,
        color: Color,
    },
    /// Draw a text with an outline.
    TextOutlineStroke {
        text: Text,
        position: Point,
        width: f32,
        color: Color,
        outside: bool,
    },
    /// Draw a text with a multi-stroke.
    TextMultiStroke {
        text: Text,
        position: Point,
        widths: Vec<f32>,
        colors: Vec<Color>,
    },
    /// Draw a text with a gradient stroke.
    TextGradientStroke {
        text: Text,
        position: Point,
        gradient: Gradient,
        width: f32,
    },
}

/// A command buffer for recording rendering commands.
///
/// Application code uses CommandBuffer to describe what to render each frame.
/// The Renderer compiles these commands into GPU command buffers.
#[allow(dead_code)]
pub struct CommandBuffer {
    /// List of commands.
    commands: Vec<Command>,
    /// Current transform stack.
    transform_stack: Vec<Transform>,
    /// Current blend mode.
    blend_mode: BlendMode,
    /// Current global opacity.
    opacity: f32,
    /// Current clip path.
    clip_path: Option<Path>,
}

impl CommandBuffer {
    /// Create a new empty command buffer.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
            transform_stack: Vec::new(),
            blend_mode: BlendMode::default(),
            opacity: 1.0,
            clip_path: None,
        }
    }

    /// Clear the entire surface to a solid color.
    pub fn clear(&mut self, color: Color) {
        self.commands.push(Command::Clear(color));
    }

    /// Fill a path with a solid color.
    pub fn fill(&mut self, path: &Path, color: &Color) {
        self.commands.push(Command::Fill {
            path: path.clone(),
            fill: FillStyle::Solid(*color),
            fill_rule: FillRule::Nonzero,
        });
    }

    /// Fill a path with a gradient.
    pub fn fill_gradient(&mut self, path: &Path, gradient: &Gradient) {
        self.commands.push(Command::Fill {
            path: path.clone(),
            fill: FillStyle::Gradient(gradient.clone()),
            fill_rule: FillRule::Nonzero,
        });
    }

    /// Stroke a path.
    pub fn stroke(&mut self, path: &Path, style: &StrokeStyle, color: &Color) {
        self.commands.push(Command::Stroke {
            path: path.clone(),
            style: style.clone(),
            color: *color,
        });
    }

    /// Draw a rectangle.
    pub fn rect(&mut self, rect: &Rect, color: &Color) {
        self.commands.push(Command::Rect {
            rect: *rect,
            fill: Some(FillStyle::Solid(*color)),
            stroke: None,
        });
    }

    /// Draw a rounded rectangle.
    pub fn rounded_rect(&mut self, rect: &Rect, radius: f32, color: &Color) {
        self.commands.push(Command::RoundedRect {
            rect: *rect,
            radius,
            fill: Some(FillStyle::Solid(*color)),
            stroke: None,
        });
    }

    /// Draw a circle.
    pub fn circle(&mut self, center: Point, radius: f32, color: &Color) {
        self.commands.push(Command::Circle {
            center,
            radius,
            fill: Some(FillStyle::Solid(*color)),
            stroke: None,
        });
    }

    /// Draw a line.
    pub fn line(&mut self, start: Point, end: Point, width: f32, color: &Color) {
        self.commands.push(Command::Line {
            start,
            end,
            style: StrokeStyle::new(width),
            color: *color,
        });
    }

    /// Draw an image.
    pub fn image(&mut self, image: &Image, dest: &Rect) {
        self.commands.push(Command::Image {
            image: image.clone(),
            dest: *dest,
            source: None,
            opacity: 1.0,
        });
    }

    /// Draw text.
    pub fn text(&mut self, content: &str, size: f32, color: &Color) {
        let text =
            Text::new(content).with_style(TextStyle::new().with_size(size).with_color(*color));
        self.commands.push(Command::Text {
            text,
            position: Point::ORIGIN,
        });
    }

    /// Draw text at a position.
    pub fn text_at(&mut self, content: &str, position: Point, size: f32, color: &Color) {
        let text =
            Text::new(content).with_style(TextStyle::new().with_size(size).with_color(*color));
        self.commands.push(Command::Text { text, position });
    }

    /// Draw a shape.
    pub fn shape(&mut self, shape: Shape) {
        self.commands.push(Command::Shape(shape));
    }

    /// Draw a pixel buffer.
    pub fn pixel_buffer(&mut self, data: Vec<u8>, size: Size, dest: &Rect) {
        self.commands.push(Command::PixelBuffer {
            data,
            size,
            dest: *dest,
        });
    }

    /// Draw a gradient rectangle.
    pub fn gradient_rect(&mut self, rect: &Rect, gradient: &Gradient) {
        self.commands.push(Command::GradientRect {
            rect: *rect,
            gradient: gradient.clone(),
        });
    }

    /// Draw a dashed line.
    pub fn dashed_line(
        &mut self,
        start: Point,
        end: Point,
        dash_pattern: Vec<f32>,
        width: f32,
        color: &Color,
    ) {
        self.commands.push(Command::DashedLine {
            start,
            end,
            dash_pattern,
            width,
            color: *color,
        });
    }

    /// Push a transform.
    pub fn transform(&mut self, transform: &Transform) {
        self.commands.push(Command::PushTransform(*transform));
    }

    /// Pop the last transform.
    pub fn pop_transform(&mut self) {
        self.commands.push(Command::PopTransform);
    }

    /// Set the blend mode.
    pub fn set_blend_mode(&mut self, blend_mode: BlendMode) {
        self.commands.push(Command::SetBlendMode(blend_mode));
    }

    /// Set the global opacity.
    pub fn set_opacity(&mut self, opacity: f32) {
        self.commands.push(Command::SetOpacity(opacity));
    }

    /// Draw a shadow.
    pub fn shadow(&mut self, path: &Path, offset: Point, blur: f32, color: &Color) {
        self.commands.push(Command::Shadow {
            path: path.clone(),
            offset,
            blur,
            color: *color,
        });
    }

    /// Set a clip region.
    pub fn clip(&mut self, path: &Path) {
        self.commands.push(Command::Clip { path: path.clone() });
    }

    /// Reset the clip region.
    pub fn reset_clip(&mut self) {
        self.commands.push(Command::ResetClip);
    }

    /// Get the number of commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the command buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Clear all commands.
    pub fn clear_commands(&mut self) {
        self.commands.clear();
    }

    /// Get the commands.
    pub fn commands(&self) -> &[Command] {
        &self.commands
    }

    /// Take ownership of the commands.
    pub fn into_commands(self) -> Vec<Command> {
        self.commands
    }
}

impl Default for CommandBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_buffer() {
        let mut cmd = CommandBuffer::new();
        cmd.clear(Color::BLACK);
        cmd.rect(&Rect::new(0.0, 0.0, 100.0, 100.0), &Color::RED);
        assert_eq!(cmd.len(), 2);
        assert!(!cmd.is_empty());
    }

    #[test]
    fn test_text_command() {
        let mut cmd = CommandBuffer::new();
        cmd.text("Hello", 16.0, &Color::WHITE);
        assert_eq!(cmd.len(), 1);
    }

    #[test]
    fn test_transform_commands() {
        let mut cmd = CommandBuffer::new();
        cmd.transform(&Transform::translate(10.0, 20.0));
        cmd.pop_transform();
        assert_eq!(cmd.len(), 2);
    }
}
