//! Raster image types and operations.
//!
//! Images are loaded from pixel data and can be rendered with various
//! filtering and wrapping modes.

use crate::geometry::Size;
use crate::color::Color;

/// Opaque identifier for a loaded image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageId(u64);

impl ImageId {
    pub const fn new(id: u64) -> Self {
        Self(id)
    }
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Pixel format for image data.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFormat {
    /// 4 bytes per pixel: R, G, B, A.
    Rgba8,
    /// 4 bytes per pixel: B, G, R, A.
    Bgra8,
    /// 3 bytes per pixel: R, G, B.
    Rgb8,
    /// 1 byte per pixel: grayscale.
    Grayscale8,
    /// 1 byte per pixel: alpha channel only.
    Alpha8,
    /// 2 bytes per pixel: R16G16B16A16 (half-float).
    Rgba16Float,
}

impl ImageFormat {
    /// Bytes per pixel for this format.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::Rgba8 | Self::Bgra8 => 4,
            Self::Rgb8 => 3,
            Self::Grayscale8 | Self::Alpha8 => 1,
            Self::Rgba16Float => 8,
        }
    }
}

/// Filtering mode for image sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageFilter {
    /// Nearest-neighbor (pixelated).
    Nearest,
    /// Bilinear interpolation (smooth).
    Linear,
    /// Trilinear with mipmaps.
    Mipmap,
}

/// Wrapping mode for texture coordinates outside [0, 1].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImageWrap {
    /// Repeat the texture.
    Repeat,
    /// Clamp to edge.
    Clamp,
    /// Mirror-repeat.
    Mirror,
    /// Transparent outside bounds.
    Border,
}

/// Raw image pixel data.
#[derive(Debug, Clone)]
pub struct ImageData {
    /// Pixel data bytes.
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel format.
    pub format: ImageFormat,
    /// Row stride in bytes (may be > width * bpp due to padding).
    pub stride: u32,
}

impl ImageData {
    /// Create new image data from RGBA8 pixels.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            stride: width * 4,
            format: ImageFormat::Rgba8,
            width,
            height,
            pixels,
        }
    }

    /// Create new image data from BGRA8 pixels.
    pub fn from_bgra(width: u32, height: u32, pixels: Vec<u8>) -> Self {
        Self {
            stride: width * 4,
            format: ImageFormat::Bgra8,
            width,
            height,
            pixels,
        }
    }

    /// Create a solid color image.
    pub fn solid_color(width: u32, height: u32, color: Color) -> Self {
        let rgba = color.to_u8_rgba();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            pixels.extend_from_slice(&rgba);
        }
        Self::from_rgba(width, height, pixels)
    }

    /// Get the size of the image.
    pub fn size(&self) -> Size {
        Size::from_u32(self.width, self.height)
    }

    /// Get pixel at (x, y) as RGBA.
    pub fn get_pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let bpp = self.format.bytes_per_pixel();
        let offset = (y * self.stride + x * bpp as u32) as usize;
        if offset + bpp > self.pixels.len() {
            return None;
        }

        match self.format {
            ImageFormat::Rgba8 => {
                Some([self.pixels[offset], self.pixels[offset + 1], self.pixels[offset + 2], self.pixels[offset + 3]])
            }
            ImageFormat::Bgra8 => {
                Some([self.pixels[offset + 2], self.pixels[offset + 1], self.pixels[offset], self.pixels[offset + 3]])
            }
            ImageFormat::Rgb8 => {
                Some([self.pixels[offset], self.pixels[offset + 1], self.pixels[offset + 2], 255])
            }
            ImageFormat::Grayscale8 => {
                let v = self.pixels[offset];
                Some([v, v, v, 255])
            }
            ImageFormat::Alpha8 => {
                Some([255, 255, 255, self.pixels[offset]])
            }
            ImageFormat::Rgba16Float => {
                // Simplified: just return first 4 bytes as approximation
                Some([self.pixels[offset], self.pixels[offset + 1], self.pixels[offset + 2], self.pixels[offset + 3]])
            }
        }
    }

    /// Set pixel at (x, y) from RGBA values.
    pub fn set_pixel(&mut self, x: u32, y: u32, rgba: [u8; 4]) {
        if x >= self.width || y >= self.height {
            return;
        }
        let bpp = self.format.bytes_per_pixel();
        let offset = (y * self.stride + x * bpp as u32) as usize;
        if offset + bpp > self.pixels.len() {
            return;
        }

        match self.format {
            ImageFormat::Rgba8 => {
                self.pixels[offset..offset + 4].copy_from_slice(&rgba);
            }
            ImageFormat::Bgra8 => {
                self.pixels[offset] = rgba[2]; // B
                self.pixels[offset + 1] = rgba[1]; // G
                self.pixels[offset + 2] = rgba[0]; // R
                self.pixels[offset + 3] = rgba[3]; // A
            }
            ImageFormat::Rgb8 => {
                self.pixels[offset] = rgba[0];
                self.pixels[offset + 1] = rgba[1];
                self.pixels[offset + 2] = rgba[2];
            }
            ImageFormat::Grayscale8 => {
                let gray = (rgba[0] as u32 * 299 + rgba[1] as u32 * 587 + rgba[2] as u32 * 114) / 1000;
                self.pixels[offset] = gray as u8;
            }
            ImageFormat::Alpha8 => {
                self.pixels[offset] = rgba[3];
            }
            ImageFormat::Rgba16Float => {}
        }
    }

    /// Convert to RGBA8 format.
    pub fn to_rgba8(&self) -> Self {
        if self.format == ImageFormat::Rgba8 {
            return self.clone();
        }
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for y in 0..self.height {
            for x in 0..self.width {
                if let Some(rgba) = self.get_pixel(x, y) {
                    pixels.extend_from_slice(&rgba);
                }
            }
        }
        Self::from_rgba(self.width, self.height, pixels)
    }

    /// Scale the image to a new size using bilinear interpolation.
    pub fn scale(&self, new_width: u32, new_height: u32) -> Self {
        let src_w = self.width as f32;
        let src_h = self.height as f32;
        let dst_w = new_width as f32;
        let dst_h = new_height as f32;

        let mut pixels = Vec::with_capacity((new_width * new_height * 4) as usize);
        for dy in 0..new_height {
            for dx in 0..new_width {
                let sx = (dx as f32 / dst_w) * src_w;
                let sy = (dy as f32 / dst_h) * src_h;
                let x0 = sx.floor() as u32;
                let y0 = sy.floor() as u32;
                let x1 = (x0 + 1).min(self.width - 1);
                let y1 = (y0 + 1).min(self.height - 1);
                let fx = sx - x0 as f32;
                let fy = sy - y0 as f32;

                let p00 = self.get_pixel(x0, y0).unwrap_or([0, 0, 0, 255]);
                let p10 = self.get_pixel(x1, y0).unwrap_or([0, 0, 0, 255]);
                let p01 = self.get_pixel(x0, y1).unwrap_or([0, 0, 0, 255]);
                let p11 = self.get_pixel(x1, y1).unwrap_or([0, 0, 0, 255]);

                for c in 0..4 {
                    let v = p00[c] as f32 * (1.0 - fx) * (1.0 - fy)
                        + p10[c] as f32 * fx * (1.0 - fy)
                        + p01[c] as f32 * (1.0 - fx) * fy
                        + p11[c] as f32 * fx * fy;
                    pixels.push(v.round() as u8);
                }
            }
        }
        Self::from_rgba(new_width, new_height, pixels)
    }
}

/// A loaded image ready for rendering.
///
/// Wraps ImageData with rendering hints and caching.
#[derive(Debug, Clone)]
pub struct Image {
    /// Raw pixel data.
    pub data: ImageData,
    /// Filtering mode for rendering.
    pub filter: ImageFilter,
    /// Wrapping mode for texture coordinates.
    pub wrap: ImageWrap,
    /// Whether to generate mipmaps.
    pub generate_mipmaps: bool,
    /// Optional pre-multiplied alpha cache.
    pub premultiplied: bool,
}

impl Image {
    /// Create a new image from pixel data.
    pub fn new(data: ImageData) -> Self {
        Self {
            filter: ImageFilter::Linear,
            wrap: ImageWrap::Clamp,
            generate_mipmaps: false,
            data,
            premultiplied: false,
        }
    }

    /// Set the filtering mode.
    pub fn with_filter(mut self, filter: ImageFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Set the wrapping mode.
    pub fn with_wrap(mut self, wrap: ImageWrap) -> Self {
        self.wrap = wrap;
        self
    }

    /// Enable mipmap generation.
    pub fn with_mipmaps(mut self) -> Self {
        self.generate_mipmaps = true;
        self
    }

    /// Get the image size.
    pub fn size(&self) -> Size {
        self.data.size()
    }

    /// Get the width.
    pub fn width(&self) -> u32 {
        self.data.width
    }

    /// Get the height.
    pub fn height(&self) -> u32 {
        self.data.height
    }

    /// Pre-multiply alpha into RGB channels.
    pub fn premultiply_alpha(&mut self) {
        if self.premultiplied {
            return;
        }
        for y in 0..self.data.height {
            for x in 0..self.data.width {
                if let Some(mut rgba) = self.data.get_pixel(x, y) {
                    let a = rgba[3] as f32 / 255.0;
                    rgba[0] = (rgba[0] as f32 * a).round() as u8;
                    rgba[1] = (rgba[1] as f32 * a).round() as u8;
                    rgba[2] = (rgba[2] as f32 * a).round() as u8;
                    self.data.set_pixel(x, y, rgba);
                }
            }
        }
        self.premultiplied = true;
    }
}

impl From<ImageData> for Image {
    fn from(data: ImageData) -> Self {
        Self::new(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_solid_color_image() {
        let img = ImageData::solid_color(2, 2, Color::RED);
        assert_eq!(img.width, 2);
        assert_eq!(img.height, 2);
        assert_eq!(img.get_pixel(0, 0), Some([255, 0, 0, 255]));
    }

    #[test]
    fn test_image_scale() {
        let img = ImageData::solid_color(4, 4, Color::BLUE);
        let scaled = img.scale(2, 2);
        assert_eq!(scaled.width, 2);
        assert_eq!(scaled.height, 2);
    }
}
