//! PipeWire capture recovery strategies.
//!
//! # Purpose
//! Implements recovery from PipeWire stream errors: disconnection,
//! buffer unavailability, and format negotiation failures.
//!
//! # Recovery Strategy
//! - Stream disconnected: Reconnect or recreate the stream.
//! - Buffer unavailable: Wait or request another frame.
//! - Format negotiation failed: Try alternative formats.
//!
//! # Invariants
//! - PipeWire buffers must be released promptly.
//! - Stream lifetime ≠ frame lifetime.
//! - Only the active stream's frames are uploaded to GPU each frame.

use crate::recovery::RecoveryResult;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

/// Configuration for capture recovery attempts.
#[derive(Debug, Clone)]
pub struct CaptureRecoveryConfig {
    /// Maximum number of reconnection attempts.
    pub max_reconnect_attempts: usize,
    /// Delay between reconnection attempts.
    pub reconnect_delay: Duration,
    /// Whether to try alternative formats on negotiation failure.
    pub try_alternative_formats: bool,
}

impl Default for CaptureRecoveryConfig {
    fn default() -> Self {
        Self {
            max_reconnect_attempts: 5,
            reconnect_delay: Duration::from_millis(500),
            try_alternative_formats: true,
        }
    }
}

/// Manages PipeWire capture recovery state and strategies.
#[derive(Debug)]
pub struct CaptureRecovery {
    config: CaptureRecoveryConfig,
    reconnect_count: AtomicUsize,
}

impl CaptureRecovery {
    /// Create a new capture recovery manager.
    pub fn new(config: CaptureRecoveryConfig) -> Self {
        Self {
            config,
            reconnect_count: AtomicUsize::new(0),
        }
    }

    /// Create with default configuration.
    pub fn default_config() -> Self {
        Self::new(CaptureRecoveryConfig::default())
    }

    /// Attempt to recover from a stream disconnection.
    ///
    /// Returns `RetryAfter(delay)` to communicate retry timing intent, but no
    /// actual reconnect/recreate action is performed by this module yet.
    pub fn recover_stream_disconnected(&self) -> RecoveryResult {
        let attempts = self.reconnect_count.fetch_add(1, Ordering::SeqCst) + 1;

        if attempts > self.config.max_reconnect_attempts {
            tracing::error!(
                subsystem = "pipewire",
                attempts = attempts,
                max_attempts = self.config.max_reconnect_attempts,
                "Capture reconnection attempts exhausted (no integrated recovery action)"
            );
            return RecoveryResult::Failed;
        }

        tracing::warn!(
            subsystem = "pipewire",
            attempt = attempts,
            delay_ms = ?self.config.reconnect_delay,
            "Capture stream recovery requested but not implemented/integrated"
        );
        RecoveryResult::RetryAfter(self.config.reconnect_delay)
    }

    /// Attempt to recover from buffer unavailability.
    ///
    /// Returns `RetryAfter(0)` to indicate the caller may try on the next
    /// frame/cycle, but no buffer acquisition is performed here.
    pub fn recover_buffer_unavailable(&self) -> RecoveryResult {
        tracing::debug!(
            subsystem = "pipewire",
            "Buffer unavailable; caller may retry later (no integrated recovery action)"
        );
        RecoveryResult::RetryAfter(Duration::from_millis(0))
    }

    /// Attempt to recover from format negotiation failure.
    ///
    /// Returns `NotImplemented` when alternative-format recovery is enabled but
    /// not integrated; `NotAttempted` when disabled.
    pub fn recover_format_negotiation(&self) -> RecoveryResult {
        if !self.config.try_alternative_formats {
            return RecoveryResult::NotAttempted;
        }

        tracing::warn!(
            subsystem = "pipewire",
            "Alternative format negotiation requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Reset the reconnection attempt counter.
    pub fn reset_attempts(&self) {
        self.reconnect_count.store(0, Ordering::SeqCst);
    }

    /// Get the current reconnection attempt count.
    pub fn reconnect_count(&self) -> usize {
        self.reconnect_count.load(Ordering::SeqCst)
    }

    /// Get the reconnect delay.
    pub fn reconnect_delay(&self) -> Duration {
        self.config.reconnect_delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CaptureRecoveryConfig::default();
        assert_eq!(config.max_reconnect_attempts, 5);
        assert!(config.try_alternative_formats);
    }

    #[test]
    fn test_stream_disconnected_recovery_returns_retry_after() {
        let recovery = CaptureRecovery::default_config();
        let result = recovery.recover_stream_disconnected();
        assert_eq!(
            result,
            RecoveryResult::RetryAfter(Duration::from_millis(500))
        );
        assert_eq!(recovery.reconnect_count(), 1);
    }

    #[test]
    fn test_max_reconnect_attempts() {
        let config = CaptureRecoveryConfig {
            max_reconnect_attempts: 2,
            ..Default::default()
        };
        let recovery = CaptureRecovery::new(config);
        recovery.recover_stream_disconnected();
        recovery.recover_stream_disconnected();
        let result = recovery.recover_stream_disconnected();
        assert_eq!(result, RecoveryResult::Failed);
    }

    #[test]
    fn test_buffer_unavailable_recovery_returns_retry_after() {
        let recovery = CaptureRecovery::default_config();
        let result = recovery.recover_buffer_unavailable();
        assert_eq!(result, RecoveryResult::RetryAfter(Duration::from_millis(0)));
    }

    #[test]
    fn test_format_negotiation_recovery_not_implemented() {
        let recovery = CaptureRecovery::default_config();
        let result = recovery.recover_format_negotiation();
        assert_eq!(result, RecoveryResult::NotImplemented);
    }

    #[test]
    fn test_reset_attempts() {
        let recovery = CaptureRecovery::default_config();
        recovery.recover_stream_disconnected();
        recovery.reset_attempts();
        assert_eq!(recovery.reconnect_count(), 0);
    }

    #[test]
    fn test_reconnect_delay() {
        let recovery = CaptureRecovery::default_config();
        assert_eq!(recovery.reconnect_delay(), Duration::from_millis(500));
    }
}
