//! Comprehensive GPU/CPU/RAM resource monitoring.
//!
//! This module provides detailed tracking of all resource consumption across
//! the rendering pipeline: GPU memory (VRAM), CPU memory (RAM), cache hit rates,
//! buffer allocation patterns, texture lifecycle, and system resource limits.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  ResourceMonitor                                             │
//! │                                                              │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
//! │  │  GPU Memory │  │  CPU Memory │  │  Cache Statistics   │  │
//! │  │  Tracker    │  │  Tracker    │  │  Collector          │  │
//! │  └──────┬──────┘  └──────┬──────┘  └──────────┬──────────┘  │
//! │         └────────────────┴─────────────────────┘             │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                        │
//! │              │  ResourceSnapshot    │                        │
//! │              │  (point-in-time)     │                        │
//! │              └──────────┬───────────┘                        │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                        │
//! │              │  ResourceHistory     │                        │
//! │              │  (time-series)       │                        │
//! │              └──────────────────────┘                        │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Integration Points
//!
//! - **gpu.rs**: TextureCache, BufferCache, GpuResources report memory usage
//! - **glyph.rs**: GlyphAtlas reports atlas memory consumption
//! - **video.rs**: VideoManager reports frame buffer memory
//! - **renderer.rs**: Renderer aggregates all resource reports
//!
//! ## Monitoring Strategy
//!
//! All monitoring is CPU-side. GPU-side monitoring (query pools, timestamps)
//! is designed for future integration via wgpu's timestamp query features.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::time::Instant;

use crate::geometry::Size;

// ---------------------------------------------------------------------------
// Memory units and formatting
// ---------------------------------------------------------------------------

/// Human-readable memory size with unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemorySize {
    /// Raw byte count.
    pub bytes: u64,
}

impl MemorySize {
    /// Create from bytes.
    pub const fn from_bytes(bytes: u64) -> Self {
        Self { bytes }
    }

    /// Create from kilobytes.
    pub const fn from_kb(kb: u64) -> Self {
        Self {
            bytes: kb.saturating_mul(1024),
        }
    }

    /// Create from megabytes.
    pub const fn from_mb(mb: u64) -> Self {
        Self {
            bytes: mb.saturating_mul(1024 * 1024),
        }
    }

    /// Create from gigabytes.
    pub const fn from_gb(gb: u64) -> Self {
        Self {
            bytes: gb.saturating_mul(1024 * 1024 * 1024),
        }
    }

    /// Get the value in kilobytes.
    pub fn kb(&self) -> f64 {
        self.bytes as f64 / 1024.0
    }

    /// Get the value in megabytes.
    pub fn mb(&self) -> f64 {
        self.bytes as f64 / (1024.0 * 1024.0)
    }

    /// Get the value in gigabytes.
    pub fn gb(&self) -> f64 {
        self.bytes as f64 / (1024.0 * 1024.0 * 1024.0)
    }

    /// Format as a human-readable string (e.g., "1.5 MB").
    pub fn display(&self) -> String {
        if self.bytes >= 1024 * 1024 * 1024 {
            format!("{:.2} GB", self.gb())
        } else if self.bytes >= 1024 * 1024 {
            format!("{:.2} MB", self.mb())
        } else if self.bytes >= 1024 {
            format!("{:.2} KB", self.kb())
        } else {
            format!("{} B", self.bytes)
        }
    }

    /// Check if this memory size is zero.
    pub fn is_zero(&self) -> bool {
        self.bytes == 0
    }

    /// Add two memory sizes, saturating on overflow.
    pub fn saturating_add(&self, other: &Self) -> Self {
        Self {
            bytes: self.bytes.saturating_add(other.bytes),
        }
    }

    /// Subtract another memory size, saturating on underflow.
    pub fn saturating_sub(&self, other: &Self) -> Self {
        Self {
            bytes: self.bytes.saturating_sub(other.bytes),
        }
    }
}

impl std::fmt::Display for MemorySize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

impl Default for MemorySize {
    fn default() -> Self {
        Self::from_bytes(0)
    }
}

// ---------------------------------------------------------------------------
// GPU Memory Tracking
// ---------------------------------------------------------------------------

/// Breakdown of GPU memory usage by category.
#[derive(Debug, Clone, Copy, Default)]
pub struct GpuMemoryBreakdown {
    /// Textures (icons, images, video frames, glyph atlases).
    pub textures: MemorySize,
    /// Vertex buffers.
    pub vertex_buffers: MemorySize,
    /// Instance buffers.
    pub instance_buffers: MemorySize,
    /// Index buffers.
    pub index_buffers: MemorySize,
    /// Uniform buffers.
    pub uniform_buffers: MemorySize,
    /// Staging buffers (CPU→GPU transfers).
    pub staging_buffers: MemorySize,
    /// Render targets and framebuffers.
    pub render_targets: MemorySize,
    /// Depth/stencil buffers.
    pub depth_stencil: MemorySize,
    /// Shader pipelines and bind groups.
    pub pipelines: MemorySize,
    /// Other GPU allocations.
    pub other: MemorySize,
}

impl GpuMemoryBreakdown {
    /// Create a new breakdown with all zeros.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute the total GPU memory usage.
    pub fn total(&self) -> MemorySize {
        self.textures
            .saturating_add(&self.vertex_buffers)
            .saturating_add(&self.instance_buffers)
            .saturating_add(&self.index_buffers)
            .saturating_add(&self.uniform_buffers)
            .saturating_add(&self.staging_buffers)
            .saturating_add(&self.render_targets)
            .saturating_add(&self.depth_stencil)
            .saturating_add(&self.pipelines)
            .saturating_add(&self.other)
    }

    /// Check if all categories are zero.
    pub fn is_empty(&self) -> bool {
        self.total().is_zero()
    }

    /// Find the largest memory category.
    pub fn largest_category(&self) -> (&'static str, MemorySize) {
        let categories = [
            ("textures", self.textures),
            ("vertex_buffers", self.vertex_buffers),
            ("instance_buffers", self.instance_buffers),
            ("index_buffers", self.index_buffers),
            ("uniform_buffers", self.uniform_buffers),
            ("staging_buffers", self.staging_buffers),
            ("render_targets", self.render_targets),
            ("depth_stencil", self.depth_stencil),
            ("pipelines", self.pipelines),
            ("other", self.other),
        ];

        categories
            .iter()
            .copied()
            .max_by_key(|(_, m)| m.bytes)
            .unwrap_or(("none", MemorySize::default()))
    }

    /// Format as a detailed report string.
    pub fn report(&self) -> String {
        let total = self.total();
        let (largest_cat, largest_mem) = self.largest_category();

        format!(
            "GPU Memory: {} total ({} = {})",
            total.display(),
            largest_mem.display(),
            largest_cat
        )
    }
}

/// GPU memory budget configuration.
#[derive(Debug, Clone, Copy)]
pub struct GpuMemoryBudget {
    /// Total VRAM budget.
    pub total: MemorySize,
    /// Texture memory budget.
    pub textures: MemorySize,
    /// Buffer memory budget.
    pub buffers: MemorySize,
    /// Render target budget.
    pub render_targets: MemorySize,
    /// Pipeline budget.
    pub pipelines: MemorySize,
}

impl GpuMemoryBudget {
    /// Create a default budget for 2D UI rendering.
    pub fn default_2d() -> Self {
        Self {
            total: MemorySize::from_mb(512),
            textures: MemorySize::from_mb(256),
            buffers: MemorySize::from_mb(64),
            render_targets: MemorySize::from_mb(128),
            pipelines: MemorySize::from_mb(32),
        }
    }

    /// Check if the given breakdown exceeds any budget.
    pub fn check_limits(&self, usage: &GpuMemoryBreakdown) -> Vec<String> {
        let mut warnings = Vec::new();

        if usage.textures.bytes > self.textures.bytes {
            warnings.push(format!(
                "Texture memory {} exceeds budget {}",
                usage.textures.display(),
                self.textures.display()
            ));
        }

        let buffer_total = usage
            .vertex_buffers
            .saturating_add(&usage.instance_buffers)
            .saturating_add(&usage.index_buffers)
            .saturating_add(&usage.uniform_buffers)
            .saturating_add(&usage.staging_buffers);

        if buffer_total.bytes > self.buffers.bytes {
            warnings.push(format!(
                "Buffer memory {} exceeds budget {}",
                buffer_total.display(),
                self.buffers.display()
            ));
        }

        if usage.render_targets.bytes > self.render_targets.bytes {
            warnings.push(format!(
                "Render target memory {} exceeds budget {}",
                usage.render_targets.display(),
                self.render_targets.display()
            ));
        }

        if usage.pipelines.bytes > self.pipelines.bytes {
            warnings.push(format!(
                "Pipeline memory {} exceeds budget {}",
                usage.pipelines.display(),
                self.pipelines.display()
            ));
        }

        let total = usage.total();
        if total.bytes > self.total.bytes {
            warnings.push(format!(
                "Total GPU memory {} exceeds budget {}",
                total.display(),
                self.total.display()
            ));
        }

        warnings
    }

    /// Get the remaining budget for a category.
    pub fn remaining(&self, usage: &GpuMemoryBreakdown) -> GpuMemoryBreakdown {
        GpuMemoryBreakdown {
            textures: self.textures.saturating_sub(&usage.textures),
            vertex_buffers: self.buffers.saturating_sub(&usage.vertex_buffers),
            instance_buffers: MemorySize::default(),
            index_buffers: MemorySize::default(),
            uniform_buffers: MemorySize::default(),
            staging_buffers: MemorySize::default(),
            render_targets: self.render_targets.saturating_sub(&usage.render_targets),
            depth_stencil: MemorySize::default(),
            pipelines: self.pipelines.saturating_sub(&usage.pipelines),
            other: MemorySize::default(),
        }
    }
}

impl Default for GpuMemoryBudget {
    fn default() -> Self {
        Self::default_2d()
    }
}

/// GPU memory usage snapshot.
#[derive(Debug, Clone, Copy)]
pub struct GpuMemoryUsage {
    /// Detailed breakdown by category.
    pub breakdown: GpuMemoryBreakdown,
    /// Total allocated GPU memory.
    pub total_allocated: MemorySize,
    /// Peak GPU memory usage since last reset.
    pub peak_usage: MemorySize,
    /// Number of active GPU allocations.
    pub allocation_count: u32,
    /// Number of GPU memory allocations this frame.
    pub allocations_this_frame: u32,
    /// Number of GPU memory deallocations this frame.
    pub deallocations_this_frame: u32,
    /// GPU memory fragmentation estimate (0.0 = none, 1.0 = severe).
    pub fragmentation_ratio: f32,
}

impl GpuMemoryUsage {
    /// Create a new usage snapshot.
    pub fn new() -> Self {
        Self {
            breakdown: GpuMemoryBreakdown::new(),
            total_allocated: MemorySize::default(),
            peak_usage: MemorySize::default(),
            allocation_count: 0,
            allocations_this_frame: 0,
            deallocations_this_frame: 0,
            fragmentation_ratio: 0.0,
        }
    }

    /// Update the peak usage if current usage exceeds it.
    pub fn update_peak(&mut self) {
        let current = self.breakdown.total();
        if current.bytes > self.peak_usage.bytes {
            self.peak_usage = current;
        }
    }

    /// Record a new GPU allocation.
    pub fn record_allocation(&mut self, size: MemorySize, category: GpuMemoryCategory) {
        self.allocations_this_frame += 1;
        self.allocation_count += 1;
        category.apply(&mut self.breakdown, size, true);
        self.total_allocated = self.breakdown.total();
        self.update_peak();
    }

    /// Record a GPU deallocation.
    pub fn record_deallocation(&mut self, size: MemorySize, category: GpuMemoryCategory) {
        self.deallocations_this_frame += 1;
        self.allocation_count = self.allocation_count.saturating_sub(1);
        category.apply(&mut self.breakdown, size, false);
        self.total_allocated = self.breakdown.total();
    }

    /// Reset per-frame counters.
    pub fn begin_frame(&mut self) {
        self.allocations_this_frame = 0;
        self.deallocations_this_frame = 0;
    }

    /// Reset peak usage tracking.
    pub fn reset_peak(&mut self) {
        self.peak_usage = self.breakdown.total();
    }

    /// Check against budget limits.
    pub fn check_budget(&self, budget: &GpuMemoryBudget) -> Vec<String> {
        budget.check_limits(&self.breakdown)
    }

    /// Format as a detailed report.
    pub fn report(&self) -> String {
        format!(
            "GPU: {} (peak {}, {} allocs, frag={:.2}%)",
            self.total_allocated.display(),
            self.peak_usage.display(),
            self.allocation_count,
            self.fragmentation_ratio * 100.0
        )
    }
}

impl Default for GpuMemoryUsage {
    fn default() -> Self {
        Self::new()
    }
}

/// GPU memory category for allocation tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuMemoryCategory {
    /// Texture memory.
    Texture,
    /// Vertex buffer.
    VertexBuffer,
    /// Instance buffer.
    InstanceBuffer,
    /// Index buffer.
    IndexBuffer,
    /// Uniform buffer.
    UniformBuffer,
    /// Staging buffer.
    StagingBuffer,
    /// Render target.
    RenderTarget,
    /// Depth/stencil buffer.
    DepthStencil,
    /// Pipeline state.
    Pipeline,
    /// Other GPU memory.
    Other,
}

impl GpuMemoryCategory {
    /// Apply a memory change to the breakdown.
    fn apply(&self, breakdown: &mut GpuMemoryBreakdown, size: MemorySize, is_allocation: bool) {
        let field = match self {
            Self::Texture => &mut breakdown.textures,
            Self::VertexBuffer => &mut breakdown.vertex_buffers,
            Self::InstanceBuffer => &mut breakdown.instance_buffers,
            Self::IndexBuffer => &mut breakdown.index_buffers,
            Self::UniformBuffer => &mut breakdown.uniform_buffers,
            Self::StagingBuffer => &mut breakdown.staging_buffers,
            Self::RenderTarget => &mut breakdown.render_targets,
            Self::DepthStencil => &mut breakdown.depth_stencil,
            Self::Pipeline => &mut breakdown.pipelines,
            Self::Other => &mut breakdown.other,
        };

        if is_allocation {
            *field = field.saturating_add(&size);
        } else {
            *field = field.saturating_sub(&size);
        }
    }
}

// ---------------------------------------------------------------------------
// CPU Memory Tracking
// ---------------------------------------------------------------------------

/// Breakdown of CPU memory (RAM) usage by category.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuMemoryBreakdown {
    /// Scene graph nodes and data.
    pub scene_graph: MemorySize,
    /// Animation state and keyframes.
    pub animations: MemorySize,
    /// Video frame buffers (CPU-side).
    pub video_frames: MemorySize,
    /// GIF decoded frames.
    pub gif_frames: MemorySize,
    /// Font data and glyph caches.
    pub font_data: MemorySize,
    /// Text shaping data.
    pub text_shaping: MemorySize,
    /// Image data (decoded, not yet uploaded to GPU).
    pub image_data: MemorySize,
    /// Wayland/Slint integration buffers.
    pub wayland_buffers: MemorySize,
    /// Command buffers and draw lists.
    pub command_buffers: MemorySize,
    /// Monitoring and profiling data.
    pub monitoring: MemorySize,
    /// Other CPU allocations.
    pub other: MemorySize,
}

impl CpuMemoryBreakdown {
    /// Create a new breakdown with all zeros.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute the total CPU memory usage.
    pub fn total(&self) -> MemorySize {
        self.scene_graph
            .saturating_add(&self.animations)
            .saturating_add(&self.video_frames)
            .saturating_add(&self.gif_frames)
            .saturating_add(&self.font_data)
            .saturating_add(&self.text_shaping)
            .saturating_add(&self.image_data)
            .saturating_add(&self.wayland_buffers)
            .saturating_add(&self.command_buffers)
            .saturating_add(&self.monitoring)
            .saturating_add(&self.other)
    }

    /// Check if all categories are zero.
    pub fn is_empty(&self) -> bool {
        self.total().is_zero()
    }

    /// Format as a report string.
    pub fn report(&self) -> String {
        let total = self.total();
        format!("CPU Memory: {} total", total.display())
    }
}

/// CPU memory usage snapshot.
#[derive(Debug, Clone, Copy)]
pub struct CpuMemoryUsage {
    /// Detailed breakdown by category.
    pub breakdown: CpuMemoryBreakdown,
    /// Total allocated CPU memory.
    pub total_allocated: MemorySize,
    /// Peak CPU memory usage since last reset.
    pub peak_usage: MemorySize,
    /// Number of active allocations.
    pub allocation_count: u32,
}

impl CpuMemoryUsage {
    /// Create a new usage snapshot.
    pub fn new() -> Self {
        Self {
            breakdown: CpuMemoryBreakdown::new(),
            total_allocated: MemorySize::default(),
            peak_usage: MemorySize::default(),
            allocation_count: 0,
        }
    }

    /// Update the peak usage if current usage exceeds it.
    pub fn update_peak(&mut self) {
        let current = self.breakdown.total();
        if current.bytes > self.peak_usage.bytes {
            self.peak_usage = current;
        }
    }

    /// Record a new CPU allocation.
    pub fn record_allocation(&mut self, size: MemorySize, category: CpuMemoryCategory) {
        self.allocation_count += 1;
        category.apply(&mut self.breakdown, size, true);
        self.total_allocated = self.breakdown.total();
        self.update_peak();
    }

    /// Record a CPU deallocation.
    pub fn record_deallocation(&mut self, size: MemorySize, category: CpuMemoryCategory) {
        self.allocation_count = self.allocation_count.saturating_sub(1);
        category.apply(&mut self.breakdown, size, false);
        self.total_allocated = self.breakdown.total();
    }

    /// Reset peak usage tracking.
    pub fn reset_peak(&mut self) {
        self.peak_usage = self.breakdown.total();
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "CPU: {} (peak {}, {} allocs)",
            self.total_allocated.display(),
            self.peak_usage.display(),
            self.allocation_count
        )
    }
}

impl Default for CpuMemoryUsage {
    fn default() -> Self {
        Self::new()
    }
}

/// CPU memory category for allocation tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuMemoryCategory {
    SceneGraph,
    Animations,
    VideoFrames,
    GifFrames,
    FontData,
    TextShaping,
    ImageData,
    WaylandBuffers,
    CommandBuffers,
    Monitoring,
    Other,
}

impl CpuMemoryCategory {
    fn apply(&self, breakdown: &mut CpuMemoryBreakdown, size: MemorySize, is_allocation: bool) {
        let field = match self {
            Self::SceneGraph => &mut breakdown.scene_graph,
            Self::Animations => &mut breakdown.animations,
            Self::VideoFrames => &mut breakdown.video_frames,
            Self::GifFrames => &mut breakdown.gif_frames,
            Self::FontData => &mut breakdown.font_data,
            Self::TextShaping => &mut breakdown.text_shaping,
            Self::ImageData => &mut breakdown.image_data,
            Self::WaylandBuffers => &mut breakdown.wayland_buffers,
            Self::CommandBuffers => &mut breakdown.command_buffers,
            Self::Monitoring => &mut breakdown.monitoring,
            Self::Other => &mut breakdown.other,
        };

        if is_allocation {
            *field = field.saturating_add(&size);
        } else {
            *field = field.saturating_sub(&size);
        }
    }
}

// ---------------------------------------------------------------------------
// Cache Statistics
// ---------------------------------------------------------------------------

/// Statistics for a single cache (texture, glyph, pipeline, buffer).
#[derive(Debug, Clone, Copy, Default)]
pub struct CacheStats {
    /// Number of cache hits.
    pub hits: u64,
    /// Number of cache misses.
    pub misses: u64,
    /// Number of evictions.
    pub evictions: u64,
    /// Number of insertions.
    pub insertions: u64,
    /// Current number of entries.
    pub entries: u32,
    /// Maximum capacity.
    pub capacity: u32,
    /// Memory used by cache entries.
    pub memory_used: MemorySize,
    /// Memory budget.
    pub memory_budget: MemorySize,
}

impl CacheStats {
    /// Create new cache stats.
    pub fn new(capacity: u32, memory_budget: MemorySize) -> Self {
        Self {
            capacity,
            memory_budget,
            ..Default::default()
        }
    }

    /// Record a cache hit.
    pub fn record_hit(&mut self) {
        self.hits += 1;
    }

    /// Record a cache miss.
    pub fn record_miss(&mut self) {
        self.misses += 1;
    }

    /// Record an eviction.
    pub fn record_eviction(&mut self) {
        self.evictions += 1;
    }

    /// Record an insertion.
    pub fn record_insertion(&mut self) {
        self.insertions += 1;
        self.entries = self.entries.saturating_add(1).min(self.capacity);
    }

    /// Compute the hit rate (0.0 to 1.0).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            1.0
        } else {
            self.hits as f64 / total as f64
        }
    }

    /// Compute the miss rate (0.0 to 1.0).
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate()
    }

    /// Compute the eviction rate (evictions per insertion).
    pub fn eviction_rate(&self) -> f64 {
        if self.insertions == 0 {
            0.0
        } else {
            self.evictions as f64 / self.insertions as f64
        }
    }

    /// Compute the fill ratio (entries / capacity).
    pub fn fill_ratio(&self) -> f64 {
        if self.capacity == 0 {
            0.0
        } else {
            self.entries as f64 / self.capacity as f64
        }
    }

    /// Compute the memory utilization ratio.
    pub fn memory_utilization(&self) -> f64 {
        if self.memory_budget.bytes == 0 {
            0.0
        } else {
            self.memory_used.bytes as f64 / self.memory_budget.bytes as f64
        }
    }

    /// Check if the cache is under pressure (high eviction rate or low hit rate).
    pub fn is_under_pressure(&self) -> bool {
        self.hit_rate() < 0.8 || self.eviction_rate() > 0.1
    }

    /// Format as a report string.
    pub fn report(&self) -> String {
        format!(
            "hits={}, misses={}, evictions={}, hitRate={:.1}%, fill={:.1}%, mem={:.1}%",
            self.hits,
            self.misses,
            self.evictions,
            self.hit_rate() * 100.0,
            self.fill_ratio() * 100.0,
            self.memory_utilization() * 100.0
        )
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.insertions = 0;
    }
}

/// Aggregate cache statistics for all caches.
#[derive(Debug, Clone)]
pub struct AggregateCacheStats {
    /// Texture cache statistics.
    pub texture_cache: CacheStats,
    /// Glyph cache statistics.
    pub glyph_cache: CacheStats,
    /// Pipeline cache statistics.
    pub pipeline_cache: CacheStats,
    /// Buffer cache statistics.
    pub buffer_cache: CacheStats,
}

impl AggregateCacheStats {
    /// Create new aggregate stats.
    pub fn new() -> Self {
        Self {
            texture_cache: CacheStats::new(0, MemorySize::from_mb(256)),
            glyph_cache: CacheStats::new(8192, MemorySize::default()),
            pipeline_cache: CacheStats::new(100, MemorySize::from_mb(32)),
            buffer_cache: CacheStats::new(0, MemorySize::from_mb(64)),
        }
    }

    /// Compute the overall hit rate across all caches.
    pub fn overall_hit_rate(&self) -> f64 {
        let total_hits = self.texture_cache.hits
            + self.glyph_cache.hits
            + self.pipeline_cache.hits
            + self.buffer_cache.hits;
        let total_misses = self.texture_cache.misses
            + self.glyph_cache.misses
            + self.pipeline_cache.misses
            + self.buffer_cache.misses;
        let total = total_hits + total_misses;
        if total == 0 {
            1.0
        } else {
            total_hits as f64 / total as f64
        }
    }

    /// Check if any cache is under pressure.
    pub fn any_under_pressure(&self) -> bool {
        self.texture_cache.is_under_pressure()
            || self.glyph_cache.is_under_pressure()
            || self.pipeline_cache.is_under_pressure()
            || self.buffer_cache.is_under_pressure()
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "Caches: overallHitRate={:.1}%, texture={}, glyph={}, pipeline={}, buffer={}",
            self.overall_hit_rate() * 100.0,
            self.texture_cache.report(),
            self.glyph_cache.report(),
            self.pipeline_cache.report(),
            self.buffer_cache.report(),
        )
    }
}

impl Default for AggregateCacheStats {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// System Resource Limits
// ---------------------------------------------------------------------------

/// Detected system resource limits.
#[derive(Debug, Clone)]
pub struct SystemResourceLimits {
    /// Total system RAM in bytes.
    pub total_ram: MemorySize,
    /// Available RAM in bytes.
    pub available_ram: MemorySize,
    /// Total GPU VRAM in bytes.
    pub total_vram: MemorySize,
    /// Available GPU VRAM in bytes.
    pub available_vram: MemorySize,
    /// Number of CPU cores.
    pub cpu_cores: u32,
    /// Number of logical CPU threads.
    pub cpu_threads: u32,
    /// GPU vendor ID.
    pub gpu_vendor_id: u32,
    /// GPU device ID.
    pub gpu_device_id: u32,
    /// GPU description string.
    pub gpu_description: String,
    /// Maximum texture dimension supported.
    pub max_texture_dimension: u32,
    /// Maximum buffer size supported.
    pub max_buffer_size: MemorySize,
    /// Maximum number of viewports.
    pub max_viewports: u32,
}

impl SystemResourceLimits {
    /// Try to detect system limits from /proc and sysfs.
    ///
    /// Returns a best-effort estimate. On failure, returns defaults.
    pub fn detect() -> Self {
        let (total_ram, available_ram) = Self::read_ram_info();
        let cpu_cores = Self::read_cpu_cores();
        let cpu_threads = Self::read_cpu_threads();

        Self {
            total_ram,
            available_ram,
            total_vram: MemorySize::from_mb(4096), // Default estimate
            available_vram: MemorySize::from_mb(4096),
            cpu_cores,
            cpu_threads,
            gpu_vendor_id: 0,
            gpu_device_id: 0,
            gpu_description: String::from("Unknown GPU"),
            max_texture_dimension: 8192,
            max_buffer_size: MemorySize::from_gb(4),
            max_viewports: 8,
        }
    }

    /// Read RAM information from /proc/meminfo.
    fn read_ram_info() -> (MemorySize, MemorySize) {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo") {
            let mut total_kb: u64 = 0;
            let mut available_kb: u64 = 0;

            for line in content.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let kb: u64 = parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
                    match parts[0] {
                        "MemTotal:" => total_kb = kb,
                        "MemAvailable:" => available_kb = kb,
                        _ => {}
                    }
                }
            }

            if total_kb > 0 {
                return (
                    MemorySize::from_kb(total_kb),
                    MemorySize::from_kb(available_kb.max(total_kb / 4)),
                );
            }
        }

        // Fallback: estimate 8 GB total, 4 GB available
        (MemorySize::from_gb(8), MemorySize::from_gb(4))
    }

    /// Read CPU core count from /proc/cpuinfo.
    fn read_cpu_cores() -> u32 {
        if let Ok(content) = std::fs::read_to_string("/proc/cpuinfo") {
            let cores = content
                .lines()
                .filter(|l| l.starts_with("processor"))
                .count();
            return cores.max(1) as u32;
        }
        1
    }

    /// Read CPU thread count from sysfs.
    fn read_cpu_threads() -> u32 {
        if let Ok(content) = std::fs::read_to_string("/sys/devices/system/cpu/online") {
            // Format: "0-7" or "0,1,2,3"
            let threads = content
                .split('-')
                .last()
                .and_then(|s| s.parse::<u32>().ok())
                .map(|n| n + 1)
                .unwrap_or(1);
            return threads.max(1);
        }
        1
    }

    /// Compute the recommended texture budget based on available VRAM.
    pub fn recommended_texture_budget(&self) -> MemorySize {
        // Use 50% of available VRAM for textures, capped at 512 MB for 2D UI
        let half_vram = self.available_vram.bytes / 2;
        let capped = half_vram.min(MemorySize::from_mb(512).bytes);
        MemorySize::from_bytes(capped)
    }

    /// Compute the recommended buffer budget.
    pub fn recommended_buffer_budget(&self) -> MemorySize {
        // Use 10% of available VRAM for buffers, capped at 64 MB
        let tenth_vram = self.available_vram.bytes / 10;
        let capped = tenth_vram.min(MemorySize::from_mb(64).bytes);
        MemorySize::from_bytes(capped)
    }

    /// Check if a texture size is supported.
    pub fn supports_texture_size(&self, size: Size) -> bool {
        size.width as u32 <= self.max_texture_dimension
            && size.height as u32 <= self.max_texture_dimension
    }

    /// Check if a buffer size is supported.
    pub fn supports_buffer_size(&self, size: MemorySize) -> bool {
        size.bytes <= self.max_buffer_size.bytes
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "System: {} cores/{} threads, RAM {}/{} VRAM {}/{} GPU: {}",
            self.cpu_cores,
            self.cpu_threads,
            self.available_ram.display(),
            self.total_ram.display(),
            self.available_vram.display(),
            self.total_vram.display(),
            self.gpu_description
        )
    }
}

impl Default for SystemResourceLimits {
    fn default() -> Self {
        Self::detect()
    }
}

// ---------------------------------------------------------------------------
// Resource Snapshot
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of all resource usage.
#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    /// When this snapshot was taken.
    pub timestamp: Instant,
    /// GPU memory usage.
    pub gpu_memory: GpuMemoryUsage,
    /// CPU memory usage.
    pub cpu_memory: CpuMemoryUsage,
    /// Cache statistics.
    pub cache_stats: AggregateCacheStats,
    /// Frame rendering statistics.
    pub frame_stats: FrameRenderStats,
    /// System resource limits.
    pub system_limits: SystemResourceLimits,
}

impl ResourceSnapshot {
    /// Create a new snapshot.
    pub fn new() -> Self {
        Self {
            timestamp: Instant::now(),
            gpu_memory: GpuMemoryUsage::new(),
            cpu_memory: CpuMemoryUsage::new(),
            cache_stats: AggregateCacheStats::new(),
            frame_stats: FrameRenderStats::new(),
            system_limits: SystemResourceLimits::detect(),
        }
    }

    /// Compute the total memory usage (GPU + CPU).
    pub fn total_memory(&self) -> MemorySize {
        self.gpu_memory
            .total_allocated
            .saturating_add(&self.cpu_memory.total_allocated)
    }

    /// Check for any budget violations.
    pub fn check_budgets(&self, gpu_budget: &GpuMemoryBudget) -> Vec<String> {
        self.gpu_memory.check_budget(gpu_budget)
    }

    /// Check overall health of the resource usage.
    pub fn health_check(&self) -> ResourceHealth {
        let mut health = ResourceHealth::Healthy;

        // Check GPU memory pressure
        let gpu_ratio = if self.system_limits.total_vram.bytes > 0 {
            self.gpu_memory.total_allocated.bytes as f64
                / self.system_limits.total_vram.bytes as f64
        } else {
            0.0
        };

        if gpu_ratio > 0.9 {
            health = ResourceHealth::Critical;
        } else if gpu_ratio > 0.7 {
            health = health.worse(ResourceHealth::Warning);
        }

        // Check CPU memory pressure
        let cpu_ratio = if self.system_limits.total_ram.bytes > 0 {
            self.cpu_memory.total_allocated.bytes as f64 / self.system_limits.total_ram.bytes as f64
        } else {
            0.0
        };

        if cpu_ratio > 0.9 {
            health = ResourceHealth::Critical;
        } else if cpu_ratio > 0.7 {
            health = health.worse(ResourceHealth::Warning);
        }

        // Check cache pressure
        if self.cache_stats.any_under_pressure() {
            health = health.worse(ResourceHealth::Warning);
        }

        // Check frame rate
        if self.frame_stats.frame_time_ms > 33.34 {
            // Below 30fps
            health = health.worse(ResourceHealth::Warning);
        }

        health
    }

    /// Format as a comprehensive report.
    pub fn report(&self) -> String {
        format!(
            "=== Resource Snapshot ===\n{}\n{}\n{}\n{}\n{}",
            self.gpu_memory.report(),
            self.cpu_memory.report(),
            self.cache_stats.report(),
            self.frame_stats.report(),
            self.system_limits.report()
        )
    }
}

impl Default for ResourceSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Overall resource health status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceHealth {
    /// All resources are within normal limits.
    Healthy,
    /// Some resources are approaching limits.
    Warning,
    /// Resources are critically low; action required.
    Critical,
}

impl ResourceHealth {
    /// Return the worse of two health statuses.
    pub fn worse(self, other: Self) -> Self {
        match (self, other) {
            (_, ResourceHealth::Critical) => ResourceHealth::Critical,
            (_, ResourceHealth::Warning) => ResourceHealth::Warning,
            (ResourceHealth::Warning, _) => ResourceHealth::Warning,
            _ => ResourceHealth::Healthy,
        }
    }
}

// ---------------------------------------------------------------------------
// Frame Render Statistics
// ---------------------------------------------------------------------------

/// Detailed frame rendering statistics.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameRenderStats {
    /// Frame rendering time in milliseconds.
    pub frame_time_ms: f32,
    /// GPU command encoding time in milliseconds.
    pub encode_time_ms: f32,
    /// GPU command submission time in milliseconds.
    pub submit_time_ms: f32,
    /// Frame presentation time in milliseconds.
    pub present_time_ms: f32,
    /// CPU-side preparation time in milliseconds.
    pub cpu_prep_time_ms: f32,
    /// Number of draw calls.
    pub draw_calls: u32,
    /// Number of instances.
    pub instances: u32,
    /// Number of texture switches.
    pub texture_switches: u32,
    /// Number of pipeline switches.
    pub pipeline_switches: u32,
    /// Number of visible nodes.
    pub visible_nodes: u32,
    /// Number of batches.
    pub batches: u32,
    /// Number of clip regions.
    pub clip_regions: u32,
    /// Current frames per second.
    pub fps: f32,
    /// Minimum FPS observed.
    pub min_fps: f32,
    /// Maximum FPS observed.
    pub max_fps: f32,
    /// Average FPS (exponential moving average).
    pub avg_fps: f32,
    /// Frame number.
    pub frame_number: u64,
}

impl FrameRenderStats {
    /// Create new frame stats.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update FPS tracking with a new frame time.
    pub fn update_fps(&mut self, frame_time_ms: f32) {
        let fps = if frame_time_ms > 0.0 {
            1000.0 / frame_time_ms
        } else {
            0.0
        };

        self.fps = fps;
        self.frame_time_ms = frame_time_ms;

        if self.frame_number == 0 || fps < self.min_fps {
            self.min_fps = fps;
        }
        if fps > self.max_fps {
            self.max_fps = fps;
        }

        // Exponential moving average (alpha = 0.1)
        let alpha = 0.1;
        if self.frame_number == 0 {
            self.avg_fps = fps;
        } else {
            self.avg_fps = alpha * fps + (1.0 - alpha) * self.avg_fps;
        }

        self.frame_number += 1;
    }

    /// Check if the frame rate is stable (within 10% of average).
    pub fn is_stable(&self) -> bool {
        if self.avg_fps <= 0.0 {
            return true;
        }
        let ratio = self.fps / self.avg_fps;
        ratio > 0.9 && ratio < 1.1
    }

    /// Check if the frame rate is acceptable (>= 30fps average).
    pub fn is_acceptable(&self) -> bool {
        self.avg_fps >= 30.0
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "Frame: {:.2}ms, {:.1}fps (avg {:.1}), {} draws, {} instances, {} batches",
            self.frame_time_ms,
            self.fps,
            self.avg_fps,
            self.draw_calls,
            self.instances,
            self.batches
        )
    }

    /// Reset all counters.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

// ---------------------------------------------------------------------------
// Resource History (Time Series)
// ---------------------------------------------------------------------------

/// A single history entry with a timestamp.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// When this entry was recorded.
    pub timestamp: Instant,
    /// GPU memory at this point.
    pub gpu_memory_bytes: u64,
    /// CPU memory at this point.
    pub cpu_memory_bytes: u64,
    /// Frame time in milliseconds.
    pub frame_time_ms: f32,
    /// FPS at this point.
    pub fps: f32,
    /// Draw calls at this point.
    pub draw_calls: u32,
}

/// Time-series history of resource usage.
#[derive(Debug, Clone)]
pub struct ResourceHistory {
    /// History entries (oldest first).
    entries: RefCell<VecDeque<HistoryEntry>>,
    /// Maximum number of entries to retain.
    max_entries: usize,
}

impl ResourceHistory {
    /// Create a new history with the given max entries.
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: RefCell::new(VecDeque::with_capacity(max_entries)),
            max_entries,
        }
    }

    /// Record a new history entry from a snapshot.
    pub fn record(&self, snapshot: &ResourceSnapshot) {
        let mut entries = self.entries.borrow_mut();
        let entry = HistoryEntry {
            timestamp: snapshot.timestamp,
            gpu_memory_bytes: snapshot.gpu_memory.total_allocated.bytes,
            cpu_memory_bytes: snapshot.cpu_memory.total_allocated.bytes,
            frame_time_ms: snapshot.frame_stats.frame_time_ms,
            fps: snapshot.frame_stats.fps,
            draw_calls: snapshot.frame_stats.draw_calls,
        };

        entries.push_back(entry);
        while entries.len() > self.max_entries {
            entries.pop_front();
        }
    }

    /// Get the number of entries.
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Check if the history is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.borrow().is_empty()
    }

    /// Get the latest entry.
    pub fn latest(&self) -> Option<HistoryEntry> {
        self.entries.borrow().back().cloned()
    }

    /// Get the oldest entry.
    pub fn oldest(&self) -> Option<HistoryEntry> {
        self.entries.borrow().front().cloned()
    }

    /// Compute the average FPS over the history.
    pub fn avg_fps(&self) -> f32 {
        let entries = self.entries.borrow();
        if entries.is_empty() {
            return 0.0;
        }
        let sum: f32 = entries.iter().map(|e| e.fps).sum();
        sum / entries.len() as f32
    }

    /// Compute the average frame time over the history.
    pub fn avg_frame_time_ms(&self) -> f32 {
        let entries = self.entries.borrow();
        if entries.is_empty() {
            return 0.0;
        }
        let sum: f32 = entries.iter().map(|e| e.frame_time_ms).sum();
        sum / entries.len() as f32
    }

    /// Compute the peak GPU memory over the history.
    pub fn peak_gpu_memory(&self) -> MemorySize {
        let entries = self.entries.borrow();
        let peak = entries
            .iter()
            .map(|e| e.gpu_memory_bytes)
            .max()
            .unwrap_or(0);
        MemorySize::from_bytes(peak)
    }

    /// Compute the peak CPU memory over the history.
    pub fn peak_cpu_memory(&self) -> MemorySize {
        let entries = self.entries.borrow();
        let peak = entries
            .iter()
            .map(|e| e.cpu_memory_bytes)
            .max()
            .unwrap_or(0);
        MemorySize::from_bytes(peak)
    }

    /// Get all entries as a slice.
    pub fn entries(&self) -> Vec<HistoryEntry> {
        self.entries.borrow().iter().cloned().collect()
    }

    /// Clear the history.
    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
    }

    /// Compute a summary over the history.
    pub fn summary(&self) -> HistorySummary {
        let entries = self.entries.borrow();
        if entries.is_empty() {
            return HistorySummary::default();
        }

        let fps_values: Vec<f32> = entries.iter().map(|e| e.fps).collect();
        let frame_times: Vec<f32> = entries.iter().map(|e| e.frame_time_ms).collect();
        let gpu_values: Vec<u64> = entries.iter().map(|e| e.gpu_memory_bytes).collect();
        let cpu_values: Vec<u64> = entries.iter().map(|e| e.cpu_memory_bytes).collect();

        let duration = if entries.len() > 1 {
            (entries.back().unwrap().timestamp - entries.front().unwrap().timestamp).as_secs_f64()
        } else {
            0.0
        };

        HistorySummary {
            duration_secs: duration,
            entry_count: entries.len(),
            avg_fps: fps_values.iter().sum::<f32>() / fps_values.len() as f32,
            min_fps: fps_values.iter().cloned().fold(f32::MAX, f32::min),
            max_fps: fps_values.iter().cloned().fold(0.0, f32::max),
            avg_frame_time_ms: frame_times.iter().sum::<f32>() / frame_times.len() as f32,
            min_frame_time_ms: frame_times.iter().cloned().fold(f32::MAX, f32::min),
            max_frame_time_ms: frame_times.iter().cloned().fold(0.0, f32::max),
            p99_frame_time_ms: percentile(&mut frame_times.clone(), 0.99),
            peak_gpu_memory: MemorySize::from_bytes(gpu_values.iter().cloned().max().unwrap_or(0)),
            peak_cpu_memory: MemorySize::from_bytes(cpu_values.iter().cloned().max().unwrap_or(0)),
            total_draw_calls: entries.iter().map(|e| e.draw_calls as u64).sum(),
        }
    }
}

impl Default for ResourceHistory {
    fn default() -> Self {
        // Default: retain last 3600 entries (~1 minute at 60fps)
        Self::new(3600)
    }
}

/// Summary statistics over a history period.
#[derive(Debug, Clone)]
pub struct HistorySummary {
    /// Duration of the history in seconds.
    pub duration_secs: f64,
    /// Number of entries.
    pub entry_count: usize,
    /// Average FPS.
    pub avg_fps: f32,
    /// Minimum FPS.
    pub min_fps: f32,
    /// Maximum FPS.
    pub max_fps: f32,
    /// Average frame time in ms.
    pub avg_frame_time_ms: f32,
    /// Minimum frame time in ms.
    pub min_frame_time_ms: f32,
    /// Maximum frame time in ms.
    pub max_frame_time_ms: f32,
    /// 99th percentile frame time in ms.
    pub p99_frame_time_ms: f32,
    /// Peak GPU memory.
    pub peak_gpu_memory: MemorySize,
    /// Peak CPU memory.
    pub peak_cpu_memory: MemorySize,
    /// Total draw calls over the period.
    pub total_draw_calls: u64,
}

impl HistorySummary {
    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "History: {:.1}s, {} frames, avg {:.1}fps (min {:.1}, max {:.1}), \
             frameTime avg {:.2}ms (p99 {:.2}ms), \
             peakGPU {}, peakCPU {}, {} drawCalls",
            self.duration_secs,
            self.entry_count,
            self.avg_fps,
            self.min_fps,
            self.max_fps,
            self.avg_frame_time_ms,
            self.p99_frame_time_ms,
            self.peak_gpu_memory.display(),
            self.peak_cpu_memory.display(),
            self.total_draw_calls,
        )
    }
}

impl Default for HistorySummary {
    fn default() -> Self {
        Self {
            duration_secs: 0.0,
            entry_count: 0,
            avg_fps: 0.0,
            min_fps: 0.0,
            max_fps: 0.0,
            avg_frame_time_ms: 0.0,
            min_frame_time_ms: 0.0,
            max_frame_time_ms: 0.0,
            p99_frame_time_ms: 0.0,
            peak_gpu_memory: MemorySize::default(),
            peak_cpu_memory: MemorySize::default(),
            total_draw_calls: 0,
        }
    }
}

/// Compute the p-th percentile of a sorted slice.
fn percentile(values: &mut [f32], p: f64) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = (p * values.len() as f64) as usize;
    let idx = idx.min(values.len() - 1);
    values[idx]
}

// ---------------------------------------------------------------------------
// Resource Monitor (Main Aggregator)
// ---------------------------------------------------------------------------

/// The main resource monitoring aggregator.
///
/// Collects GPU, CPU, cache, and frame statistics into unified snapshots
/// and maintains a time-series history for trend analysis.
pub struct ResourceMonitor {
    /// Current GPU memory usage.
    gpu_memory: GpuMemoryUsage,
    /// Current CPU memory usage.
    cpu_memory: CpuMemoryUsage,
    /// Cache statistics.
    cache_stats: AggregateCacheStats,
    /// Frame rendering statistics.
    frame_stats: FrameRenderStats,
    /// GPU memory budget.
    gpu_budget: GpuMemoryBudget,
    /// System resource limits.
    system_limits: SystemResourceLimits,
    /// Time-series history.
    history: ResourceHistory,
    /// Whether monitoring is enabled.
    enabled: bool,
    /// Snapshot interval (record every Nth frame).
    snapshot_interval: u32,
    /// Frame counter for snapshot interval.
    frame_counter: u32,
}

impl ResourceMonitor {
    /// Create a new resource monitor.
    pub fn new() -> Self {
        Self {
            gpu_memory: GpuMemoryUsage::new(),
            cpu_memory: CpuMemoryUsage::new(),
            cache_stats: AggregateCacheStats::new(),
            frame_stats: FrameRenderStats::new(),
            gpu_budget: GpuMemoryBudget::default_2d(),
            system_limits: SystemResourceLimits::detect(),
            history: ResourceHistory::default(),
            enabled: true,
            snapshot_interval: 1,
            frame_counter: 0,
        }
    }

    /// Enable or disable monitoring.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Check if monitoring is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Set the snapshot interval (every Nth frame).
    pub fn set_snapshot_interval(&mut self, interval: u32) {
        self.snapshot_interval = interval.max(1);
    }

    /// Set the GPU memory budget.
    pub fn set_gpu_budget(&mut self, budget: GpuMemoryBudget) {
        self.gpu_budget = budget;
    }

    /// Begin a new frame: reset per-frame counters.
    pub fn begin_frame(&mut self) {
        if !self.enabled {
            return;
        }
        self.gpu_memory.begin_frame();
        self.frame_counter += 1;
    }

    /// End a new frame: record snapshot if interval matches.
    pub fn end_frame(&mut self, frame_time_ms: f32) {
        if !self.enabled {
            return;
        }

        self.frame_stats.update_fps(frame_time_ms);

        if self.frame_counter % self.snapshot_interval == 0 {
            let snapshot = self.take_snapshot();
            self.history.record(&snapshot);
        }
    }

    /// Take a point-in-time snapshot of all resources.
    pub fn take_snapshot(&self) -> ResourceSnapshot {
        ResourceSnapshot {
            timestamp: Instant::now(),
            gpu_memory: self.gpu_memory,
            cpu_memory: self.cpu_memory,
            cache_stats: self.cache_stats.clone(),
            frame_stats: self.frame_stats,
            system_limits: self.system_limits.clone(),
        }
    }

    /// Record a GPU memory allocation.
    pub fn record_gpu_allocation(&mut self, size: MemorySize, category: GpuMemoryCategory) {
        if !self.enabled {
            return;
        }
        self.gpu_memory.record_allocation(size, category);
    }

    /// Record a GPU memory deallocation.
    pub fn record_gpu_deallocation(&mut self, size: MemorySize, category: GpuMemoryCategory) {
        if !self.enabled {
            return;
        }
        self.gpu_memory.record_deallocation(size, category);
    }

    /// Record a CPU memory allocation.
    pub fn record_cpu_allocation(&mut self, size: MemorySize, category: CpuMemoryCategory) {
        if !self.enabled {
            return;
        }
        self.cpu_memory.record_allocation(size, category);
    }

    /// Record a CPU memory deallocation.
    pub fn record_cpu_deallocation(&mut self, size: MemorySize, category: CpuMemoryCategory) {
        if !self.enabled {
            return;
        }
        self.cpu_memory.record_deallocation(size, category);
    }

    /// Update cache statistics from GPU resources.
    pub fn update_cache_stats(
        &mut self,
        texture_count: u32,
        texture_memory: MemorySize,
        glyph_count: u32,
        pipeline_count: u32,
        buffer_count: u32,
    ) {
        if !self.enabled {
            return;
        }
        self.cache_stats.texture_cache.entries = texture_count;
        self.cache_stats.texture_cache.memory_used = texture_memory;
        self.cache_stats.glyph_cache.entries = glyph_count;
        self.cache_stats.pipeline_cache.entries = pipeline_count;
        self.cache_stats.buffer_cache.entries = buffer_count;
    }

    /// Get the current GPU memory usage.
    pub fn gpu_memory(&self) -> GpuMemoryUsage {
        self.gpu_memory
    }

    /// Get the current CPU memory usage.
    pub fn cpu_memory(&self) -> CpuMemoryUsage {
        self.cpu_memory
    }

    /// Get the current cache statistics.
    pub fn cache_stats(&self) -> &AggregateCacheStats {
        &self.cache_stats
    }

    /// Get the current frame statistics.
    pub fn frame_stats(&self) -> FrameRenderStats {
        self.frame_stats
    }

    /// Get the system resource limits.
    pub fn system_limits(&self) -> SystemResourceLimits {
        self.system_limits.clone()
    }

    /// Get the resource history.
    pub fn history(&self) -> &ResourceHistory {
        &self.history
    }

    /// Get the history summary.
    pub fn history_summary(&self) -> HistorySummary {
        self.history.summary()
    }

    /// Check overall resource health.
    pub fn health_check(&self) -> ResourceHealth {
        let snapshot = self.take_snapshot();
        snapshot.health_check()
    }

    /// Check for budget violations.
    pub fn check_budgets(&self) -> Vec<String> {
        let snapshot = self.take_snapshot();
        snapshot.check_budgets(&self.gpu_budget)
    }

    /// Format a comprehensive report.
    pub fn report(&self) -> String {
        let snapshot = self.take_snapshot();
        let summary = self.history.summary();
        format!(
            "=== Resource Monitor ===\n{}\n\n{}\n\nHealth: {:?}",
            snapshot.report(),
            summary.report(),
            self.health_check()
        )
    }

    /// Reset all tracking state.
    pub fn reset(&mut self) {
        self.gpu_memory = GpuMemoryUsage::new();
        self.cpu_memory = CpuMemoryUsage::new();
        self.cache_stats = AggregateCacheStats::new();
        self.frame_stats = FrameRenderStats::new();
        self.history.clear();
        self.frame_counter = 0;
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_size_from_bytes() {
        let m = MemorySize::from_bytes(1024);
        assert_eq!(m.bytes, 1024);
        assert!((m.kb() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_memory_size_from_mb() {
        let m = MemorySize::from_mb(1);
        assert_eq!(m.bytes, 1024 * 1024);
        assert!((m.mb() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_memory_size_display() {
        assert!(MemorySize::from_bytes(500).display().contains("B"));
        assert!(MemorySize::from_kb(1).display().contains("KB"));
        assert!(MemorySize::from_mb(1).display().contains("MB"));
        assert!(MemorySize::from_gb(1).display().contains("GB"));
    }

    #[test]
    fn test_memory_size_arithmetic() {
        let a = MemorySize::from_mb(1);
        let b = MemorySize::from_mb(2);
        let c = a.saturating_add(&b);
        assert_eq!(c.bytes, 3 * 1024 * 1024);
        let d = c.saturating_sub(&a);
        assert_eq!(d.bytes, 2 * 1024 * 1024);
    }

    #[test]
    fn test_gpu_memory_breakdown() {
        let mut bd = GpuMemoryBreakdown::new();
        bd.textures = MemorySize::from_mb(100);
        bd.vertex_buffers = MemorySize::from_mb(10);
        assert_eq!(bd.total().mb(), 110.0);
        assert!(!bd.is_empty());
    }

    #[test]
    fn test_gpu_memory_breakdown_empty() {
        let bd = GpuMemoryBreakdown::new();
        assert!(bd.is_empty());
        assert!(bd.total().is_zero());
    }

    #[test]
    fn test_gpu_memory_breakdown_largest() {
        let mut bd = GpuMemoryBreakdown::new();
        bd.textures = MemorySize::from_mb(100);
        bd.vertex_buffers = MemorySize::from_mb(10);
        let (name, size) = bd.largest_category();
        assert_eq!(name, "textures");
        assert_eq!(size.bytes, MemorySize::from_mb(100).bytes);
    }

    #[test]
    fn test_gpu_memory_budget_check() {
        let budget = GpuMemoryBudget::default_2d();
        let mut usage = GpuMemoryBreakdown::new();
        usage.textures = MemorySize::from_mb(300); // Exceeds 256 MB budget

        let warnings = budget.check_limits(&usage);
        assert!(!warnings.is_empty());
        assert!(warnings[0].contains("Texture"));
    }

    #[test]
    fn test_gpu_memory_budget_ok() {
        let budget = GpuMemoryBudget::default_2d();
        let usage = GpuMemoryBreakdown::new();
        let warnings = budget.check_limits(&usage);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_gpu_memory_usage() {
        let mut usage = GpuMemoryUsage::new();
        usage.record_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        assert_eq!(usage.allocation_count, 1);
        assert_eq!(usage.allocations_this_frame, 1);
        assert_eq!(
            usage.breakdown.textures.bytes,
            MemorySize::from_mb(10).bytes
        );
        usage.update_peak();
        assert_eq!(usage.peak_usage.bytes, MemorySize::from_mb(10).bytes);
    }

    #[test]
    fn test_gpu_memory_usage_deallocation() {
        let mut usage = GpuMemoryUsage::new();
        usage.record_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        usage.record_deallocation(MemorySize::from_mb(5), GpuMemoryCategory::Texture);
        assert_eq!(usage.allocation_count, 0);
        assert_eq!(usage.breakdown.textures.bytes, MemorySize::from_mb(5).bytes);
    }

    #[test]
    fn test_gpu_memory_begin_frame() {
        let mut usage = GpuMemoryUsage::new();
        usage.record_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        assert_eq!(usage.allocations_this_frame, 1);
        usage.begin_frame();
        assert_eq!(usage.allocations_this_frame, 0);
    }

    #[test]
    fn test_cpu_memory_breakdown() {
        let mut bd = CpuMemoryBreakdown::new();
        bd.scene_graph = MemorySize::from_mb(50);
        bd.video_frames = MemorySize::from_mb(200);
        assert!((bd.total().mb() - 250.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_hit_rate() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(64));
        for _ in 0..80 {
            stats.record_hit();
        }
        for _ in 0..20 {
            stats.record_miss();
        }
        assert!((stats.hit_rate() - 0.8).abs() < 0.01);
        assert!((stats.miss_rate() - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_empty() {
        let stats = CacheStats::new(100, MemorySize::from_mb(64));
        assert!((stats.hit_rate() - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_eviction_rate() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(64));
        for _ in 0..100 {
            stats.record_insertion();
        }
        for _ in 0..10 {
            stats.record_eviction();
        }
        assert!((stats.eviction_rate() - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_pressure() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(64));
        // Low hit rate = pressure
        for _ in 0..10 {
            stats.record_hit();
        }
        for _ in 0..90 {
            stats.record_miss();
        }
        assert!(stats.is_under_pressure());
    }

    #[test]
    fn test_cache_stats_no_pressure() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(64));
        for _ in 0..95 {
            stats.record_hit();
        }
        for _ in 0..5 {
            stats.record_miss();
        }
        assert!(!stats.is_under_pressure());
    }

    #[test]
    fn test_aggregate_cache_stats() {
        let mut stats = AggregateCacheStats::new();
        stats.texture_cache.record_hit();
        stats.glyph_cache.record_hit();
        stats.glyph_cache.record_miss();
        assert!((stats.overall_hit_rate() - 2.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn test_system_limits_detect() {
        let limits = SystemResourceLimits::detect();
        assert!(limits.cpu_cores >= 1);
        assert!(limits.cpu_threads >= 1);
        assert!(!limits.total_ram.is_zero());
    }

    #[test]
    fn test_system_limits_texture_support() {
        let limits = SystemResourceLimits::detect();
        assert!(limits.supports_texture_size(Size::new(1024.0, 1024.0)));
        assert!(!limits.supports_texture_size(Size::new(99999.0, 99999.0)));
    }

    #[test]
    fn test_frame_render_stats_fps() {
        let mut stats = FrameRenderStats::new();
        stats.update_fps(16.67); // ~60fps
        assert!(stats.fps > 50.0);
        assert!(stats.fps < 70.0);
        assert!(stats.avg_fps > 0.0);
    }

    #[test]
    fn test_frame_render_stats_stability() {
        let mut stats = FrameRenderStats::new();
        for _ in 0..10 {
            stats.update_fps(16.0);
        }
        assert!(stats.is_stable());
        assert!(stats.is_acceptable());
    }

    #[test]
    fn test_frame_render_stats_unstable() {
        let mut stats = FrameRenderStats::new();
        for _ in 0..5 {
            stats.update_fps(16.0);
        }
        stats.update_fps(100.0); // Sudden spike
        assert!(!stats.is_stable());
    }

    #[test]
    fn test_resource_snapshot() {
        let snapshot = ResourceSnapshot::new();
        assert!(
            !snapshot.total_memory().is_zero() || snapshot.gpu_memory.total_allocated.is_zero()
        );
    }

    #[test]
    fn test_resource_health() {
        assert!(ResourceHealth::Healthy.worse(ResourceHealth::Healthy) == ResourceHealth::Healthy);
        assert!(ResourceHealth::Healthy.worse(ResourceHealth::Warning) == ResourceHealth::Warning);
        assert!(
            ResourceHealth::Warning.worse(ResourceHealth::Critical) == ResourceHealth::Critical
        );
    }

    #[test]
    fn test_resource_history() {
        let history = ResourceHistory::new(100);
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);

        let snapshot = ResourceSnapshot::new();
        history.record(&snapshot);
        assert_eq!(history.len(), 1);
        assert!(!history.is_empty());
        assert!(history.latest().is_some());
    }

    #[test]
    fn test_resource_history_max_entries() {
        let history = ResourceHistory::new(5);
        for _ in 0..10 {
            history.record(&ResourceSnapshot::new());
        }
        assert_eq!(history.len(), 5);
    }

    #[test]
    fn test_resource_history_summary() {
        let history = ResourceHistory::new(100);
        let summary = history.summary();
        assert_eq!(summary.entry_count, 0);
        assert_eq!(summary.avg_fps, 0.0);
    }

    #[test]
    fn test_resource_monitor() {
        let mut monitor = ResourceMonitor::new();
        assert!(monitor.is_enabled());

        monitor.begin_frame();
        monitor.record_gpu_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        monitor.end_frame(16.67);

        let stats = monitor.frame_stats();
        assert!(stats.fps > 0.0);
    }

    #[test]
    fn test_resource_monitor_disabled() {
        let mut monitor = ResourceMonitor::new();
        monitor.set_enabled(false);

        monitor.begin_frame();
        monitor.record_gpu_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        monitor.end_frame(16.67);

        // Should not record anything when disabled
        assert_eq!(monitor.gpu_memory.allocation_count, 0);
    }

    #[test]
    fn test_resource_monitor_health() {
        let monitor = ResourceMonitor::new();
        let health = monitor.health_check();
        assert!(health == ResourceHealth::Healthy);
    }

    #[test]
    fn test_resource_monitor_reset() {
        let mut monitor = ResourceMonitor::new();
        monitor.record_gpu_allocation(MemorySize::from_mb(10), GpuMemoryCategory::Texture);
        monitor.reset();
        assert_eq!(monitor.gpu_memory.allocation_count, 0);
    }

    #[test]
    fn test_gpu_memory_category_apply() {
        let mut bd = GpuMemoryBreakdown::new();
        GpuMemoryCategory::VertexBuffer.apply(&mut bd, MemorySize::from_mb(5), true);
        assert_eq!(bd.vertex_buffers.bytes, MemorySize::from_mb(5).bytes);
    }

    #[test]
    fn test_cpu_memory_category_apply() {
        let mut bd = CpuMemoryBreakdown::new();
        CpuMemoryCategory::VideoFrames.apply(&mut bd, MemorySize::from_mb(100), true);
        assert_eq!(bd.video_frames.bytes, MemorySize::from_mb(100).bytes);
    }

    #[test]
    fn test_memory_size_saturating() {
        let max = MemorySize::from_bytes(u64::MAX);
        let one = MemorySize::from_bytes(1);
        let result = max.saturating_add(&one);
        assert_eq!(result.bytes, u64::MAX);
    }

    #[test]
    fn test_cache_stats_fill_ratio() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(64));
        for _ in 0..50 {
            stats.record_insertion();
        }
        assert!((stats.fill_ratio() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_cache_stats_memory_utilization() {
        let mut stats = CacheStats::new(100, MemorySize::from_mb(100));
        stats.memory_used = MemorySize::from_mb(50);
        assert!((stats.memory_utilization() - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_gpu_budget_remaining() {
        let budget = GpuMemoryBudget::default_2d();
        let mut usage = GpuMemoryBreakdown::new();
        usage.textures = MemorySize::from_mb(100);
        let remaining = budget.remaining(&usage);
        assert_eq!(remaining.textures.bytes, MemorySize::from_mb(156).bytes);
    }

    #[test]
    fn test_history_entry() {
        let entry = HistoryEntry {
            timestamp: Instant::now(),
            gpu_memory_bytes: 1024,
            cpu_memory_bytes: 2048,
            frame_time_ms: 16.67,
            fps: 60.0,
            draw_calls: 5,
        };
        assert_eq!(entry.draw_calls, 5);
    }

    #[test]
    fn test_percentile() {
        let mut values = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        // p=0.5 → idx=5 → value=6.0 (0-indexed sorted)
        assert!((percentile(&mut values, 0.5) - 6.0).abs() < 0.01);
        assert!((percentile(&mut values, 1.0) - 10.0).abs() < 0.01);
    }

    #[test]
    fn test_percentile_empty() {
        let mut values: Vec<f32> = vec![];
        assert_eq!(percentile(&mut values, 0.5), 0.0);
    }

    #[test]
    fn test_frame_stats_reset() {
        let mut stats = FrameRenderStats::new();
        stats.draw_calls = 10;
        stats.instances = 100;
        stats.reset();
        assert_eq!(stats.draw_calls, 0);
        assert_eq!(stats.instances, 0);
    }

    #[test]
    fn test_resource_monitor_snapshot_interval() {
        let mut monitor = ResourceMonitor::new();
        monitor.set_snapshot_interval(5);

        for _ in 0..10 {
            monitor.begin_frame();
            monitor.end_frame(16.0);
        }

        // Should have recorded ~2 snapshots (frames 5 and 10)
        assert!(monitor.history.len() >= 1);
    }

    #[test]
    fn test_history_avg_fps() {
        let history = ResourceHistory::new(100);
        let mut snapshot = ResourceSnapshot::new();
        snapshot.frame_stats.fps = 60.0;
        history.record(&snapshot);

        let mut snapshot2 = ResourceSnapshot::new();
        snapshot2.frame_stats.fps = 40.0;
        history.record(&snapshot2);

        assert!((history.avg_fps() - 50.0).abs() < 0.01);
    }

    #[test]
    fn test_history_peak_memory() {
        let history = ResourceHistory::new(100);
        let mut snapshot = ResourceSnapshot::new();
        snapshot.gpu_memory.total_allocated = MemorySize::from_mb(100);
        snapshot.cpu_memory.total_allocated = MemorySize::from_mb(50);
        history.record(&snapshot);

        let mut snapshot2 = ResourceSnapshot::new();
        snapshot2.gpu_memory.total_allocated = MemorySize::from_mb(200);
        snapshot2.cpu_memory.total_allocated = MemorySize::from_mb(30);
        history.record(&snapshot2);

        assert_eq!(
            history.peak_gpu_memory().bytes,
            MemorySize::from_mb(200).bytes
        );
        assert_eq!(
            history.peak_cpu_memory().bytes,
            MemorySize::from_mb(50).bytes
        );
    }
}
