//! Recovery subsystem for Dock.
//!
//! # Purpose
//! Implements automatic recovery strategies for subsystem failures.
//! Not every error should trigger automatic recovery — recovery must
//! respect resource ownership and the state of the subsystem.
//!
//! # Design
//! - `gpu_recovery`: GPU device reinitialization and resource recreation.
//! - `capture_recovery`: PipeWire stream reconnection and recreation.
//! - `resource_recovery`: GPU resource release and cache eviction.
//! - `service_recovery`: Background service restart with bounded backoff.
//!
//! # Invariants
//! - Recovery must not violate resource ownership rules.
//! - Recovery attempts are bounded to prevent infinite loops.
//! - Failed recovery escalates to the application level.

mod capture_recovery;
mod gpu_recovery;
mod resource_recovery;
mod service_recovery;

pub use capture_recovery::*;
pub use gpu_recovery::*;
pub use resource_recovery::*;
pub use service_recovery::*;

/// Result of a recovery attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoveryResult {
    /// Recovery action completed successfully.
    Recovered,
    /// Recovery can be retried after the given delay.
    RetryAfter(std::time::Duration),
    /// Recovery failed but may be retried (caller decides timing/backoff).
    Retryable,
    /// Recovery failed permanently or attempts are exhausted.
    Failed,
    /// Recovery was intentionally not attempted for this error/state.
    NotAttempted,
    /// Recovery path is not implemented/integrated for this subsystem.
    NotImplemented,
}

impl std::fmt::Display for RecoveryResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecoveryResult::Recovered => write!(f, "recovered"),
            RecoveryResult::RetryAfter(delay) => write!(f, "retry after {:?}", delay),
            RecoveryResult::Retryable => write!(f, "retryable"),
            RecoveryResult::Failed => write!(f, "failed"),
            RecoveryResult::NotAttempted => write!(f, "not attempted"),
            RecoveryResult::NotImplemented => write!(f, "not implemented"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_recovery_result_display() {
        assert_eq!(format!("{}", RecoveryResult::Recovered), "recovered");
        assert_eq!(
            format!("{}", RecoveryResult::RetryAfter(Duration::from_millis(250))),
            "retry after 250ms"
        );
        assert_eq!(format!("{}", RecoveryResult::Retryable), "retryable");
        assert_eq!(format!("{}", RecoveryResult::Failed), "failed");
        assert_eq!(format!("{}", RecoveryResult::NotAttempted), "not attempted");
        assert_eq!(
            format!("{}", RecoveryResult::NotImplemented),
            "not implemented"
        );
    }

    #[test]
    fn test_recovery_result_equality() {
        assert_eq!(RecoveryResult::Recovered, RecoveryResult::Recovered);
        assert_eq!(
            RecoveryResult::RetryAfter(Duration::from_secs(1)),
            RecoveryResult::RetryAfter(Duration::from_secs(1))
        );
        assert_ne!(RecoveryResult::Recovered, RecoveryResult::Failed);
        assert_ne!(RecoveryResult::Retryable, RecoveryResult::NotImplemented);
    }
}
