//! GPU recovery strategies.
//!
//! # Purpose
//! Implements recovery from GPU errors: device loss, memory pressure,
//! and presentation failures.
//!
//! # Recovery Strategy
//! - Device lost: Reinitialize GPU if safe.
//! - Out of memory: Release resources and reduce allocations.
//! - Presentation failed: Retry or recreate swapchain.
//!
//! # Invariants
//! - GPU resources are owned by the renderer. Recovery must go through
//!   the renderer's API, not directly manipulate wgpu resources.
//! - Recovery attempts are bounded to prevent infinite loops.

use crate::recovery::RecoveryResult;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Configuration for GPU recovery attempts.
#[derive(Debug, Clone)]
pub struct GpuRecoveryConfig {
    /// Maximum number of recovery attempts before giving up.
    pub max_attempts: usize,
    /// Whether to attempt device reinitialization on device loss.
    pub retry_device_loss: bool,
    /// Whether to release textures on memory pressure.
    pub release_on_pressure: bool,
}

impl Default for GpuRecoveryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            retry_device_loss: true,
            release_on_pressure: true,
        }
    }
}

/// Manages GPU recovery state and strategies.
#[derive(Debug)]
pub struct GpuRecovery {
    config: GpuRecoveryConfig,
    attempt_count: AtomicUsize,
}

impl GpuRecovery {
    /// Create a new GPU recovery manager.
    pub fn new(config: GpuRecoveryConfig) -> Self {
        Self {
            config,
            attempt_count: AtomicUsize::new(0),
        }
    }

    /// Create a new GPU recovery manager with default configuration.
    pub fn default_config() -> Self {
        Self::new(GpuRecoveryConfig::default())
    }

    /// Attempt to recover from a GPU device loss.
    ///
    /// Returns `NotImplemented` because no renderer-integrated recovery action
    /// is currently wired here. Attempt tracking is preserved for observability.
    pub fn recover_device_loss(&self) -> RecoveryResult {
        if !self.config.retry_device_loss {
            return RecoveryResult::NotAttempted;
        }

        let attempts = self.attempt_count.fetch_add(1, Ordering::SeqCst) + 1;
        if attempts > self.config.max_attempts {
            tracing::error!(
                subsystem = "gpu",
                attempts = attempts,
                max_attempts = self.config.max_attempts,
                "GPU recovery attempts exhausted (no integrated recovery action)"
            );
            return RecoveryResult::Failed;
        }

        tracing::warn!(
            subsystem = "gpu",
            attempt = attempts,
            "GPU device recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Attempt to recover from GPU memory pressure.
    ///
    /// Returns `NotImplemented` because this module does not own renderer
    /// resources and cannot safely evict/recreate them without integration.
    pub fn recover_memory_pressure(&self) -> RecoveryResult {
        if !self.config.release_on_pressure {
            return RecoveryResult::NotAttempted;
        }
        tracing::warn!(
            subsystem = "gpu",
            "GPU memory pressure recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Attempt to recover from a presentation failure.
    ///
    /// Returns `NotImplemented` because presentation/swapchain recovery is not
    /// integrated with the renderer from this module.
    pub fn recover_presentation(&self) -> RecoveryResult {
        tracing::warn!(
            subsystem = "gpu",
            "GPU presentation recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Reset the recovery attempt counter.
    pub fn reset_attempts(&self) {
        self.attempt_count.store(0, Ordering::SeqCst);
    }

    /// Get the current attempt count.
    pub fn attempt_count(&self) -> usize {
        self.attempt_count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = GpuRecoveryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert!(config.retry_device_loss);
        assert!(config.release_on_pressure);
    }

    #[test]
    fn test_device_loss_recovery_not_implemented() {
        let recovery = GpuRecovery::default_config();
        let result = recovery.recover_device_loss();
        assert_eq!(result, RecoveryResult::NotImplemented);
        assert_eq!(recovery.attempt_count(), 1);
    }

    #[test]
    fn test_device_loss_max_attempts() {
        let config = GpuRecoveryConfig {
            max_attempts: 2,
            ..Default::default()
        };
        let recovery = GpuRecovery::new(config);
        recovery.recover_device_loss();
        recovery.recover_device_loss();
        let result = recovery.recover_device_loss();
        assert_eq!(result, RecoveryResult::Failed);
    }

    #[test]
    fn test_memory_pressure_recovery_not_implemented() {
        let recovery = GpuRecovery::default_config();
        let result = recovery.recover_memory_pressure();
        assert_eq!(result, RecoveryResult::NotImplemented);
    }

    #[test]
    fn test_presentation_recovery_not_implemented() {
        let recovery = GpuRecovery::default_config();
        let result = recovery.recover_presentation();
        assert_eq!(result, RecoveryResult::NotImplemented);
    }

    #[test]
    fn test_reset_attempts() {
        let recovery = GpuRecovery::default_config();
        recovery.recover_device_loss();
        recovery.reset_attempts();
        assert_eq!(recovery.attempt_count(), 0);
    }

    #[test]
    fn test_no_retry_on_disabled() {
        let config = GpuRecoveryConfig {
            retry_device_loss: false,
            ..Default::default()
        };
        let recovery = GpuRecovery::new(config);
        let result = recovery.recover_device_loss();
        assert_eq!(result, RecoveryResult::NotAttempted);
    }
}
