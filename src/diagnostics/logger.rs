//! Structured logging with subsystem-specific filters.
//!
//! # Purpose
//! Provides typed logging helpers that integrate with `tracing` for
//! structured diagnostics across all subsystems.
//!
//! # Usage
//! ```rust
//! use dock::diagnostics::SubsystemLogger;
//!
//! let logger = SubsystemLogger::new("gpu");
//! logger.info("Renderer initialized");
//! logger.error("Device lost", "Reinitializing GPU");
//! ```

use tracing::{error, info, warn, debug, trace};

/// Identifies a subsystem for structured logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsystem {
    Gpu,
    Renderer,
    Physics,
    Video,
    Wayland,
    Scene,
    Layout,
    Text,
    Interaction,
    Animation,
    Ui,
    System,
    Dbus,
    Service,
}

impl std::fmt::Display for Subsystem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Subsystem::Gpu => write!(f, "gpu"),
            Subsystem::Renderer => write!(f, "renderer"),
            Subsystem::Physics => write!(f, "physics"),
            Subsystem::Video => write!(f, "video"),
            Subsystem::Wayland => write!(f, "wayland"),
            Subsystem::Scene => write!(f, "scene"),
            Subsystem::Layout => write!(f, "layout"),
            Subsystem::Text => write!(f, "text"),
            Subsystem::Interaction => write!(f, "interaction"),
            Subsystem::Animation => write!(f, "animation"),
            Subsystem::Ui => write!(f, "ui"),
            Subsystem::System => write!(f, "system"),
            Subsystem::Dbus => write!(f, "dbus"),
            Subsystem::Service => write!(f, "service"),
        }
    }
}

/// A subsystem-specific logger that adds structured context to all log messages.
///
/// # Examples
/// ```
/// use dock::diagnostics::SubsystemLogger;
///
/// let logger = SubsystemLogger::new(Subsystem::Gpu);
/// logger.info("Renderer initialized successfully");
/// ```
#[derive(Debug, Clone)]
pub struct SubsystemLogger {
    subsystem: Subsystem,
}

impl SubsystemLogger {
    /// Create a new logger for the given subsystem.
    pub fn new(subsystem: Subsystem) -> Self {
        Self { subsystem }
    }

    /// Log an info message with subsystem context.
    pub fn info(&self, message: impl std::fmt::Display) {
        info!(subsystem = %self.subsystem, %message);
    }

    /// Log a warning message with subsystem context.
    pub fn warn(&self, message: impl std::fmt::Display, context: impl std::fmt::Display) {
        warn!(subsystem = %self.subsystem, context = %context, %message);
    }

    /// Log an error message with subsystem context.
    pub fn error(&self, message: impl std::fmt::Display, context: impl std::fmt::Display) {
        error!(subsystem = %self.subsystem, context = %context, %message);
    }

    /// Log a debug message with subsystem context.
    pub fn debug(&self, message: impl std::fmt::Display) {
        debug!(subsystem = %self.subsystem, %message);
    }

    /// Log a trace message with subsystem context.
    pub fn trace(&self, message: impl std::fmt::Display) {
        trace!(subsystem = %self.subsystem, %message);
    }

    /// Report a GPU error with structured context.
    pub fn report_gpu_error(&self, message: impl std::fmt::Display) {
        error!(
            subsystem = "gpu",
            error = %message,
            "GPU operation failed"
        );
    }

    /// Report a capture recovery attempt.
    pub fn report_capture_recovery(&self) {
        warn!(
            subsystem = "pipewire",
            "Capture stream disconnected; attempting recovery"
        );
    }

    /// Report that a subsystem is ready.
    pub fn report_ready(&self) {
        info!(
            subsystem = %self.subsystem,
            "Subsystem initialized successfully"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subsystem_display() {
        assert_eq!(format!("{}", Subsystem::Gpu), "gpu");
        assert_eq!(format!("{}", Subsystem::Wayland), "wayland");
        assert_eq!(format!("{}", Subsystem::Video), "video");
    }

    #[test]
    fn test_logger_creation() {
        let logger = SubsystemLogger::new(Subsystem::Gpu);
        assert_eq!(logger.subsystem, Subsystem::Gpu);
    }

    #[test]
    fn test_logger_clone() {
        let logger = SubsystemLogger::new(Subsystem::Physics);
        let cloned = logger.clone();
        assert_eq!(cloned.subsystem, Subsystem::Physics);
    }
}
