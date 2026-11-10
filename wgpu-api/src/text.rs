//! Text layout and rendering types.
//!
//! Text is rendered using font files with glyph caching and sub-pixel
//! positioning for smooth appearance.

use crate::color::Color;
use crate::geometry::{Point, Size};

/// Font weight (boldness).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontWeight {
    Thin = 100,
    ExtraLight = 200,
    Light = 300,
    Normal = 400,
    Medium = 500,
    SemiBold = 600,
    Bold = 700,
    ExtraBold = 800,
    Black = 900,
}

impl Default for FontWeight {
    fn default() -> Self {
        Self::Normal
    }
}

/// Font style (italic, oblique, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

impl Default for FontStyle {
    fn default() -> Self {
        Self::Normal
    }
}

/// Text alignment within a bounding box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
    Justified,
}

impl Default for TextAlign {
    fn default() -> Self {
        Self::Left
    }
}

/// Text baseline for vertical positioning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextBaseline {
    Alphabetic,
    Top,
    Bottom,
    Middle,
    Ideographic,
    Hanging,
}

impl Default for TextBaseline {
    fn default() -> Self {
        Self::Alphabetic
    }
}

/// Font descriptor for loading a font.
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// Path to the font file (TTF/OTF).
    pub path: String,
    /// Font weight.
    pub weight: FontWeight,
    /// Font style.
    pub style: FontStyle,
    /// Font family name.
    pub family: String,
}

impl Font {
    /// Create a new font descriptor from a file path.
    pub fn from_path<P: AsRef<str>>(path: P) -> Self {
        Self {
            path: path.as_ref().to_string(),
            weight: FontWeight::default(),
            style: FontStyle::default(),
            family: String::new(),
        }
    }

    /// Set the font weight.
    pub fn with_weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Set the font style.
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self
    }

    /// Set the font family name.
    pub fn with_family<F: AsRef<str>>(mut self, family: F) -> Self {
        self.family = family.as_ref().to_string();
        self
    }
}

/// Text style for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font descriptor.
    pub font: Option<Font>,
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
    /// Line height multiplier (1.0 = tight, 1.5 = comfortable).
    pub line_height: f32,
    /// Whether to enable sub-pixel anti-aliasing.
    pub subpixel_aa: bool,
    /// Text shadow offset and color.
    pub shadow: Option<(Point, Color, f32)>,
}

impl TextStyle {
    /// Create a new text style with default values.
    pub fn new() -> Self {
        Self {
            font: None,
            font_size: 14.0,
            color: Color::WHITE,
            align: TextAlign::default(),
            baseline: TextBaseline::default(),
            letter_spacing: 0.0,
            line_height: 1.2,
            subpixel_aa: true,
            shadow: None,
        }
    }

    /// Set the font.
    pub fn with_font(mut self, font: Font) -> Self {
        self.font = Some(font);
        self
    }

    /// Set the font size.
    pub fn with_size(mut self, size: f32) -> Self {
        self.font_size = size;
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

    /// Add a text shadow.
    pub fn with_shadow(mut self, offset: Point, color: Color, blur: f32) -> Self {
        self.shadow = Some((offset, color, blur));
        self
    }
}

impl Default for TextStyle {
    fn default() -> Self {
        Self::new()
    }
}

/// A text item for rendering.
#[derive(Debug, Clone)]
pub struct Text {
    /// The text string.
    pub content: String,
    /// Text style.
    pub style: TextStyle,
    /// Maximum width for wrapping (None = no wrapping).
    pub max_width: Option<f32>,
    /// Maximum height for truncation (None = no truncation).
    pub max_height: Option<f32>,
    /// Truncation suffix (e.g., "...").
    pub truncation_suffix: String,
}

impl Text {
    /// Create a new text item.
    pub fn new<S: Into<String>>(content: S) -> Self {
        Self {
            content: content.into(),
            style: TextStyle::default(),
            max_width: None,
            max_height: None,
            truncation_suffix: "...".to_string(),
        }
    }

    /// Set the text style.
    pub fn with_style(mut self, style: TextStyle) -> Self {
        self.style = style;
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

    /// Set the truncation suffix.
    pub fn with_truncation<S: AsRef<str>>(mut self, suffix: S) -> Self {
        self.truncation_suffix = suffix.as_ref().to_string();
        self
    }

    /// Estimate the text width (approximate, in logical pixels).
    /// This is a rough estimate; actual width depends on the font.
    pub fn estimate_width(&self) -> f32 {
        let char_count = self.content.chars().count() as f32;
        let avg_char_width = self.style.font_size * 0.6;
        let letter_spacing = self.style.letter_spacing * char_count;
        char_count * avg_char_width + letter_spacing
    }

    /// Estimate the text height (approximate, in logical pixels).
    pub fn estimate_height(&self) -> f32 {
        self.style.font_size * self.style.line_height
    }

    /// Estimate the text bounding box.
    pub fn estimate_size(&self) -> Size {
        Size::new(self.estimate_width(), self.estimate_height())
    }

    /// Split text into lines based on max_width.
    pub fn wrap_lines(&self) -> Vec<String> {
        match self.max_width {
            None => vec![self.content.clone()],
            Some(max_w) => {
                let mut lines = Vec::new();
                for paragraph in self.content.split('\n') {
                    let mut current_line = String::new();
                    let mut current_width = 0.0;
                    for word in paragraph.split_whitespace() {
                        let word_chars = word.chars().count() as f32;
                        let word_width = self.estimated_width_for_char_count(word_chars);
                        let separator_width = if current_line.is_empty() {
                            0.0
                        } else {
                            self.estimated_width_for_char_count(1.0)
                        };
                        if current_width + separator_width + word_width <= max_w {
                            if !current_line.is_empty() {
                                current_line.push(' ');
                            }
                            current_line.push_str(word);
                            current_width += separator_width + word_width;
                        } else {
                            if !current_line.is_empty() {
                                lines.push(std::mem::take(&mut current_line));
                            }
                            current_line.push_str(word);
                            current_width = word_width;
                        }
                    }
                    if !current_line.is_empty() {
                        lines.push(std::mem::take(&mut current_line));
                    }
                }
                lines
            }
        }
    }

    fn estimated_width_for_char_count(&self, char_count: f32) -> f32 {
        char_count * (self.style.font_size * 0.6 + self.style.letter_spacing)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_estimate() {
        let text = Text::new("Hello, wgpu-api!").with_style(TextStyle::new().with_size(16.0));
        let width = text.estimate_width();
        assert!(width > 0.0);
        assert!(text.estimate_height() > 0.0);
    }

    #[test]
    fn test_text_style_builder() {
        let style = TextStyle::new()
            .with_size(20.0)
            .with_color(Color::BLUE)
            .with_align(TextAlign::Center);
        assert!((style.font_size - 20.0).abs() < f32::EPSILON);
        assert_eq!(style.color, Color::BLUE);
        assert_eq!(style.align, TextAlign::Center);
    }
}
