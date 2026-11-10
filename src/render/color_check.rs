//! Pixel format verification utility.
//!
//! The dock's canvas buffers use **BGRA8888** pixel format throughout all renderers.
//! This means each pixel is stored as 4 bytes in the order: `[B, G, R, A]`.
//!
//! This format is used by:
//! - `slint_dock.rs` — glyph rasterization
//! - `tray.rs` — icon blending and attention dots
//! - `dashed_block.rs` — border drawing and SVG icons
//! - `popup.rs` — window list popup frames
//!
//! The Wayland SHM format is declared as `wl_shm::Format::Argb8888`, but the actual
//! memory layout written by all renderers is BGRA8888. This mismatch between the
//! declared format and the actual byte order is a known quirk — the compositor
//! interprets the buffer correctly because the renderer and compositor agree on
//! the effective pixel layout.
//!
//! # Why This Module Exists
//!
//! Resvg's `tiny_skia::Pixmap` outputs pixels in **RGBA** order `[R, G, B, A]`.
//! When blending resvg output into the canvas, the R and B channels must be swapped.
//! Forgetting this swap causes blue to appear as red/purple and vice versa.
//!
//! This module provides:
//! 1. A compile-time constant documenting the format
//! 2. A helper function for writing BGRA pixels from RGBA source data
//! 3. A unit test that verifies the channel mapping is correct
//!
//! # Usage
//!
//! When writing a pixel from resvg's RGBA pixmap to the canvas:
//!
//! ```ignore
//! use crate::render::color_check::write_bgra_pixel;
//!
//! let svg_pixels = pixmap.data(); // RGBA order
//! let svg_idx = (row * width + col) * 4;
//! let canvas_idx = (y * w + x) * 4;
//! write_bgra_pixel(canvas, canvas_idx, svg_pixels[svg_idx], svg_pixels[svg_idx + 1],
//!                   svg_pixels[svg_idx + 2], svg_pixels[svg_idx + 3], widget_alpha);
//! ```
//!
//! When writing a solid color directly:
//!
//! ```ignore
//! use crate::render::color_check::write_bgra_pixel;
//!
//! // Green #00FF00
//! write_bgra_pixel(canvas, idx, 0, 255, 0, 255);
//! ```

/// Canvas pixel format: BGRA8888 (Blue, Green, Red, Alpha).
///
/// Each pixel is 4 bytes: `[B, G, R, A]` at indices `[idx, idx+1, idx+2, idx+3]`.
pub const CANVAS_FORMAT: &str = "BGRA8888";

/// Number of bytes per pixel in the canvas.
pub const BYTES_PER_PIXEL: usize = 4;

/// Writes a single BGRA8888 pixel to the canvas.
///
/// Takes RGBA components (matching resvg's output order) and writes them
/// in the correct BGRA order to the canvas buffer.
///
/// # Arguments
/// - `canvas` — mutable slice of the canvas buffer
/// - `idx` — byte index of the pixel (must be divisible by 4)
/// - `r` — red component (0-255)
/// - `g` — green component (0-255)
/// - `b` — blue component (0-255)
/// - `a` — alpha component (0-255), already multiplied by widget alpha
///
/// # Panics
/// Panics in debug mode if `idx + 3 >= canvas.len()`.
#[inline]
pub fn write_bgra_pixel(canvas: &mut [u8], idx: usize, r: u8, g: u8, b: u8, a: u8) {
    debug_assert!(idx + 3 < canvas.len(), "Pixel index {} out of bounds for canvas size {}", idx, canvas.len());
    canvas[idx] = b;       // B
    canvas[idx + 1] = g;   // G
    canvas[idx + 2] = r;   // R
    canvas[idx + 3] = a;   // A
}

/// Writes a solid color pixel to the canvas in BGRA8888 format.
///
/// Convenience wrapper when the color is known at compile time.
/// The `color` tuple is in `(R, G, B)` order (standard CSS/HTML convention).
///
/// # Arguments
/// - `canvas` — mutable slice of the canvas buffer
/// - `idx` — byte index of the pixel
/// - `color` — RGB color tuple
/// - `a` — alpha component (0-255)
#[inline]
pub fn write_bgra_color(canvas: &mut [u8], idx: usize, color: (u8, u8, u8), a: u8) {
    write_bgra_pixel(canvas, idx, color.0, color.1, color.2, a);
}

/// Verifies that a known color is correctly mapped to BGRA8888 format.
///
/// Returns `true` if the blue channel is at index 0, green at index 1,
/// red at index 2, and alpha at index 3.
pub fn verify_bgra_format(canvas: &[u8], idx: usize, expected_r: u8, expected_g: u8, expected_b: u8, expected_a: u8) -> bool {
    if idx + 3 >= canvas.len() {
        return false;
    }
    canvas[idx] == expected_b
        && canvas[idx + 1] == expected_g
        && canvas[idx + 2] == expected_r
        && canvas[idx + 3] == expected_a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_pixel_writes_correct_channel_order() {
        let mut canvas = [0u8; 16];
        // Write a pure blue pixel (R=0, G=0, B=255, A=255)
        write_bgra_pixel(&mut canvas, 0, 0, 0, 255, 255);
        assert_eq!(canvas[0], 255, "Blue should be at index 0");
        assert_eq!(canvas[1], 0, "Green should be at index 1");
        assert_eq!(canvas[2], 0, "Red should be at index 2");
        assert_eq!(canvas[3], 255, "Alpha should be at index 3");

        // Write a pure red pixel (R=255, G=0, B=0, A=255)
        write_bgra_pixel(&mut canvas, 4, 255, 0, 0, 255);
        assert_eq!(canvas[4], 0, "Blue should be at index 4");
        assert_eq!(canvas[5], 0, "Green should be at index 5");
        assert_eq!(canvas[6], 255, "Red should be at index 6");
        assert_eq!(canvas[7], 255, "Alpha should be at index 7");
    }

    #[test]
    fn bgra_color_writes_correctly() {
        let mut canvas = [0u8; 16];
        // Green #00FF00
        write_bgra_color(&mut canvas, 0, (0, 255, 0), 255);
        assert!(verify_bgra_format(&canvas, 0, 0, 255, 0, 255));
    }

    #[test]
    fn verify_format_catches_wrong_order() {
        let mut canvas = [0u8; 16];
        // Intentionally write in wrong ARGB order for a non-symmetric color: red (#FF0000)
        // ARGB would be: A=255, R=255, G=0, B=0
        canvas[0] = 255; // A slot (wrong - should be B)
        canvas[1] = 255; // R slot (wrong - should be G)
        canvas[2] = 0;   // G slot (wrong - should be R)
        canvas[3] = 0;   // B slot (wrong - should be A)
        // This should fail the BGRA verification for pure red (R=255, G=0, B=0, A=255)
        assert!(!verify_bgra_format(&canvas, 0, 255, 0, 0, 255),
                "ARGB order should not pass BGRA verification for red");
    }

    #[test]
    fn resvg_to_canvas_channel_swap() {
        let mut canvas = [0u8; 16];
        // Simulate resvg RGBA output: pure blue (#0000FF)
        let svg_r: u8 = 0;
        let svg_g: u8 = 0;
        let svg_b: u8 = 255;
        let svg_a: u8 = 255;
        // Write using the helper (correctly swaps R and B)
        write_bgra_pixel(&mut canvas, 0, svg_r, svg_g, svg_b, svg_a);
        // Verify: blue should be at index 0 in BGRA
        assert_eq!(canvas[0], 255, "Blue channel at BGRA index 0");
        assert_eq!(canvas[2], 0, "Red channel at BGRA index 2");
    }
}
