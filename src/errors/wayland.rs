//! Wayland-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to Wayland compositor integration, surface management,
//! and protocol communication.
//!
//! # Recovery
//! - Connection lost: Reconnect if supported by the compositor.
//! - Surface errors: Recreate the affected surface.
//! - Protocol errors: Fix the protocol usage.
//!
//! # Dependencies
//! Depends on `wgpu_api::WaylandError` for low-level Wayland errors.

use thiserror::Error;

/// Wayland-specific errors with application-level context.
#[derive(Debug, Error)]
pub enum WaylandAppError {
    #[error("Wayland connection lost: {0}")]
    ConnectionLost(String),

    #[error("Wayland surface destroyed by compositor")]
    SurfaceDestroyedByCompositor,

    #[error("Layer shell surface rejected: {0}")]
    LayerShellRejected(String),

    #[error("Wayland protocol version mismatch: expected {expected}, got {actual}")]
    ProtocolVersionMismatch { expected: u32, actual: u32 },

    #[error("Wayland event queue blocked for {ms} ms")]
    EventQueueBlocked { ms: u64 },

    #[error("Output configuration changed unexpectedly")]
    OutputConfigurationChanged,

    #[error("Scale factor changed: {old} -> {new}")]
    ScaleFactorChanged { old: f64, new: f64 },
}

impl WaylandAppError {
    /// Returns `true` if this Wayland error can be recovered.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            WaylandAppError::OutputConfigurationChanged
                | WaylandAppError::ScaleFactorChanged { .. }
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            WaylandAppError::ConnectionLost(_) => {
                "Attempt to reconnect to Wayland display"
            }
            WaylandAppError::SurfaceDestroyedByCompositor => {
                "Recreate Wayland surface and reconfigure GPU"
            }
            WaylandAppError::LayerShellRejected(_) => {
                "Check compositor support for layer shell protocol"
            }
            WaylandAppError::ProtocolVersionMismatch { .. } => {
                "Update Wayland protocol version compatibility"
            }
            WaylandAppError::EventQueueBlocked { .. } => {
                "Check for blocking operations in event handlers"
            }
            WaylandAppError::OutputConfigurationChanged => {
                "Requery output configuration and resize surfaces"
            }
            WaylandAppError::ScaleFactorChanged { .. } => {
                "Resize all surfaces to match new scale factor"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_config_recoverable() {
        let err = WaylandAppError::OutputConfigurationChanged;
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_scale_factor_recoverable() {
        let err = WaylandAppError::ScaleFactorChanged { old: 1.0, new: 2.0 };
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("scale"));
    }

    #[test]
    fn test_connection_lost_not_recoverable() {
        let err = WaylandAppError::ConnectionLost("display closed".into());
        assert!(!err.is_recoverable());
    }
}
