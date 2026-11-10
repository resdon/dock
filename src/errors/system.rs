//! System-specific error types and recovery strategies.
//!
//! # Purpose
//! Defines errors related to D-Bus communication, system monitoring,
//! process resolution, and other system integration issues.
//!
//! # Recovery
//! - D-Bus unavailable: Retry with bounded backoff.
//! - Process resolution: Skip the failed process.
//!
//! # Dependencies
//! Independent of wgpu-api errors; focuses on system-level integration.

use thiserror::Error;

/// System-specific errors with application-level context.
#[derive(Debug, Error)]
pub enum SystemAppError {
    #[error("D-Bus connection failed: {0}")]
    DbusConnectionFailed(String),

    #[error("D-Bus method call timeout: {service}.{method}")]
    DbusTimeout { service: String, method: String },

    #[error("D-Bus service not found: {service}")]
    DbusServiceNotFound { service: String },

    #[error("System monitoring failed: {0}")]
    SystemMonitorFailed(String),

    #[error("Process resolution failed: {0}")]
    ProcessResolutionFailed(String),

    #[error("Icon theme lookup failed: {0}")]
    IconThemeFailed(String),

    #[error("Configuration file not found: {path}")]
    ConfigNotFound { path: String },

    #[error("Configuration parse error: {0}")]
    ConfigParseError(String),

    #[error("File system access denied: {path}")]
    FileSystemAccessDenied { path: String },
}

impl SystemAppError {
    /// Returns `true` if this system error can be recovered.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            SystemAppError::DbusConnectionFailed(_)
                | SystemAppError::DbusTimeout { .. }
                | SystemAppError::SystemMonitorFailed(_)
                | SystemAppError::ProcessResolutionFailed(_)
                | SystemAppError::IconThemeFailed(_)
        )
    }

    /// Returns the recovery strategy description.
    pub fn recovery_strategy(&self) -> &'static str {
        match self {
            SystemAppError::DbusConnectionFailed(_) => {
                "Retry D-Bus connection with exponential backoff"
            }
            SystemAppError::DbusTimeout { .. } => "Retry the D-Bus method call",
            SystemAppError::DbusServiceNotFound { .. } => {
                "Check if the service is installed and running"
            }
            SystemAppError::SystemMonitorFailed(_) => "Retry system monitoring on next interval",
            SystemAppError::ProcessResolutionFailed(_) => "Skip the failed process and continue",
            SystemAppError::IconThemeFailed(_) => "Fall back to default icon",
            SystemAppError::ConfigNotFound { .. } => "Use default configuration values",
            SystemAppError::ConfigParseError(_) => "Reload configuration file or use defaults",
            SystemAppError::FileSystemAccessDenied { .. } => "Check file permissions",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbus_connection_recoverable() {
        let err = SystemAppError::DbusConnectionFailed("refused".into());
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("backoff"));
    }

    #[test]
    fn test_dbus_timeout_recoverable() {
        let err = SystemAppError::DbusTimeout {
            service: "org.freedesktop".into(),
            method: "Get".into(),
        };
        assert!(err.is_recoverable());
    }

    #[test]
    fn test_config_not_found_recoverable() {
        let err = SystemAppError::ConfigNotFound {
            path: "/etc/dock/config.toml".into(),
        };
        assert!(!err.is_recoverable());
        assert!(err.recovery_strategy().contains("default"));
    }

    #[test]
    fn test_icon_theme_fallback() {
        let err = SystemAppError::IconThemeFailed("not found".into());
        assert!(err.is_recoverable());
        assert!(err.recovery_strategy().contains("default"));
    }
}
