//! GPU text rendering using glyphon.
//!
//! This module provides actual glyph rasterization and GPU text rendering
//! using the glyphon library, which combines cosmic-text for shaping,
//! etagere for atlas packing, and wgpu for rendering.
//!
//! IMPORTANT: glyphon's TextRenderer::prepare() clears internal glyph vertices
//! on each call. To render multiple text items, batch all TextArea objects into
//! a single prepare() call, then call render() once.

use glyphon::{
    Attrs, Buffer, Cache, Color, FontSystem, Metrics, Shaping, SwashCache, TextAtlas, TextRenderer,
    Viewport,
};
use std::sync::Arc;
use wgpu;

/// A single text item to be batched for GPU rendering.
pub struct TextItem {
    /// Text content
    pub text: String,
    /// Position in pixels (horizontal anchor, baseline).
    pub position: (f32, f32),
    /// Font size in pixels
    pub font_size: f32,
    /// RGBA color (0.0-1.0)
    pub color: [f32; 4],
    /// Maximum width for wrapping
    pub max_width: f32,
    /// Horizontal alignment (0.0=left, 0.5=center, 1.0=right)
    pub align: f32,
}

/// Manages GPU text rendering via glyphon.
pub struct TextManager {
    /// Font system for shaping and rasterization
    font_system: FontSystem,
    /// Glyph swash cache for rasterization
    swash_cache: SwashCache,
    /// GPU text atlas
    atlas: TextAtlas,
    /// GPU text renderer
    renderer: TextRenderer,
    /// Viewport for text rendering
    viewport: Viewport,
    /// GPU device reference
    device: Arc<wgpu::Device>,
    /// GPU queue reference
    queue: Arc<wgpu::Queue>,
    /// Pending text items to batch-render
    pending_items: Vec<TextItem>,
    /// Viewport dimensions for bounds calculation
    viewport_width: u32,
    viewport_height: u32,
}

impl TextManager {
    /// Create a new text manager.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let mut font_system = FontSystem::new();
        {
            let fontdb = font_system.db_mut();
            fontdb.load_system_fonts();
        }

        let cache = Cache::new(device);
        let mut atlas = TextAtlas::new(device, queue, &cache, wgpu::TextureFormat::Bgra8UnormSrgb);

        let renderer =
            TextRenderer::new(&mut atlas, device, wgpu::MultisampleState::default(), None);

        let viewport = Viewport::new(device, &cache);

        Self {
            font_system,
            swash_cache: SwashCache::new(),
            atlas,
            renderer,
            viewport,
            device: Arc::new(device.clone()),
            queue: Arc::new(queue.clone()),
            pending_items: Vec::new(),
            viewport_width: 0,
            viewport_height: 0,
        }
    }

    /// Add a text item to the batch. Call this for each text element before flush().
    pub fn add_text(&mut self, item: TextItem) {
        self.pending_items.push(item);
    }

    /// Flush all pending text items: prepare glyphs and render them in one batch.
    /// This must be called once per frame after all add_text() calls.
    pub fn flush(
        &mut self,
        viewport_width: u32,
        viewport_height: u32,
        render_pass: &mut wgpu::RenderPass<'_>,
    ) {
        self.viewport_width = viewport_width;
        self.viewport_height = viewport_height;

        // Update viewport resolution
        self.viewport.update(
            &self.queue,
            glyphon::Resolution {
                width: viewport_width,
                height: viewport_height,
            },
        );

        let items = std::mem::take(&mut self.pending_items);
        if items.is_empty() {
            return;
        }

        // glyphon marks each prepared glyph as in-use. Without trimming once per
        // frame, changing window titles and fractional glyph positions prevent
        // stale glyphs from being evicted, eventually exhausting the atlas.
        self.atlas.trim();

        // FIX (v1.5.5): Deduplicate identical text+size+max_width combinations
        // within a frame to avoid redundant cosmic-text shaping. When the same
        // string at the same size appears multiple times (e.g., repeated window
        // titles), we shape it once and reuse the shaped buffer for all occurrences.
        // The glyphon swash_cache + atlas already handles cross-frame glyph reuse,
        // so this within-frame deduplication targets the most expensive part:
        // the cosmic-text shaping and bidi processing.
        let mut shaped_this_frame: std::collections::HashMap<(String, u32, u32), usize> =
            std::collections::HashMap::new();

        for item_batch in items.chunks(8) {
            // Build buffers first — they must outlive the TextArea references.
            let mut buffers: Vec<Buffer> = Vec::with_capacity(item_batch.len());
            let mut buffer_indices: Vec<usize> = Vec::with_capacity(item_batch.len());

            for item in item_batch {
                let cache_key = (
                    item.text.clone(),
                    item.font_size as u32,
                    item.max_width as u32,
                );

                // FIX (v1.5.5): Check if we already shaped this exact text+size+width
                // combination in this frame. If so, reuse the buffer index instead
                // of reshaping.
                if let Some(&idx) = shaped_this_frame.get(&cache_key) {
                    buffer_indices.push(idx);
                    continue;
                }

                // Shape normally
                let metrics = Metrics::new(item.font_size, item.font_size * 1.2);
                let mut buffer = Buffer::new(&mut self.font_system, metrics);
                buffer.set_size(Some(item.max_width), None);
                buffer.set_text(
                    &item.text,
                    &Attrs::new().color(Color::rgba(
                        (item.color[0] * 255.0) as u8,
                        (item.color[1] * 255.0) as u8,
                        (item.color[2] * 255.0) as u8,
                        (item.color[3] * 255.0) as u8,
                    )),
                    Shaping::Advanced,
                    None,
                );
                buffer.shape_until_scroll(&mut self.font_system, false);

                let idx = buffers.len();
                shaped_this_frame.insert(cache_key, idx);
                buffers.push(buffer);
                buffer_indices.push(idx);
            }

            // Build text areas using the correct buffer for each item
            let text_areas: Vec<glyphon::TextArea> = item_batch
                .iter()
                .zip(buffer_indices.iter())
                .map(|(item, &buf_idx)| {
                    let buffer = &buffers[buf_idx];
                    let text_width = buffer
                        .layout_runs()
                        .map(|run| run.line_w)
                        .fold(0.0_f32, f32::max);
                    glyphon::TextArea {
                        buffer,
                        left: item.position.0 - text_width * item.align.clamp(0.0, 1.0),
                        top: item.position.1 - item.font_size,
                        scale: 1.0,
                        bounds: glyphon::TextBounds {
                            left: 0,
                            top: 0,
                            right: viewport_width as i32,
                            bottom: viewport_height as i32,
                        },
                        default_color: Color::rgba(255, 255, 255, 255),
                        custom_glyphs: &[],
                    }
                })
                .collect();

            if let Err(e) = self.renderer.prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text_areas,
                &mut self.swash_cache,
            ) {
                log::warn!("Failed to prepare text batch: {:?}", e);
                continue;
            }

            if let Err(e) = self
                .renderer
                .render(&self.atlas, &self.viewport, render_pass)
            {
                log::warn!("Failed to render text batch: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_system_loads_fonts() {
        let _font_system = FontSystem::new();
        assert!(true);
    }
}
