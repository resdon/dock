//! UI-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to Slint widget rendering, dock UI components,
//! and visual element management.
//!
//! # Recovery
//! - Rendering failure: Report and restart the affected rendering path.
//! - Widget errors: Recreate the affected widget.
//!
//! # Dependencies
//! Independent of wgpu-api errors; focuses on application-level UI issues.

use thiserror::Error;

/// UI-specific errors with application-level context.
#[derive(Debug, Error)]
pub enum UiAppError {
    #[error("Slint widget creation failed: {0}")]
    WidgetCreationFailed(String),

    #[error("Slint rendering failed: {0}")]
    RenderingFailed(String),

    #[error("Dock icon layout failed: {0}")]
    IconLayoutFailed(String),

    #[error("System bar update failed: {0}")]
    SystemBarUpdateFailed(String),

    #[error("Popup surface creation failed: {0}")]
    PopupCreationFailed(String),

    #[error("Widget state synchronization failed")]
    StateSyncFailed,

    #[error("UI event handler panic: {0}")]
    EventHandlerPanic(String),
}

impl UiAppError {
    /// Returns `true` if this UI error can be recovered.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            UiAppError::RenderingFailed(_)
                | UiAppError::IconLayoutFailed(_)
                | UiAppError::SystemBarUpdateFailed(_)
                | UiAppError::StateSyncFailed
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            UiAppError::WidgetCreationFailed(_) => "Recreate the widget with default configuration",
            UiAppError::RenderingFailed(_) => "Restart the affected rendering path",
            UiAppError::IconLayoutFailed(_) => "Recalculate dock icon layout",
            UiAppError::SystemBarUpdateFailed(_) => "Retry system bar update with cached data",
            UiAppError::PopupCreationFailed(_) => "Defer popup creation until next trigger",
            UiAppError::StateSyncFailed => "Reset widget state and resync",
            UiAppError::EventHandlerPanic(_) => "Restart the event handler",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rendering_failed_recoverable() {
        let err = UiAppError::RenderingFailed("timeout".into());
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_state_sync_recoverable() {
        let err = UiAppError::StateSyncFailed;
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("resync"));
    }

    #[test]
    fn test_widget_creation_not_recoverable() {
        let err = UiAppError::WidgetCreationFailed("invalid config".into());
        assert!(!err.is_recoverable());
    }
}
