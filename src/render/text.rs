//! Text rendering utilities.
//!
//! Helpers for measuring and drawing text with the font manager.

use std::path::Path;

use crate::render::font::CachedGlyph;
use crate::render::font::FontManager;

#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    canvas: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    font_manager: &mut FontManager,
    text: &str,
    size: i32,
    start_x: i32,
    start_y: i32,
    color: (u8, u8, u8),
) {
    let mut x_offset = start_x;
    let glyph_size = size as f32;

    for c in text.chars() {
        if c == '\u{200b}' || c.is_control() {
            continue;
        }

        // FontManager owns the glyph cache. The previous implementation
        // called Font::rasterize() directly for every character on every
        // redraw, which made popup opening and hover changes CPU-heavy.
        let glyph = font_manager.get_glyph(c, glyph_size);
        // FIX (v1.5.5): Changed from glyph.metrics.advance_width to glyph.advance_width
        // after replacing fontdue with swash (flat fields instead of fontdue::Metrics).
        let advance = glyph.advance_width.round() as i32;

        if !glyph.bitmap.is_empty() {
            World::blit_glyph(
                canvas,
                canvas_width.max(0) as usize,
                canvas_height.max(0) as usize,
                glyph,
                x_offset,
                start_y,
                [color.0, color.1, color.2],
            );
        }

        x_offset = x_offset.saturating_add(advance);
        if x_offset >= canvas_width {
            break;
        }
    }
}

pub struct World {
    pub font_manager: FontManager,
    pub width: usize,
    pub height: usize,
}

impl World {
    pub fn from_font_path<P: AsRef<Path>>(
        width: usize,
        height: usize,
        font_path: P,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let font_manager = FontManager::from_file(font_path)?;
        Ok(Self {
            font_manager,
            width,
            height,
        })
    }

    pub fn new(width: usize, height: usize, font_data: &[u8]) -> Self {
        Self {
            font_manager: FontManager::new(font_data),
            width,
            height,
        }
    }

    pub fn draw_blue_box(
        &self,
        frame: &mut [u8],
        x_start: i32,
        x_end: i32,
        y_start: i32,
        y_end: i32,
    ) {
        let blue_pixel: [u8; 4] = [255, 100, 0, 255];

        for y in y_start..=y_end {
            for x in x_start..=x_end {
                let is_edge = x == x_start || x == x_end || y == y_start || y == y_end;

                if is_edge {
                    let pixel_index = (y * self.width as i32 + x) * 4;

                    if ((pixel_index + 3) as usize) < frame.len() {
                        frame[(pixel_index as usize)..(pixel_index as usize + 4_usize)]
                            .copy_from_slice(&blue_pixel);
                    }
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &mut self,
        frame: &mut [u8],
        text: &str,
        start_x: i32,
        baseline_y: i32,
        size: f32,
        color: [u8; 3],
        frame_width: usize,
        frame_height: usize,
    ) -> Result<(), String> {
        let mut cursor_x = start_x;

        for c in text.chars() {
            if c == '\u{200b}' || c.is_control() {
                continue;
            }

            let glyph = self.font_manager.get_glyph(c, size);
            // FIX (v1.5.5): Changed from glyph.metrics.advance_width to glyph.advance_width.
            let advance = glyph.advance_width.round() as usize;

            if !glyph.bitmap.is_empty() {
                Self::blit_glyph(
                    frame,
                    frame_width,
                    frame_height,
                    glyph,
                    cursor_x,
                    baseline_y,
                    color,
                );
            }

            cursor_x += advance as i32;
            if cursor_x >= frame_width as i32 {
                break;
            }
        }

        Ok(())
    }

    fn blit_glyph(
        frame: &mut [u8],
        width: usize,
        height: usize,
        glyph: &CachedGlyph,
        x: i32,
        y: i32,
        color: [u8; 3],
    ) {
        // FIX (v1.5.5): Changed from glyph.metrics.{width,height,xmin,ymin} to flat fields
        // (width, height, bearing_x, bearing_y) after replacing fontdue with swash.
        let g_width = glyph.width as i32;
        let g_height = glyph.height as i32;
        if g_width <= 0 || g_height <= 0 || width == 0 || height == 0 {
            return;
        }

        let base_x = x + glyph.bearing_x;
        let base_y = y - glyph.bearing_y - g_height;

        let start_col = (-base_x).max(0);
        let start_row = (-base_y).max(0);
        let end_col = (width as i32 - base_x).min(g_width);
        let end_row = (height as i32 - base_y).min(g_height);
        if start_col >= end_col || start_row >= end_row {
            return;
        }

        for row in start_row..end_row {
            let target_y = (base_y + row) as usize;
            let row_base = row as usize * glyph.width as usize;
            for col in start_col..end_col {
                let bitmap_idx = row_base + col as usize;
                if bitmap_idx >= glyph.bitmap.len() {
                    continue;
                }
                let opacity_u8 = glyph.bitmap[bitmap_idx];
                if opacity_u8 == 0 {
                    continue;
                }

                let target_x = (base_x + col) as usize;
                let pixel_index = (target_y * width + target_x) * 4;
                if pixel_index + 3 >= frame.len() {
                    continue;
                }

                // Canvas is Argb8888 little-endian: [B, G, R, A], color is [R, G, B]
                let alpha = opacity_u8 as u16;
                if alpha == 255 {
                    frame[pixel_index] = color[2]; // B
                    frame[pixel_index + 1] = color[1]; // G
                    frame[pixel_index + 2] = color[0]; // R
                    frame[pixel_index + 3] = 255; // A
                } else {
                    let inv_alpha = 255u16 - alpha;
                    frame[pixel_index] =
                        ((color[2] as u16 * alpha + frame[pixel_index] as u16 * inv_alpha + 127)
                            / 255) as u8; // B
                    frame[pixel_index + 1] = ((color[1] as u16 * alpha
                        + frame[pixel_index + 1] as u16 * inv_alpha
                        + 127)
                        / 255) as u8; // G
                    frame[pixel_index + 2] = ((color[0] as u16 * alpha
                        + frame[pixel_index + 2] as u16 * inv_alpha
                        + 127)
                        / 255) as u8; // R
                    frame[pixel_index + 3] =
                        ((255u16 * alpha + frame[pixel_index + 3] as u16 * inv_alpha + 127) / 255)
                            as u8; // A
                }
            }
        }
    }

    pub fn clear_font_cache(&mut self) {
        self.font_manager.clear_cache();
    }
}
