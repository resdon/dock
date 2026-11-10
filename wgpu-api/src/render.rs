//! Rendering pipeline — the bridge between the scene graph and the GPU backend.
//!
//! This module provides the types and methods that traverse a [`Scene`], collect
//! visible nodes, resolve transforms and clipping, and produce draw commands
//! that the GPU backend can execute.
//!
//! The public rendering API is simple:
//!
//! ```text
//! renderer.begin_frame(surface_id);
//! renderer.render_scene(&scene);
//! renderer.end_frame();
//! ```
//!
//! Or, as a single call:
//!
//! ```text
//! renderer.render(target, &scene).await?;
//! ```
//!
//! All wgpu internals (TextureView, RenderPipeline, BindGroup, CommandEncoder,
//! Buffer) are hidden inside the backend. Application code only describes *what*
//! to render.

use std::fmt;

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::surface::SurfaceId;
use crate::texture::TextureId;
use crate::transform::Transform;

// ---------------------------------------------------------------------------
// Render error
// ---------------------------------------------------------------------------

/// Error type for rendering operations.
///
/// Returned by [`Renderer::render`](crate::renderer::Renderer::render) and
/// related methods when a rendering operation fails.
#[derive(Debug, Clone)]
pub enum RenderError {
    /// The surface was not found or has been destroyed.
    SurfaceNotFound(SurfaceId),
    /// The texture was not found or has been destroyed.
    TextureNotFound(TextureId),
    /// The GPU device was lost or became unavailable.
    DeviceLost,
    /// Frame presentation failed (e.g., surface out of date).
    PresentationFailed(String),
    /// Out of memory allocating GPU resources.
    OutOfMemory,
    /// Other rendering error.
    Other(String),
}

impl fmt::Display for RenderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RenderError::SurfaceNotFound(id) => write!(f, "surface {:?} not found", id),
            RenderError::TextureNotFound(id) => write!(f, "texture {:?} not found", id),
            RenderError::DeviceLost => write!(f, "GPU device lost"),
            RenderError::PresentationFailed(msg) => write!(f, "presentation failed: {}", msg),
            RenderError::OutOfMemory => write!(f, "out of memory"),
            RenderError::Other(msg) => write!(f, "render error: {}", msg),
        }
    }
}

impl std::error::Error for RenderError {}

// ---------------------------------------------------------------------------
// Render target
// ---------------------------------------------------------------------------

/// Target for rendering output.
///
/// A render target abstracts over different output destinations:
/// a presentable surface (window), an offscreen texture, or a render-to-texture
/// pass. This keeps the rendering API independent of whether the target is a
/// Wayland surface, a popup, or an offscreen buffer.
#[derive(Debug, Clone)]
pub enum RenderTarget {
    /// Render to a surface (presentable window or offscreen surface).
    Surface(SurfaceId),
    /// Render to a specific texture (render-to-texture).
    Texture(TextureId),
    /// Render to an offscreen buffer with the given size and scale.
    Offscreen {
        /// Logical size in pixels.
        size: Size,
        /// Device pixel ratio.
        scale_factor: f32,
    },
}

impl RenderTarget {
    /// Create a surface render target.
    pub fn surface(id: SurfaceId) -> Self {
        Self::Surface(id)
    }

    /// Create a texture render target.
    pub fn texture(id: TextureId) -> Self {
        Self::Texture(id)
    }

    /// Create an offscreen render target.
    pub fn offscreen(size: Size, scale_factor: f32) -> Self {
        Self::Offscreen { size, scale_factor }
    }

    /// Check if this target is presentable (can be shown on screen).
    pub fn is_presentable(&self) -> bool {
        matches!(self, Self::Surface(_))
    }
}

impl From<SurfaceId> for RenderTarget {
    fn from(id: SurfaceId) -> Self {
        Self::Surface(id)
    }
}

// ---------------------------------------------------------------------------
// Draw command
// ---------------------------------------------------------------------------

/// A single draw command produced by the rendering pipeline.
///
/// The renderer traverses the scene graph and produces a list of draw commands
/// that the GPU backend executes. Each command represents one visual element.
#[derive(Debug, Clone)]
pub enum DrawCommand {
    /// Draw a filled rectangle.
    Rectangle {
        /// Bounding rectangle in screen space.
        rect: Rect,
        /// Fill color.
        color: Color,
        /// Corner radius (0 = sharp corners).
        radius: f32,
        /// Optional stroke color.
        stroke: Option<Color>,
        /// Stroke width.
        stroke_width: f32,
    },
    /// Draw a textured quad.
    Image {
        /// Destination rectangle in screen space.
        dest_rect: Rect,
        /// Texture to render.
        texture: TextureId,
        /// Source UV rectangle (0..1).
        source_uv: Rect,
        /// Tint color applied to the texture.
        tint: Color,
    },
    /// Draw text glyphs.
    Text {
        /// Baseline position in screen space.
        position: Point,
        /// Text content.
        content: String,
        /// Font size in logical pixels.
        font_size: f32,
        /// Text color.
        color: Color,
    },
    /// Begin a clipped region.
    PushClip {
        /// Clipping rectangle in screen space.
        clip_rect: Rect,
    },
    /// End the most recent clipped region.
    PopClip,
    /// Set the transform matrix for subsequent commands.
    SetTransform {
        /// Transform matrix.
        transform: Transform,
    },
    /// Set the blend opacity for subsequent commands.
    SetOpacity {
        /// Opacity from 0.0 to 1.0.
        opacity: f32,
    },
    /// Draw a video frame.
    Video {
        /// Destination rectangle in screen space.
        dest_rect: Rect,
        /// Video texture to render.
        texture: TextureId,
        /// Source UV rectangle (0..1).
        source_uv: Rect,
        /// Tint color applied to the video.
        tint: Color,
    },
}

// ---------------------------------------------------------------------------
// Frame state
// ---------------------------------------------------------------------------

/// Internal state maintained during a frame's rendering.
///
/// Created by `begin_frame()` and consumed by `end_frame()`.
/// Tracks the current surface, transform stack, clip stack, and
/// accumulated draw commands.
#[derive(Debug, Clone)]
pub struct FrameState {
    /// Current render target surface.
    surface_id: Option<SurfaceId>,
    /// Surface size in logical pixels.
    surface_size: Size,
    /// Device pixel ratio.
    scale_factor: f32,
    /// Accumulated draw commands for this frame.
    draw_commands: Vec<DrawCommand>,
    /// Transform stack (for nested transforms).
    transform_stack: Vec<Transform>,
    /// Clip stack (for nested clipping regions).
    clip_stack: Vec<Rect>,
    /// Current accumulated opacity.
    current_opacity: f32,
    /// Frame number.
    frame_number: u64,
}

impl FrameState {
    /// Create a new frame state for the given surface.
    pub fn new(
        surface_id: SurfaceId,
        surface_size: Size,
        scale_factor: f32,
        frame_number: u64,
    ) -> Self {
        Self {
            surface_id: Some(surface_id),
            surface_size,
            scale_factor,
            draw_commands: Vec::new(),
            transform_stack: Vec::new(),
            clip_stack: Vec::new(),
            current_opacity: 1.0,
            frame_number,
        }
    }

    /// Get the current surface ID.
    pub fn surface_id(&self) -> Option<SurfaceId> {
        self.surface_id
    }

    /// Get the surface size.
    pub fn surface_size(&self) -> Size {
        self.surface_size
    }

    /// Get the scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Get the accumulated draw commands.
    pub fn draw_commands(&self) -> &[DrawCommand] {
        &self.draw_commands
    }

    /// Get mutable access to draw commands.
    pub fn draw_commands_mut(&mut self) -> &mut Vec<DrawCommand> {
        &mut self.draw_commands
    }

    /// Get the number of draw commands.
    pub fn draw_command_count(&self) -> usize {
        self.draw_commands.len()
    }

    /// Push a transform onto the stack.
    pub fn push_transform(&mut self, transform: Transform) {
        self.transform_stack.push(transform);
    }

    /// Pop the most recent transform.
    pub fn pop_transform(&mut self) -> Option<Transform> {
        self.transform_stack.pop()
    }

    /// Get the current combined transform.
    pub fn current_transform(&self) -> Transform {
        let mut result = Transform::new();
        for t in &self.transform_stack {
            result = result.compose(t);
        }
        result
    }

    /// Push a clip rectangle.
    pub fn push_clip(&mut self, clip_rect: Rect) {
        self.clip_stack.push(clip_rect);
    }

    /// Pop the most recent clip rectangle.
    pub fn pop_clip(&mut self) -> Option<Rect> {
        self.clip_stack.pop()
    }

    /// Get the current clipping rectangle (intersection of all clips).
    pub fn current_clip(&self) -> Option<Rect> {
        if self.clip_stack.is_empty() {
            None
        } else {
            let mut clip = self.clip_stack[0];
            for c in &self.clip_stack[1..] {
                if let Some(intersection) = clip.intersection(c) {
                    clip = intersection;
                } else {
                    // No intersection — everything is clipped out
                    return Some(Rect::new(0.0, 0.0, 0.0, 0.0));
                }
            }
            Some(clip)
        }
    }

    /// Set the current opacity.
    pub fn set_opacity(&mut self, opacity: f32) {
        self.current_opacity = opacity.clamp(0.0, 1.0);
    }

    /// Get the current opacity.
    pub fn current_opacity(&self) -> f32 {
        self.current_opacity
    }

    /// Get the frame number.
    pub fn frame_number(&self) -> u64 {
        self.frame_number
    }

    /// Check if anything is visible (non-zero opacity).
    pub fn is_visible(&self) -> bool {
        self.current_opacity > 0.0
    }

    /// Get the number of clip regions on the stack.
    pub fn clip_count(&self) -> usize {
        self.clip_stack.len()
    }
}

// ---------------------------------------------------------------------------
// Render pass info
// ---------------------------------------------------------------------------

/// Information about a completed render pass.
///
/// Returned by `render_scene()` and `render()` after a frame is rendered.
#[derive(Debug, Clone)]
pub struct RenderPassInfo {
    /// Number of visible nodes processed.
    pub visible_node_count: usize,
    /// Number of draw commands produced.
    pub draw_command_count: usize,
    /// Number of unique textures used.
    pub texture_count: usize,
    /// Number of clip regions pushed.
    pub clip_count: usize,
    /// Frame number.
    pub frame_number: u64,
}

impl RenderPassInfo {
    /// Create a new render pass info.
    pub fn new(
        visible_node_count: usize,
        draw_command_count: usize,
        texture_count: usize,
        clip_count: usize,
        frame_number: u64,
    ) -> Self {
        Self {
            visible_node_count,
            draw_command_count,
            texture_count,
            clip_count,
            frame_number,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_error_display() {
        let err = RenderError::SurfaceNotFound(SurfaceId::new(1));
        let msg = format!("{}", err);
        assert!(msg.contains("not found"));

        let err = RenderError::DeviceLost;
        assert!(format!("{}", err).contains("device lost"));

        let err = RenderError::OutOfMemory;
        assert!(format!("{}", err).contains("out of memory"));
    }

    #[test]
    fn test_render_target_surface() {
        let target = RenderTarget::surface(SurfaceId::new(1));
        assert!(target.is_presentable());
    }

    #[test]
    fn test_render_target_texture() {
        let target = RenderTarget::texture(TextureId::new(1));
        assert!(!target.is_presentable());
    }

    #[test]
    fn test_render_target_offscreen() {
        let target = RenderTarget::offscreen(Size::new(800.0, 600.0), 2.0);
        assert!(!target.is_presentable());
    }

    #[test]
    fn test_render_target_from_surface_id() {
        let id = SurfaceId::new(42);
        let target = RenderTarget::from(id);
        assert!(target.is_presentable());
    }

    #[test]
    fn test_frame_state_creation() {
        let state = FrameState::new(SurfaceId::new(1), Size::new(800.0, 600.0), 1.0, 0);
        assert_eq!(state.surface_id(), Some(SurfaceId::new(1)));
        assert_eq!(state.surface_size(), Size::new(800.0, 600.0));
        assert!((state.scale_factor() - 1.0).abs() < f32::EPSILON);
        assert_eq!(state.draw_command_count(), 0);
        assert!(state.is_visible());
    }

    #[test]
    fn test_frame_state_transform_stack() {
        let mut state = FrameState::new(SurfaceId::new(1), Size::new(800.0, 600.0), 1.0, 0);

        let t1 = Transform::translate(10.0, 20.0);
        state.push_transform(t1);
        assert_eq!(state.transform_stack.len(), 1);

        let t2 = Transform::translate(5.0, 0.0);
        state.push_transform(t2);
        assert_eq!(state.transform_stack.len(), 2);

        let popped = state.pop_transform().unwrap();
        assert_eq!(popped, t2);
        assert_eq!(state.transform_stack.len(), 1);
    }

    #[test]
    fn test_frame_state_clip_stack() {
        let mut state = FrameState::new(SurfaceId::new(1), Size::new(800.0, 600.0), 1.0, 0);
        assert_eq!(state.current_clip(), None);

        state.push_clip(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert!(state.current_clip().is_some());

        state.push_clip(Rect::new(50.0, 50.0, 100.0, 100.0));
        let clip = state.current_clip().unwrap();
        assert!((clip.x - 50.0).abs() < f32::EPSILON);
        assert!((clip.y - 50.0).abs() < f32::EPSILON);
        assert!((clip.width - 50.0).abs() < f32::EPSILON);
        assert!((clip.height - 50.0).abs() < f32::EPSILON);

        state.pop_clip();
        let clip = state.current_clip().unwrap();
        assert!((clip.x - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_frame_state_opacity() {
        let mut state = FrameState::new(SurfaceId::new(1), Size::new(800.0, 600.0), 1.0, 0);
        assert!((state.current_opacity() - 1.0).abs() < f32::EPSILON);

        state.set_opacity(0.5);
        assert!((state.current_opacity() - 0.5).abs() < f32::EPSILON);

        state.set_opacity(0.0);
        assert!(!state.is_visible());
    }

    #[test]
    fn test_render_pass_info() {
        let info = RenderPassInfo::new(10, 8, 3, 2, 42);
        assert_eq!(info.visible_node_count, 10);
        assert_eq!(info.draw_command_count, 8);
        assert_eq!(info.texture_count, 3);
        assert_eq!(info.clip_count, 2);
        assert_eq!(info.frame_number, 42);
    }

    #[test]
    fn test_draw_command_rectangle() {
        let cmd = DrawCommand::Rectangle {
            rect: Rect::new(10.0, 20.0, 100.0, 50.0),
            color: Color::RED,
            radius: 4.0,
            stroke: Some(Color::BLACK),
            stroke_width: 2.0,
        };
        assert!(matches!(cmd, DrawCommand::Rectangle { .. }));
    }

    #[test]
    fn test_draw_command_image() {
        let cmd = DrawCommand::Image {
            dest_rect: Rect::new(0.0, 0.0, 200.0, 200.0),
            texture: TextureId::new(1),
            source_uv: Rect::new(0.0, 0.0, 1.0, 1.0),
            tint: Color::WHITE,
        };
        assert!(matches!(cmd, DrawCommand::Image { .. }));
    }

    #[test]
    fn test_frame_state_clip_empty_intersection() {
        let mut state = FrameState::new(SurfaceId::new(1), Size::new(800.0, 600.0), 1.0, 0);
        // Two non-overlapping clips
        state.push_clip(Rect::new(0.0, 0.0, 50.0, 50.0));
        state.push_clip(Rect::new(100.0, 100.0, 50.0, 50.0));
        let clip = state.current_clip().unwrap();
        // Should be zero-sized (everything clipped out)
        assert_eq!(clip.width, 0.0);
        assert_eq!(clip.height, 0.0);
    }
}
