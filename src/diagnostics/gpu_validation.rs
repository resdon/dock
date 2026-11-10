//! GPU validation and error scope monitoring.
//!
//! # Purpose
//! Provides explicit GPU error and warning recording. This helper does not
//! install wgpu error scopes or an uncaptured-error callback.
//!
//! # Usage
//! ```rust
//! use dock::diagnostics::GpuValidator;
//!
//! let validator = GpuValidator::new();
//! validator.record_error("texture upload", "invalid image dimensions");
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// A GPU validation error recorded by [`GpuValidator`].
#[derive(Debug, Clone)]
pub struct GpuValidationError {
    /// Label of the resource that caused the error.
    pub label: String,
    /// Description of the error.
    pub message: String,
    /// Error count at the time of detection.
    pub error_count: usize,
}

/// Counts and logs GPU errors and warnings explicitly reported by callers.
///
/// This helper does not install wgpu error scopes or a device error callback.
#[derive(Debug)]
pub struct GpuValidator {
    error_count: Arc<AtomicUsize>,
    warning_count: Arc<AtomicUsize>,
}

impl GpuValidator {
    /// Create a new GPU validator.
    pub fn new() -> Self {
        Self {
            error_count: Arc::new(AtomicUsize::new(0)),
            warning_count: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Record a GPU error.
    pub fn record_error(
        &self,
        label: impl Into<String>,
        message: impl Into<String>,
    ) -> GpuValidationError {
        let label = label.into();
        let message = message.into();
        let count = self.error_count.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::error!(
            subsystem = "gpu",
            error_count = count,
            label = %label,
            error = %message,
            "GPU validation error"
        );
        GpuValidationError {
            label,
            message,
            error_count: count,
        }
    }

    /// Record a GPU warning.
    pub fn record_warning(&self, label: impl Into<String>, message: impl Into<String>) {
        let count = self.warning_count.fetch_add(1, Ordering::Relaxed) + 1;
        tracing::warn!(
            subsystem = "gpu",
            warning_count = count,
            label = %label.into(),
            warning = %message.into(),
            "GPU validation warning"
        );
    }

    /// Get the total error count.
    pub fn error_count(&self) -> usize {
        self.error_count.load(Ordering::Relaxed)
    }

    /// Get the total warning count.
    pub fn warning_count(&self) -> usize {
        self.warning_count.load(Ordering::Relaxed)
    }

    /// Check if any errors have been recorded.
    pub fn has_errors(&self) -> bool {
        self.error_count() > 0
    }

    /// Reset all error and warning counts.
    pub fn reset(&self) {
        self.error_count.store(0, Ordering::Relaxed);
        self.warning_count.store(0, Ordering::Relaxed);
    }

    /// Get a summary of GPU validation status.
    pub fn status_summary(&self) -> String {
        format!(
            "GPU Validation: {} errors, {} warnings",
            self.error_count(),
            self.warning_count()
        )
    }
}

impl Default for GpuValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Callback type for GPU error handling.
pub type GpuErrorCallback = Box<dyn Fn(&str, &str) + Send + Sync>;

/// Callback wrapper for uncaptured GPU errors.
///
/// # Purpose
/// Provides a callback for an integration layer to install on a wgpu device.
/// This type does not install itself because `dock` does not own wgpu devices.
pub struct UncapturedErrorHandler {
    callback: Option<GpuErrorCallback>,
}

impl UncapturedErrorHandler {
    /// Create a new uncaptured error handler.
    pub fn new(callback: impl Fn(&str, &str) + Send + Sync + 'static) -> Self {
        Self {
            callback: Some(Box::new(callback)),
        }
    }

    /// Handle an uncaptured GPU error.
    pub fn handle(&self, label: &str, message: &str) {
        if let Some(ref callback) = self.callback {
            callback(label, message);
        }
        tracing::error!(
            subsystem = "gpu",
            label = label,
            error = message,
            "Uncaptured GPU error"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validator_creation() {
        let validator = GpuValidator::new();
        assert_eq!(validator.error_count(), 0);
        assert_eq!(validator.warning_count(), 0);
        assert!(!validator.has_errors());
    }

    #[test]
    fn test_record_error() {
        let validator = GpuValidator::new();
        let error = validator.record_error("test", "validation failed");
        assert_eq!(validator.error_count(), 1);
        assert!(validator.has_errors());
        assert_eq!(error.label, "test");
        assert_eq!(error.message, "validation failed");
        assert_eq!(error.error_count, 1);
    }

    #[test]
    fn test_record_warning() {
        let validator = GpuValidator::new();
        validator.record_warning("test", "deprecation");
        assert_eq!(validator.warning_count(), 1);
    }

    #[test]
    fn test_reset() {
        let validator = GpuValidator::new();
        validator.record_error("test", "error");
        validator.record_warning("test", "warning");
        validator.reset();

        assert_eq!(validator.error_count(), 0);
        assert_eq!(validator.warning_count(), 0);
    }

    #[test]
    fn test_status_summary() {
        let validator = GpuValidator::new();
        validator.record_error("test", "error");
        let summary = validator.status_summary();
        assert!(summary.contains("1 errors"));
    }

    #[test]
    fn test_uncaptured_handler() {
        let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let called_clone = called.clone();

        let handler = UncapturedErrorHandler::new(move |_label, _msg| {
            called_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        handler.handle("test", "error");
        assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    }
}
