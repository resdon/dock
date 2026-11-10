//! Memory-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to memory allocation, GPU memory management,
//! and resource lifetime violations.
//!
//! # Recovery
//! - Allocation failure: Propagate failure; avoid unsafe continuation.
//! - Memory pressure: Release unused resources.
//!
//! # Invariants
//! Memory errors should never be silently ignored. They must either
//! trigger recovery or propagate to the application level.

use thiserror::Error;

/// Memory-specific errors with application-level context.
#[derive(Debug, Error)]
pub enum MemoryAppError {
    #[error("Memory allocation failed: requested {requested_mb} MB")]
    AllocationFailed { requested_mb: u64 },

    #[error("GPU memory limit exceeded: {used_mb}/{limit_mb} MB")]
    GpuMemoryLimitExceeded { used_mb: u64, limit_mb: u64 },

    #[error("Texture cache overflow: {count} textures, {memory_mb} MB")]
    TextureCacheOverflow { count: u32, memory_mb: u64 },

    #[error("Memory leak detected: {growth_mb} MB growth in {seconds} seconds")]
    LeakDetected { growth_mb: u64, seconds: u64 },

    #[error("Buffer pool exhausted")]
    BufferPoolExhausted,

    #[error("DMA-BUF memory mapping failed: {0}")]
    DmaBufMappingFailed(String),
}

impl MemoryAppError {
    /// Returns `true` if this memory error can be recovered.
    /// Memory allocation failures are generally not recoverable
    /// without releasing resources first.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            MemoryAppError::TextureCacheOverflow { .. } | MemoryAppError::BufferPoolExhausted
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            MemoryAppError::AllocationFailed { .. } => {
                "Cannot recover: propagate failure to caller"
            }
            MemoryAppError::GpuMemoryLimitExceeded { .. } => {
                "Release unused GPU textures and buffers"
            }
            MemoryAppError::TextureCacheOverflow { .. } => {
                "Evict least-recently-used textures from cache"
            }
            MemoryAppError::LeakDetected { .. } => "Investigate and fix the memory leak source",
            MemoryAppError::BufferPoolExhausted => "Wait for buffers to be returned or expand pool",
            MemoryAppError::DmaBufMappingFailed(_) => "Fall back to CPU-based buffer handling",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_failed_not_recoverable() {
        let err = MemoryAppError::AllocationFailed { requested_mb: 1024 };
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_cache_overflow_recoverable() {
        let err = MemoryAppError::TextureCacheOverflow {
            count: 1000,
            memory_mb: 4096,
        };
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("Evict"));
    }

    #[test]
    fn test_buffer_pool_recoverable() {
        let err = MemoryAppError::BufferPoolExhausted;
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_leak_detected_not_recoverable() {
        let err = MemoryAppError::LeakDetected {
            growth_mb: 500,
            seconds: 60,
        };
        assert!(!err.is_recoverable());
    }
}
