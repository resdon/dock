use std::path::Path;

use crate::render::font::FontManager;
use crate::render::font::CachedGlyph;


#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    canvas: &mut [u8],
    canvas_width: i32,
    canvas_height: i32,
    font_manager: &FontManager,
    text: &str,
    size: i32,
    start_x: i32,
    start_y: i32,
    color: (u8, u8, u8),
) {
    let mut x_offset: i32 = start_x;
    for c in text.chars() {
        let font = font_manager.get_font(c);
        let (metrics, bitmap) = font.rasterize(c, size as f32);
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let canvas_x: i32 = x_offset + x as i32 + metrics.xmin;
                let canvas_y: i32 =
                    start_y + y as i32 + (size - metrics.height as i32 - metrics.ymin);

                if canvas_x < canvas_width && canvas_y < canvas_height {
                    let canvas_idx: usize = ((canvas_y * canvas_width + canvas_x) * 4) as usize;
                    let alpha = bitmap[(y as i32 * metrics.width as i32 + x as i32) as usize]
                        as f32
                        / 255.0;
                    if alpha > 0.0 {
                        let b = color.2 as f32;
                        let g = color.1 as f32;
                        let r = color.0 as f32;

                        let cur_b = canvas[canvas_idx] as f32;
                        let cur_g = canvas[canvas_idx + 1] as f32;
                        let cur_r = canvas[canvas_idx + 2] as f32;

                        canvas[canvas_idx] = ((b * alpha) + (cur_b * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 1] = ((g * alpha) + (cur_g * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 2] = ((r * alpha) + (cur_r * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }
        x_offset += metrics.advance_width as i32;
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

        eprintln!("[DrawText] Rendering string: \"{}\"", text);

        for c in text.chars() {
            if c == '\u{200b}' || c.is_control() {
                continue;
            }

            let glyph = self.font_manager.get_glyph(c, size);
            let advance = glyph.metrics.advance_width.round() as usize;

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
        let g_width = glyph.metrics.width;
        let g_height = glyph.metrics.height;

        for row in 0..g_height {
            for col in 0..g_width {
                let bitmap_idx = row * g_width + col;
                let opacity_u8 = glyph.bitmap[bitmap_idx];

                if opacity_u8 == 0 {
                    continue;
                }

                let target_x = (x + glyph.metrics.xmin + col as i32) as isize;
                let target_y = (y - glyph.metrics.ymin - g_height as i32 + row as i32) as isize;

                if target_x < 0
                    || target_x >= width as isize
                    || target_y < 0
                    || target_y >= height as isize
                {
                    continue;
                }

                let pixel_index = (target_y as usize * width + target_x as usize) * 4;

                if pixel_index + 3 >= frame.len() {
                    continue;
                }

                let alpha = opacity_u8 as f32 / 255.0;

                for i in 0..3 {
                    let dst = frame[pixel_index + i] as f32;
                    let src = color[i] as f32;
                    let blended = (src * alpha) + (dst * (1.0 - alpha));
                    frame[pixel_index + i] = blended as u8;
                }
            }
        }
    }

    pub fn clear_font_cache(&mut self) {
        self.font_manager.clear_cache();
    }
}
