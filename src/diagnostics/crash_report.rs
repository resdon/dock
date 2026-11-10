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

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
#[derive(Debug)]
pub struct CrashReporter {
    reports: Vec<CrashReport>,
    report_dir: PathBuf,
    max_reports: usize,
    max_files: usize,
}

impl Default for CrashReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl CrashReporter {
    /// Create a new crash reporter with default persistence settings.
    pub fn new() -> Self {
        Self::with_dir(Self::default_report_dir(), 32, 64)
    }

    /// Create a crash reporter with explicit persistence settings.
    pub fn with_dir(report_dir: PathBuf, max_reports: usize, max_files: usize) -> Self {
        Self {
            reports: Vec::new(),
            report_dir,
            max_reports: max_reports.max(1),
            max_files: max_files.max(1),
        }
    }

    fn default_report_dir() -> PathBuf {
        let base = std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
            .unwrap_or_else(|| PathBuf::from("/tmp"));
        base.join("dock").join("crash-reports")
    }

    /// Capture a crash report, persist it to disk, and keep it in-memory.
    pub fn capture(&mut self, message: impl Into<String>, subsystem: impl Into<String>) {
        let report = CrashReport::new(message, subsystem);
        tracing::error!(
            subsystem = report.subsystem.as_str(),
            error = report.message.as_str(),
            "Crash captured"
        );
        if let Err(e) = self.persist_report(&report) {
            tracing::warn!(
                error = %e,
                dir = %self.report_dir.display(),
                "Failed to persist crash report to disk"
            );
        }
        self.reports.push(report);
        if self.reports.len() > self.max_reports {
            let drop_count = self.reports.len() - self.max_reports;
            self.reports.drain(0..drop_count);
        }
    }

    fn persist_report(&self, report: &CrashReport) -> std::io::Result<()> {
        fs::create_dir_all(&self.report_dir)?;
        let ts = report
            .timestamp
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let pid = std::process::id();
        let subsystem = sanitize_component(&report.subsystem);
        let filename = format!("crash-{ts}-{pid}-{subsystem}.log");
        let path = self.report_dir.join(filename);
        let mut file = fs::File::create(&path)?;
        file.write_all(report.format().as_bytes())?;
        file.flush()?;
        self.rotate_files()?;
        Ok(())
    }

    fn rotate_files(&self) -> std::io::Result<()> {
        let mut entries: Vec<PathBuf> = fs::read_dir(&self.report_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_file())
            .collect();
        if entries.len() <= self.max_files {
            return Ok(());
        }
        entries.sort_by_key(|p| fs::metadata(p).and_then(|m| m.modified()).ok());
        let remove_count = entries.len().saturating_sub(self.max_files);
        for path in entries.into_iter().take(remove_count) {
            let _ = fs::remove_file(path);
        }
        Ok(())
    }

    /// Directory where crash reports are persisted.
    pub fn report_dir(&self) -> &Path {
        &self.report_dir
    }

    /// Get all in-memory crash reports.
    pub fn reports(&self) -> &[CrashReport] {
        &self.reports
    }

    /// Clear all in-memory crash reports (does not delete persisted files).
    pub fn clear(&mut self) {
        self.reports.clear();
    }

    /// Format all in-memory crash reports.
    pub fn format_all(&self) -> String {
        self.reports
            .iter()
            .map(|r| r.format())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn sanitize_component(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        out.push_str("unknown");
    }
    out.truncate(32);
    out
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
