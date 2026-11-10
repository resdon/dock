//! Crash report generation with backtraces and context.
//!
//! # Purpose
//! Captures crash information including backtraces, subsystem state,
//! and environment details for post-mortem analysis.
//!
//! # Usage
//! ```rust
//! use dock::diagnostics::CrashReporter;
//!
//! let reporter = CrashReporter::new();
//! reporter.capture("GPU device lost");
//! ```

use std::time::SystemTime;

/// Information captured when a crash or critical error occurs.
#[derive(Debug, Clone)]
pub struct CrashReport {
    /// Timestamp of the crash.
    pub timestamp: SystemTime,
    /// Description of the crash.
    pub message: String,
    /// Subsystem that encountered the crash.
    pub subsystem: String,
    /// Rust backtrace at the time of the crash (as string).
    pub backtrace: String,
    /// Thread name if available.
    pub thread_name: Option<String>,
}

impl CrashReport {
    /// Create a new crash report.
    pub fn new(message: impl Into<String>, subsystem: impl Into<String>) -> Self {
        let thread = std::thread::current();
        Self {
            timestamp: SystemTime::now(),
            message: message.into(),
            subsystem: subsystem.into(),
            backtrace: std::backtrace::Backtrace::force_capture().to_string(),
            thread_name: thread.name().map(|s| s.to_string()),
        }
    }

    /// Returns `true` if the backtrace contains useful information.
    pub fn has_backtrace(&self) -> bool {
        !self.backtrace.is_empty()
    }

    /// Format the crash report as a human-readable string.
    pub fn format(&self) -> String {
        let mut report = String::new();
        report.push_str(&format!("=== Crash Report ===\n"));
        report.push_str(&format!("Timestamp: {:?}\n", self.timestamp));
        report.push_str(&format!("Subsystem: {}\n", self.subsystem));
        report.push_str(&format!("Message: {}\n", self.message));
        if let Some(ref name) = self.thread_name {
            report.push_str(&format!("Thread: {}\n", name));
        }
        report.push_str(&format!("Backtrace:\n{}\n", self.backtrace));
        report
    }
}

/// Manages crash report capture and storage.
#[derive(Debug, Default)]
pub struct CrashReporter {
    reports: Vec<CrashReport>,
}

impl CrashReporter {
    /// Create a new crash reporter.
    pub fn new() -> Self {
        Self::default()
    }

    /// Capture a crash report.
    pub fn capture(&mut self, message: impl Into<String>, subsystem: impl Into<String>) {
        let report = CrashReport::new(message, subsystem);
        tracing::error!(
            subsystem = report.subsystem,
            error = report.message,
            "Crash captured"
        );
        self.reports.push(report);
    }

    /// Get all crash reports.
    pub fn reports(&self) -> &[CrashReport] {
        &self.reports
    }

    /// Clear all crash reports.
    pub fn clear(&mut self) {
        self.reports.clear();
    }

    /// Format all crash reports.
    pub fn format_all(&self) -> String {
        self.reports
            .iter()
            .map(|r| r.format())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crash_report_creation() {
        let report = CrashReport::new("test error", "gpu");
        assert_eq!(report.subsystem, "gpu");
        assert_eq!(report.message, "test error");
    }

    #[test]
    fn test_crash_report_format() {
        let report = CrashReport::new("test error", "gpu");
        let formatted = report.format();
        assert!(formatted.contains("Crash Report"));
        assert!(formatted.contains("gpu"));
        assert!(formatted.contains("test error"));
    }

    #[test]
    fn test_crash_reporter_capture() {
        let mut reporter = CrashReporter::new();
        reporter.capture("error 1", "gpu");
        reporter.capture("error 2", "wayland");
        assert_eq!(reporter.reports().len(), 2);
    }

    #[test]
    fn test_crash_reporter_clear() {
        let mut reporter = CrashReporter::new();
        reporter.capture("error", "gpu");
        reporter.clear();
        assert_eq!(reporter.reports().len(), 0);
    }

    #[test]
    fn test_crash_report_has_backtrace() {
        let report = CrashReport::new("test", "gpu");
        assert!(report.has_backtrace());
    }
}
