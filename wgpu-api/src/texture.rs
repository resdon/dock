//! Texture types for GPU image storage.
//!
//! Textures are GPU-resident images used for rendering. They can be created
//! from Image data or allocated as empty render targets.

use crate::geometry::Size;
use crate::image::Image;

/// Opaque identifier for a texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u64);

impl TextureId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Pixel format for GPU textures.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextureFormat {
    /// 8-bit per channel: R, G, B, A (unorm).
    Rgba8Unorm,
    /// 8-bit per channel: B, G, R, A (unorm sRGB).
    Bgra8UnormSrgb,
    /// 8-bit per channel: R, G, B, A (unorm sRGB).
    Rgba8UnormSrgb,
    /// 16-bit float per channel: R, G, B, A.
    Rgba16Float,
    /// 32-bit float per channel: R, G, B, A.
    Rgba32Float,
    /// Depth/stencil format.
    Depth32Float,
    /// Depth/stencil format with 8-bit stencil.
    Depth24PlusStencil8,
}

impl Default for TextureFormat {
    fn default() -> Self {
        Self::Bgra8UnormSrgb
    }
}

/// Filtering mode for texture sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextureFilter {
    /// Nearest-neighbor (pixelated).
    Nearest,
    /// Linear interpolation (smooth).
    Linear,
}

/// Wrapping mode for texture coordinates outside [0, 1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextureWrap {
    /// Repeat the texture.
    Repeat,
    /// Clamp to edge.
    Clamp,
    /// Mirror-repeat.
    Mirror,
}

/// Usage flags for a texture.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextureUsage {
    /// Can be used as a render target.
    pub render_target: bool,
    /// Can be sampled in shaders.
    pub sample: bool,
    /// Can be copied from CPU memory.
    pub copy_source: bool,
    /// Can be copied to from CPU memory.
    pub copy_destination: bool,
}

impl TextureUsage {
    /// Texture that can be rendered to and sampled.
    pub fn render_and_sample() -> Self {
        Self {
            render_target: true,
            sample: true,
            copy_source: false,
            copy_destination: false,
        }
    }

    /// Texture that can be uploaded from CPU and sampled.
    pub fn upload_and_sample() -> Self {
        Self {
            render_target: false,
            sample: true,
            copy_source: false,
            copy_destination: true,
        }
    }

    /// Texture with all usages enabled.
    pub fn all() -> Self {
        Self {
            render_target: true,
            sample: true,
            copy_source: true,
            copy_destination: true,
        }
    }
}

impl Default for TextureUsage {
    fn default() -> Self {
        Self::render_and_sample()
    }
}

/// Configuration for creating a texture.
#[derive(Debug, Clone)]
pub struct TextureConfig {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format.
    pub format: TextureFormat,
    /// Usage flags.
    pub usage: TextureUsage,
    /// Whether to generate mipmaps.
    pub mipmapped: bool,
    /// Number of array layers (for cubemaps or array textures).
    pub array_layers: u32,
    /// Number of mip levels (0 = auto).
    pub mip_levels: u32,
}

impl TextureConfig {
    /// Create a new texture config.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            format: TextureFormat::default(),
            usage: TextureUsage::default(),
            mipmapped: false,
            array_layers: 1,
            mip_levels: 0,
        }
    }

    /// Set the pixel format.
    pub fn with_format(mut self, format: TextureFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the usage flags.
    pub fn with_usage(mut self, usage: TextureUsage) -> Self {
        self.usage = usage;
        self
    }

    /// Enable mipmap generation.
    pub fn with_mipmaps(mut self) -> Self {
        self.mipmapped = true;
        self
    }
}

impl Default for TextureConfig {
    fn default() -> Self {
        Self::new(256, 256)
    }
}

/// A GPU texture.
///
/// Represents a texture resource on the GPU. The texture holds metadata
/// about its size, format, and usage, but the actual GPU resource is
/// managed internally by the Renderer.
#[derive(Debug, Clone)]
pub struct Texture {
    /// Unique identifier for this texture.
    pub id: TextureId,
    /// Texture configuration.
    pub config: TextureConfig,
    /// Size of the texture.
    pub size: Size,
    /// Whether the texture has been uploaded to the GPU.
    pub uploaded: bool,
    /// Sampling filter for this texture.
    pub filter: TextureFilter,
    /// Wrapping mode for this texture.
    pub wrap: TextureWrap,
}

impl Texture {
    /// Create a new texture with the given configuration.
    pub fn new(id: TextureId, config: TextureConfig) -> Self {
        Self {
            id,
            size: Size::from_u32(config.width, config.height),
            uploaded: false,
            filter: TextureFilter::Linear,
            wrap: TextureWrap::Clamp,
            config,
        }
    }

    /// Create a texture from an Image.
    pub fn from_image(id: TextureId, image: &Image) -> Self {
        let config = TextureConfig::new(image.width(), image.height())
            .with_usage(TextureUsage::upload_and_sample());
        let mut texture = Self::new(id, config);
        texture.uploaded = true;
        texture
    }

    /// Set the sampling filter.
    pub fn with_filter(mut self, filter: TextureFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Set the wrapping mode.
    pub fn with_wrap(mut self, wrap: TextureWrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Get the width.
    pub fn width(&self) -> u32 {
        self.config.width
    }

    /// Get the height.
    pub fn height(&self) -> u32 {
        self.config.height
    }

    /// Get the size.
    pub fn size(&self) -> Size {
        self.size
    }

    /// Check if the texture has been uploaded.
    pub fn is_uploaded(&self) -> bool {
        self.uploaded
    }

    /// Mark the texture as uploaded.
    pub fn mark_uploaded(&mut self) {
        self.uploaded = true;
    }
}

impl From<TextureConfig> for Texture {
    fn from(config: TextureConfig) -> Self {
        Self::new(TextureId::new(0), config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_creation() {
        let config = TextureConfig::new(512, 512);
        let texture = Texture::new(TextureId::new(1), config);
        assert_eq!(texture.width(), 512);
        assert_eq!(texture.height(), 512);
    }

    #[test]
    fn test_texture_usage() {
        let usage = TextureUsage::all();
        assert!(usage.render_target);
        assert!(usage.sample);
        assert!(usage.copy_source);
        assert!(usage.copy_destination);
    }
}
