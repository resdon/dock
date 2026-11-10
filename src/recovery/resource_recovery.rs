//! GPU resource recovery strategies.
//!
//! # Purpose
//! Implements recovery from GPU resource exhaustion: texture cache overflow,
//! buffer pool exhaustion, and memory pressure.
//!
//! # Recovery Strategy
//! - Texture cache overflow: Evict least-recently-used textures.
//! - Buffer pool exhausted: Wait for buffers to be returned.
//! - Memory pressure: Release unused resources.
//!
//! # Invariants
//! - GPU resources are owned by the renderer. Resource recovery must go
//!   through the renderer's API.
//! - Never release actively used resources.

use crate::recovery::RecoveryResult;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Configuration for resource recovery.
#[derive(Debug, Clone)]
pub struct ResourceRecoveryConfig {
    /// Maximum number of textures to evict in a single recovery attempt.
    pub max_eviction_count: usize,
    /// Minimum free memory percentage before triggering recovery.
    pub min_free_memory_percent: u8,
    /// Whether to reduce texture quality as a last resort.
    pub reduce_quality: bool,
}

impl Default for ResourceRecoveryConfig {
    fn default() -> Self {
        Self {
            max_eviction_count: 10,
            min_free_memory_percent: 10,
            reduce_quality: true,
        }
    }
}

/// Manages GPU resource recovery state and strategies.
#[derive(Debug)]
pub struct ResourceRecovery {
    config: ResourceRecoveryConfig,
    eviction_count: AtomicUsize,
}

impl ResourceRecovery {
    /// Create a new resource recovery manager.
    pub fn new(config: ResourceRecoveryConfig) -> Self {
        Self {
            config,
            eviction_count: AtomicUsize::new(0),
        }
    }

    /// Create with default configuration.
    pub fn default_config() -> Self {
        Self::new(ResourceRecoveryConfig::default())
    }

    /// Attempt to recover from texture cache overflow.
    ///
    /// Returns `NotImplemented` because no renderer-owned texture eviction is
    /// performed by this module.
    pub fn recover_texture_cache_overflow(&self) -> RecoveryResult {
        let evicted = self.eviction_count.fetch_add(1, Ordering::SeqCst) + 1;

        if evicted > self.config.max_eviction_count {
            tracing::error!(
                subsystem = "gpu",
                evicted,
                max_eviction = self.config.max_eviction_count,
                "Texture cache recovery budget exhausted (no integrated recovery action)"
            );
            return RecoveryResult::Failed;
        }

        tracing::warn!(
            subsystem = "gpu",
            evicted,
            max_eviction = self.config.max_eviction_count,
            "Texture cache overflow recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Attempt to recover from buffer pool exhaustion.
    ///
    /// Returns `NotImplemented` because buffer pool management is not integrated
    /// with this recovery module.
    pub fn recover_buffer_pool(&self) -> RecoveryResult {
        tracing::warn!(
            subsystem = "gpu",
            "Buffer pool recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Attempt to recover from memory pressure by reducing quality.
    ///
    /// Returns `NotImplemented` when enabled (no quality reduction action taken),
    /// or `NotAttempted` when disabled.
    pub fn recover_by_quality_reduction(&self) -> RecoveryResult {
        if !self.config.reduce_quality {
            return RecoveryResult::NotAttempted;
        }
        tracing::warn!(
            subsystem = "gpu",
            "Quality-reduction recovery requested but not implemented/integrated"
        );
        RecoveryResult::NotImplemented
    }

    /// Check if memory pressure recovery should be triggered.
    ///
    /// # Arguments
    /// * `free_memory_percent` - Current free memory as a percentage.
    pub fn should_trigger_recovery(&self, free_memory_percent: u8) -> bool {
        free_memory_percent < self.config.min_free_memory_percent
    }

    /// Reset the eviction counter.
    pub fn reset_eviction_count(&self) {
        self.eviction_count.store(0, Ordering::SeqCst);
    }

    /// Get the current eviction count.
    pub fn eviction_count(&self) -> usize {
        self.eviction_count.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = ResourceRecoveryConfig::default();
        assert_eq!(config.max_eviction_count, 10);
        assert_eq!(config.min_free_memory_percent, 10);
        assert!(config.reduce_quality);
    }

    #[test]
    fn test_texture_cache_recovery_not_implemented() {
        let recovery = ResourceRecovery::default_config();
        let result = recovery.recover_texture_cache_overflow();
        assert_eq!(result, RecoveryResult::NotImplemented);
        assert_eq!(recovery.eviction_count(), 1);
    }

    #[test]
    fn test_texture_cache_max_eviction() {
        let config = ResourceRecoveryConfig {
            max_eviction_count: 2,
            ..Default::default()
        };
        let recovery = ResourceRecovery::new(config);
        recovery.recover_texture_cache_overflow();
        recovery.recover_texture_cache_overflow();
        let result = recovery.recover_texture_cache_overflow();
        assert_eq!(result, RecoveryResult::Failed);
    }

    #[test]
    fn test_buffer_pool_recovery_not_implemented() {
        let recovery = ResourceRecovery::default_config();
        let result = recovery.recover_buffer_pool();
        assert_eq!(result, RecoveryResult::NotImplemented);
    }

    #[test]
    fn test_quality_reduction_not_implemented() {
        let recovery = ResourceRecovery::default_config();
        let result = recovery.recover_by_quality_reduction();
        assert_eq!(result, RecoveryResult::NotImplemented);
    }

    #[test]
    fn test_quality_reduction_disabled() {
        let config = ResourceRecoveryConfig {
            reduce_quality: false,
            ..Default::default()
        };
        let recovery = ResourceRecovery::new(config);
        let result = recovery.recover_by_quality_reduction();
        assert_eq!(result, RecoveryResult::NotAttempted);
    }

    #[test]
    fn test_should_trigger_recovery() {
        let recovery = ResourceRecovery::default_config();
        assert!(recovery.should_trigger_recovery(5));
        assert!(!recovery.should_trigger_recovery(15));
    }

    #[test]
    fn test_reset_eviction_count() {
        let recovery = ResourceRecovery::default_config();
        recovery.recover_texture_cache_overflow();
        recovery.reset_eviction_count();
        assert_eq!(recovery.eviction_count(), 0);
    }
}
