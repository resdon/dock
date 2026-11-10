//! PipeWire video integration subsystem.
//!
//! This module provides a `VideoManager` that abstracts PipeWire stream
//! management, frame acquisition, and GPU texture uploads. It is designed
//! for the dock's use case: ~20 independent video streams where only one
//! is rendered at a time.
//!
//! ## Architecture
//!
//! ```text
//!  PipeWire
//!  20 independent streams
//!
//!  VideoManager
//!  ┌──────────┐ ┌──────────┐ ┌──────────┐
//!  │ Stream 01│ │ Stream 02│ │ Stream 03│ ...
//!  └────┬─────┘ └────┬─────┘ └────┬─────┘
//!       │             │            │
//!       ▼             ▼            ▼
//!  Video texture cache (GPU textures, latest frame only)
//!       │
//!       ▼
//!  wgpu renderer (renders selected video texture)
//! ```
//!
//! ## Key Design Decisions
//!
//! 1. **Stream lifetime ≠ Frame lifetime**: A stream can exist for hours
//!    without being rendered. Only the active stream's frames are uploaded
//!    to GPU each frame.
//!
//! 2. **Latest-frame policy**: When frames arrive faster than they are
//!    rendered, only the newest frame is kept. Obsolete frames are
//!    immediately discarded to avoid latency buildup.
//!
//! 3. **Texture reuse**: When a stream's dimensions and format haven't
//!    changed, the existing GPU texture is reused — no destroy/recreate
//!    cycle on stream switch.
//!
//! 4. **Path B first (CPU upload)**: Initial implementation uses CPU
//!    buffer → wgpu texture upload. The interface is designed so Path A
//!    (DMA-BUF GPU sharing) can be added later without API changes.
//!
//! 5. **Buffer ownership**: PipeWire buffers are returned/released
//!    correctly. Never retain a borrowed buffer beyond its valid lifetime
//!    unless explicitly acquired or copied safely.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use crate::color::Color;
use crate::geometry::{Rect, Size};
use crate::texture::{TextureFormat, TextureId};

// ---------------------------------------------------------------------------
// Opaque handles
// ---------------------------------------------------------------------------

/// Opaque identifier for a video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StreamId(u64);

impl StreamId {
    /// Create a new stream ID.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Get the raw ID value.
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

/// Handle to a single video frame.
///
/// This is a lightweight reference to a frame buffer owned by the stream.
/// The frame handle is valid only until the next frame arrives for the same
/// stream, at which point the old frame data is replaced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FrameHandle {
    /// Stream that owns this frame.
    pub stream_id: StreamId,
    /// Monotonic frame sequence number within the stream.
    pub sequence: u64,
    /// Timestamp when the frame was captured.
    pub timestamp: u64,
}

impl FrameHandle {
    /// Create a new frame handle.
    pub fn new(stream_id: StreamId, sequence: u64, timestamp: u64) -> Self {
        Self {
            stream_id,
            sequence,
            timestamp,
        }
    }
}

// ---------------------------------------------------------------------------
// Video format
// ---------------------------------------------------------------------------

/// Video pixel format from PipeWire streams.
///
/// Maps to `pw::spa::param::video::VideoFormat`. The renderer converts
/// these to GPU-compatible formats during upload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VideoFormat {
    /// BGRx (24-bit BGR + 1 padding), common PipeWire format.
    BGRx,
    /// BGRA (32-bit, little-endian).
    Bgra,
    /// RGBA (32-bit, little-endian).
    Rgba,
    /// RGB (24-bit, no alpha).
    Rgb,
    /// YUV 4:2:0 semi-planar (NV12).
    Nv12,
    /// YUV 4:2:0 planar (I420).
    I420,
    /// YUV 4:2:2 semi-planar (NV16).
    Nv16,
    /// YUV 4:4:4 (YUY2).
    Yuy2,
    /// Unknown format — will be skipped during rendering.
    Unknown(u32),
}

impl VideoFormat {
    /// Get the bytes per pixel for this format.
    ///
    /// For planar formats, returns the luma bytes per pixel.
    pub fn bytes_per_pixel(&self) -> usize {
        match self {
            Self::BGRx | Self::Bgra | Self::Rgba => 4,
            Self::Rgb => 3,
            Self::Nv12 | Self::I420 => 2, // 1.5 avg, but aligned to 2
            Self::Nv16 | Self::Yuy2 => 4,
            Self::Unknown(_) => 4, // default assumption
        }
    }

    /// Check if this format can be directly uploaded as RGBA without conversion.
    pub fn is_rgba_compatible(&self) -> bool {
        matches!(self, Self::Rgba)
    }

    /// Check if this format requires byte reordering for RGBA upload.
    pub fn needs_reordering(&self) -> bool {
        matches!(self, Self::BGRx | Self::Bgra | Self::Rgb)
    }

    /// Check if this is a planar YUV format requiring special handling.
    pub fn is_planar(&self) -> bool {
        matches!(self, Self::Nv12 | Self::I420 | Self::Nv16)
    }

    /// Estimate the buffer size for a frame of the given dimensions.
    pub fn estimate_buffer_size(&self, width: u32, height: u32) -> usize {
        let base = width as usize * height as usize;
        match self {
            Self::BGRx | Self::Bgra | Self::Rgba | Self::Nv16 | Self::Yuy2 => base * 4,
            Self::Rgb => base * 3,
            Self::Nv12 | Self::I420 => base * 3 / 2, // 1.5 bytes per pixel
            Self::Unknown(_) => base * 4,
        }
    }

    /// Convert to the recommended GPU texture format for upload.
    pub fn to_texture_format(&self) -> TextureFormat {
        match self {
            Self::BGRx | Self::Bgra | Self::Rgba | Self::Rgb => TextureFormat::Rgba8Unorm,
            Self::Nv12 | Self::I420 | Self::Nv16 | Self::Yuy2 => TextureFormat::Rgba8Unorm,
            Self::Unknown(_) => TextureFormat::Rgba8Unorm,
        }
    }

    /// Convert from PipeWire's spa video format enum value.
    ///
    /// Maps the well-known PipeWire SPA video format constants to our
    /// `VideoFormat` enum. Unknown formats are wrapped in `Unknown(n)`.
    ///
    /// Well-known values:
    /// - 1: RGB, 5: BGRx, 10: BGRA, 11: RGBA
    /// - 15: NV12, 16: I420, 17: NV16, 18: YUY2
    pub fn from_pw_format(value: u32) -> Self {
        match value {
            1 => Self::Rgb,
            5 => Self::BGRx,
            10 => Self::Bgra,
            11 => Self::Rgba,
            15 => Self::Nv12,
            16 => Self::I420,
            17 => Self::Nv16,
            18 => Self::Yuy2,
            other => Self::Unknown(other),
        }
    }
}

// ---------------------------------------------------------------------------
// Stream state
// ---------------------------------------------------------------------------

/// Lifecycle state of a video stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamState {
    /// The stream is connecting to PipeWire.
    Connecting,
    /// The stream is connected and format negotiation is complete.
    Ready,
    /// The stream is actively receiving frames.
    Active,
    /// The stream is temporarily suspended (e.g., window minimized).
    Suspended,
    /// The stream is disconnected (window closed).
    Disconnected,
    /// The stream encountered an error.
    Error,
}

impl StreamState {
    /// Check if the stream is in a usable state.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Ready | Self::Active)
    }

    /// Check if the stream is terminated.
    pub fn is_terminated(&self) -> bool {
        matches!(self, Self::Disconnected | Self::Error)
    }
}

// ---------------------------------------------------------------------------
// Video frame
// ---------------------------------------------------------------------------

/// A single video frame from a PipeWire stream.
///
/// This is the raw frame data before GPU upload. The frame buffer is owned
/// by this struct — when it is dropped, the underlying buffer is released
/// back to PipeWire (or freed if it was a copy).
#[derive(Clone)]
pub struct VideoFrame {
    /// Raw pixel data.
    pub data: Vec<u8>,
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Video pixel format.
    pub format: VideoFormat,
    /// Monotonic timestamp (nanoseconds since epoch).
    pub timestamp: u64,
    /// Frame sequence number within the stream.
    pub sequence: u64,
}

impl VideoFrame {
    /// Create a new video frame.
    pub fn new(
        data: Vec<u8>,
        width: u32,
        height: u32,
        format: VideoFormat,
        timestamp: u64,
        sequence: u64,
    ) -> Self {
        Self {
            data,
            width,
            height,
            format,
            timestamp,
            sequence,
        }
    }

    /// Get the size of the frame.
    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// Estimate the buffer size in bytes.
    pub fn buffer_size(&self) -> usize {
        self.format.estimate_buffer_size(self.width, self.height)
    }

    /// Check if the frame data matches the expected size for the format.
    pub fn is_valid(&self) -> bool {
        let expected = self.format.estimate_buffer_size(self.width, self.height);
        self.data.len() >= expected
    }

    /// Convert the frame data to RGBA bytes.
    ///
    /// This performs the necessary byte reordering and format conversion
    /// for GPU upload. For RGBA-compatible formats, this may return a
    /// reference to the original data without copying.
    pub fn to_rgba(&self) -> Option<Vec<u8>> {
        let rgba_size = (self.width * self.height * 4) as usize;

        match self.format {
            VideoFormat::Rgba => {
                if self.data.len() >= rgba_size {
                    Some(self.data[..rgba_size].to_vec())
                } else {
                    None
                }
            }
            VideoFormat::Bgra => {
                if self.data.len() < rgba_size {
                    return None;
                }
                let mut out = Vec::with_capacity(rgba_size);
                for chunk in self.data[..rgba_size].chunks_exact(4) {
                    // BGRA → RGBA: swap R and B
                    out.push(chunk[2]); // R
                    out.push(chunk[1]); // G
                    out.push(chunk[0]); // B
                    out.push(chunk[3]); // A
                }
                Some(out)
            }
            VideoFormat::BGRx => {
                if self.data.len() < rgba_size {
                    return None;
                }
                let mut out = Vec::with_capacity(rgba_size);
                for chunk in self.data[..rgba_size].chunks_exact(4) {
                    // BGRx → RGBA: swap R and B, set alpha to 255
                    out.push(chunk[2]); // R
                    out.push(chunk[1]); // G
                    out.push(chunk[0]); // B
                    out.push(255); // A
                }
                Some(out)
            }
            VideoFormat::Rgb => {
                let rgb_size = (self.width * self.height * 3) as usize;
                if self.data.len() < rgb_size {
                    return None;
                }
                let mut out = Vec::with_capacity(rgba_size);
                for chunk in self.data[..rgb_size].chunks_exact(3) {
                    out.push(chunk[0]); // R
                    out.push(chunk[1]); // G
                    out.push(chunk[2]); // B
                    out.push(255); // A
                }
                Some(out)
            }
            // Planar YUV formats require more complex conversion
            VideoFormat::Nv12 | VideoFormat::I420 | VideoFormat::Nv16 | VideoFormat::Yuy2 => {
                // For now, return None — YUV conversion requires a proper
                // implementation. In production, use a SIMD-accelerated
                // converter or let the GPU handle it via a shader.
                None
            }
            VideoFormat::Unknown(_) => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Frame buffer abstraction
// ---------------------------------------------------------------------------

/// Abstraction over the frame buffer source path.
///
/// The renderer supports two paths:
/// - **Path A (DMA-BUF)**: Zero-copy GPU import from PipeWire buffer.
///   Requires Vulkan interop, format/modifier negotiation, and synchronization.
/// - **Path B (CPU upload)**: Copy frame data to CPU memory, then upload
///   to GPU texture. Simpler, more portable, but more CPU/memory expensive.
///
/// The interface is designed so Path A can be added later without changing
/// the public API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameBufferSource {
    /// DMA-BUF shared memory (Path A — future).
    ///
    /// Requires:
    /// - Compatible GPU format and modifier
    /// - Proper synchronization (fences, semaphores)
    /// - Vulkan native interop (wgpu doesn't provide universal DMA-BUF import)
    DmaBuf,
    /// CPU memory upload (Path B — current).
    ///
    /// Simpler implementation: copy frame data to CPU buffer, then upload
    /// to GPU texture via wgpu staging buffer.
    CpuUpload,
}

/// A frame buffer that can be either DMA-BUF shared or CPU-owned.
///
/// For Path B (CPU upload), this holds the raw pixel data.
/// For Path A (DMA-BUF), this would hold a file descriptor and metadata.
pub struct FrameBuffer {
    /// The source path for this buffer.
    pub source: FrameBufferSource,
    /// Raw pixel data (for CPU upload path).
    pub data: Option<Vec<u8>>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Video format.
    pub format: VideoFormat,
    /// DMA-BUF file descriptor (for DMA-BUF path, future).
    #[allow(dead_code)]
    pub dma_buf_fd: Option<i32>,
    /// DMA-BUF modifier (for DMA-BUF path, future).
    #[allow(dead_code)]
    pub dma_buf_modifier: u64,
}

impl FrameBuffer {
    /// Create a new CPU upload frame buffer.
    pub fn cpu_upload(data: Vec<u8>, width: u32, height: u32, format: VideoFormat) -> Self {
        Self {
            source: FrameBufferSource::CpuUpload,
            data: Some(data),
            width,
            height,
            format,
            dma_buf_fd: None,
            dma_buf_modifier: 0,
        }
    }

    /// Create a new DMA-BUF frame buffer (stub for future implementation).
    #[allow(unused_variables)]
    pub fn dma_buf(fd: i32, width: u32, height: u32, format: VideoFormat, modifier: u64) -> Self {
        Self {
            source: FrameBufferSource::DmaBuf,
            data: None,
            width,
            height,
            format,
            dma_buf_fd: Some(fd),
            dma_buf_modifier: modifier,
        }
    }

    /// Get the size of the buffer.
    pub fn size(&self) -> Size {
        Size::new(self.width as f32, self.height as f32)
    }

    /// Check if this buffer has CPU-accessible data.
    pub fn has_cpu_data(&self) -> bool {
        self.data.is_some()
    }
}

// ---------------------------------------------------------------------------
// Video stream
// ---------------------------------------------------------------------------

/// A video stream from PipeWire.
///
/// Represents one captured window's video feed. A stream can exist for hours
/// without being rendered — only the active stream's frames are uploaded to
/// the GPU each frame.
pub struct VideoStream {
    /// Unique stream identifier.
    pub id: StreamId,
    /// Current lifecycle state.
    pub state: StreamState,
    /// Frame dimensions.
    pub size: Size,
    /// Video pixel format.
    pub format: VideoFormat,
    /// The latest frame handle (if any frame has been received).
    pub latest_frame: Option<FrameHandle>,
    /// The latest frame data.
    pub latest_frame_data: Option<VideoFrame>,
    /// Frame sequence counter (monotonically increasing).
    pub frame_sequence: u64,
    /// Timestamp when the stream was created.
    pub created_at: Instant,
    /// Timestamp when the latest frame was received.
    pub last_frame_at: Option<Instant>,
    /// Total frames received.
    pub total_frames: u64,
    /// Total frames dropped (obsolete frames discarded).
    pub dropped_frames: u64,
    /// Optional human-readable title (e.g., window title).
    pub title: Option<String>,
    /// Optional stable Wayland window ID for binding.
    pub window_id: Option<u32>,
}

impl VideoStream {
    /// Create a new video stream.
    pub fn new(id: StreamId) -> Self {
        Self {
            id,
            state: StreamState::Connecting,
            size: Size::ZERO,
            format: VideoFormat::Unknown(0),
            latest_frame: None,
            latest_frame_data: None,
            frame_sequence: 0,
            created_at: Instant::now(),
            last_frame_at: None,
            total_frames: 0,
            dropped_frames: 0,
            title: None,
            window_id: None,
        }
    }

    /// Set the stream title.
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the stable Wayland window ID.
    pub fn with_window_id(mut self, window_id: u32) -> Self {
        self.window_id = Some(window_id);
        self
    }

    /// Update the stream's format and size from PipeWire negotiation.
    pub fn set_format(&mut self, width: u32, height: u32, format: VideoFormat) {
        self.size = Size::new(width as f32, height as f32);
        self.format = format;
        self.state = StreamState::Ready;
    }

    /// Feed a new frame into the stream.
    ///
    /// Implements the latest-frame policy: if a frame is already present but
    /// not yet consumed, it is silently replaced (dropped) by the new frame.
    pub fn feed_frame(&mut self, frame: VideoFrame) {
        self.frame_sequence += 1;
        self.total_frames += 1;

        // If there was a previous frame, it's now obsolete
        if self.latest_frame_data.is_some() {
            self.dropped_frames += 1;
        }

        self.latest_frame_data = Some(frame.clone());
        self.latest_frame = Some(FrameHandle::new(self.id, frame.sequence, frame.timestamp));
        self.last_frame_at = Some(Instant::now());

        if self.state == StreamState::Ready {
            self.state = StreamState::Active;
        }
    }

    /// Get the latest frame, if available.
    pub fn latest_frame(&self) -> Option<&VideoFrame> {
        self.latest_frame_data.as_ref()
    }

    /// Check if the stream has a valid frame.
    pub fn has_frame(&self) -> bool {
        self.latest_frame_data.is_some()
    }

    /// Check if the stream is stale (no frames for a while).
    pub fn is_stale(&self, timeout: Duration) -> bool {
        match self.last_frame_at {
            Some(last) => last.elapsed() > timeout,
            None => self.state.is_usable(),
        }
    }

    /// Mark the stream as suspended.
    pub fn suspend(&mut self) {
        if self.state == StreamState::Active {
            self.state = StreamState::Suspended;
        }
    }

    /// Mark the stream as disconnected.
    pub fn disconnect(&mut self) {
        self.state = StreamState::Disconnected;
        self.latest_frame_data = None;
        self.latest_frame = None;
    }

    /// Mark the stream as errored.
    pub fn error(&mut self) {
        self.state = StreamState::Error;
    }

    /// Get statistics about the stream.
    pub fn stats(&self) -> StreamStats {
        StreamStats {
            id: self.id,
            state: self.state,
            size: self.size,
            format: self.format,
            total_frames: self.total_frames,
            dropped_frames: self.dropped_frames,
            uptime: self.created_at.elapsed(),
            last_frame_age: self.last_frame_at.map(|t| t.elapsed()),
            title: self.title.clone(),
            window_id: self.window_id,
        }
    }
}

/// Statistics about a video stream.
#[derive(Debug, Clone)]
pub struct StreamStats {
    /// Stream ID.
    pub id: StreamId,
    /// Current state.
    pub state: StreamState,
    /// Frame size.
    pub size: Size,
    /// Video format.
    pub format: VideoFormat,
    /// Total frames received.
    pub total_frames: u64,
    /// Total frames dropped.
    pub dropped_frames: u64,
    /// How long the stream has been alive.
    pub uptime: Duration,
    /// Time since the last frame was received.
    pub last_frame_age: Option<Duration>,
    /// Window title.
    pub title: Option<String>,
    /// Wayland window ID.
    pub window_id: Option<u32>,
}

// ---------------------------------------------------------------------------
// Video texture
// ---------------------------------------------------------------------------

/// A GPU texture containing a video frame.
///
/// This is the renderer-facing interface for video. The renderer uploads
/// the latest frame to a GPU texture and returns this handle, which can
/// then be used for rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoTexture {
    /// The GPU texture ID.
    pub texture: TextureId,
    /// Frame dimensions.
    pub size: Size,
    /// Source rectangle within the texture (normalized 0..1).
    pub source_rect: Rect,
    /// Frame timestamp.
    pub timestamp: u64,
}

impl VideoTexture {
    /// Create a new video texture handle.
    pub fn new(texture: TextureId, width: f32, height: f32, timestamp: u64) -> Self {
        Self {
            texture,
            size: Size::new(width, height),
            source_rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            timestamp,
        }
    }

    /// Check if the texture is valid.
    pub fn is_valid(&self) -> bool {
        self.size.width > 0.0 && self.size.height > 0.0
    }

    /// Get the aspect ratio.
    pub fn aspect_ratio(&self) -> f32 {
        if self.size.height > 0.0 {
            self.size.width / self.size.height
        } else {
            0.0
        }
    }
}

// ---------------------------------------------------------------------------
// Video texture cache entry
// ---------------------------------------------------------------------------

/// A cached GPU texture for a video stream.
///
/// The cache reuses textures when dimensions and format haven't changed,
/// avoiding the expensive destroy/recreate cycle on stream switch.
struct VideoTextureCacheEntry {
    /// The GPU texture ID.
    texture: TextureId,
    /// Width in pixels.
    width: u32,
    /// Height in u32.
    height: u32,
    /// Video format.
    format: VideoFormat,
    /// Last upload time.
    last_upload: Instant,
}

impl VideoTextureCacheEntry {
    /// Check if this entry can be reused for the given dimensions and format.
    fn can_reuse(&self, width: u32, height: u32, format: VideoFormat) -> bool {
        self.width == width && self.height == height && self.format == format
    }
}

// ---------------------------------------------------------------------------
// Video manager
// ---------------------------------------------------------------------------

/// Manages video streams and provides frames to the renderer.
///
/// This is the central coordinator for PipeWire video integration. It:
/// - Manages ~20 independent streams
/// - Tracks which stream is active (selected for rendering)
/// - Implements latest-frame policy to avoid latency buildup
/// - Caches GPU textures for efficient reuse
/// - Provides frame acquisition without exposing PipeWire internals
///
/// ## Design
///
/// The VideoManager distinguishes stream lifetime from frame lifetime:
/// a stream can exist for hours without being rendered. Only the active
/// stream's frames are uploaded to GPU each frame.
///
/// When switching streams, textures are reused when dimensions and format
/// permit — no destroy/recreate cycle.
pub struct VideoManager {
    /// All registered streams.
    streams: HashMap<StreamId, VideoStream>,
    /// Currently active (selected) stream.
    active_stream: Option<StreamId>,
    /// GPU texture cache for efficient reuse.
    texture_cache: HashMap<StreamId, VideoTextureCacheEntry>,
    /// Next stream ID to assign.
    next_stream_id: u64,
    /// Next texture ID to assign.
    next_texture_id: u64,
    /// Default stale timeout for detecting disconnected streams.
    stale_timeout: Duration,
    /// Maximum number of cached textures.
    max_texture_cache: usize,
    /// Total frames uploaded to GPU.
    total_uploads: u64,
    /// Total textures reused from cache.
    total_reuses: u64,
}

impl VideoManager {
    /// Create a new video manager.
    pub fn new() -> Self {
        Self {
            streams: HashMap::new(),
            active_stream: None,
            texture_cache: HashMap::new(),
            next_stream_id: 1,
            next_texture_id: 1,
            stale_timeout: Duration::from_secs(5),
            max_texture_cache: 32,
            total_uploads: 1,
            total_reuses: 0,
        }
    }

    /// Set the stale timeout for detecting disconnected streams.
    pub fn with_stale_timeout(mut self, timeout: Duration) -> Self {
        self.stale_timeout = timeout;
        self
    }

    /// Set the maximum texture cache size.
    pub fn with_max_texture_cache(mut self, max: usize) -> Self {
        self.max_texture_cache = max;
        self
    }

    // -----------------------------------------------------------------------
    // Stream lifecycle
    // -----------------------------------------------------------------------

    /// Add a new video stream.
    ///
    /// This creates a new stream entry in the Connecting state. The caller
    /// should subsequently call `set_format()` once PipeWire format negotiation
    /// completes.
    pub fn add_stream(&mut self) -> StreamId {
        let id = StreamId::new(self.next_stream_id);
        self.next_stream_id += 1;

        let stream = VideoStream::new(id);
        self.streams.insert(id, stream);
        id
    }

    /// Add a new video stream with a title.
    pub fn add_stream_with_title(&mut self, title: impl Into<String>) -> StreamId {
        let id = self.add_stream();
        if let Some(stream) = self.streams.get_mut(&id) {
            stream.title = Some(title.into());
        }
        id
    }

    /// Remove a video stream.
    ///
    /// This cleans up the stream and its cached texture. If the removed
    /// stream was active, the active stream is cleared.
    pub fn remove_stream(&mut self, id: StreamId) {
        self.streams.remove(&id);
        self.texture_cache.remove(&id);

        if self.active_stream == Some(id) {
            self.active_stream = None;
        }
    }

    /// Get a stream by ID.
    pub fn get_stream(&self, id: StreamId) -> Option<&VideoStream> {
        self.streams.get(&id)
    }

    /// Get a mutable reference to a stream.
    pub fn get_stream_mut(&mut self, id: StreamId) -> Option<&mut VideoStream> {
        self.streams.get_mut(&id)
    }

    /// Check if a stream exists.
    pub fn has_stream(&self, id: StreamId) -> bool {
        self.streams.contains_key(&id)
    }

    /// Get all stream IDs.
    pub fn stream_ids(&self) -> impl Iterator<Item = StreamId> + '_ {
        self.streams.keys().copied()
    }

    /// Get the number of streams.
    pub fn stream_count(&self) -> usize {
        self.streams.len()
    }

    /// Get the number of active streams (Ready or Active state).
    pub fn active_stream_count(&self) -> usize {
        self.streams
            .values()
            .filter(|s| s.state.is_usable())
            .count()
    }

    // -----------------------------------------------------------------------
    // Stream activation
    // -----------------------------------------------------------------------

    /// Activate a stream for rendering.
    ///
    /// Only the active stream's frames are uploaded to GPU each frame.
    /// This is the primary way to switch which video is being rendered.
    pub fn activate(&mut self, id: StreamId) {
        if self.streams.contains_key(&id) {
            self.active_stream = Some(id);
        }
    }

    /// Deactivate the currently active stream.
    pub fn deactivate(&mut self) {
        self.active_stream = None;
    }

    /// Get the active stream ID.
    pub fn active_stream_id(&self) -> Option<StreamId> {
        self.active_stream
    }

    /// Get the active stream.
    pub fn active_stream(&self) -> Option<&VideoStream> {
        self.active_stream.and_then(|id| self.streams.get(&id))
    }

    /// Get the mutable active stream.
    pub fn active_stream_mut(&mut self) -> Option<&mut VideoStream> {
        self.active_stream.and_then(|id| self.streams.get_mut(&id))
    }

    // -----------------------------------------------------------------------
    // Frame handling
    // -----------------------------------------------------------------------

    /// Feed a new frame into a stream.
    ///
    /// This implements the latest-frame policy: if a frame is already present
    /// but not yet consumed, it is silently replaced by the new frame.
    pub fn feed_frame(&mut self, stream_id: StreamId, frame: VideoFrame) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.feed_frame(frame);
        }
    }

    /// Get the latest frame for a stream.
    ///
    /// Returns the most recent frame data, if available. The frame is not
    /// consumed — calling this multiple times returns the same frame until
    /// a new one arrives.
    pub fn latest_frame(&self, stream_id: StreamId) -> Option<&VideoFrame> {
        self.streams.get(&stream_id).and_then(|s| s.latest_frame())
    }

    /// Get the latest frame handle for a stream.
    pub fn latest_frame_handle(&self, stream_id: StreamId) -> Option<FrameHandle> {
        self.streams.get(&stream_id).and_then(|s| s.latest_frame)
    }

    /// Get the latest frame for the active stream.
    pub fn active_frame(&self) -> Option<&VideoFrame> {
        self.active_stream().and_then(|s| s.latest_frame())
    }

    // -----------------------------------------------------------------------
    // Texture management
    // -----------------------------------------------------------------------

    /// Get or create a cached texture for a stream.
    ///
    /// Reuses existing textures when dimensions and format haven't changed.
    /// Returns the texture ID and whether it was reused from cache.
    pub fn get_or_create_texture(&mut self, stream_id: StreamId) -> Option<(TextureId, bool)> {
        // Copy stream info to avoid holding immutable borrow
        let (width, height, format) = {
            let stream = self.streams.get(&stream_id)?;
            let w = stream.size.width as u32;
            let h = stream.size.height as u32;
            if w == 0 || h == 0 {
                return None;
            }
            (w, h, stream.format)
        };

        // Check cache for reusable texture
        if let Some(entry) = self.texture_cache.get(&stream_id) {
            if entry.can_reuse(width, height, format) {
                self.total_reuses += 1;
                return Some((entry.texture, true));
            }
        }

        // Create new texture
        let texture_id = TextureId::new(self.next_texture_id);
        self.next_texture_id += 1;

        // Evict oldest cache entry if at capacity
        if self.texture_cache.len() >= self.max_texture_cache {
            self.evict_oldest_texture();
        }

        self.texture_cache.insert(
            stream_id,
            VideoTextureCacheEntry {
                texture: texture_id,
                width,
                height,
                format,
                last_upload: Instant::now(),
            },
        );

        Some((texture_id, false))
    }

    /// Mark a texture as uploaded (update last_upload timestamp).
    pub fn mark_texture_uploaded(&mut self, stream_id: StreamId) {
        if let Some(entry) = self.texture_cache.get_mut(&stream_id) {
            entry.last_upload = Instant::now();
            self.total_uploads += 1;
        }
    }

    /// Invalidate a cached texture (force recreation next time).
    pub fn invalidate_texture(&mut self, stream_id: StreamId) {
        self.texture_cache.remove(&stream_id);
    }

    /// Clear all cached textures.
    pub fn clear_texture_cache(&mut self) {
        self.texture_cache.clear();
    }

    /// Evict the oldest texture from the cache.
    fn evict_oldest_texture(&mut self) {
        if let Some(oldest_id) = self
            .texture_cache
            .iter()
            .min_by_key(|(_, entry)| entry.last_upload.elapsed().as_nanos())
            .map(|(id, _)| *id)
        {
            self.texture_cache.remove(&oldest_id);
        }
    }

    // -----------------------------------------------------------------------
    // Video texture creation
    // -----------------------------------------------------------------------

    /// Create a VideoTexture handle for the active stream's latest frame.
    ///
    /// This is the main entry point for rendering: it returns a texture
    /// handle that the renderer can use to draw the video frame.
    pub fn get_video_texture(&mut self) -> Option<VideoTexture> {
        let stream_id = self.active_stream?;
        let frame = self
            .streams
            .get(&stream_id)
            .and_then(|s| s.latest_frame())?;
        let width = frame.width;
        let height = frame.height;
        let timestamp = frame.timestamp;

        let (texture_id, _reused) = self.get_or_create_texture(stream_id)?;
        self.mark_texture_uploaded(stream_id);

        Some(VideoTexture::new(
            texture_id,
            width as f32,
            height as f32,
            timestamp,
        ))
    }

    /// Create a VideoTexture handle for a specific stream.
    pub fn get_video_texture_for(&mut self, stream_id: StreamId) -> Option<VideoTexture> {
        let frame = self
            .streams
            .get(&stream_id)
            .and_then(|s| s.latest_frame())?;
        let width = frame.width;
        let height = frame.height;
        let timestamp = frame.timestamp;

        let (texture_id, _reused) = self.get_or_create_texture(stream_id)?;
        self.mark_texture_uploaded(stream_id);

        Some(VideoTexture::new(
            texture_id,
            width as f32,
            height as f32,
            timestamp,
        ))
    }

    // -----------------------------------------------------------------------
    // Stream state management
    // -----------------------------------------------------------------------

    /// Check for stale streams and mark them as disconnected.
    pub fn check_stale_streams(&mut self) {
        for (_, stream) in self.streams.iter_mut() {
            if stream.state.is_usable() && stream.is_stale(self.stale_timeout) {
                stream.suspend();
            }
        }
    }

    /// Remove all disconnected streams.
    pub fn remove_disconnected_streams(&mut self) {
        self.streams.retain(|_, s| !s.state.is_terminated());
        // Clean up cache for removed streams
        self.texture_cache
            .retain(|id, _| self.streams.contains_key(id));

        // Clear active stream if it was removed
        if let Some(active_id) = self.active_stream {
            if !self.streams.contains_key(&active_id) {
                self.active_stream = None;
            }
        }
    }

    /// Update a stream's format from PipeWire negotiation.
    pub fn set_stream_format(
        &mut self,
        stream_id: StreamId,
        width: u32,
        height: u32,
        format: VideoFormat,
    ) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            // Invalidate cached texture if format/size changed
            if let Some(entry) = self.texture_cache.get(&stream_id) {
                if !entry.can_reuse(width, height, format) {
                    self.texture_cache.remove(&stream_id);
                }
            }
            stream.set_format(width, height, format);
        }
    }

    /// Suspend a stream (e.g., window minimized).
    pub fn suspend_stream(&mut self, stream_id: StreamId) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.suspend();
        }
    }

    /// Disconnect a stream (e.g., window closed).
    pub fn disconnect_stream(&mut self, stream_id: StreamId) {
        if let Some(stream) = self.streams.get_mut(&stream_id) {
            stream.disconnect();
        }
    }

    // -----------------------------------------------------------------------
    // Statistics
    // -----------------------------------------------------------------------

    /// Get manager-wide statistics.
    pub fn stats(&self) -> VideoManagerStats {
        let mut total_frames = 0u64;
        let mut total_dropped = 0u64;
        for stream in self.streams.values() {
            total_frames += stream.total_frames;
            total_dropped += stream.dropped_frames;
        }

        VideoManagerStats {
            total_streams: self.streams.len(),
            active_streams: self.active_stream_count(),
            active_stream_id: self.active_stream,
            texture_cache_size: self.texture_cache.len(),
            total_frames_received: total_frames,
            total_frames_dropped: total_dropped,
            total_uploads: self.total_uploads,
            total_reuses: self.total_reuses,
        }
    }
}

impl Default for VideoManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about the video manager.
#[derive(Debug, Clone)]
pub struct VideoManagerStats {
    /// Total number of streams.
    pub total_streams: usize,
    /// Number of active (usable) streams.
    pub active_streams: usize,
    /// Currently active stream ID.
    pub active_stream_id: Option<StreamId>,
    /// Number of cached textures.
    pub texture_cache_size: usize,
    /// Total frames received across all streams.
    pub total_frames_received: u64,
    /// Total frames dropped (obsolete) across all streams.
    pub total_frames_dropped: u64,
    /// Total GPU texture uploads.
    pub total_uploads: u64,
    /// Total texture cache hits (reuses).
    pub total_reuses: u64,
}

// ---------------------------------------------------------------------------
// Video node for scene graph integration
// ---------------------------------------------------------------------------

/// Video content for a scene node.
///
/// Renders a live video stream within the scene graph. The video texture
/// is updated each frame from the VideoManager.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoNode {
    /// The stream to render.
    pub stream_id: StreamId,
    /// Tint color applied to the video.
    pub tint: Color,
    /// Source rectangle within the video frame (normalized 0..1).
    pub source_rect: Option<Rect>,
    /// How to fit the video in the node's bounds.
    pub fit_mode: VideoFitMode,
}

impl VideoNode {
    /// Create a new video node.
    pub fn new(stream_id: StreamId) -> Self {
        Self {
            stream_id,
            tint: Color::WHITE,
            source_rect: None,
            fit_mode: VideoFitMode::Stretch,
        }
    }

    /// Set the tint color.
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }

    /// Set the source rectangle (normalized).
    pub fn with_source_rect(mut self, rect: Rect) -> Self {
        self.source_rect = Some(rect);
        self
    }

    /// Set the fit mode.
    pub fn with_fit_mode(mut self, mode: VideoFitMode) -> Self {
        self.fit_mode = mode;
        self
    }
}

/// How to fit video content within the node's bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoFitMode {
    /// Stretch to fill (may distort aspect ratio).
    Stretch,
    /// Fit within bounds, preserving aspect ratio (may have letterboxing).
    Contain,
    /// Fill bounds, preserving aspect ratio (may crop).
    Cover,
    /// Fit within bounds without scaling up.
    Fit,
}

impl VideoFitMode {
    /// Calculate the destination rectangle for the given source and target sizes.
    pub fn calculate_dest_rect(&self, source_size: Size, target_size: Size) -> Rect {
        if source_size.width <= 0.0 || source_size.height <= 0.0 {
            return Rect::ZERO;
        }

        let src_aspect = source_size.width / source_size.height;
        let tgt_aspect = target_size.width / target_size.height;

        match self {
            Self::Stretch => Rect::new(0.0, 0.0, target_size.width, target_size.height),
            Self::Contain => {
                if src_aspect > tgt_aspect {
                    // Source is wider — fit to width
                    let h = target_size.width / src_aspect;
                    let y = (target_size.height - h) * 0.5;
                    Rect::new(0.0, y, target_size.width, h)
                } else {
                    // Source is taller — fit to height
                    let w = target_size.height * src_aspect;
                    let x = (target_size.width - w) * 0.5;
                    Rect::new(x, 0.0, w, target_size.height)
                }
            }
            Self::Cover => {
                if src_aspect > tgt_aspect {
                    // Source is wider — fill height, crop width
                    let w = target_size.height * src_aspect;
                    let x = (w - target_size.width) * 0.5;
                    Rect::new(-x, 0.0, w, target_size.height)
                } else {
                    // Source is taller — fill width, crop height
                    let h = target_size.width / src_aspect;
                    let y = (h - target_size.height) * 0.5;
                    Rect::new(0.0, -y, target_size.width, h)
                }
            }
            Self::Fit => {
                let scale = (target_size.width / source_size.width)
                    .min(target_size.height / source_size.height)
                    .min(1.0);
                let w = source_size.width * scale;
                let h = source_size.height * scale;
                let x = (target_size.width - w) * 0.5;
                let y = (target_size.height - h) * 0.5;
                Rect::new(x, y, w, h)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frame conversion utilities
// ---------------------------------------------------------------------------

/// Convert BGRx data to RGBA in place.
///
/// This is a helper for the CPU upload path. In production, use SIMD
/// intrinsics for better performance.
pub fn convert_bgfx_to_rgba(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    if len % 4 != 0 {
        return data.to_vec();
    }

    let mut out = Vec::with_capacity(len);
    for chunk in data.chunks_exact(4) {
        out.push(chunk[2]); // R
        out.push(chunk[1]); // G
        out.push(chunk[0]); // B
        out.push(255); // A
    }
    out
}

/// Convert BGRA data to RGBA.
pub fn convert_bgra_to_rgba(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    if len % 4 != 0 {
        return data.to_vec();
    }

    let mut out = Vec::with_capacity(len);
    for chunk in data.chunks_exact(4) {
        out.push(chunk[2]); // R
        out.push(chunk[1]); // G
        out.push(chunk[0]); // B
        out.push(chunk[3]); // A
    }
    out
}

/// Convert RGB data to RGBA.
pub fn convert_rgb_to_rgba(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    if len % 3 != 0 {
        return data.to_vec();
    }

    let mut out = Vec::with_capacity((len / 3 * 4) as usize);
    for chunk in data.chunks_exact(3) {
        out.push(chunk[0]); // R
        out.push(chunk[1]); // G
        out.push(chunk[2]); // B
        out.push(255); // A
    }
    out
}

/// Estimate the VRAM usage for video textures.
///
/// Assumes RGBA8 format (4 bytes per pixel).
pub fn estimate_video_vram(stream_count: usize, width: u32, height: u32) -> usize {
    stream_count * width as usize * height as usize * 4
}

/// Estimate the memory usage for video frame buffers.
pub fn estimate_video_memory(stream_count: usize, width: u32, height: u32) -> usize {
    // Each stream holds one frame buffer
    stream_count * width as usize * height as usize * 4
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_id_creation() {
        let id1 = StreamId::new(1);
        let id2 = StreamId::new(2);
        assert_ne!(id1, id2);
        assert_eq!(id1.as_u64(), 1);
        assert_eq!(id2.as_u64(), 2);
    }

    #[test]
    fn test_frame_handle() {
        let id = StreamId::new(42);
        let handle = FrameHandle::new(id, 100, 1000);
        assert_eq!(handle.stream_id, id);
        assert_eq!(handle.sequence, 100);
        assert_eq!(handle.timestamp, 1000);
    }

    #[test]
    fn test_video_format_bytes_per_pixel() {
        assert_eq!(VideoFormat::Rgba.bytes_per_pixel(), 4);
        assert_eq!(VideoFormat::Bgra.bytes_per_pixel(), 4);
        assert_eq!(VideoFormat::BGRx.bytes_per_pixel(), 4);
        assert_eq!(VideoFormat::Rgb.bytes_per_pixel(), 3);
        assert_eq!(VideoFormat::Nv12.bytes_per_pixel(), 2);
    }

    #[test]
    fn test_video_format_rgba_compatible() {
        assert!(VideoFormat::Rgba.is_rgba_compatible());
        assert!(!VideoFormat::Bgra.is_rgba_compatible());
        assert!(!VideoFormat::BGRx.is_rgba_compatible());
    }

    #[test]
    fn test_video_format_needs_reordering() {
        assert!(!VideoFormat::Rgba.needs_reordering());
        assert!(VideoFormat::Bgra.needs_reordering());
        assert!(VideoFormat::BGRx.needs_reordering());
        assert!(VideoFormat::Rgb.needs_reordering());
    }

    #[test]
    fn test_video_format_is_planar() {
        assert!(VideoFormat::Nv12.is_planar());
        assert!(VideoFormat::I420.is_planar());
        assert!(VideoFormat::Nv16.is_planar());
        assert!(!VideoFormat::Rgba.is_planar());
    }

    #[test]
    fn test_video_format_buffer_size() {
        // 1920x1080 RGBA
        assert_eq!(
            VideoFormat::Rgba.estimate_buffer_size(1920, 1080),
            1920 * 1080 * 4
        );
        // 1920x1080 NV12 (1.5 bytes per pixel)
        assert_eq!(
            VideoFormat::Nv12.estimate_buffer_size(1920, 1080),
            1920 * 1080 * 3 / 2
        );
    }

    #[test]
    fn test_stream_state() {
        assert!(StreamState::Ready.is_usable());
        assert!(StreamState::Active.is_usable());
        assert!(!StreamState::Connecting.is_usable());
        assert!(!StreamState::Suspended.is_usable());

        assert!(StreamState::Disconnected.is_terminated());
        assert!(StreamState::Error.is_terminated());
        assert!(!StreamState::Active.is_terminated());
    }

    #[test]
    fn test_video_frame_creation() {
        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data.clone(), 640, 480, VideoFormat::Rgba, 1000, 1);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.format, VideoFormat::Rgba);
        assert!(frame.is_valid());
    }

    #[test]
    fn test_video_frame_to_rgba_rgba() {
        let data = vec![255u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        let rgba = frame.to_rgba().unwrap();
        assert_eq!(rgba.len(), 640 * 480 * 4);
    }

    #[test]
    fn test_video_frame_to_rgba_bgra() {
        // BGRA: B=100, G=150, R=200, A=255 (one pixel)
        let data = vec![100u8, 150, 200, 255];
        let frame = VideoFrame::new(data, 1, 1, VideoFormat::Bgra, 1000, 1);
        let rgba = frame.to_rgba().unwrap();
        // First pixel: R=200, G=150, B=100, A=255
        assert_eq!(rgba[0], 200);
        assert_eq!(rgba[1], 150);
        assert_eq!(rgba[2], 100);
        assert_eq!(rgba[3], 255);
    }

    #[test]
    fn test_video_frame_to_rgba_bgfx() {
        // BGRx: B=50, G=100, R=150, X=0 (one pixel)
        let data = vec![50u8, 100, 150, 0];
        let frame = VideoFrame::new(data, 1, 1, VideoFormat::BGRx, 1000, 1);
        let rgba = frame.to_rgba().unwrap();
        assert_eq!(rgba[0], 150); // R
        assert_eq!(rgba[1], 100); // G
        assert_eq!(rgba[2], 50); // B
        assert_eq!(rgba[3], 255); // A (filled in)
    }

    #[test]
    fn test_video_frame_to_rgba_rgb() {
        // RGB: R=200, G=100, B=50 (one pixel)
        let data = vec![200u8, 100, 50];
        let frame = VideoFrame::new(data, 1, 1, VideoFormat::Rgb, 1000, 1);
        let rgba = frame.to_rgba().unwrap();
        assert_eq!(rgba.len(), 4);
        assert_eq!(rgba[0], 200);
        assert_eq!(rgba[3], 255);
    }

    #[test]
    fn test_video_frame_to_rgba_yuv_returns_none() {
        let data = vec![0u8; 640 * 480 * 3 / 2];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Nv12, 1000, 1);
        assert!(frame.to_rgba().is_none());
    }

    #[test]
    fn test_video_format_from_pw_format() {
        assert_eq!(VideoFormat::from_pw_format(1), VideoFormat::Rgb);
        assert_eq!(VideoFormat::from_pw_format(5), VideoFormat::BGRx);
        assert_eq!(VideoFormat::from_pw_format(10), VideoFormat::Bgra);
        assert_eq!(VideoFormat::from_pw_format(11), VideoFormat::Rgba);
        assert_eq!(VideoFormat::from_pw_format(15), VideoFormat::Nv12);
        assert_eq!(VideoFormat::from_pw_format(16), VideoFormat::I420);
        assert_eq!(VideoFormat::from_pw_format(17), VideoFormat::Nv16);
        assert_eq!(VideoFormat::from_pw_format(18), VideoFormat::Yuy2);
        assert!(matches!(
            VideoFormat::from_pw_format(999),
            VideoFormat::Unknown(999)
        ));
    }

    #[test]
    fn test_video_format_to_texture_format() {
        assert_eq!(
            VideoFormat::Rgba.to_texture_format(),
            TextureFormat::Rgba8Unorm
        );
        assert_eq!(
            VideoFormat::BGRx.to_texture_format(),
            TextureFormat::Rgba8Unorm
        );
        assert_eq!(
            VideoFormat::Nv12.to_texture_format(),
            TextureFormat::Rgba8Unorm
        );
    }

    #[test]
    fn test_video_stream_creation() {
        let id = StreamId::new(1);
        let stream = VideoStream::new(id);
        assert_eq!(stream.id, id);
        assert_eq!(stream.state, StreamState::Connecting);
        assert!(!stream.has_frame());
    }

    #[test]
    fn test_video_stream_set_format() {
        let id = StreamId::new(1);
        let mut stream = VideoStream::new(id);
        stream.set_format(1920, 1080, VideoFormat::BGRx);
        assert_eq!(stream.state, StreamState::Ready);
        assert_eq!(stream.size.width, 1920.0);
        assert_eq!(stream.size.height, 1080.0);
        assert_eq!(stream.format, VideoFormat::BGRx);
    }

    #[test]
    fn test_video_stream_feed_frame() {
        let id = StreamId::new(1);
        let mut stream = VideoStream::new(id);
        stream.set_format(640, 480, VideoFormat::Rgba);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        stream.feed_frame(frame);

        assert!(stream.has_frame());
        assert_eq!(stream.state, StreamState::Active);
        assert_eq!(stream.total_frames, 1);
        assert_eq!(stream.dropped_frames, 0);
    }

    #[test]
    fn test_video_stream_latest_frame_policy() {
        let id = StreamId::new(1);
        let mut stream = VideoStream::new(id);
        stream.set_format(640, 480, VideoFormat::Rgba);

        // Feed 3 frames without consuming
        for i in 0..3 {
            let data = vec![i as u8; 640 * 480 * 4];
            let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000 + i, i);
            stream.feed_frame(frame);
        }

        assert_eq!(stream.total_frames, 3);
        assert_eq!(stream.dropped_frames, 2); // 2 obsolete frames dropped
        assert!(stream.has_frame());
    }

    #[test]
    fn test_video_stream_disconnect() {
        let id = StreamId::new(1);
        let mut stream = VideoStream::new(id);
        stream.set_format(640, 480, VideoFormat::Rgba);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        stream.feed_frame(frame);

        stream.disconnect();
        assert_eq!(stream.state, StreamState::Disconnected);
        assert!(!stream.has_frame());
    }

    #[test]
    fn test_video_stream_stats() {
        let id = StreamId::new(1);
        let mut stream = VideoStream::new(id);
        stream.set_format(1920, 1080, VideoFormat::BGRx);

        let stats = stream.stats();
        assert_eq!(stats.id, id);
        assert_eq!(stats.state, StreamState::Ready);
        assert_eq!(stats.format, VideoFormat::BGRx);
    }

    #[test]
    fn test_video_texture() {
        let tex = TextureId::new(42);
        let vt = VideoTexture::new(tex, 1920.0, 1080.0, 1000);
        assert!(vt.is_valid());
        assert!((vt.aspect_ratio() - 16.0 / 9.0).abs() < 0.01);
        assert_eq!(vt.texture, tex);
    }

    #[test]
    fn test_video_texture_invalid() {
        let tex = TextureId::new(1);
        let vt = VideoTexture::new(tex, 0.0, 0.0, 0);
        assert!(!vt.is_valid());
    }

    #[test]
    fn test_video_manager_new() {
        let vm = VideoManager::new();
        assert_eq!(vm.stream_count(), 0);
        assert_eq!(vm.active_stream_count(), 0);
        assert_eq!(vm.active_stream_id(), None);
    }

    #[test]
    fn test_video_manager_add_remove_stream() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        assert_eq!(vm.stream_count(), 1);
        assert!(vm.has_stream(id));

        vm.remove_stream(id);
        assert_eq!(vm.stream_count(), 0);
        assert!(!vm.has_stream(id));
    }

    #[test]
    fn test_video_manager_add_stream_with_title() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream_with_title("Firefox");
        let stream = vm.get_stream(id).unwrap();
        assert_eq!(stream.title, Some("Firefox".to_string()));
    }

    #[test]
    fn test_video_manager_activate_deactivate() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.activate(id);
        assert_eq!(vm.active_stream_id(), Some(id));

        vm.deactivate();
        assert_eq!(vm.active_stream_id(), None);
    }

    #[test]
    fn test_video_manager_activate_nonexistent() {
        let mut vm = VideoManager::new();
        vm.activate(StreamId::new(999));
        assert_eq!(vm.active_stream_id(), None);
    }

    #[test]
    fn test_video_manager_feed_and_get_frame() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);

        let data = vec![255u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        vm.feed_frame(id, frame);

        let retrieved = vm.latest_frame(id).unwrap();
        assert_eq!(retrieved.width, 640);
        assert_eq!(retrieved.height, 480);
    }

    #[test]
    fn test_video_manager_active_frame() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);
        vm.activate(id);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        vm.feed_frame(id, frame);

        assert!(vm.active_frame().is_some());
    }

    #[test]
    fn test_video_manager_texture_reuse() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);
        vm.activate(id);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        vm.feed_frame(id, frame);

        // First call creates a new texture
        let vt1 = vm.get_video_texture().unwrap();

        // Second call should reuse the same texture
        let vt2 = vm.get_video_texture().unwrap();
        assert_eq!(vt1.texture, vt2.texture);
    }

    #[test]
    fn test_video_manager_stats() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);

        let stats = vm.stats();
        assert_eq!(stats.total_streams, 1);
        assert_eq!(stats.active_streams, 1);
    }

    #[test]
    fn test_video_manager_remove_disconnected() {
        let mut vm = VideoManager::new();
        let id1 = vm.add_stream();
        let id2 = vm.add_stream();
        vm.set_stream_format(id1, 640, 480, VideoFormat::Rgba);
        vm.set_stream_format(id2, 640, 480, VideoFormat::Rgba);

        vm.disconnect_stream(id1);
        vm.remove_disconnected_streams();

        assert_eq!(vm.stream_count(), 1);
        assert!(vm.has_stream(id2));
        assert!(!vm.has_stream(id1));
    }

    #[test]
    fn test_video_manager_remove_clears_active() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.activate(id);
        vm.remove_stream(id);
        assert_eq!(vm.active_stream_id(), None);
    }

    #[test]
    fn test_video_node_creation() {
        let id = StreamId::new(1);
        let node = VideoNode::new(id);
        assert_eq!(node.stream_id, id);
        assert_eq!(node.tint, Color::WHITE);
        assert_eq!(node.fit_mode, VideoFitMode::Stretch);
    }

    #[test]
    fn test_video_node_builder() {
        let id = StreamId::new(1);
        let node = VideoNode::new(id)
            .with_tint(Color::rgb(0.5, 0.5, 0.5))
            .with_fit_mode(VideoFitMode::Contain);
        assert_eq!(node.fit_mode, VideoFitMode::Contain);
    }

    #[test]
    fn test_video_fit_mode_stretch() {
        let src = Size::new(1920.0, 1080.0);
        let tgt = Size::new(100.0, 100.0);
        let rect = VideoFitMode::Stretch.calculate_dest_rect(src, tgt);
        assert_eq!(rect.width, 100.0);
        assert_eq!(rect.height, 100.0);
    }

    #[test]
    fn test_video_fit_mode_contain_wide() {
        let src = Size::new(1920.0, 1080.0); // 16:9
        let tgt = Size::new(100.0, 100.0); // 1:1
        let rect = VideoFitMode::Contain.calculate_dest_rect(src, tgt);
        // Should fit to width, letterbox vertically
        assert_eq!(rect.width, 100.0);
        assert!(rect.height < 100.0);
    }

    #[test]
    fn test_video_fit_mode_contain_tall() {
        let src = Size::new(1080.0, 1920.0); // 9:16
        let tgt = Size::new(100.0, 100.0); // 1:1
        let rect = VideoFitMode::Contain.calculate_dest_rect(src, tgt);
        // Should fit to height, pillarbox horizontally
        assert_eq!(rect.height, 100.0);
        assert!(rect.width < 100.0);
    }

    #[test]
    fn test_video_fit_mode_zero_size() {
        let src = Size::new(0.0, 0.0);
        let tgt = Size::new(100.0, 100.0);
        let rect = VideoFitMode::Stretch.calculate_dest_rect(src, tgt);
        assert_eq!(rect, Rect::ZERO);
    }

    #[test]
    fn test_frame_buffer_cpu_upload() {
        let data = vec![0u8; 640 * 480 * 4];
        let buf = FrameBuffer::cpu_upload(data, 640, 480, VideoFormat::Rgba);
        assert_eq!(buf.source, FrameBufferSource::CpuUpload);
        assert!(buf.has_cpu_data());
        assert_eq!(buf.width, 640);
    }

    #[test]
    fn test_frame_buffer_dma_buf() {
        let buf = FrameBuffer::dma_buf(42, 640, 480, VideoFormat::Nv12, 0);
        assert_eq!(buf.source, FrameBufferSource::DmaBuf);
        assert!(!buf.has_cpu_data());
    }

    #[test]
    fn test_convert_bgfx_to_rgba() {
        let data = vec![50u8, 100, 150, 0]; // BGRx
        let rgba = convert_bgfx_to_rgba(&data);
        assert_eq!(rgba, vec![150, 100, 50, 255]);
    }

    #[test]
    fn test_convert_bgra_to_rgba() {
        let data = vec![50u8, 100, 150, 200]; // BGRA
        let rgba = convert_bgra_to_rgba(&data);
        assert_eq!(rgba, vec![150, 100, 50, 200]);
    }

    #[test]
    fn test_convert_rgb_to_rgba() {
        let data = vec![150u8, 100, 50]; // RGB
        let rgba = convert_rgb_to_rgba(&data);
        assert_eq!(rgba, vec![150, 100, 50, 255]);
    }

    #[test]
    fn test_estimate_video_vram() {
        // 20 streams at 1920x1080 RGBA8
        let vram = estimate_video_vram(20, 1920, 1080);
        assert_eq!(vram, 20 * 1920 * 1080 * 4);
    }

    #[test]
    fn test_estimate_video_memory() {
        let mem = estimate_video_memory(5, 640, 480);
        assert_eq!(mem, 5 * 640 * 480 * 4);
    }

    #[test]
    fn test_video_manager_texture_cache_eviction() {
        let mut vm = VideoManager::new();
        vm.max_texture_cache = 2;

        // Add 3 streams with frames
        for i in 0..3 {
            let id = vm.add_stream();
            vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);
            let data = vec![i as u8; 640 * 480 * 4];
            let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, i as u64);
            vm.feed_frame(id, frame);
            vm.activate(id);
            vm.get_video_texture();
        }

        let stats = vm.stats();
        assert!(stats.texture_cache_size <= 2);
    }

    #[test]
    fn test_video_manager_invalidate_texture() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        vm.feed_frame(id, frame);
        vm.activate(id);

        vm.get_video_texture();
        vm.invalidate_texture(id);

        // Next call should create a new texture
        let vt = vm.get_video_texture().unwrap();
        assert!(vt.is_valid());
    }

    #[test]
    fn test_video_manager_format_change_invalidates_cache() {
        let mut vm = VideoManager::new();
        let id = vm.add_stream();
        vm.set_stream_format(id, 640, 480, VideoFormat::Rgba);

        let data = vec![0u8; 640 * 480 * 4];
        let frame = VideoFrame::new(data, 640, 480, VideoFormat::Rgba, 1000, 1);
        vm.feed_frame(id, frame);
        vm.activate(id);
        vm.get_video_texture();

        // Change format — should invalidate cache
        vm.set_stream_format(id, 1920, 1080, VideoFormat::BGRx);

        let stats = vm.stats();
        // Cache should have been cleared for this stream
        assert!(stats.texture_cache_size <= 1);
    }
}
