// src/render/text.rs

use crate::FontManager;

pub fn draw_text(
    canvas: &mut [u8],
    canvas_width: u32,
    canvas_height: u32,
    font_manager: &FontManager,
    text: &str,
    size: f32,
    start_x: usize,
    start_y: usize,
    color: (u8, u8, u8),
) {
    let mut x_offset = start_x;
    for c in text.chars() {
        let font = font_manager.get_font(c);
        let (metrics, bitmap) = font.rasterize(c, size);
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let canvas_x = (x_offset as isize + x as isize + metrics.xmin as isize) as usize;
                let canvas_y = (start_y as isize + y as isize + (size as isize - metrics.height as isize - metrics.ymin as isize)) as usize;

                if canvas_x < canvas_width as usize && canvas_y < canvas_height as usize {
                    let canvas_idx = (canvas_y * canvas_width as usize + canvas_x) * 4;
                    let alpha = bitmap[y * metrics.width + x] as f32 / 255.0;
                    if alpha > 0.0 {
                        let b = color.2 as f32;
                        let g = color.1 as f32;
                        let r = color.0 as f32;

                        let cur_b = canvas[canvas_idx] as f32;
                        let cur_g = canvas[canvas_idx + 1] as f32;
                        let cur_r = canvas[canvas_idx + 2] as f32;

                        canvas[canvas_idx]     = ((b * alpha) + (cur_b * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 1] = ((g * alpha) + (cur_g * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 2] = ((r * alpha) + (cur_r * (1.0 - alpha))) as u8;
                        canvas[canvas_idx + 3] = 255;
                    }
                }
            }
        }
        x_offset += metrics.advance_width as usize;
    }
}