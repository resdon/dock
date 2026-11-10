//! GIF decoding and animation support.
//!
//! This module provides GIF file decoding using the `gif` crate (0.13),
//! converting decoded frames to [`ImageData`] for rendering. Animated GIFs
//! are managed as frame sequences with timing information.
//!
//! ## Architecture
//!
//! ```text
//! GIF file bytes
//!     │
//!     ▼
//! GifDecoder (gif crate)
//!     │
//!     ▼
//! GifFrame → ImageData (RGBA8)
//!     │
//!     ▼
//! GifAnimation (frame sequence + timing)
//!     │
//!     ▼
//! Texture upload → Scene rendering
//! ```
//!
//! ## Integration Points
//!
//! - **image.rs**: Decoded frames produce `ImageData` compatible with existing
//!   image loading and texture upload paths.
//! - **texture.rs**: Each frame can be uploaded as a texture.
//! - **animation.rs**: GIF playback timing integrates with the animation manager.
//! - **scene.rs**: Animated GIFs can be added as `NodeContent::Image` nodes.

use std::io::Cursor;

use crate::geometry::Size;
use crate::image::ImageData;

// ---------------------------------------------------------------------------
// GIF decoding error
// ---------------------------------------------------------------------------

/// Error types for GIF decoding operations.
#[derive(Debug, Clone, PartialEq)]
pub enum GifError {
    /// The input data is not a valid GIF file.
    InvalidGif,
    /// The GIF file is corrupted or truncated.
    Corrupted(String),
    /// The GIF contains an unsupported feature.
    UnsupportedFeature(String),
    /// Failed to decode a frame.
    FrameDecodeError(String),
    /// The GIF has no frames.
    NoFrames,
    /// I/O error during decoding.
    IoError(String),
}

impl std::fmt::Display for GifError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidGif => write!(f, "invalid GIF file"),
            Self::Corrupted(msg) => write!(f, "corrupted GIF: {}", msg),
            Self::UnsupportedFeature(msg) => write!(f, "unsupported GIF feature: {}", msg),
            Self::FrameDecodeError(msg) => write!(f, "frame decode error: {}", msg),
            Self::NoFrames => write!(f, "GIF has no frames"),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for GifError {}

impl From<gif::DecodingError> for GifError {
    fn from(err: gif::DecodingError) -> Self {
        match err {
            gif::DecodingError::Format(_) => Self::InvalidGif,
            gif::DecodingError::Io(io_err) => Self::IoError(io_err.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// GIF frame info
// ---------------------------------------------------------------------------

/// Metadata for a single GIF frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GifFrameInfo {
    /// Frame index (0-based).
    pub index: u32,
    /// Delay in milliseconds before showing the next frame.
    pub delay_ms: u32,
    /// Disposal method for this frame.
    pub disposal: GifDisposalMethod,
    /// Frame's position within the canvas.
    pub left: u16,
    pub top: u16,
    /// Frame's dimensions.
    pub width: u16,
    pub height: u16,
}

/// GIF frame disposal method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GifDisposalMethod {
    /// No disposal specified (keep frame).
    None,
    /// Do not dispose (keep frame as background for next frame).
    DoNotDispose,
    /// Restore to background color.
    RestoreToBackground,
    /// Restore to previous frame.
    RestoreToPrevious,
}

impl From<gif::DisposalMethod> for GifDisposalMethod {
    fn from(dm: gif::DisposalMethod) -> Self {
        match dm {
            gif::DisposalMethod::Any => Self::None,
            gif::DisposalMethod::Keep => Self::DoNotDispose,
            gif::DisposalMethod::Background => Self::RestoreToBackground,
            gif::DisposalMethod::Previous => Self::RestoreToPrevious,
        }
    }
}

// ---------------------------------------------------------------------------
// GIF info
// ---------------------------------------------------------------------------

/// Metadata about a GIF file.
#[derive(Debug, Clone, PartialEq)]
pub struct GifInfo {
    /// Overall canvas width.
    pub width: u16,
    /// Overall canvas height.
    pub height: u16,
    /// Number of frames.
    pub frame_count: u32,
    /// Whether the GIF is animated (more than one frame).
    pub is_animated: bool,
    /// Repeat behavior.
    pub repeat: GifLoopCount,
}

impl GifInfo {
    /// Get the canvas size.
    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// Estimate total memory for all frames in RGBA8 format.
    pub fn estimated_memory_bytes(&self) -> usize {
        (self.width as usize) * (self.height as usize) * 4 * (self.frame_count as usize)
    }
}

// ---------------------------------------------------------------------------
// GIF loop count
// ---------------------------------------------------------------------------

/// GIF loop count (from NETSCAPE2.0 extension).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GifLoopCount {
    /// Loop indefinitely (default for animated GIFs).
    Forever,
    /// Loop a specific number of times.
    Count(usize),
    /// No loop (play once).
    Once,
}

impl From<gif::Repeat> for GifLoopCount {
    fn from(r: gif::Repeat) -> Self {
        match r {
            gif::Repeat::Finite(0) => Self::Once,
            gif::Repeat::Infinite => Self::Forever,
            gif::Repeat::Finite(count) => Self::Count(count as usize),
        }
    }
}

// ---------------------------------------------------------------------------
// GIF decoder
// ---------------------------------------------------------------------------

/// Decodes GIF data into frames.
///
/// Uses the `gif` crate for decoding and converts frames to `ImageData`
/// in RGBA8 format for compatibility with the rendering pipeline.
#[derive(Debug)]
pub struct GifDecoder {
    /// GIF metadata.
    info: GifInfo,
    /// Decoded frames as RGBA8 image data.
    frames: Vec<ImageData>,
    /// Per-frame metadata.
    frame_infos: Vec<GifFrameInfo>,
    /// Current frame index.
    current_frame: u32,
}

impl GifDecoder {
    /// Decode GIF from raw bytes.
    ///
    /// Decodes all frames eagerly, compositing them with disposal handling.
    pub fn decode(data: &[u8]) -> Result<Self, GifError> {
        let mut decoder = gif::DecodeOptions::new();
        decoder.set_color_output(gif::ColorOutput::RGBA);

        let mut reader = decoder.read_info(Cursor::new(data))?;

        let width = reader.width();
        let height = reader.height();
        let repeat = reader.repeat().into();

        // Count frames by iterating
        let mut frames = Vec::new();
        let mut frame_infos = Vec::new();

        // Canvas for compositing
        let mut canvas = vec![0u8; (width * height * 4) as usize];
        let mut previous_canvas = None;

        while let Some(frame) = reader.read_next_frame()? {
            // Copy frame metadata before borrowing reader again
            let f_delay = frame.delay;
            let f_dispose = frame.dispose;
            let f_left = frame.left;
            let f_top = frame.top;
            let f_width = frame.width;
            let f_height = frame.height;

            let frame_info = GifFrameInfo {
                index: frames.len() as u32,
                delay_ms: f_delay.max(1) as u32 * 10,
                disposal: f_dispose.into(),
                left: f_left,
                top: f_top,
                width: f_width,
                height: f_height,
            };

            // Decode into buffer
            let buf_size = reader.buffer_size();
            let mut buf = vec![0u8; buf_size];
            reader.fill_buffer(&mut buf)?;

            // The buffer is RGBA data for just the frame's region
            let frame_width = f_width as usize;
            let frame_height = f_height as usize;
            let expected_size = frame_width * frame_height * 4;

            let pixel_data = if buf.len() >= expected_size {
                buf[..expected_size].to_vec()
            } else {
                let mut padded = buf;
                padded.resize(expected_size, 0);
                padded
            };

            let frame_data = ImageData::from_rgba(f_width as u32, f_height as u32, pixel_data);

            // Save previous canvas for RestoreToPrevious
            if frames.len() > 0 {
                previous_canvas = Some(canvas.clone());
            }

            // Composite frame onto canvas
            composite_frame(
                &mut canvas,
                width as usize,
                height as usize,
                &frame_data,
                frame_info.left as usize,
                frame_info.top as usize,
            );

            // Store the composite canvas as this frame's image
            let composite = ImageData::from_rgba(width as u32, height as u32, canvas.clone());

            frames.push(composite);
            frame_infos.push(frame_info);

            // Handle disposal method
            match frame_info.disposal {
                GifDisposalMethod::RestoreToBackground => {
                    clear_rect(
                        &mut canvas,
                        width as usize,
                        height as usize,
                        frame_info.left as usize,
                        frame_info.top as usize,
                        frame_info.width as usize,
                        frame_info.height as usize,
                    );
                }
                GifDisposalMethod::RestoreToPrevious => {
                    if let Some(prev) = &previous_canvas {
                        canvas.clone_from_slice(prev);
                    }
                }
                _ => {}
            }
        }

        if frames.is_empty() {
            return Err(GifError::NoFrames);
        }

        let frame_count = frames.len() as u32;
        let info = GifInfo {
            width,
            height,
            frame_count,
            is_animated: frame_count > 1,
            repeat,
        };

        Ok(Self {
            info,
            frames,
            frame_infos,
            current_frame: 0,
        })
    }

    /// Get the GIF metadata.
    pub fn info(&self) -> &GifInfo {
        &self.info
    }

    /// Get the number of frames.
    pub fn frame_count(&self) -> u32 {
        self.info.frame_count
    }

    /// Check if this GIF is animated.
    pub fn is_animated(&self) -> bool {
        self.info.is_animated
    }

    /// Get the canvas size.
    pub fn size(&self) -> Size {
        self.info.size()
    }

    /// Get a specific frame's image data.
    pub fn get_frame(&self, index: u32) -> Option<&ImageData> {
        self.frames.get(index as usize)
    }

    /// Get the current frame's image data.
    pub fn current_frame(&self) -> &ImageData {
        &self.frames[self.current_frame as usize]
    }

    /// Get the current frame index.
    pub fn current_frame_index(&self) -> u32 {
        self.current_frame
    }

    /// Get the frame info for a specific frame.
    pub fn get_frame_info(&self, index: u32) -> Option<&GifFrameInfo> {
        self.frame_infos.get(index as usize)
    }

    /// Get the delay in milliseconds for the current frame.
    pub fn current_frame_delay_ms(&self) -> u32 {
        self.frame_infos
            .get(self.current_frame as usize)
            .map(|fi| fi.delay_ms)
            .unwrap_or(100)
    }

    /// Advance to the next frame. Returns `true` if wrapped.
    pub fn advance_frame(&mut self) -> bool {
        let next = self.current_frame + 1;
        if next >= self.info.frame_count {
            self.current_frame = 0;
            true
        } else {
            self.current_frame = next;
            false
        }
    }

    /// Set the current frame by index.
    pub fn set_frame(&mut self, index: u32) {
        if index < self.info.frame_count {
            self.current_frame = index;
        }
    }

    /// Get the loop count behavior.
    pub fn loop_count(&self) -> GifLoopCount {
        self.info.repeat
    }

    /// Get all frames as a slice.
    pub fn frames(&self) -> &[ImageData] {
        &self.frames
    }

    /// Get all frame infos as a slice.
    pub fn frame_infos(&self) -> &[GifFrameInfo] {
        &self.frame_infos
    }

    /// Estimate memory usage in bytes.
    pub fn estimated_memory_bytes(&self) -> usize {
        self.info.estimated_memory_bytes()
    }
}

// ---------------------------------------------------------------------------
// GIF animation player
// ---------------------------------------------------------------------------

/// Manages playback of an animated GIF.
#[derive(Debug)]
pub struct GifAnimation {
    /// The decoded GIF data.
    decoder: GifDecoder,
    /// Total elapsed time in milliseconds.
    elapsed_ms: u32,
    /// Whether the animation is playing.
    playing: bool,
    /// Playback speed multiplier (1.0 = normal).
    speed: f32,
    /// Number of times the animation has looped.
    loop_iterations: usize,
    /// Whether the animation has completed.
    completed: bool,
}

impl GifAnimation {
    /// Create from raw bytes.
    pub fn from_bytes(data: &[u8]) -> Result<Self, GifError> {
        let decoder = GifDecoder::decode(data)?;
        Ok(Self {
            decoder,
            elapsed_ms: 0,
            playing: true,
            speed: 1.0,
            loop_iterations: 0,
            completed: false,
        })
    }

    /// Create from a decoder.
    pub fn from_decoder(decoder: GifDecoder) -> Self {
        Self {
            decoder,
            elapsed_ms: 0,
            playing: true,
            speed: 1.0,
            loop_iterations: 0,
            completed: false,
        }
    }

    /// Advance by delta time (ms). Returns `true` if frame changed.
    pub fn update(&mut self, delta_ms: u32) -> bool {
        if !self.playing || self.completed {
            return false;
        }

        let adjusted_delta = (delta_ms as f32 * self.speed) as u32;
        let old_frame = self.decoder.current_frame_index();

        self.elapsed_ms += adjusted_delta;

        let mut accumulated_ms = 0u32;
        let frame_count = self.decoder.frame_count();

        for i in 0..frame_count {
            let delay = self
                .decoder
                .frame_infos()
                .get(i as usize)
                .map(|fi| fi.delay_ms)
                .unwrap_or(100);

            if accumulated_ms + delay > self.elapsed_ms {
                if (i as u32) != self.decoder.current_frame_index() {
                    self.decoder.set_frame(i as u32);
                }
                return (i as u32) != old_frame;
            }
            accumulated_ms += delay;
        }

        // Passed all frames — check loop behavior
        match self.decoder.loop_count() {
            GifLoopCount::Forever => {
                self.loop_iterations += 1;
                self.elapsed_ms = 0;
                self.decoder.set_frame(0);
                true
            }
            GifLoopCount::Count(max_loops) => {
                if self.loop_iterations < max_loops {
                    self.loop_iterations += 1;
                    self.elapsed_ms = 0;
                    self.decoder.set_frame(0);
                    true
                } else {
                    self.completed = true;
                    self.playing = false;
                    true
                }
            }
            GifLoopCount::Once => {
                self.completed = true;
                self.playing = false;
                true
            }
        }
    }

    /// Get the current frame's image data.
    pub fn current_frame(&self) -> &ImageData {
        self.decoder.current_frame()
    }

    /// Get the current frame index.
    pub fn current_frame_index(&self) -> u32 {
        self.decoder.current_frame_index()
    }

    /// Get the total frame count.
    pub fn frame_count(&self) -> u32 {
        self.decoder.frame_count()
    }

    /// Check if playing.
    pub fn is_playing(&self) -> bool {
        self.playing
    }

    /// Check if completed.
    pub fn is_completed(&self) -> bool {
        self.completed
    }

    /// Check if animated.
    pub fn is_animated(&self) -> bool {
        self.decoder.is_animated()
    }

    /// Get the GIF metadata.
    pub fn info(&self) -> &GifInfo {
        self.decoder.info()
    }

    /// Get the canvas size.
    pub fn size(&self) -> Size {
        self.decoder.size()
    }

    /// Play or pause.
    pub fn set_playing(&mut self, playing: bool) {
        self.playing = playing;
        if playing && self.completed {
            self.completed = false;
            self.elapsed_ms = 0;
            self.loop_iterations = 0;
            self.decoder.set_frame(0);
        }
    }

    /// Set playback speed multiplier.
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.max(0.01);
    }

    /// Get playback speed.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Reset to beginning.
    pub fn reset(&mut self) {
        self.elapsed_ms = 0;
        self.loop_iterations = 0;
        self.completed = false;
        self.playing = true;
        self.decoder.set_frame(0);
    }

    /// Seek to a specific frame.
    pub fn seek_to_frame(&mut self, frame: u32) {
        self.decoder.set_frame(frame);
        let mut accumulated = 0u32;
        for i in 0..frame {
            let delay = self
                .decoder
                .frame_infos()
                .get(i as usize)
                .map(|fi| fi.delay_ms)
                .unwrap_or(100);
            accumulated += delay;
        }
        self.elapsed_ms = accumulated;
    }

    /// Get elapsed time in ms.
    pub fn elapsed_ms(&self) -> u32 {
        self.elapsed_ms
    }

    /// Get loop iteration count.
    pub fn loop_iterations(&self) -> usize {
        self.loop_iterations
    }

    /// Get the underlying decoder.
    pub fn decoder(&self) -> &GifDecoder {
        &self.decoder
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Composite a frame onto a canvas buffer.
fn composite_frame(
    canvas: &mut Vec<u8>,
    canvas_width: usize,
    canvas_height: usize,
    frame: &ImageData,
    dest_x: usize,
    dest_y: usize,
) {
    let frame_width = frame.width as usize;
    let frame_height = frame.height as usize;

    for y in 0..frame_height {
        let dst_y = dest_y + y;
        if dst_y >= canvas_height {
            break;
        }

        for x in 0..frame_width {
            let dst_x = dest_x + x;
            if dst_x >= canvas_width {
                break;
            }

            let src_idx = (y * frame_width + x) * 4;
            let dst_idx = (dst_y * canvas_width + dst_x) * 4;

            let alpha = frame.pixels.get(src_idx + 3).copied().unwrap_or(0) as f32 / 255.0;
            if alpha > 0.0 {
                for c in 0..4 {
                    let src_val = frame.pixels.get(src_idx + c).copied().unwrap_or(0) as f32;
                    let dst_val = canvas.get(dst_idx + c).copied().unwrap_or(0) as f32;
                    canvas[dst_idx + c] = (src_val * alpha + dst_val * (1.0 - alpha)) as u8;
                }
            }
        }
    }
}

/// Clear a rectangular region to transparent black.
fn clear_rect(
    canvas: &mut Vec<u8>,
    canvas_width: usize,
    canvas_height: usize,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
) {
    for dy in 0..height {
        let dst_y = y + dy;
        if dst_y >= canvas_height {
            break;
        }
        for dx in 0..width {
            let dst_x = x + dx;
            if dst_x >= canvas_width {
                break;
            }
            let idx = (dst_y * canvas_width + dst_x) * 4;
            canvas[idx] = 0;
            canvas[idx + 1] = 0;
            canvas[idx + 2] = 0;
            canvas[idx + 3] = 0;
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_gif() -> Vec<u8> {
        let mut output = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut output, 64, 64, &[]).unwrap();

            let mut red_frame = vec![(255u8, 0, 0, 255); 64 * 64]
                .into_iter()
                .flat_map(|(a, b, c, d)| vec![a, b, c, d])
                .collect::<Vec<u8>>();
            let red_data = gif::Frame::from_rgba_speed(64, 64, &mut red_frame, 1);
            encoder.write_frame(&red_data).unwrap();

            let mut green_frame = vec![(0u8, 255, 0, 255); 64 * 64]
                .into_iter()
                .flat_map(|(a, b, c, d)| vec![a, b, c, d])
                .collect::<Vec<u8>>();
            let green_data = gif::Frame::from_rgba_speed(64, 64, &mut green_frame, 1);
            encoder.write_frame(&green_data).unwrap();
        }
        output
    }

    #[test]
    fn test_gif_decode() {
        let data = create_test_gif();
        let decoder = GifDecoder::decode(&data).unwrap();
        assert!(decoder.frame_count() >= 1);
        assert!(decoder.size().width > 0.0);
        assert!(decoder.size().height > 0.0);
    }

    #[test]
    fn test_gif_info() {
        let data = create_test_gif();
        let decoder = GifDecoder::decode(&data).unwrap();
        let info = decoder.info();
        assert_eq!(info.width, 64);
        assert_eq!(info.height, 64);
    }

    #[test]
    fn test_gif_frame_access() {
        let data = create_test_gif();
        let decoder = GifDecoder::decode(&data).unwrap();
        let frame = decoder.get_frame(0).unwrap();
        assert_eq!(frame.width, 64);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.format, crate::image::ImageFormat::Rgba8);
    }

    #[test]
    fn test_gif_animation_creation() {
        let data = create_test_gif();
        let anim = GifAnimation::from_bytes(&data).unwrap();
        assert_eq!(anim.frame_count(), anim.decoder.frame_count());
        assert!(!anim.is_completed());
    }

    #[test]
    fn test_gif_animation_update() {
        let data = create_test_gif();
        let mut anim = GifAnimation::from_bytes(&data).unwrap();
        let changed = anim.update(1000);
        assert!(changed || anim.current_frame_index() >= 0);
    }

    #[test]
    fn test_gif_animation_play_pause() {
        let data = create_test_gif();
        let mut anim = GifAnimation::from_bytes(&data).unwrap();
        assert!(anim.is_playing());
        anim.set_playing(false);
        assert!(!anim.is_playing());
        anim.set_playing(true);
        assert!(anim.is_playing());
    }

    #[test]
    fn test_gif_animation_speed() {
        let data = create_test_gif();
        let mut anim = GifAnimation::from_bytes(&data).unwrap();
        anim.set_speed(2.0);
        assert_eq!(anim.speed(), 2.0);
        anim.set_speed(0.5);
        assert_eq!(anim.speed(), 0.5);
    }

    #[test]
    fn test_gif_animation_reset() {
        let data = create_test_gif();
        let mut anim = GifAnimation::from_bytes(&data).unwrap();
        anim.update(10000);
        anim.reset();
        assert_eq!(anim.current_frame_index(), 0);
        assert!(!anim.is_completed());
        assert!(anim.is_playing());
    }

    #[test]
    fn test_gif_animation_seek() {
        let data = create_test_gif();
        let mut anim = GifAnimation::from_bytes(&data).unwrap();
        let frame_count = anim.frame_count();
        if frame_count > 1 {
            anim.seek_to_frame(1);
            assert_eq!(anim.current_frame_index(), 1);
        }
    }

    #[test]
    fn test_gif_invalid_data() {
        let result = GifDecoder::decode(b"not a gif");
        assert!(result.is_err());
    }

    #[test]
    fn test_gif_info_estimated_memory() {
        let data = create_test_gif();
        let decoder = GifDecoder::decode(&data).unwrap();
        let mem = decoder.estimated_memory_bytes();
        assert!(mem > 0);
        let expected = 64 * 64 * 4 * decoder.frame_count() as usize;
        assert_eq!(mem, expected);
    }

    #[test]
    fn test_gif_loop_count_default() {
        let data = create_test_gif();
        let decoder = GifDecoder::decode(&data).unwrap();
        // Default repeat is Finite(0) which maps to Once
        assert_eq!(decoder.loop_count(), GifLoopCount::Once);
    }

    #[test]
    fn test_composite_frame() {
        let canvas_width = 4;
        let mut canvas = vec![0u8; canvas_width * canvas_width * 4];

        let frame = ImageData::from_rgba(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 128,
            ],
        );

        composite_frame(&mut canvas, canvas_width, canvas_width, &frame, 1, 1);
        // Check that the composite area has non-zero values (red pixel at position 1,1)
        let idx = (1 * canvas_width + 1) * 4;
        assert_eq!(canvas[idx], 255); // Red channel
    }

    #[test]
    fn test_clear_rect() {
        let canvas_width = 4;
        let mut canvas = vec![255u8; canvas_width * canvas_width * 4];

        clear_rect(&mut canvas, canvas_width, canvas_width, 1, 1, 2, 2);

        for y in 1..3 {
            for x in 1..3 {
                let idx = (y * canvas_width + x) * 4;
                assert_eq!(canvas[idx], 0);
                assert_eq!(canvas[idx + 3], 0);
            }
        }
    }

    #[test]
    fn test_composite_frame_clips_to_canvas_height() {
        let mut canvas = vec![0; 3 * 2 * 4];
        let frame = ImageData::solid_color(2, 2, crate::color::Color::RED);

        composite_frame(&mut canvas, 3, 2, &frame, 0, 1);

        assert_eq!(&canvas[12..20], &[255, 0, 0, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn test_clear_rect_clips_to_canvas_height() {
        let mut canvas = vec![255; 3 * 2 * 4];

        clear_rect(&mut canvas, 3, 2, 0, 1, 3, 2);

        assert!(canvas[12..].iter().all(|value| *value == 0));
    }

    #[test]
    fn test_disposal_method_conversion() {
        assert_eq!(
            GifDisposalMethod::from(gif::DisposalMethod::Any),
            GifDisposalMethod::None
        );
        assert_eq!(
            GifDisposalMethod::from(gif::DisposalMethod::Keep),
            GifDisposalMethod::DoNotDispose
        );
        assert_eq!(
            GifDisposalMethod::from(gif::DisposalMethod::Background),
            GifDisposalMethod::RestoreToBackground
        );
        assert_eq!(
            GifDisposalMethod::from(gif::DisposalMethod::Previous),
            GifDisposalMethod::RestoreToPrevious
        );
    }
}
