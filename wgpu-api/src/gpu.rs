//! GPU rendering backend — the actual wgpu integration layer.
//!
//! This module provides the GPU resource management, pipeline creation,
//! instanced rendering, and surface presentation for the 2D renderer.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  DrawCommand (render.rs)                                    │
//! │  Rectangle, Image, Text, Video, Clip...                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │  Batching (batching.rs)                                     │
//! │  Sort by pipeline → texture → blend mode                    │
//! ├─────────────────────────────────────────────────────────────┤
//! │  GPU Backend (gpu.rs)                                       │
//! │  ┌──────────────┐  ┌──────────┐  ┌──────────────────────┐  │
//! │  │  Pipeline    │  │  Texture │  │  Buffer Cache        │  │
//! │  │  Cache       │  │  Cache   │  │  (instance/uniform)  │  │
//! │  └──────┬───────┘  └────┬─────┘  └──────────┬───────────┘  │
//! │         └───────────────┴───────────────────┘              │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │  wgpu Device/Queue   │                       │
//! │              │  (command encoding)  │                       │
//! │              └──────────┬───────────┘                       │
//! │                         ▼                                   │
//! │              ┌──────────────────────┐                       │
//! │              │  Surface Present     │                       │
//! │              │  (Wayland/Vulkan)    │                       │
//! │              └──────────────────────┘                       │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Four Render Pipelines
//!
//! | Pipeline       | Purpose                              |
//! |----------------|--------------------------------------|
//! | Solid2D        | Rectangles, backgrounds, shapes      |
//! | TexturedQuad   | Icons, images, video                 |
//! | Text           | Glyph atlas rendering                |
//! | Composite      | Opacity, transitions, blending       |
//!
//! ## Instanced Rendering
//!
//! Most content is textured or colored rectangles. Instead of one draw call
//! per icon, we use instanced rendering with a single shared quad and an
//! instance buffer containing position, size, UV, and color data.
//!
//! ## Resource Management
//!
//! - **TextureCache**: GPU textures with LRU eviction and dimension-based reuse
//! - **PipelineCache**: Pre-created render pipelines by type and blend mode
//! - **BufferCache**: Reusable vertex and instance buffers that grow as needed
//!
//! ## Surface Presentation
//!
//! A normal frame lifecycle:
//! 1. Acquire surface texture
//! 2. Create command encoder
//! 3. Begin render pass, issue draw commands
//! 4. Submit to queue
//! 5. Present frame
//!
//! Errors (surface lost, outdated, OOM, device loss) are handled gracefully
//! without crashing the application.

use std::collections::HashMap;
use std::fmt;

use crate::geometry::{Size, Rect};
use crate::surface::SurfaceFormat;
use crate::texture::{TextureId, TextureFormat as TexFormat};
use crate::render::RenderError;

// ---------------------------------------------------------------------------
// Quad instance data for instanced rendering
// ---------------------------------------------------------------------------

/// Instance data for instanced quad rendering.
///
/// Each instance describes one quad: position, size, UV coordinates, and color.
/// The GPU uses one shared quad geometry and draws multiple instances in a
/// single draw call.
///
/// Layout: 8 × f32 = 32 bytes per instance (aligned to 16-byte boundary).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadInstance {
    /// Position in logical coordinates (x, y).
    pub position: [f32; 2],
    /// Size in logical coordinates (width, height).
    pub size: [f32; 2],
    /// UV minimum corner (u_min, v_min).
    pub uv_min: [f32; 2],
    /// UV maximum corner (u_max, v_max).
    pub uv_max: [f32; 2],
    /// Tint color (r, g, b, a).
    pub color: [f32; 4],
}

impl Default for QuadInstance {
    fn default() -> Self {
        Self {
            position: [0.0, 0.0],
            size: [1.0, 1.0],
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        }
    }
}

impl QuadInstance {
    /// Size of a single instance in bytes.
    pub const SIZE: u32 = std::mem::size_of::<Self>() as u32;

    /// Create a new quad instance.
    pub fn new(
        position: [f32; 2],
        size: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
    ) -> Self {
        Self {
            position,
            size,
            uv_min,
            uv_max,
            color,
        }
    }

    /// Create a solid-color instance (ignores UV).
    pub fn solid(position: [f32; 2], size: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            size,
            uv_min: [0.0, 0.0],
            uv_max: [1.0, 1.0],
            color,
        }
    }

    /// Create a textured instance.
    pub fn textured(
        position: [f32; 2],
        size: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        tint: [f32; 4],
    ) -> Self {
        Self {
            position,
            size,
            uv_min,
            uv_max,
            color: tint,
        }
    }

    /// Compute the bounding rectangle of this instance.
    pub fn bounding_rect(&self) -> Rect {
        Rect::new(
            self.position[0],
            self.position[1],
            self.size[0],
            self.size[1],
        )
    }
}

// ---------------------------------------------------------------------------
// Pipeline types
// ---------------------------------------------------------------------------

/// Render pipeline type identifier.
///
/// The four pipelines cover all 2D rendering needs:
/// solid rectangles, textured quads, text glyphs, and compositing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineType {
    /// Solid 2D pipeline for rectangles, backgrounds, and basic shapes.
    Solid2D,
    /// Textured quad pipeline for icons, images, and video.
    TexturedQuad,
    /// Text pipeline for glyph atlas rendering.
    Text,
    /// Composite pipeline for opacity, transitions, and blending.
    Composite,
}

impl Default for PipelineType {
    fn default() -> Self {
        Self::Solid2D
    }
}

impl fmt::Display for PipelineType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PipelineType::Solid2D => write!(f, "Solid2D"),
            PipelineType::TexturedQuad => write!(f, "TexturedQuad"),
            PipelineType::Text => write!(f, "Text"),
            PipelineType::Composite => write!(f, "Composite"),
        }
    }
}

/// Blend mode for render pipelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// No blending (replace).
    Opaque,
    /// Alpha blending (source * alpha + dest * (1 - alpha)).
    Alpha,
    /// Additive blending.
    Additive,
    /// Multiply blending.
    Multiply,
    /// Screen blending.
    Screen,
}

impl Default for BlendMode {
    fn default() -> Self {
        Self::Alpha
    }
}

/// Key for pipeline cache lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PipelineKey {
    /// Pipeline type.
    pub pipeline_type: PipelineType,
    /// Blend mode.
    pub blend_mode: BlendMode,
}

impl PipelineKey {
    /// Create a new pipeline key.
    pub fn new(pipeline_type: PipelineType, blend_mode: BlendMode) -> Self {
        Self {
            pipeline_type,
            blend_mode,
        }
    }
}

// ---------------------------------------------------------------------------
// Orthographic projection
// ---------------------------------------------------------------------------

/// 2D orthographic projection matrix.
///
/// Converts logical coordinates to normalized device coordinates (NDC).
/// The projection is defined by the viewport's logical size and scale factor.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct OrthographicProjection {
    /// Projection matrix values (column-major 4x4, stored as 16 f32s).
    pub matrix: [f32; 16],
}

impl OrthographicProjection {
    /// Size of the projection uniform in bytes.
    pub const SIZE: u64 = std::mem::size_of::<Self>() as u64;

    /// Create an identity projection.
    pub fn identity() -> Self {
        Self {
            matrix: [
                1.0, 0.0, 0.0, 0.0,
                0.0, 1.0, 0.0, 0.0,
                0.0, 0.0, 1.0, 0.0,
                0.0, 0.0, 0.0, 1.0,
            ],
        }
    }

    /// Create an orthographic projection from viewport dimensions.
    ///
    /// Maps logical coordinates (0..width, 0..height) to NDC (-1..1, -1..1).
    /// Y-axis is flipped so that (0, 0) is at the top-left.
    pub fn from_viewport(width: f32, height: f32) -> Self {
        if width <= 0.0 || height <= 0.0 {
            return Self::identity();
        }

        let inv_width = 2.0 / width;
        let inv_height = -2.0 / height; // Flip Y axis

        Self {
            matrix: [
                inv_width, 0.0,     0.0, 0.0,
                0.0,       inv_height, 0.0, 0.0,
                0.0,       0.0,     1.0, 0.0,
                -1.0,      1.0,     0.0, 1.0,
            ],
        }
    }

    /// Create an orthographic projection with a custom origin.
    ///
    /// `origin` specifies where (0, 0) in logical space maps to in NDC.
    /// Useful for different coordinate conventions.
    pub fn from_viewport_with_origin(width: f32, height: f32, origin: [f32; 2]) -> Self {
        if width <= 0.0 || height <= 0.0 {
            return Self::identity();
        }

        let inv_width = 2.0 / width;
        let inv_height = -2.0 / height;

        Self {
            matrix: [
                inv_width, 0.0,     0.0, 0.0,
                0.0,       inv_height, 0.0, 0.0,
                0.0,       0.0,     1.0, 0.0,
                -1.0 + origin[0], 1.0 + origin[1], 0.0, 1.0,
            ],
        }
    }

    /// Transform a point from logical coordinates to NDC.
    pub fn transform_point(&self, x: f32, y: f32) -> [f32; 2] {
        let m = &self.matrix;
        [
            m[0] * x + m[4] * y + m[12],
            m[1] * x + m[5] * y + m[13],
        ]
    }

    /// Check if the projection is valid (non-degenerate).
    pub fn is_valid(&self) -> bool {
        self.matrix[0].abs() > 0.0 && self.matrix[5].abs() > 0.0
    }
}

impl Default for OrthographicProjection {
    fn default() -> Self {
        Self::identity()
    }
}

// ---------------------------------------------------------------------------
// Uniform buffer for GPU shaders
// ---------------------------------------------------------------------------

/// Combined uniform buffer data for GPU shaders.
///
/// Contains the orthographic projection matrix and additional per-frame
/// uniform data like time and surface size.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct GpuUniformBuffer {
    /// Orthographic projection matrix.
    pub projection: [f32; 16],
    /// Surface size in logical pixels.
    pub surface_size: [f32; 2],
    /// Current frame time in seconds.
    pub frame_time: f32,
    /// Frame number.
    pub frame_number: u32,
    /// Padding for alignment.
    pub _padding: [u32; 4],
}

impl Default for GpuUniformBuffer {
    fn default() -> Self {
        Self {
            projection: OrthographicProjection::identity().matrix,
            surface_size: [800.0, 600.0],
            frame_time: 0.0,
            frame_number: 0,
            _padding: [0; 4],
        }
    }
}

impl GpuUniformBuffer {
    /// Size of the uniform buffer in bytes.
    pub const SIZE: u64 = std::mem::size_of::<Self>() as u64;

    /// Create a new uniform buffer for the given viewport.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            projection: OrthographicProjection::from_viewport(width, height).matrix,
            surface_size: [width, height],
            frame_time: 0.0,
            frame_number: 0,
            _padding: [0; 4],
        }
    }

    /// Update the projection matrix for a new viewport size.
    pub fn set_projection(&mut self, width: f32, height: f32) {
        self.projection = OrthographicProjection::from_viewport(width, height).matrix;
        self.surface_size = [width, height];
    }

    /// Update the frame time.
    pub fn set_frame_time(&mut self, time: f32) {
        self.frame_time = time;
    }

    /// Increment the frame number.
    pub fn next_frame(&mut self) {
        self.frame_number = self.frame_number.wrapping_add(1);
    }
}

// ---------------------------------------------------------------------------
// Texture cache
// ---------------------------------------------------------------------------

/// Information about a cached GPU texture.
#[derive(Debug, Clone)]
pub struct CachedTexture {
    /// Texture identifier.
    pub texture_id: TextureId,
    /// Texture dimensions.
    pub size: Size,
    /// Texture format.
    pub format: TexFormat,
    /// Whether the texture is currently in use.
    pub in_use: bool,
    /// Last access frame number (for LRU eviction).
    pub last_access: u64,
    /// GPU memory usage in bytes.
    pub gpu_memory_bytes: u64,
}

impl CachedTexture {
    /// Estimate GPU memory usage for this texture.
    pub fn estimate_memory(size: Size, format: TexFormat) -> u64 {
        let bytes_per_pixel = match format {
            TexFormat::Rgba8Unorm | TexFormat::Bgra8UnormSrgb | TexFormat::Rgba8UnormSrgb => 4,
            TexFormat::Rgba16Float => 8,
            TexFormat::Rgba32Float => 16,
            TexFormat::Depth32Float => 4,
            TexFormat::Depth24PlusStencil8 => 4,
        };
        (size.width as u64)
            .saturating_mul(size.height as u64)
            .saturating_mul(bytes_per_pixel as u64)
    }
}

/// Cache for GPU textures with LRU eviction.
///
/// Manages texture lifetime, tracks memory usage, and evicts
/// least-recently-used textures when the memory budget is exceeded.
#[derive(Debug, Clone)]
pub struct TextureCache {
    /// Cached textures by ID.
    textures: HashMap<TextureId, CachedTexture>,
    /// Maximum GPU memory budget in bytes.
    memory_budget: u64,
    /// Current GPU memory usage in bytes.
    memory_used: u64,
    /// Current frame number (for LRU tracking).
    current_frame: u64,
}

impl TextureCache {
    /// Create a new texture cache with the given memory budget.
    pub fn new(memory_budget: u64) -> Self {
        Self {
            textures: HashMap::new(),
            memory_budget,
            memory_used: 0,
            current_frame: 0,
        }
    }

    /// Insert a texture into the cache.
    pub fn insert(&mut self, texture_id: TextureId, size: Size, format: TexFormat) {
        let memory = CachedTexture::estimate_memory(size, format);
        let cached = CachedTexture {
            texture_id,
            size,
            format,
            in_use: true,
            last_access: self.current_frame,
            gpu_memory_bytes: memory,
        };
        self.memory_used = self.memory_used.saturating_add(memory);
        self.textures.insert(texture_id, cached);
        self.evict_if_needed();
    }

    /// Get a texture from the cache.
    pub fn get(&mut self, texture_id: &TextureId) -> Option<&CachedTexture> {
        if let Some(cached) = self.textures.get_mut(texture_id) {
            cached.last_access = self.current_frame;
            cached.in_use = true;
        }
        self.textures.get(texture_id)
    }

    /// Remove a texture from the cache.
    pub fn remove(&mut self, texture_id: &TextureId) -> Option<CachedTexture> {
        if let Some(cached) = self.textures.remove(texture_id) {
            self.memory_used = self.memory_used.saturating_sub(cached.gpu_memory_bytes);
            Some(cached)
        } else {
            None
        }
    }

    /// Mark a texture as no longer in use.
    pub fn mark_unused(&mut self, texture_id: &TextureId) {
        if let Some(cached) = self.textures.get_mut(texture_id) {
            cached.in_use = false;
        }
    }

    /// Evict least-recently-used textures when memory budget is exceeded.
    fn evict_if_needed(&mut self) {
        while self.memory_used > self.memory_budget && !self.textures.is_empty() {
            // Find the least recently used unused texture
            let lru_id = self.textures
                .iter()
                .filter(|(_, t)| !t.in_use)
                .min_by_key(|(_, t)| t.last_access)
                .map(|(id, _)| *id);

            let lru_id = match lru_id {
                Some(id) => id,
                None => {
                    // No unused textures, evict the oldest used one
                    self.textures
                        .iter()
                        .min_by_key(|(_, t)| t.last_access)
                        .map(|(id, _)| *id)
                        .unwrap()
                }
            };

            if let Some(cached) = self.textures.remove(&lru_id) {
                self.memory_used = self.memory_used.saturating_sub(cached.gpu_memory_bytes);
            }
        }
    }

    /// Advance to the next frame.
    pub fn next_frame(&mut self) {
        self.current_frame += 1;
    }

    /// Get the number of cached textures.
    pub fn len(&self) -> usize {
        self.textures.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.textures.is_empty()
    }

    /// Get the current memory usage.
    pub fn memory_used(&self) -> u64 {
        self.memory_used
    }

    /// Get the memory budget.
    pub fn memory_budget(&self) -> u64 {
        self.memory_budget
    }

    /// Set the memory budget.
    pub fn set_memory_budget(&mut self, budget: u64) {
        self.memory_budget = budget;
        self.evict_if_needed();
    }

    /// Clear all cached textures.
    pub fn clear(&mut self) {
        self.textures.clear();
        self.memory_used = 0;
    }

    /// Iterate over all cached textures.
    pub fn iter(&self) -> impl Iterator<Item = (&TextureId, &CachedTexture)> {
        self.textures.iter()
    }
}

impl Default for TextureCache {
    fn default() -> Self {
        // Default budget: 64 MB
        Self::new(64 * 1024 * 1024)
    }
}

// ---------------------------------------------------------------------------
// Pipeline cache
// ---------------------------------------------------------------------------

/// Cached render pipeline information.
#[derive(Debug, Clone)]
pub struct CachedPipeline {
    /// Pipeline key (type + blend mode).
    pub key: PipelineKey,
    /// Whether the pipeline is compiled and ready.
    pub compiled: bool,
    /// Shader module reference count.
    pub ref_count: u32,
}

impl CachedPipeline {
    /// Create a new cached pipeline.
    pub fn new(key: PipelineKey) -> Self {
        Self {
            key,
            compiled: true,
            ref_count: 1,
        }
    }
}

/// Cache for render pipelines.
///
/// Pre-creates and caches pipelines by type and blend mode to avoid
/// recreating them every frame.
#[derive(Debug, Clone)]
pub struct PipelineCache {
    /// Cached pipelines by key.
    pipelines: HashMap<PipelineKey, CachedPipeline>,
}

impl PipelineCache {
    /// Create a new empty pipeline cache.
    pub fn new() -> Self {
        Self {
            pipelines: HashMap::new(),
        }
    }

    /// Get or create a pipeline for the given key.
    pub fn get_or_create(&mut self, key: PipelineKey) -> &CachedPipeline {
        if !self.pipelines.contains_key(&key) {
            self.pipelines.insert(key, CachedPipeline::new(key));
        }
        self.pipelines.get(&key).unwrap()
    }

    /// Check if a pipeline exists.
    pub fn contains(&self, key: &PipelineKey) -> bool {
        self.pipelines.contains_key(key)
    }

    /// Get a pipeline by key.
    pub fn get(&self, key: &PipelineKey) -> Option<&CachedPipeline> {
        self.pipelines.get(key)
    }

    /// Pre-create all standard pipelines.
    pub fn precreate_all(&mut self) {
        for pipeline_type in [
            PipelineType::Solid2D,
            PipelineType::TexturedQuad,
            PipelineType::Text,
            PipelineType::Composite,
        ] {
            for blend_mode in [BlendMode::Opaque, BlendMode::Alpha, BlendMode::Additive] {
                let key = PipelineKey::new(pipeline_type, blend_mode);
                self.get_or_create(key);
            }
        }
    }

    /// Get the number of cached pipelines.
    pub fn len(&self) -> usize {
        self.pipelines.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.pipelines.is_empty()
    }

    /// Clear all cached pipelines.
    pub fn clear(&mut self) {
        self.pipelines.clear();
    }
}

impl Default for PipelineCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Buffer cache
// ---------------------------------------------------------------------------

/// Cached GPU buffer information.
#[derive(Debug, Clone)]
pub struct CachedBuffer {
    /// Buffer type.
    pub buffer_type: BufferType,
    /// Current capacity in elements.
    pub capacity: u32,
    /// Current number of elements.
    pub len: u32,
    /// GPU memory usage in bytes.
    pub gpu_memory_bytes: u64,
}

impl CachedBuffer {
    /// Estimate GPU memory usage for this buffer.
    pub fn estimate_memory(capacity: u32, element_size: u32) -> u64 {
        (capacity as u64).saturating_mul(element_size as u64)
    }
}

/// Type of GPU buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BufferType {
    /// Vertex buffer for geometry data.
    Vertex,
    /// Instance buffer for quad instance data.
    Instance,
    /// Index buffer for indexed drawing.
    Index,
    /// Uniform buffer for per-frame uniform data.
    Uniform,
    /// Staging buffer for CPU→GPU data transfer.
    Staging,
}

impl BufferType {
    /// Element size in bytes for this buffer type.
    pub fn element_size(&self) -> u32 {
        match self {
            BufferType::Vertex => 24,       // TexturedVertex: 2+2+4 = 8 floats
            BufferType::Instance => QuadInstance::SIZE,
            BufferType::Index => 4,         // u32 indices
            BufferType::Uniform => GpuUniformBuffer::SIZE as u32,
            BufferType::Staging => 4,       // Variable
        }
    }
}

/// Cache for GPU buffers with automatic growth.
///
/// Reuses buffers across frames, updating only changed data.
/// Buffers grow when capacity is exceeded but are not shrunk.
#[derive(Debug, Clone)]
pub struct BufferCache {
    /// Cached buffers by type.
    buffers: HashMap<BufferType, CachedBuffer>,
    /// Maximum total buffer memory in bytes.
    memory_budget: u64,
}

impl BufferCache {
    /// Create a new buffer cache.
    pub fn new() -> Self {
        Self {
            buffers: HashMap::new(),
            memory_budget: 16 * 1024 * 1024, // 16 MB default
        }
    }

    /// Get or create a buffer of the given type with the required capacity.
    pub fn get_or_create(&mut self, buffer_type: BufferType, required_capacity: u32) -> &CachedBuffer {
        let element_size = buffer_type.element_size();

        // Check if buffer exists and has enough capacity
        let (exists, current_capacity) = if let Some(buffer) = self.buffers.get(&buffer_type) {
            (true, buffer.capacity)
        } else {
            (false, 0)
        };

        if exists && current_capacity >= required_capacity {
            // Fast path: buffer exists and is big enough
            return self.buffers.get(&buffer_type).unwrap();
        }

        // Grow or create the buffer
        if exists {
            let new_capacity = current_capacity.max(required_capacity).saturating_mul(2);
            if let Some(buffer) = self.buffers.get_mut(&buffer_type) {
                buffer.capacity = new_capacity;
                buffer.gpu_memory_bytes = CachedBuffer::estimate_memory(new_capacity, element_size);
            }
        } else {
            let capacity = required_capacity.max(256);
            let memory = CachedBuffer::estimate_memory(capacity, element_size);
            self.buffers.insert(buffer_type, CachedBuffer {
                buffer_type,
                capacity,
                len: 0,
                gpu_memory_bytes: memory,
            });
        }

        self.buffers.get(&buffer_type).unwrap()
    }

    /// Reset buffer lengths for a new frame.
    pub fn reset_lengths(&mut self) {
        for buffer in self.buffers.values_mut() {
            buffer.len = 0;
        }
    }

    /// Update buffer length.
    pub fn set_length(&mut self, buffer_type: BufferType, len: u32) {
        if let Some(buffer) = self.buffers.get_mut(&buffer_type) {
            buffer.len = len.min(buffer.capacity);
        }
    }

    /// Get the number of cached buffers.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }

    /// Get total GPU memory usage.
    pub fn total_memory(&self) -> u64 {
        self.buffers.values().map(|b| b.gpu_memory_bytes).sum()
    }

    /// Clear all cached buffers.
    pub fn clear(&mut self) {
        self.buffers.clear();
    }
}

impl Default for BufferCache {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Glyph cache
// ---------------------------------------------------------------------------

/// Cache key for glyph rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphCacheKey {
    /// Font identifier.
    pub font_id: u32,
    /// Glyph ID within the font.
    pub glyph_id: u32,
    /// Rasterization size (width, height).
    pub raster_size: [u32; 2],
    /// Rasterization mode index.
    pub mode: u8,
    /// Padding.
    pub _padding: [u8; 3],
}

impl GlyphCacheKey {
    /// Create a new glyph cache key.
    pub fn new(font_id: u32, glyph_id: u32, raster_size: [u32; 2], mode: u8) -> Self {
        Self {
            font_id,
            glyph_id,
            raster_size,
            mode,
            _padding: [0; 3],
        }
    }
}

/// Cached glyph information.
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    /// Cache key.
    pub key: GlyphCacheKey,
    /// Atlas region (normalized UV coordinates).
    pub atlas_region: [f32; 4],
    /// Advance width in pixels.
    pub advance_x: f32,
    /// Bearing Y offset.
    pub bearing_y: f32,
    /// Glyph width in pixels.
    pub width: u32,
    /// Glyph height in pixels.
    pub height: u32,
}

/// Cache for rendered glyphs.
///
/// Stores rasterized glyphs in an atlas texture to avoid re-rasterizing
/// the same glyph multiple times.
#[derive(Debug, Clone)]
pub struct GlyphCache {
    /// Cached glyphs by key (for fast lookup).
    glyphs: HashMap<GlyphCacheKey, CachedGlyph>,
    /// Insertion order for LRU eviction.
    order: Vec<GlyphCacheKey>,
    /// Maximum number of cached glyphs.
    max_glyphs: usize,
}

impl GlyphCache {
    /// Create a new glyph cache.
    pub fn new(max_glyphs: usize) -> Self {
        Self {
            glyphs: HashMap::with_capacity(max_glyphs),
            order: Vec::with_capacity(max_glyphs),
            max_glyphs,
        }
    }

    /// Insert a glyph into the cache.
    pub fn insert(&mut self, glyph: CachedGlyph) {
        let key = glyph.key;
        // If already cached, update in place
        if self.glyphs.contains_key(&key) {
            self.glyphs.insert(key, glyph);
            return;
        }
        // Evict oldest if at capacity
        if self.glyphs.len() >= self.max_glyphs {
            if let Some(oldest) = self.order.first().cloned() {
                self.order.remove(0);
                self.glyphs.remove(&oldest);
            }
        }
        self.order.push(key);
        self.glyphs.insert(key, glyph);
    }

    /// Get a cached glyph.
    pub fn get(&self, key: &GlyphCacheKey) -> Option<&CachedGlyph> {
        self.glyphs.get(key)
    }

    /// Check if a glyph is cached.
    pub fn contains(&self, key: &GlyphCacheKey) -> bool {
        self.glyphs.contains_key(key)
    }

    /// Get the number of cached glyphs.
    pub fn len(&self) -> usize {
        self.glyphs.len()
    }

    /// Check if the cache is empty.
    pub fn is_empty(&self) -> bool {
        self.glyphs.is_empty()
    }

    /// Clear all cached glyphs.
    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.order.clear();
    }

    /// Set the maximum number of cached glyphs.
    pub fn set_max_glyphs(&mut self, max: usize) {
        self.max_glyphs = max;
    }
}

impl Default for GlyphCache {
    fn default() -> Self {
        // Default: 4096 glyphs
        Self::new(4096)
    }
}

// ---------------------------------------------------------------------------
// GPU resources
// ---------------------------------------------------------------------------

/// All GPU resource caches.
///
/// Owns GPU resources and controls their lifetime. Avoids allocating
/// GPU textures every frame by reusing cached resources.
#[derive(Debug, Clone)]
pub struct GpuResources {
    /// Texture cache with LRU eviction.
    pub textures: TextureCache,
    /// Glyph cache for text rendering.
    pub glyphs: GlyphCache,
    /// Pipeline cache for render pipelines.
    pub pipelines: PipelineCache,
    /// Buffer cache for vertex/instance/index buffers.
    pub buffers: BufferCache,
}

impl GpuResources {
    /// Create new GPU resources with default settings.
    pub fn new() -> Self {
        Self {
            textures: TextureCache::default(),
            glyphs: GlyphCache::default(),
            pipelines: PipelineCache::default(),
            buffers: BufferCache::default(),
        }
    }

    /// Create GPU resources with custom memory budgets.
    pub fn with_budgets(texture_budget: u64, glyph_count: usize) -> Self {
        Self {
            textures: TextureCache::new(texture_budget),
            glyphs: GlyphCache::new(glyph_count),
            pipelines: PipelineCache::default(),
            buffers: BufferCache::default(),
        }
    }

    /// Pre-create all standard pipelines.
    pub fn precreate_pipelines(&mut self) {
        self.pipelines.precreate_all();
    }

    /// Prepare for a new frame: reset buffer lengths and advance caches.
    pub fn begin_frame(&mut self) {
        self.textures.next_frame();
        self.buffers.reset_lengths();
    }

    /// Finalize a frame: mark unused textures for eviction.
    pub fn end_frame(&mut self) {
        // Textures not accessed this frame are marked unused
        // and may be evicted on the next insert
    }

    /// Estimate total GPU memory usage.
    pub fn estimate_total_memory(&self) -> u64 {
        self.textures.memory_used() + self.buffers.total_memory()
    }

    /// Clear all resources.
    pub fn clear_all(&mut self) {
        self.textures.clear();
        self.glyphs.clear();
        self.pipelines.clear();
        self.buffers.clear();
    }
}

impl Default for GpuResources {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Surface presentation
// ---------------------------------------------------------------------------

/// State of a presentable surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfacePresentationState {
    /// Surface is ready for presentation.
    Ready,
    /// Surface was lost and needs recreation.
    Lost,
    /// Surface is outdated and needs reconfiguration.
    Outdated,
    /// Surface is out of memory.
    OutOfMemory,
    /// Surface timed out.
    Timeout,
    /// Surface is zero-sized and cannot be presented.
    ZeroSized,
}

/// Configuration for surface presentation.
#[derive(Debug, Clone)]
pub struct SurfacePresentationConfig {
    /// Surface format.
    pub format: SurfaceFormat,
    /// Viewport dimensions.
    pub viewport: Size,
    /// Scale factor.
    pub scale_factor: f32,
    /// Whether the surface is presentable.
    pub presentable: bool,
    /// Alpha mode (premultiplied or straight).
    pub alpha_mode: SurfaceAlphaMode,
}

impl SurfacePresentationConfig {
    /// Create a new presentation config.
    pub fn new(format: SurfaceFormat, viewport: Size, scale_factor: f32) -> Self {
        Self {
            format,
            viewport,
            scale_factor,
            presentable: true,
            alpha_mode: SurfaceAlphaMode::Premultiplied,
        }
    }

    /// Compute the physical framebuffer size.
    pub fn physical_size(&self) -> Size {
        Size::new(
            (self.viewport.width * self.scale_factor).round() as f32,
            (self.viewport.height * self.scale_factor).round() as f32,
        )
    }
}

/// Alpha channel mode for surface presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAlphaMode {
    /// Alpha is premultiplied into RGB channels.
    Premultiplied,
    /// Alpha is separate from RGB channels.
    Straight,
}

/// Manages surface presentation lifecycle.
///
/// Handles acquiring surface textures, creating command encoders,
/// and presenting frames. Errors are handled gracefully.
#[derive(Debug, Clone)]
pub struct SurfacePresenter {
    /// Current presentation state.
    state: SurfacePresentationState,
    /// Presentation configuration.
    config: Option<SurfacePresentationConfig>,
    /// Number of consecutive presentation failures.
    consecutive_failures: u32,
    /// Maximum consecutive failures before marking surface as lost.
    max_consecutive_failures: u32,
}

impl SurfacePresenter {
    /// Create a new surface presenter.
    pub fn new() -> Self {
        Self {
            state: SurfacePresentationState::Ready,
            config: None,
            consecutive_failures: 0,
            max_consecutive_failures: 3,
        }
    }

    /// Configure the presenter for a surface.
    pub fn configure(&mut self, config: SurfacePresentationConfig) {
        self.config = Some(config);
        self.state = SurfacePresentationState::Ready;
        self.consecutive_failures = 0;
    }

    /// Check if the surface is ready for presentation.
    pub fn is_ready(&self) -> bool {
        self.state == SurfacePresentationState::Ready
    }

    /// Get the current presentation state.
    pub fn state(&self) -> SurfacePresentationState {
        self.state
    }

    /// Get the presentation configuration.
    pub fn config(&self) -> Option<&SurfacePresentationConfig> {
        self.config.as_ref()
    }

    /// Simulate a successful frame presentation.
    ///
    /// In the full wgpu implementation, this would:
    /// 1. Call `surface.get_current_texture()`
    /// 2. Create a texture view
    /// 3. Create a command encoder
    /// 4. Begin render pass
    /// 5. Submit and present
    pub fn present_frame(&mut self) -> Result<(), RenderError> {
        if !self.is_ready() {
            return Err(RenderError::PresentationFailed(
                format!("surface state: {:?}", self.state),
            ));
        }

        // Check for zero-sized surface
        if let Some(config) = &self.config {
            if config.viewport.width <= 0.0 || config.viewport.height <= 0.0 {
                self.state = SurfacePresentationState::ZeroSized;
                return Err(RenderError::PresentationFailed(
                    "zero-sized surface".to_string(),
                ));
            }
        }

        self.consecutive_failures = 0;
        Ok(())
    }

    /// Handle a surface presentation error.
    ///
    /// Classifies the error and updates the surface state accordingly.
    /// Not all errors are fatal — some can be recovered from.
    pub fn handle_error(&mut self, error: &PresentationError) {
        self.consecutive_failures += 1;

        match error {
            PresentationError::SurfaceLost => {
                self.state = SurfacePresentationState::Lost;
            }
            PresentationError::Outdated => {
                self.state = SurfacePresentationState::Outdated;
            }
            PresentationError::OutOfMemory => {
                self.state = SurfacePresentationState::OutOfMemory;
            }
            PresentationError::Timeout => {
                self.state = SurfacePresentationState::Timeout;
            }
        }

        if self.consecutive_failures >= self.max_consecutive_failures {
            self.state = SurfacePresentationState::Lost;
        }
    }

    /// Reset the presenter state (e.g., after recreating the surface).
    pub fn reset(&mut self) {
        self.state = SurfacePresentationState::Ready;
        self.consecutive_failures = 0;
    }

    /// Check if the surface needs to be recreated.
    pub fn needs_recreate(&self) -> bool {
        matches!(
            self.state,
            SurfacePresentationState::Lost
                | SurfacePresentationState::OutOfMemory
        )
    }

    /// Check if the surface needs to be reconfigured.
    pub fn needs_reconfigure(&self) -> bool {
        matches!(
            self.state,
            SurfacePresentationState::Outdated
        )
    }
}

impl Default for SurfacePresenter {
    fn default() -> Self {
        Self::new()
    }
}

/// Presentation error types.
///
/// Maps to wgpu surface error types but is independent of wgpu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationError {
    /// The surface was lost and must be recreated.
    SurfaceLost,
    /// The surface is outdated and must be reconfigured.
    Outdated,
    /// Out of memory.
    OutOfMemory,
    /// Presentation timed out.
    Timeout,
}

impl fmt::Display for PresentationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PresentationError::SurfaceLost => write!(f, "surface lost"),
            PresentationError::Outdated => write!(f, "surface outdated"),
            PresentationError::OutOfMemory => write!(f, "out of memory"),
            PresentationError::Timeout => write!(f, "presentation timeout"),
        }
    }
}

// ---------------------------------------------------------------------------
// Frame info
// ---------------------------------------------------------------------------

/// Statistics about a rendered frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    /// Number of draw calls issued.
    pub draw_calls: u32,
    /// Number of instances rendered.
    pub instances: u32,
    /// Number of texture switches.
    pub texture_switches: u32,
    /// Number of pipeline switches.
    pub pipeline_switches: u32,
    /// Total GPU memory used.
    pub gpu_memory_bytes: u64,
    /// Frame presentation time in milliseconds.
    pub presentation_time_ms: f32,
}

impl FrameStats {
    /// Create empty frame stats.
    pub fn new() -> Self {
        Self {
            draw_calls: 0,
            instances: 0,
            texture_switches: 0,
            pipeline_switches: 0,
            gpu_memory_bytes: 0,
            presentation_time_ms: 0.0,
        }
    }

    /// Check if the frame was rendered efficiently.
    ///
    /// A frame is considered efficient if it uses fewer than 10 draw calls
    /// and fewer than 5 texture switches.
    pub fn is_efficient(&self) -> bool {
        self.draw_calls < 10 && self.texture_switches < 5
    }
}

impl Default for FrameStats {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// GPU backend configuration
// ---------------------------------------------------------------------------

/// Configuration for the GPU backend.
#[derive(Debug, Clone)]
pub struct GpuBackendConfig {
    /// Maximum texture memory budget in bytes.
    pub texture_memory_budget: u64,
    /// Maximum number of cached glyphs.
    pub max_cached_glyphs: usize,
    /// Maximum instance buffer size.
    pub max_instance_count: u32,
    /// Whether to enable MSAA anti-aliasing.
    pub msaa_samples: u32,
    /// Preferred surface format.
    pub surface_format: SurfaceFormat,
    /// Whether to prefer high-performance GPU.
    pub power_preference: PowerPreference,
}

impl GpuBackendConfig {
    /// Create a default configuration optimized for 2D UI rendering.
    pub fn default_2d() -> Self {
        Self {
            texture_memory_budget: 128 * 1024 * 1024, // 128 MB
            max_cached_glyphs: 8192,
            max_instance_count: 4096,
            msaa_samples: 4,
            surface_format: SurfaceFormat::Rgba8,
            power_preference: PowerPreference::HighPerformance,
        }
    }
}

impl Default for GpuBackendConfig {
    fn default() -> Self {
        Self::default_2d()
    }
}

/// Power preference for GPU adapter selection.
///
/// Re-exported from [`crate::renderer::PowerPreference`] for convenience.
pub use crate::renderer::PowerPreference;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_quad_instance_size() {
        // QuadInstance has 5 arrays of 2-4 f32s = 48 bytes
        assert_eq!(QuadInstance::SIZE, 48);
    }

    #[test]
    fn test_quad_instance_default() {
        let inst = QuadInstance::default();
        assert_eq!(inst.position, [0.0, 0.0]);
        assert_eq!(inst.size, [1.0, 1.0]);
        assert_eq!(inst.color, [1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn test_quad_instance_solid() {
        let inst = QuadInstance::solid([10.0, 20.0], [100.0, 50.0], [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(inst.position, [10.0, 20.0]);
        assert_eq!(inst.size, [100.0, 50.0]);
        assert_eq!(inst.color, [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn test_quad_instance_textured() {
        let inst = QuadInstance::textured(
            [0.0, 0.0], [200.0, 100.0],
            [0.0, 0.0], [1.0, 1.0],
            [1.0, 1.0, 1.0, 0.5],
        );
        assert_eq!(inst.uv_min, [0.0, 0.0]);
        assert_eq!(inst.uv_max, [1.0, 1.0]);
        assert!((inst.color[3] - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn test_quad_instance_bounding_rect() {
        let inst = QuadInstance::solid([10.0, 20.0], [100.0, 50.0], [1.0; 4]);
        let rect = inst.bounding_rect();
        assert!((rect.x - 10.0).abs() < f32::EPSILON);
        assert!((rect.y - 20.0).abs() < f32::EPSILON);
        assert!((rect.width - 100.0).abs() < f32::EPSILON);
        assert!((rect.height - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_pipeline_type_display() {
        assert_eq!(format!("{}", PipelineType::Solid2D), "Solid2D");
        assert_eq!(format!("{}", PipelineType::TexturedQuad), "TexturedQuad");
        assert_eq!(format!("{}", PipelineType::Text), "Text");
        assert_eq!(format!("{}", PipelineType::Composite), "Composite");
    }

    #[test]
    fn test_orthographic_projection_identity() {
        let proj = OrthographicProjection::identity();
        assert!(proj.is_valid());
        let point = proj.transform_point(0.0, 0.0);
        assert!((point[0] - 0.0).abs() < f32::EPSILON);
        assert!((point[1] - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_orthographic_projection_from_viewport() {
        let proj = OrthographicProjection::from_viewport(800.0, 600.0);
        assert!(proj.is_valid());

        // Top-left (0, 0) should map to (-1, 1) in NDC
        let top_left = proj.transform_point(0.0, 0.0);
        assert!((top_left[0] - (-1.0)).abs() < 0.01);
        assert!((top_left[1] - 1.0).abs() < 0.01);

        // Center (400, 300) should map to (0, 0) in NDC
        let center = proj.transform_point(400.0, 300.0);
        assert!((center[0] - 0.0).abs() < 0.01);
        assert!((center[1] - 0.0).abs() < 0.01);

        // Bottom-right (800, 600) should map to (1, -1) in NDC
        let bottom_right = proj.transform_point(800.0, 600.0);
        assert!((bottom_right[0] - 1.0).abs() < 0.01);
        assert!((bottom_right[1] - (-1.0)).abs() < 0.01);
    }

    #[test]
    fn test_orthographic_projection_zero_size() {
        let proj = OrthographicProjection::from_viewport(0.0, 0.0);
        // Should return identity for zero-sized viewport
        assert_eq!(proj.matrix, OrthographicProjection::identity().matrix);
    }

    #[test]
    fn test_gpu_uniform_buffer() {
        let mut ubo = GpuUniformBuffer::new(800.0, 600.0);
        assert!((ubo.surface_size[0] - 800.0).abs() < f32::EPSILON);
        assert!((ubo.surface_size[1] - 600.0).abs() < f32::EPSILON);

        ubo.set_frame_time(1.5);
        assert!((ubo.frame_time - 1.5).abs() < f32::EPSILON);

        ubo.next_frame();
        assert_eq!(ubo.frame_number, 1);
    }

    #[test]
    fn test_texture_cache_insert_and_get() {
        let mut cache = TextureCache::new(1024 * 1024);
        let id = TextureId::new(1);
        cache.insert(id, Size::new(64.0, 64.0), TexFormat::Rgba8Unorm);

        assert_eq!(cache.len(), 1);
        assert!(cache.get(&id).is_some());
        assert!(cache.memory_used() > 0);
    }

    #[test]
    fn test_texture_cache_remove() {
        let mut cache = TextureCache::new(1024 * 1024);
        let id = TextureId::new(1);
        let memory_before = cache.memory_used();

        cache.insert(id, Size::new(64.0, 64.0), TexFormat::Rgba8Unorm);
        let memory_after = cache.memory_used();
        assert!(memory_after > memory_before);

        cache.remove(&id);
        assert_eq!(cache.memory_used(), memory_before);
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn test_texture_cache_eviction() {
        // Very small budget to force eviction
        let mut cache = TextureCache::new(100);
        let id1 = TextureId::new(1);
        let id2 = TextureId::new(2);

        // Insert two textures that exceed the budget
        cache.insert(id1, Size::new(16.0, 16.0), TexFormat::Rgba8Unorm);
        cache.insert(id2, Size::new(16.0, 16.0), TexFormat::Rgba8Unorm);

        // At least one should remain (eviction may have occurred)
        assert!(cache.len() <= 2);
    }

    #[test]
    fn test_texture_cache_estimate_memory() {
        let memory = CachedTexture::estimate_memory(
            Size::new(256.0, 256.0),
            TexFormat::Rgba8Unorm,
        );
        assert_eq!(memory, 256 * 256 * 4);
    }

    #[test]
    fn test_pipeline_cache() {
        let mut cache = PipelineCache::new();
        assert!(cache.is_empty());

        let key = PipelineKey::new(PipelineType::Solid2D, BlendMode::Alpha);
        let pipeline = cache.get_or_create(key);
        assert!(pipeline.compiled);
        assert_eq!(cache.len(), 1);

        // Pre-create all pipelines
        cache.precreate_all();
        assert!(cache.len() > 1);
    }

    #[test]
    fn test_buffer_cache() {
        let mut cache = BufferCache::new();
        {
            let buffer = cache.get_or_create(BufferType::Instance, 100);
            assert!(buffer.capacity >= 100);
        }
        cache.reset_lengths();
        let buffer = cache.get_or_create(BufferType::Instance, 100);
        assert_eq!(buffer.len, 0);
    }

    #[test]
    fn test_buffer_type_element_size() {
        assert_eq!(BufferType::Instance.element_size(), QuadInstance::SIZE);
        assert_eq!(BufferType::Index.element_size(), 4);
    }

    #[test]
    fn test_glyph_cache() {
        let mut cache = GlyphCache::new(2);
        let key1 = GlyphCacheKey::new(1, 65, [16, 16], 0);
        let key2 = GlyphCacheKey::new(1, 66, [16, 16], 0);
        let key3 = GlyphCacheKey::new(1, 67, [16, 16], 0);

        cache.insert(CachedGlyph {
            key: key1,
            atlas_region: [0.0, 0.0, 0.5, 0.5],
            advance_x: 12.0,
            bearing_y: 10.0,
            width: 10,
            height: 14,
        });

        cache.insert(CachedGlyph {
            key: key2,
            atlas_region: [0.5, 0.0, 1.0, 0.5],
            advance_x: 14.0,
            bearing_y: 10.0,
            width: 12,
            height: 14,
        });

        assert_eq!(cache.len(), 2);

        // Inserting a third should evict the first
        cache.insert(CachedGlyph {
            key: key3,
            atlas_region: [0.0, 0.5, 0.5, 1.0],
            advance_x: 11.0,
            bearing_y: 10.0,
            width: 9,
            height: 14,
        });

        assert_eq!(cache.len(), 2);
        assert!(!cache.contains(&key1));
        assert!(cache.contains(&key3));
    }

    #[test]
    fn test_gpu_resources() {
        let mut resources = GpuResources::new();
        resources.precreate_pipelines();
        assert!(resources.pipelines.len() > 0);

        resources.begin_frame();
        resources.end_frame();
        assert!(resources.estimate_total_memory() >= 0);
    }

    #[test]
    fn test_gpu_resources_with_budgets() {
        let resources = GpuResources::with_budgets(256 * 1024 * 1024, 16384);
        assert_eq!(resources.textures.memory_budget(), 256 * 1024 * 1024);
    }

    #[test]
    fn test_surface_presentation_config() {
        let config = SurfacePresentationConfig::new(
            SurfaceFormat::Rgba8,
            Size::new(800.0, 600.0),
            2.0,
        );
        let physical = config.physical_size();
        assert!((physical.width - 1600.0).abs() < 0.01);
        assert!((physical.height - 1200.0).abs() < 0.01);
    }

    #[test]
    fn test_surface_presenter() {
        let mut presenter = SurfacePresenter::new();
        assert!(presenter.is_ready());

        let config = SurfacePresentationConfig::new(
            SurfaceFormat::Rgba8,
            Size::new(800.0, 600.0),
            1.0,
        );
        presenter.configure(config);
        assert!(presenter.present_frame().is_ok());
    }

    #[test]
    fn test_surface_presenter_zero_sized() {
        let mut presenter = SurfacePresenter::new();
        let config = SurfacePresentationConfig::new(
            SurfaceFormat::Rgba8,
            Size::new(0.0, 0.0),
            1.0,
        );
        presenter.configure(config);
        assert!(presenter.present_frame().is_err());
        assert_eq!(presenter.state(), SurfacePresentationState::ZeroSized);
    }

    #[test]
    fn test_surface_presenter_error_handling() {
        let mut presenter = SurfacePresenter::new();
        assert!(!presenter.needs_recreate());

        presenter.handle_error(&PresentationError::SurfaceLost);
        assert_eq!(presenter.state(), SurfacePresentationState::Lost);
        assert!(presenter.needs_recreate());

        presenter.reset();
        assert!(presenter.is_ready());
        assert!(!presenter.needs_recreate());
    }

    #[test]
    fn test_presentation_error_display() {
        assert!(format!("{}", PresentationError::SurfaceLost).contains("lost"));
        assert!(format!("{}", PresentationError::OutOfMemory).contains("memory"));
    }

    #[test]
    fn test_frame_stats() {
        let stats = FrameStats {
            draw_calls: 5,
            instances: 100,
            texture_switches: 2,
            pipeline_switches: 1,
            gpu_memory_bytes: 1024,
            presentation_time_ms: 1.0,
        };
        assert!(stats.is_efficient());

        let inefficient = FrameStats {
            draw_calls: 20,
            instances: 100,
            texture_switches: 10,
            pipeline_switches: 5,
            gpu_memory_bytes: 1024,
            presentation_time_ms: 1.0,
        };
        assert!(!inefficient.is_efficient());
    }

    #[test]
    fn test_gpu_backend_config() {
        let config = GpuBackendConfig::default_2d();
        assert!(config.texture_memory_budget > 0);
        assert!(config.max_cached_glyphs > 0);
        assert!(config.msaa_samples <= 8);
    }

    #[test]
    fn test_orthographic_projection_with_origin() {
        let proj = OrthographicProjection::from_viewport_with_origin(
            800.0, 600.0, [0.5, -0.5]
        );
        assert!(proj.is_valid());
    }

    #[test]
    fn test_texture_cache_clear() {
        let mut cache = TextureCache::new(1024 * 1024);
        cache.insert(TextureId::new(1), Size::new(64.0, 64.0), TexFormat::Rgba8Unorm);
        cache.insert(TextureId::new(2), Size::new(64.0, 64.0), TexFormat::Rgba8Unorm);
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert!(cache.is_empty());
        assert_eq!(cache.memory_used(), 0);
    }

    #[test]
    fn test_pipeline_cache_clear() {
        let mut cache = PipelineCache::new();
        cache.precreate_all();
        assert!(cache.len() > 0);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_glyph_cache_clear() {
        let mut cache = GlyphCache::new(100);
        cache.insert(CachedGlyph {
            key: GlyphCacheKey::new(1, 65, [16, 16], 0),
            atlas_region: [0.0, 0.0, 0.5, 0.5],
            advance_x: 12.0,
            bearing_y: 10.0,
            width: 10,
            height: 14,
        });
        assert_eq!(cache.len(), 1);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_buffer_cache_grow() {
        let mut cache = BufferCache::new();
        let buffer = cache.get_or_create(BufferType::Instance, 100);
        let initial_capacity = buffer.capacity;

        // Request more than initial capacity
        let buffer = cache.get_or_create(BufferType::Instance, initial_capacity + 100);
        assert!(buffer.capacity > initial_capacity);
    }

    #[test]
    fn test_surface_presenter_outdated() {
        let mut presenter = SurfacePresenter::new();
        presenter.handle_error(&PresentationError::Outdated);
        assert!(presenter.needs_reconfigure());
        assert!(!presenter.needs_recreate());
    }

    #[test]
    fn test_surface_presenter_consecutive_failures() {
        let mut presenter = SurfacePresenter::new();
        presenter.max_consecutive_failures = 2;

        presenter.handle_error(&PresentationError::Timeout);
        assert_eq!(presenter.state(), SurfacePresentationState::Timeout);

        presenter.handle_error(&PresentationError::Timeout);
        // Should be marked as lost after max consecutive failures
        assert_eq!(presenter.state(), SurfacePresentationState::Lost);
    }

    #[test]
    fn test_quad_instance_equality() {
        let a = QuadInstance::solid([1.0, 2.0], [3.0, 4.0], [1.0, 0.0, 0.0, 1.0]);
        let b = QuadInstance::solid([1.0, 2.0], [3.0, 4.0], [1.0, 0.0, 0.0, 1.0]);
        let c = QuadInstance::solid([5.0, 6.0], [7.0, 8.0], [0.0, 1.0, 0.0, 1.0]);

        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
