//! Shared error handling architecture for Dock.
//!
//! # Purpose
//! Provides a modular error system with subsystem-specific error types,
//! unified error propagation, and integration with diagnostics and recovery.
//!
//! # Design
//! - Each subsystem has its own error type file for clarity and maintainability.
//! - `AppError` serves as the unified application-level error type.
//! - Errors integrate with `tracing` for structured logging.
//! - Errors integrate with `miette` for human-readable diagnostics.
//! - Recovery strategies are defined per error type.
//!
//! # Subsystem Boundaries
//! Each error module owns its subsystem's error definitions. Cross-subsystem
//! errors are wrapped in `AppError` with context from the calling subsystem.
//!
//! # Dependencies
//! - `wgpu_api::error`: Subsystem-specific errors from the library crate.
//! - `thiserror`: Error derive macro.
//! - `anyhow`: Application-level error propagation.
//! - `miette`: Human-readable diagnostics.
//! - `tracing`: Structured logging integration.

mod gpu;
mod wayland;
mod pipewire;
mod ui;
mod memory;
mod system;

pub use gpu::*;
pub use wayland::*;
pub use pipewire::*;
pub use ui::*;
pub use memory::*;
pub use system::*;

// Re-export from parent module for convenience
pub use crate::error::{AppError, AppResult};

/// Determines whether an error is recoverable without application restart.
///
/// # Recovery Strategy
/// - Recoverable errors trigger automatic recovery attempts.
/// - Non-recoverable errors require user intervention or application restart.
pub trait Recoverable {
    /// Returns `true` if this error can be automatically recovered.
    fn is_recoverable(&self) -> bool;

    /// Returns a human-readable description of the recovery strategy.
    fn recovery_strategy(&self) -> Option<&'static str>;
}

impl Recoverable for AppError {
    fn is_recoverable(&self) -> bool {
        matches!(
            self,
            AppError::ServiceError(_)
                | AppError::ListenerError(_)
                | AppError::IconError(_)
                | AppError::ProcessError(_)
        )
    }

    fn recovery_strategy(&self) -> Option<&'static str> {
        match self {
            AppError::ServiceError(_) => Some("Restart the affected service"),
            AppError::ListenerError(_) => Some("Reconnect the listener"),
            AppError::IconError(_) => Some("Fallback to default icon"),
            AppError::ProcessError(_) => Some("Skip the failed process resolution"),
            AppError::DbusError(_) => Some("Retry D-Bus connection with backoff"),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recoverable_service_error() {
        let err = AppError::ServiceError("test".into());
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().is_some());
    }

    #[test]
    fn test_not_recoverable_init_error() {
        let err = AppError::InitFailed("test".into());
        assert!(!err.is_recoverable());
        assert!(err.recovery_strategy().is_none());
    }

    #[test]
    fn test_recoverable_dbus_error() {
        let err = AppError::DbusError("connection refused".into());
        assert!(!err.is_recoverable());
        assert_eq!(err.recovery_strategy(), Some("Retry D-Bus connection with backoff"));
    }
}
