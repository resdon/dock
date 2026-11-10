//! Surface types for rendering targets.
//!
//! A Surface represents a renderable area (window, offscreen buffer, etc.)
//! that the Renderer can draw to.

use crate::geometry::Size;
use crate::color::Color;

/// Opaque identifier for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceId(u64);

impl SurfaceId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Pixel format for a surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SurfaceFormat {
    /// 8-bit per channel: B, G, R, A (BGRA8888).
    Bgra8,
    /// 8-bit per channel: R, G, B, A (RGBA8888).
    Rgba8,
    /// 16-bit float per channel: R, G, B, A (HDR).
    Rgba16Float,
    /// 32-bit float per channel: R, G, B, A (HDR).
    Rgba32Float,
    /// High dynamic range (alias for Rgba16Float).
    Hdr,
}

impl Default for SurfaceFormat {
    fn default() -> Self {
        Self::Bgra8
    }
}

/// Descriptor for creating a surface.
///
/// A simpler surface creation type that focuses on the essential parameters:
/// size, scale factor, pixel format, and optional clear color.
///
/// A logical rendering surface is not necessarily a Wayland surface.
/// A logical surface might be an offscreen texture. A Wayland surface is
/// an OS-managed presentation object. Keeping these concepts separate ensures
/// popup rendering and video previews don't become tied to Wayland window creation.
#[derive(Debug, Clone)]
pub struct SurfaceDescriptor {
    /// Logical size of the surface in pixels.
    pub size: Size,
    /// Device pixel ratio (DPI scale factor).
    pub scale_factor: f32,
    /// Pixel format.
    pub format: SurfaceFormat,
    /// Optional clear color used before each frame render.
    pub clear_color: Option<Color>,
}

impl SurfaceDescriptor {
    /// Create a new descriptor with the given size.
    pub fn new(size: Size) -> Self {
        Self {
            size,
            scale_factor: 1.0,
            format: SurfaceFormat::default(),
            clear_color: None,
        }
    }

    /// Set the scale factor.
    pub fn with_scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Set the pixel format.
    pub fn with_format(mut self, format: SurfaceFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the clear color.
    pub fn with_clear_color(mut self, color: Color) -> Self {
        self.clear_color = Some(color);
        self
    }

    /// Convert to a SurfaceConfig for backward compatibility.
    pub fn to_config(&self) -> SurfaceConfig {
        SurfaceConfig {
            width: self.size.width as u32,
            height: self.size.height as u32,
            format: self.format,
            scale_factor: self.scale_factor,
            alpha: true,
            srgb: true,
            antialiasing_samples: 4,
        }
    }
}

impl From<Size> for SurfaceDescriptor {
    fn from(size: Size) -> Self {
        Self::new(size)
    }
}

/// Configuration for creating a surface.
#[derive(Debug, Clone)]
pub struct SurfaceConfig {
    /// Width in logical pixels.
    pub width: u32,
    /// Height in logical pixels.
    pub height: u32,
    /// Pixel format.
    pub format: SurfaceFormat,
    /// Device pixel ratio (DPI scale factor).
    pub scale_factor: f32,
    /// Whether the surface supports alpha transparency.
    pub alpha: bool,
    /// Whether to enable sRGB color space.
    pub srgb: bool,
    /// Whether to enable anti-aliasing (MSAA samples).
    pub antialiasing_samples: u32,
}

impl SurfaceConfig {
    /// Create a new surface config with default values.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: SurfaceFormat::default(),
            scale_factor: 1.0,
            alpha: true,
            srgb: true,
            antialiasing_samples: 4,
        }
    }

    /// Set the pixel format.
    pub fn with_format(mut self, format: SurfaceFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the scale factor.
    pub fn with_scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Enable/disable alpha.
    pub fn with_alpha(mut self, alpha: bool) -> Self {
        self.alpha = alpha;
        self
    }

    /// Set the MSAA samples.
    pub fn with_antialiasing_samples(mut self, samples: u32) -> Self {
        self.antialiasing_samples = samples;
        self
    }
}

impl Default for SurfaceConfig {
    fn default() -> Self {
        Self::new(800, 600)
    }
}

/// A rendering surface.
///
/// Represents a target for rendering, such as a window, offscreen buffer,
/// or texture. The surface holds the configuration and a reference to the
/// underlying GPU resource.
#[derive(Debug, Clone)]
pub struct Surface {
    /// Unique identifier for this surface.
    pub id: SurfaceId,
    /// Surface configuration.
    pub config: SurfaceConfig,
    /// Whether the surface is currently visible.
    pub visible: bool,
    /// Whether the surface needs to be redrawn.
    pub needs_redraw: bool,
    /// Physical size in pixels (logical size * scale factor).
    pub physical_size: Size,
}

impl Surface {
    /// Create a new surface with the given configuration.
    pub fn new(id: SurfaceId, config: SurfaceConfig) -> Self {
        let physical_w = (config.width as f32 * config.scale_factor).round() as u32;
        let physical_h = (config.height as f32 * config.scale_factor).round() as u32;

        Self {
            id,
            config,
            visible: true,
            needs_redraw: true,
            physical_size: Size::from_u32(physical_w, physical_h),
        }
    }

    /// Get the logical size.
    pub fn logical_size(&self) -> Size {
        Size::from_u32(self.config.width, self.config.height)
    }

    /// Get the physical size.
    pub fn physical_size(&self) -> Size {
        self.physical_size
    }

    /// Get the scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.config.scale_factor
    }

    /// Resize the surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width;
        self.config.height = height;
        let physical_w = (width as f32 * self.config.scale_factor).round() as u32;
        let physical_h = (height as f32 * self.config.scale_factor).round() as u32;
        self.physical_size = Size::from_u32(physical_w, physical_h);
        self.needs_redraw = true;
    }

    /// Set the scale factor.
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.config.scale_factor = scale;
        let physical_w = (self.config.width as f32 * scale).round() as u32;
        let physical_h = (self.config.height as f32 * scale).round() as u32;
        self.physical_size = Size::from_u32(physical_w, physical_h);
        self.needs_redraw = true;
    }

    /// Mark the surface as needing a redraw.
    pub fn mark_dirty(&mut self) {
        self.needs_redraw = true;
    }

    /// Check if the surface needs a redraw.
    pub fn is_dirty(&self) -> bool {
        self.needs_redraw
    }

    /// Show the surface.
    pub fn show(&mut self) {
        self.visible = true;
        self.needs_redraw = true;
    }

    /// Hide the surface.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if the surface is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_creation() {
        let config = SurfaceConfig::new(800, 600);
        let surface = Surface::new(SurfaceId::new(1), config);
        assert_eq!(surface.logical_size().width, 800.0);
        assert_eq!(surface.physical_size().width, 800.0);
    }

    #[test]
    fn test_surface_resize() {
        let config = SurfaceConfig::new(800, 600);
        let mut surface = Surface::new(SurfaceId::new(1), config);
        surface.resize(1024, 768);
        assert_eq!(surface.logical_size().width, 1024.0);
        assert!(surface.is_dirty());
    }

    #[test]
    fn test_surface_scale() {
        let config = SurfaceConfig::new(800, 600).with_scale_factor(2.0);
        let surface = Surface::new(SurfaceId::new(1), config);
        assert_eq!(surface.physical_size().width, 1600.0);
    }
}
