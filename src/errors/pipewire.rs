//! PipeWire-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to PipeWire screen capture, stream management,
//! and buffer handling.
//!
//! # Recovery
//! - Stream disconnected: Reconnect or recreate the stream.
//! - Buffer unavailable: Wait or request another frame.
//! - Format negotiation failed: Try alternative formats.
//!
//! # Dependencies
//! Depends on `wgpu_api::VideoError` for low-level PipeWire errors.

use thiserror::Error;

/// PipeWire-specific errors with application-level context.
#[derive(Debug, Error)]
pub enum PipeWireAppError {
    #[error("PipeWire stream disconnected: {0}")]
    StreamDisconnected(String),

    #[error("PipeWire buffer unavailable, no frame available")]
    BufferUnavailable,

    #[error("PipeWire format negotiation failed: {0}")]
    FormatNegotiationFailed(String),

    #[error("PipeWire permission denied for capture")]
    PermissionDenied,

    #[error("PipeWire node not found: {node_id}")]
    NodeNotFound { node_id: u32 },

    #[error("PipeWire main loop error: {0}")]
    MainLoopError(String),

    #[error("DMA-BUF sharing not available, falling back to CPU upload")]
    DmaBufFallback,

    #[error("PipeWire stream underrun: frames dropped")]
    StreamUnderrun,
}

impl PipeWireAppError {
    /// Returns `true` if this PipeWire error can be recovered.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            PipeWireAppError::StreamDisconnected(_)
                | PipeWireAppError::BufferUnavailable
                | PipeWireAppError::DmaBufFallback
                | PipeWireAppError::StreamUnderrun
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            PipeWireAppError::StreamDisconnected(_) => {
                "Reconnect to PipeWire and recreate capture stream"
            }
            PipeWireAppError::BufferUnavailable => "Wait for next frame or skip this frame",
            PipeWireAppError::FormatNegotiationFailed(_) => "Try alternative format or resolution",
            PipeWireAppError::PermissionDenied => "Grant screen capture permission via portal",
            PipeWireAppError::NodeNotFound { .. } => "List available nodes and select alternative",
            PipeWireAppError::MainLoopError(_) => "Restart PipeWire main loop",
            PipeWireAppError::DmaBufFallback => "Continue with CPU upload path",
            PipeWireAppError::StreamUnderrun => {
                "Wait for the next buffer/frame, skip underrun frames, or adjust stream buffering/latency if configured"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stream_disconnected_recoverable() {
        let err = PipeWireAppError::StreamDisconnected("timeout".into());
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("Reconnect"));
    }

    #[test]
    fn test_buffer_unavailable_recoverable() {
        let err = PipeWireAppError::BufferUnavailable;
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_permission_denied_not_recoverable() {
        let err = PipeWireAppError::PermissionDenied;
        assert!(!err.is_recoverable());
    }

    #[test]
    fn test_dma_buf_fallback_recoverable() {
        let err = PipeWireAppError::DmaBufFallback;
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("CPU"));
    }
}
