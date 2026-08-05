use crate::FontManager;

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
                let canvas_x: i32 = (x_offset as i32 + x as i32 + metrics.xmin as i32) as i32;
                let canvas_y: i32 = (start_y as i32 + y as i32 + (size as i32 - metrics.height as i32 - metrics.ymin as i32)) as i32;

                if canvas_x < canvas_width as i32 && canvas_y < canvas_height as i32 {
                    let canvas_idx: usize = ((canvas_y * canvas_width as i32 + canvas_x) * 4) as usize;
                    let alpha = bitmap[(y as i32 * metrics.width as i32 + x as i32) as usize] as f32 / 255.0;
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
        x_offset += metrics.advance_width as i32;
    }
}
