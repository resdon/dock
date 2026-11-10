//! Health monitoring across all subsystems.
//!
//! # Purpose
//! Periodically checks the health of GPU, Wayland, PipeWire, and system
//! subsystems, reporting status and triggering alerts on degradation.
//!
//! # Usage
//! ```rust
//! use dock::diagnostics::HealthMonitor;
//!
//! let monitor = HealthMonitor::new();
//! let status = monitor.check_all();
//! ```

use std::time::{Duration, Instant};

/// Health status of a subsystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// Subsystem is operating normally.
    Healthy,
    /// Subsystem is degraded but still functional.
    Degraded,
    /// Subsystem has failed and may need recovery.
    Unhealthy,
    /// Subsystem status is unknown (not checked yet).
    Unknown,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => write!(f, "healthy"),
            HealthStatus::Degraded => write!(f, "degraded"),
            HealthStatus::Unhealthy => write!(f, "unhealthy"),
            HealthStatus::Unknown => write!(f, "unknown"),
        }
    }
}

/// Health check result for a single subsystem.
#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    /// Subsystem name.
    pub subsystem: String,
    /// Current health status.
    pub status: HealthStatus,
    /// Last check timestamp.
    pub last_check: Instant,
    /// Optional message describing the status.
    pub message: Option<String>,
    /// Number of consecutive failures.
    pub consecutive_failures: usize,
}

/// Monitors the health of all subsystems.
#[derive(Debug)]
pub struct HealthMonitor {
    gpu_status: HealthCheckResult,
    wayland_status: HealthCheckResult,
    pipewire_status: HealthCheckResult,
    ui_status: HealthCheckResult,
    system_status: HealthCheckResult,
    check_interval: Duration,
    failure_threshold: usize,
}

impl HealthMonitor {
    /// Create a new health monitor.
    ///
    /// # Arguments
    /// * `check_interval` - How often to run health checks.
    /// * `failure_threshold` - Consecutive failures before marking unhealthy.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            gpu_status: HealthCheckResult {
                subsystem: "gpu".to_string(),
                status: HealthStatus::Unknown,
                last_check: now,
                message: None,
                consecutive_failures: 0,
            },
            wayland_status: HealthCheckResult {
                subsystem: "wayland".to_string(),
                status: HealthStatus::Unknown,
                last_check: now,
                message: None,
                consecutive_failures: 0,
            },
            pipewire_status: HealthCheckResult {
                subsystem: "pipewire".to_string(),
                status: HealthStatus::Unknown,
                last_check: now,
                message: None,
                consecutive_failures: 0,
            },
            ui_status: HealthCheckResult {
                subsystem: "ui".to_string(),
                status: HealthStatus::Unknown,
                last_check: now,
                message: None,
                consecutive_failures: 0,
            },
            system_status: HealthCheckResult {
                subsystem: "system".to_string(),
                status: HealthStatus::Unknown,
                last_check: now,
                message: None,
                consecutive_failures: 0,
            },
            check_interval: Duration::from_secs(5),
            failure_threshold: 3,
        }
    }

    /// Report a successful health check for a subsystem.
    pub fn report_healthy(&mut self, subsystem: &str) {
        if let Some(status) = self.get_status_mut(subsystem) {
            status.status = HealthStatus::Healthy;
            status.last_check = Instant::now();
            status.consecutive_failures = 0;
            status.message = None;
        }
    }

    /// Report a degraded health check for a subsystem.
    pub fn report_degraded(&mut self, subsystem: &str, message: impl Into<String>) {
        if let Some(status) = self.get_status_mut(subsystem) {
            status.status = HealthStatus::Degraded;
            status.last_check = Instant::now();
            status.message = Some(message.into());
        }
    }

    /// Report an unhealthy health check for a subsystem.
    pub fn report_unhealthy(&mut self, subsystem: &str, message: impl Into<String>) {
        if let Some(status) = self.get_status_mut(subsystem) {
            status.consecutive_failures += 1;
            status.status = HealthStatus::Unhealthy;
            status.last_check = Instant::now();
            let msg = message.into();
            status.message = Some(msg.clone());

            tracing::error!(
                subsystem = subsystem,
                consecutive_failures = status.consecutive_failures,
                error = %msg,
                "Subsystem unhealthy"
            );
        }
    }

    /// Check if all subsystems are healthy.
    pub fn all_healthy(&self) -> bool {
        self.gpu_status.status == HealthStatus::Healthy
            && self.wayland_status.status == HealthStatus::Healthy
            && self.ui_status.status == HealthStatus::Healthy
    }

    /// Check if any subsystem is unhealthy.
    pub fn any_unhealthy(&self) -> bool {
        self.gpu_status.status == HealthStatus::Unhealthy
            || self.wayland_status.status == HealthStatus::Unhealthy
            || self.pipewire_status.status == HealthStatus::Unhealthy
            || self.ui_status.status == HealthStatus::Unhealthy
            || self.system_status.status == HealthStatus::Unhealthy
    }

    /// Get all health check results.
    pub fn check_all(&self) -> Vec<&HealthCheckResult> {
        vec![
            &self.gpu_status,
            &self.wayland_status,
            &self.pipewire_status,
            &self.ui_status,
            &self.system_status,
        ]
    }

    /// Get the health status of a specific subsystem.
    pub fn get_status(&self, subsystem: &str) -> Option<&HealthCheckResult> {
        match subsystem {
            "gpu" => Some(&self.gpu_status),
            "wayland" => Some(&self.wayland_status),
            "pipewire" => Some(&self.pipewire_status),
            "ui" => Some(&self.ui_status),
            "system" => Some(&self.system_status),
            _ => None,
        }
    }

    /// Get a mutable reference to a subsystem's health status.
    fn get_status_mut(&mut self, subsystem: &str) -> Option<&mut HealthCheckResult> {
        match subsystem {
            "gpu" => Some(&mut self.gpu_status),
            "wayland" => Some(&mut self.wayland_status),
            "pipewire" => Some(&mut self.pipewire_status),
            "ui" => Some(&mut self.ui_status),
            "system" => Some(&mut self.system_status),
            _ => None,
        }
    }

    /// Get a summary of all subsystem health.
    pub fn health_summary(&self) -> String {
        let results = self.check_all();
        let healthy = results
            .iter()
            .filter(|r| r.status == HealthStatus::Healthy)
            .count();
        let degraded = results
            .iter()
            .filter(|r| r.status == HealthStatus::Degraded)
            .count();
        let unhealthy = results
            .iter()
            .filter(|r| r.status == HealthStatus::Unhealthy)
            .count();
        let unknown = results
            .iter()
            .filter(|r| r.status == HealthStatus::Unknown)
            .count();

        format!(
            "Health: {} healthy, {} degraded, {} unhealthy, {} unknown",
            healthy, degraded, unhealthy, unknown
        )
    }

    /// Get the check interval.
    pub fn check_interval(&self) -> Duration {
        self.check_interval
    }

    /// Get the failure threshold.
    pub fn failure_threshold(&self) -> usize {
        self.failure_threshold
    }
}

impl Default for HealthMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_status() {
        let monitor = HealthMonitor::new();
        assert!(!monitor.all_healthy());
        assert!(!monitor.any_unhealthy());
    }

    #[test]
    fn test_report_healthy() {
        let mut monitor = HealthMonitor::new();
        monitor.report_healthy("gpu");
        assert_eq!(
            monitor.get_status("gpu").unwrap().status,
            HealthStatus::Healthy
        );
    }

    #[test]
    fn test_report_degraded() {
        let mut monitor = HealthMonitor::new();
        monitor.report_degraded("pipewire", "slow frames");
        assert_eq!(
            monitor.get_status("pipewire").unwrap().status,
            HealthStatus::Degraded
        );
    }

    #[test]
    fn test_report_unhealthy() {
        let mut monitor = HealthMonitor::new();
        monitor.report_unhealthy("gpu", "device lost");
        assert!(monitor.any_unhealthy());
        assert_eq!(monitor.get_status("gpu").unwrap().consecutive_failures, 1);
    }

    #[test]
    fn test_all_healthy() {
        let mut monitor = HealthMonitor::new();
        monitor.report_healthy("gpu");
        monitor.report_healthy("wayland");
        monitor.report_healthy("ui");
        assert!(monitor.all_healthy());
    }

    #[test]
    fn test_health_summary() {
        let mut monitor = HealthMonitor::new();
        monitor.report_healthy("gpu");
        monitor.report_degraded("pipewire", "slow");
        let summary = monitor.health_summary();
        assert!(summary.contains("1 healthy"));
        assert!(summary.contains("1 degraded"));
    }

    #[test]
    fn test_consecutive_failures() {
        let mut monitor = HealthMonitor::new();
        monitor.report_unhealthy("gpu", "error 1");
        monitor.report_unhealthy("gpu", "error 2");
        assert_eq!(monitor.get_status("gpu").unwrap().consecutive_failures, 2);
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", HealthStatus::Healthy), "healthy");
        assert_eq!(format!("{}", HealthStatus::Degraded), "degraded");
        assert_eq!(format!("{}", HealthStatus::Unhealthy), "unhealthy");
    }
}
