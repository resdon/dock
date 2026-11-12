//! Rendering modules.
//!
//! GPU rendering via wgpu-api with DMA-BUF export.

pub mod debug_overlay;
pub mod dmabuf_export;
pub mod theme;
pub mod utils;

pub use utils::hsl_to_rgb;
