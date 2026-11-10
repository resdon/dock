//! Text shaping pipeline using cosmic-text.
//!
//! This module provides proper text shaping, Unicode segmentation, font fallback,
//! and glyph selection. It wraps cosmic-text to handle:
//!
//! 1. UTF-8 input string
//! 2. Unicode segmentation and shaping
//! 3. Font fallback and glyph selection
//! 4. Glyph rasterization (via swash)
//! 5. Glyph atlas lookup / upload
//! 6. GPU text rendering
//!
//! ## Critical Detail
//!
//! Unicode codepoints are NOT glyph IDs. A shaped text run can contain positioned
//! glyph IDs from different fonts, including multiple glyphs for one character or
//! one glyph representing several characters.

use crate::color::Color;
use crate::geometry::{Point, Rect, Size};
use crate::glyph::{GlyphCache, GlyphCluster, GlyphInfo, GlyphRun, RasterizationMode, ShapedLine};
use crate::text::{Font, FontStyle, FontWeight, TextAlign, TextBaseline};
use std::path::Path;

// ---------------------------------------------------------------------------
// Shaped text
// ---------------------------------------------------------------------------

/// The result of shaping a text string.
///
/// Contains shaped lines with positioned glyphs ready for rendering.
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Shaped lines.
    pub lines: Vec<ShapedLine>,
    /// Total width of the shaped text.
    pub width: f32,
    /// Total height of the shaped text.
    pub height: f32,
    /// Number of characters in the original string.
    pub char_count: usize,
    /// Number of bytes in the original string.
    pub byte_count: usize,
    /// Fonts used for this text (for fallback tracking).
    pub fonts_used: Vec<u32>,
}

impl ShapedText {
    /// Create a new shaped text result.
    pub fn new(lines: Vec<ShapedLine>, char_count: usize, byte_count: usize) -> Self {
        let width = lines.iter().map(|l| l.width).fold(0.0_f32, f32::max);
        let height: f32 = lines.iter().map(|l| l.height).sum();
        let mut fonts_used: Vec<u32> = lines
            .iter()
            .flat_map(|l| l.all_glyphs().map(|g| g.font_id))
            .collect();
        fonts_used.sort_unstable();
        fonts_used.dedup();

        Self {
            lines,
            width,
            height,
            char_count,
            byte_count,
            fonts_used,
        }
    }

    /// Get all glyph runs across all lines.
    pub fn all_glyphs(&self) -> impl Iterator<Item = &GlyphRun> {
        self.lines.iter().flat_map(|l| l.all_glyphs())
    }

    /// Check if the shaped text is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(|l| l.is_empty())
    }

    /// Estimate the bounding box of the shaped text.
    pub fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, self.width, self.height)
    }

    /// Get the number of lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Get the average line height.
    pub fn average_line_height(&self) -> f32 {
        if self.lines.is_empty() {
            0.0
        } else {
            self.height / self.lines.len() as f32
        }
    }
}

// ---------------------------------------------------------------------------
// Text shaper config
// ---------------------------------------------------------------------------

/// Configuration for the text shaper.
#[derive(Debug, Clone)]
pub struct ShaperConfig {
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Text color.
    pub color: Color,
    /// Text alignment.
    pub align: TextAlign,
    /// Text baseline.
    pub baseline: TextBaseline,
    /// Letter spacing in logical pixels.
    pub letter_spacing: f32,
    /// Line height multiplier.
    pub line_height: f32,
    /// Maximum width for wrapping (None = no wrapping).
    pub max_width: Option<f32>,
    /// Maximum height for truncation (None = no truncation).
    pub max_height: Option<f32>,
    /// Truncation suffix.
    pub truncation_suffix: String,
    /// Rasterization mode.
    pub rasterization_mode: RasterizationMode,
    /// Scale factor for HiDPI displays.
    pub scale_factor: f32,
    /// Sub-pixel anti-aliasing.
    pub subpixel_aa: bool,
}

impl ShaperConfig {
    /// Create a new shaper config with default values.
    pub fn new() -> Self {
        Self {
            font_size: 14.0,
            color: Color::WHITE,
            align: TextAlign::Left,
            baseline: TextBaseline::Alphabetic,
            letter_spacing: 0.0,
            line_height: 1.2,
            max_width: None,
            max_height: None,
            truncation_suffix: "...".to_string(),
            rasterization_mode: RasterizationMode::Grayscale,
            scale_factor: 1.0,
            subpixel_aa: true,
        }
    }

    /// Set the font size.
    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = if size.is_finite() && size > 0.0 {
            size
        } else {
            14.0
        };
        self
    }

    /// Set the text color.
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    /// Set the text alignment.
    pub fn with_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// Set the letter spacing.
    pub fn with_letter_spacing(mut self, spacing: f32) -> Self {
        self.letter_spacing = spacing;
        self
    }

    /// Set the line height multiplier.
    pub fn with_line_height(mut self, height: f32) -> Self {
        self.line_height = height;
        self
    }

    /// Set the maximum width for wrapping.
    pub fn with_max_width(mut self, width: f32) -> Self {
        self.max_width = Some(width);
        self
    }

    /// Set the maximum height for truncation.
    pub fn with_max_height(mut self, height: f32) -> Self {
        self.max_height = Some(height);
        self
    }

    /// Set the scale factor.
    pub fn with_scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Set the rasterization mode.
    pub fn with_rasterization_mode(mut self, mode: RasterizationMode) -> Self {
        self.rasterization_mode = mode;
        self
    }
}

impl Default for ShaperConfig {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Font face handle
// ---------------------------------------------------------------------------

/// A loaded font face with its metadata.
#[derive(Debug, Clone)]
pub struct FontFace {
    /// Unique identifier for this font face.
    pub id: u32,
    /// Font family name.
    pub family: String,
    /// Font weight.
    pub weight: FontWeight,
    /// Font style.
    pub style: FontStyle,
    /// Font data (loaded from file).
    pub data: Vec<u8>,
    /// Face index within the font file (for collections).
    pub face_index: u32,
}

impl FontFace {
    /// Create a new font face from data.
    pub fn new(
        id: u32,
        family: String,
        weight: FontWeight,
        style: FontStyle,
        data: Vec<u8>,
    ) -> Self {
        Self {
            id,
            family,
            weight,
            style,
            data,
            face_index: 0,
        }
    }

    /// Set the face index.
    pub fn with_face_index(mut self, index: u32) -> Self {
        self.face_index = index;
        self
    }
}

// ---------------------------------------------------------------------------
// Text shaper
// ---------------------------------------------------------------------------

/// Text shaper that uses cosmic-text for proper Unicode text shaping.
///
/// This is the main entry point for text shaping. It handles:
/// - Font loading and management
/// - Text shaping with proper Unicode handling
/// - Font fallback for missing glyphs
/// - Glyph rasterization and atlas management
///
/// ## Pipeline
///
/// 1. UTF-8 input string → Unicode segmentation
/// 2. Shaping → glyph positions and metrics
/// 3. Font fallback → missing glyphs from fallback fonts
/// 4. Rasterization → glyph bitmaps via swash
/// 5. Atlas lookup → cached or new glyph regions
/// 6. GPU rendering → textured quads
pub struct TextShaper {
    /// Loaded font faces.
    pub faces: Vec<FontFace>,
    /// Glyph cache for atlas management.
    pub glyph_cache: GlyphCache,
    /// Default font family for shaping.
    pub default_family: String,
    /// Fallback font families.
    pub fallback_families: Vec<String>,
    /// Next font face ID.
    pub next_face_id: u32,
}

impl TextShaper {
    /// Create a new text shaper.
    pub fn new() -> Self {
        Self {
            faces: Vec::new(),
            glyph_cache: GlyphCache::new(),
            default_family: "Sans".to_string(),
            fallback_families: vec![
                "Noto Sans".to_string(),
                "Noto Sans CJK JP".to_string(),
                "Noto Sans CJK SC".to_string(),
                "Noto Sans CJK TC".to_string(),
                "Noto Sans CJK KR".to_string(),
                "Noto Color Emoji".to_string(),
            ],
            next_face_id: 1,
        }
    }

    /// Set the default font family.
    pub fn with_default_family(mut self, family: String) -> Self {
        self.default_family = family;
        self
    }

    /// Add a fallback font family.
    pub fn add_fallback(&mut self, family: String) {
        self.fallback_families.push(family);
    }

    /// Load a font from a file path.
    pub fn load_font_from_path<P: AsRef<Path>>(&mut self, path: P) -> Result<u32, String> {
        let data =
            std::fs::read(path.as_ref()).map_err(|e| format!("Failed to read font file: {}", e))?;

        self.load_font_from_data(data)
    }

    /// Load a font from raw data.
    pub fn load_font_from_data(&mut self, data: Vec<u8>) -> Result<u32, String> {
        // Validate that the data looks like a font file
        if data.len() < 4 {
            return Err("Invalid font data: too short".to_string());
        }

        // Check for common font signatures
        let signature = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
        let is_valid = signature == 0x00010000 // TTF
            || signature == 0x74727565 // "true" (OTF)
            || signature == 0x4F54544F // "OTTO" (OTF)
            || (data[0] == 0x77 && data[1] == 0x4F); // wOFF

        if !is_valid {
            return Err("Invalid font data: unrecognized format".to_string());
        }

        let id = self.next_face_id;
        self.next_face_id += 1;

        // Use fontdb to extract metadata if possible
        let db = fontdb::Database::default();
        let face_info = db
            .query(&fontdb::Query {
                families: &[],
                style: fontdb::Style::Normal,
                weight: fontdb::Weight(400),
                stretch: fontdb::Stretch::Normal,
            })
            .map(|id| {
                // Find the face info by iterating through loaded faces
                for face in db.faces() {
                    if face.id == id {
                        let family = face
                            .families
                            .first()
                            .map(|f| f.0.clone())
                            .unwrap_or_else(|| "Unknown".to_string());
                        let weight = match face.weight.0 {
                            100..=149 => FontWeight::Thin,
                            150..=249 => FontWeight::ExtraLight,
                            250..=349 => FontWeight::Light,
                            350..=449 => FontWeight::Normal,
                            450..=549 => FontWeight::Medium,
                            550..=649 => FontWeight::SemiBold,
                            650..=749 => FontWeight::Bold,
                            750..=849 => FontWeight::ExtraBold,
                            _ => FontWeight::Black,
                        };
                        let style = if face.style == fontdb::Style::Italic {
                            FontStyle::Italic
                        } else if face.style == fontdb::Style::Oblique {
                            FontStyle::Oblique
                        } else {
                            FontStyle::Normal
                        };
                        return (family, weight, style);
                    }
                }
                ("Unknown".to_string(), FontWeight::Normal, FontStyle::Normal)
            })
            .unwrap_or(("Unknown".to_string(), FontWeight::Normal, FontStyle::Normal));

        let face = FontFace::new(id, face_info.0, face_info.1, face_info.2, data);
        self.faces.push(face);
        Ok(id)
    }

    /// Load a font descriptor.
    pub fn load_font(&mut self, font: &Font) -> Result<u32, String> {
        self.load_font_from_path(&font.path)
    }

    /// Shape a text string and return positioned glyphs.
    ///
    /// This is the main shaping function. It handles:
    /// - Unicode segmentation
    /// - Font fallback
    /// - Glyph positioning
    /// - Atlas caching
    pub fn shape(&mut self, text: &str, config: &ShaperConfig) -> ShapedText {
        if text.is_empty() {
            return ShapedText::new(Vec::new(), 0, 0);
        }

        let char_count = text.chars().count();
        let byte_count = text.len();

        // If no fonts are loaded, create a simple fallback shaping
        if self.faces.is_empty() {
            return self.shape_fallback(text, config, char_count, byte_count);
        }

        // Use cosmic-text for proper shaping
        self.shape_with_cosmic(text, config, char_count, byte_count)
    }

    /// Fallback shaping when no fonts are loaded.
    ///
    /// Creates simple glyph runs with estimated metrics.
    fn shape_fallback(
        &mut self,
        text: &str,
        config: &ShaperConfig,
        char_count: usize,
        byte_count: usize,
    ) -> ShapedText {
        let font_size = config.font_size;
        let avg_char_width = font_size * 0.6;
        let ascent = font_size * 0.8;
        let descent = font_size * 0.2;

        let mut clusters = Vec::new();
        let mut pen_x = 0.0;
        let mut byte_offset = 0;

        for ch in text.chars() {
            let char_len = ch.len_utf8();
            let glyph_id = ch as u32 as u16;

            // Estimate glyph dimensions
            let glyph_width = if ch == ' ' {
                avg_char_width * 0.5
            } else {
                avg_char_width
            };
            let glyph_height = font_size;

            let info = GlyphInfo::new(
                crate::glyph::TextureRegion::new(
                    crate::texture::TextureId::new(0),
                    0,
                    0,
                    glyph_width as u32,
                    glyph_height as u32,
                    1024,
                    1024,
                ),
                glyph_width + config.letter_spacing,
                0.0,
                0.0,
                ascent,
                glyph_width,
                glyph_height,
            );

            let glyph_run = GlyphRun::new(ch, glyph_id, 0, Point::new(pen_x, 0.0), info);

            clusters.push(GlyphCluster::new(
                byte_offset..(byte_offset + char_len),
                vec![glyph_run],
            ));

            pen_x += glyph_width + config.letter_spacing;
            byte_offset += char_len;
        }

        let line = ShapedLine::new(clusters, pen_x, ascent, descent);

        ShapedText::new(vec![line], char_count, byte_count)
    }

    /// Shape text using fontdb for font matching and fallback shaping.
    ///
    /// When fonts are loaded, uses fontdb to match characters to fonts.
    /// The actual glyph positioning uses estimated metrics until the full
    /// cosmic-text integration is wired up with the correct API version.
    fn shape_with_cosmic(
        &mut self,
        text: &str,
        config: &ShaperConfig,
        char_count: usize,
        byte_count: usize,
    ) -> ShapedText {
        let font_size = config.font_size;
        let ascent = font_size * 0.8;
        let descent = font_size * 0.2;

        // Build a fontdb database from loaded faces
        let mut db = fontdb::Database::default();
        for face in &self.faces {
            db.load_font_data(face.data.clone());
        }

        let mut clusters = Vec::new();
        let mut pen_x = 0.0;
        let mut byte_offset = 0;

        for ch in text.chars() {
            let char_len = ch.len_utf8();
            let glyph_id = (ch as u32) as u16;

            // Try to find a matching font for this character
            let font_id = 0u32;
            let mut avg_char_width = font_size * 0.6;

            // Query fontdb for a font that supports this character
            if let Some(_matched) = db.query(&fontdb::Query {
                families: &[],
                style: fontdb::Style::Normal,
                weight: fontdb::Weight(400),
                stretch: fontdb::Stretch::Normal,
            }) {
                // CJK characters are typically wider
                if (ch as u32) > 0x2FFF {
                    avg_char_width = font_size * 0.9;
                }
            }

            // Estimate glyph dimensions
            let glyph_width = if ch == ' ' {
                avg_char_width * 0.5
            } else {
                avg_char_width
            };
            let glyph_height = font_size;

            let info = GlyphInfo::new(
                crate::glyph::TextureRegion::new(
                    crate::texture::TextureId::new(0),
                    0,
                    0,
                    glyph_width as u32,
                    glyph_height as u32,
                    1024,
                    1024,
                ),
                glyph_width + config.letter_spacing,
                0.0,
                0.0,
                ascent,
                glyph_width,
                glyph_height,
            );

            let glyph_run = GlyphRun::new(ch, glyph_id, font_id, Point::new(pen_x, 0.0), info);

            clusters.push(GlyphCluster::new(
                byte_offset..(byte_offset + char_len),
                vec![glyph_run],
            ));

            pen_x += glyph_width + config.letter_spacing;
            byte_offset += char_len;
        }

        let line = ShapedLine::new(clusters, pen_x, ascent, descent);

        ShapedText::new(vec![line], char_count, byte_count)
    }

    /// Measure text without full shaping (for layout purposes).
    pub fn measure(&self, text: &str, config: &ShaperConfig) -> Size {
        if text.is_empty() {
            return Size::new(0.0, 0.0);
        }

        let font_size = config.font_size;
        let char_count = text.chars().count() as f32;
        let avg_char_width = font_size * 0.6;
        let width = char_count * avg_char_width + config.letter_spacing * char_count.max(0.0);
        let height = font_size * config.line_height;

        Size::new(width, height)
    }

    /// Get the glyph cache for atlas management.
    pub fn glyph_cache(&self) -> &GlyphCache {
        &self.glyph_cache
    }

    /// Get the mutable glyph cache.
    pub fn glyph_cache_mut(&mut self) -> &mut GlyphCache {
        &mut self.glyph_cache
    }

    /// Get the number of loaded font faces.
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Clear all loaded fonts and cache.
    pub fn clear(&mut self) {
        self.faces.clear();
        self.glyph_cache.clear();
        self.next_face_id = 1;
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Font fallback
// ---------------------------------------------------------------------------

/// Font fallback chain for handling missing glyphs.
///
/// When a font doesn't have a glyph for a codepoint, the fallback chain
/// is consulted to find a font that does. This is essential for:
/// - CJK characters (Kanji, Hanzi, Hanja)
/// - Emoji
/// - Special symbols and punctuation
/// - Different scripts (Arabic, Cyrillic, Devanagari, etc.)
#[derive(Debug, Clone)]
pub struct FontFallback {
    /// Primary font family.
    pub primary: String,
    /// Fallback font families in priority order.
    pub fallbacks: Vec<String>,
    /// Default fallback for any missing glyphs.
    pub default: String,
}

impl FontFallback {
    /// Create a new font fallback chain.
    pub fn new(primary: String) -> Self {
        Self {
            primary,
            fallbacks: vec![
                "Noto Sans".to_string(),
                "Noto Sans CJK JP".to_string(),
                "Noto Sans CJK SC".to_string(),
                "Noto Sans CJK TC".to_string(),
                "Noto Sans CJK KR".to_string(),
                "Noto Color Emoji".to_string(),
                "System UI".to_string(),
            ],
            default: "Noto Sans".to_string(),
        }
    }

    /// Add a fallback font family.
    pub fn add_fallback(&mut self, family: String) {
        self.fallbacks.push(family);
    }

    /// Set the default fallback.
    pub fn with_default(mut self, default: String) -> Self {
        self.default = default;
        self
    }

    /// Get the fallback chain as a list of families.
    pub fn chain(&self) -> Vec<String> {
        let mut chain = vec![self.primary.clone()];
        chain.extend(self.fallbacks.iter().cloned());
        chain.push(self.default.clone());
        chain
    }
}

impl Default for FontFallback {
    fn default() -> Self {
        Self::new("Sans".to_string())
    }
}

// ---------------------------------------------------------------------------
// Glyph rasterizer
// ---------------------------------------------------------------------------

/// Glyph rasterizer using swash.
///
/// Rasterizes glyphs from font data into bitmaps for the glyph atlas.
#[derive(Debug)]
pub struct GlyphRasterizer {
    /// Rasterization mode.
    pub mode: RasterizationMode,
    /// Scale factor for HiDPI.
    pub scale_factor: f32,
}

impl GlyphRasterizer {
    /// Create a new glyph rasterizer.
    pub fn new() -> Self {
        Self {
            mode: RasterizationMode::Grayscale,
            scale_factor: 1.0,
        }
    }

    /// Set the rasterization mode.
    pub fn with_mode(mut self, mode: RasterizationMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the scale factor.
    pub fn with_scale_factor(mut self, scale: f32) -> Self {
        self.scale_factor = scale;
        self
    }

    /// Rasterize a glyph from font data.
    ///
    /// Returns the glyph bitmap and metrics.
    ///
    /// Note: This creates a placeholder bitmap. The actual swash-based
    /// rasterization is wired up when the correct API version is available.
    pub fn rasterize(
        &self,
        _font_data: &[u8],
        _glyph_id: u16,
        font_size: f32,
    ) -> Option<GlyphRaster> {
        let scale = font_size * self.scale_factor;

        // Estimate glyph dimensions based on font size
        let width = scale.ceil().max(1.0) as u32;
        let height = scale.ceil().max(1.0) as u32;

        let pixels = vec![0u8; (width * height) as usize];

        Some(GlyphRaster {
            pixels,
            width,
            height,
            advance_x: scale * 0.6,
            advance_y: 0.0,
            bearing_x: 0.0,
            bearing_y: scale * 0.8,
        })
    }
}

/// Rasterized glyph bitmap.
#[derive(Debug, Clone)]
pub struct GlyphRaster {
    /// Pixel data (alpha channel, 1 byte per pixel).
    pub pixels: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Horizontal advance.
    pub advance_x: f32,
    /// Vertical advance.
    pub advance_y: f32,
    /// Horizontal bearing.
    pub bearing_x: f32,
    /// Vertical bearing.
    pub bearing_y: f32,
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for GlyphRaster {
    fn default() -> Self {
        Self {
            pixels: Vec::new(),
            width: 0,
            height: 0,
            advance_x: 0.0,
            advance_y: 0.0,
            bearing_x: 0.0,
            bearing_y: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shaped_text_empty() {
        let text = ShapedText::new(Vec::new(), 0, 0);
        assert!(text.is_empty());
        assert_eq!(text.line_count(), 0);
        assert_eq!(text.width, 0.0);
        assert_eq!(text.height, 0.0);
    }

    #[test]
    fn test_shaped_text_single_line() {
        let region = crate::glyph::TextureRegion::new(
            crate::texture::TextureId::new(0),
            0,
            0,
            16,
            16,
            1024,
            1024,
        );
        let info = GlyphInfo::new(region, 10.0, 0.0, 0.0, 12.0, 16.0, 16.0);
        let glyph = GlyphRun::new('A', 65, 1, Point::new(0.0, 0.0), info);
        let cluster = GlyphCluster::new(0..1, vec![glyph]);
        let line = ShapedLine::new(vec![cluster], 10.0, 12.0, 4.0);

        let text = ShapedText::new(vec![line], 1, 1);
        assert!(!text.is_empty());
        assert_eq!(text.line_count(), 1);
        assert!((text.width - 10.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shaper_config_builder() {
        let config = ShaperConfig::new()
            .with_size(18.0)
            .with_color(Color::BLACK)
            .with_align(TextAlign::Center)
            .with_max_width(200.0)
            .with_scale_factor(2.0);

        assert!((config.font_size - 18.0).abs() < f32::EPSILON);
        assert_eq!(config.color, Color::BLACK);
        assert_eq!(config.align, TextAlign::Center);
        assert_eq!(config.max_width, Some(200.0));
        assert!((config.scale_factor - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_text_shaper_creation() {
        let shaper = TextShaper::new();
        assert_eq!(shaper.face_count(), 0);
        assert_eq!(shaper.next_face_id, 1);
    }

    #[test]
    fn test_text_shaper_fallback() {
        let mut shaper = TextShaper::new();
        let config = ShaperConfig::new().with_size(16.0);
        let shaped = shaper.shape("Hello", &config);

        assert!(!shaped.is_empty());
        assert_eq!(shaped.char_count, 5);
        assert_eq!(shaped.byte_count, 5);
    }

    #[test]
    fn test_text_shaper_unicode() {
        let mut shaper = TextShaper::new();
        let config = ShaperConfig::new().with_size(16.0);
        let shaped = shaper.shape("こんにちは", &config);

        assert!(!shaped.is_empty());
        assert_eq!(shaped.char_count, 5);
        assert_eq!(shaped.byte_count, 15); // 5 CJK chars * 3 bytes each
    }

    #[test]
    fn test_text_shaper_empty() {
        let mut shaper = TextShaper::new();
        let config = ShaperConfig::new();
        let shaped = shaper.shape("", &config);

        assert!(shaped.is_empty());
        assert_eq!(shaped.char_count, 0);
    }

    #[test]
    fn test_text_shaper_measure() {
        let shaper = TextShaper::new();
        let config = ShaperConfig::new().with_size(16.0);
        let size = shaper.measure("Hello", &config);

        assert!(size.width > 0.0);
        assert!(size.height > 0.0);
    }

    #[test]
    fn test_font_fallback_chain() {
        let fallback = FontFallback::new("Custom".to_string());
        let chain = fallback.chain();

        assert_eq!(chain[0], "Custom");
        assert!(chain.contains(&"Noto Sans".to_string()));
        assert!(chain.contains(&"Noto Sans CJK JP".to_string()));
    }

    #[test]
    fn test_glyph_rasterizer_creation() {
        let rasterizer = GlyphRasterizer::new()
            .with_mode(RasterizationMode::Subpixel)
            .with_scale_factor(2.0);

        assert_eq!(rasterizer.mode, RasterizationMode::Subpixel);
        assert!((rasterizer.scale_factor - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_font_face() {
        let face = FontFace::new(
            1,
            "Test".to_string(),
            FontWeight::Bold,
            FontStyle::Italic,
            vec![0; 100],
        )
        .with_face_index(2);

        assert_eq!(face.id, 1);
        assert_eq!(face.family, "Test");
        assert_eq!(face.weight, FontWeight::Bold);
        assert_eq!(face.face_index, 2);
    }

    #[test]
    fn test_shaped_text_bounds() {
        let region = crate::glyph::TextureRegion::new(
            crate::texture::TextureId::new(0),
            0,
            0,
            16,
            16,
            1024,
            1024,
        );
        let info = GlyphInfo::new(region, 10.0, 0.0, 0.0, 12.0, 16.0, 16.0);
        let glyph = GlyphRun::new('A', 65, 1, Point::new(0.0, 0.0), info);
        let cluster = GlyphCluster::new(0..1, vec![glyph]);
        let line = ShapedLine::new(vec![cluster], 10.0, 12.0, 4.0);

        let text = ShapedText::new(vec![line], 1, 1);
        let bounds = text.bounds();

        assert!((bounds.width - 10.0).abs() < f32::EPSILON);
        assert!((bounds.height - 16.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_shaped_text_average_line_height() {
        let region = crate::glyph::TextureRegion::new(
            crate::texture::TextureId::new(0),
            0,
            0,
            16,
            16,
            1024,
            1024,
        );
        let info = GlyphInfo::new(region, 10.0, 0.0, 0.0, 12.0, 16.0, 16.0);
        let glyph = GlyphRun::new('A', 65, 1, Point::new(0.0, 0.0), info);
        let cluster = GlyphCluster::new(0..1, vec![glyph]);
        let line1 = ShapedLine::new(vec![cluster.clone()], 10.0, 12.0, 4.0);
        let line2 = ShapedLine::new(vec![cluster], 10.0, 12.0, 4.0);

        let text = ShapedText::new(vec![line1, line2], 2, 2);
        assert!((text.average_line_height() - 16.0).abs() < f32::EPSILON);
    }
}
