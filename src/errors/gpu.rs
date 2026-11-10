//! GPU-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to GPU operations, wgpu device management,
//! and Vulkan backend issues.
//!
//! # Recovery
//! - Device lost: Reinitialize GPU if safe.
//! - Out of memory: Release resources and reduce allocations.
//! - Validation errors: Fix the offending operation.
//!
//! # Dependencies
//! Depends on `wgpu_api::GpuError` for low-level GPU errors.

use thiserror::Error;

/// GPU-specific errors that extend the wgpu-api GPU errors with
/// application-level context and recovery strategies.
#[derive(Debug, Error)]
pub enum GpuAppError {
    #[error("GPU device lost during rendering: {0}")]
    DeviceLostDuringRendering(String),

    #[error("GPU surface recreation failed after device loss")]
    SurfaceRecreationAfterDeviceLoss,

    #[error("GPU memory pressure: {current_mb} MB used, {limit_mb} MB limit")]
    MemoryPressure { current_mb: u64, limit_mb: u64 },

    #[error("GPU frame timeout, possible hang")]
    FrameTimeout,

    #[error("GPU presentation failed: {0}")]
    PresentationFailed(String),

    #[error("DMA-BUF export failed: {0}")]
    DmaBufExportFailed(String),

    #[error("GPU shader compilation cache corrupted")]
    ShaderCacheCorrupted,

    #[error("GPU texture format not supported: {format}")]
    TextureFormatNotSupported { format: String },
}

impl GpuAppError {
    /// Returns `true` if this GPU error can be recovered without full restart.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            GpuAppError::MemoryPressure { .. }
                | GpuAppError::PresentationFailed(_)
                | GpuAppError::DmaBufExportFailed(_)
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            GpuAppError::DeviceLostDuringRendering(_) => {
                "Reinitialize GPU device and recreate all textures"
            }
            GpuAppError::SurfaceRecreationAfterDeviceLoss => {
                "Recreate Wayland surface and reconfigure GPU"
            }
            GpuAppError::MemoryPressure { .. } => {
                "Release unused textures and reduce allocation size"
            }
            GpuAppError::FrameTimeout => "Reset GPU command encoder and retry frame",
            GpuAppError::PresentationFailed(_) => "Retry presentation or recreate swapchain",
            GpuAppError::DmaBufExportFailed(_) => "Fall back to CPU-based presentation",
            GpuAppError::ShaderCacheCorrupted => "Clear shader cache and recompile",
            GpuAppError::TextureFormatNotSupported { .. } => "Use fallback texture format",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_pressure_recoverable() {
        let err = GpuAppError::MemoryPressure {
            current_mb: 7500,
            limit_mb: 8192,
        };
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("Release"));
    }

    #[test]
    fn test_device_lost_not_recoverable() {
        let err = GpuAppError::DeviceLostDuringRendering("test".into());
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_presentation_failed_recoverable() {
        let err = GpuAppError::PresentationFailed("timeout".into());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_dma_buf_fallback() {
        let err = GpuAppError::DmaBufExportFailed("not supported".into());
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("CPU"));
    }
}
