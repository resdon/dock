//! Performance metrics collection and reporting.
//!
//! # Purpose
//! Tracks frame rates, rendering times, and subsystem performance
//! to identify bottlenecks and verify FPS targets.
//!
//! # Usage
//! ```rust
//! use dock::diagnostics::PerformanceMonitor;
//!
//! let mut monitor = PerformanceMonitor::new();
//! monitor.start_frame();
//! // ... rendering work ...
//! monitor.end_frame();
//! let fps = monitor.current_fps();
//! ```

use std::time::{Duration, Instant};

/// Performance metrics for a single frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameMetrics {
    /// Total frame duration in milliseconds.
    pub frame_duration_ms: f64,
    /// Rendering time in milliseconds.
    pub render_time_ms: f64,
    /// Physics simulation time in milliseconds.
    pub physics_time_ms: f64,
    /// Frames per second.
    pub fps: f64,
}

/// Monitors application performance metrics.
#[derive(Debug)]
pub struct PerformanceMonitor {
    frame_times: Vec<Duration>,
    last_frame_time: Option<Instant>,
    frame_start: Option<Instant>,
    max_samples: usize,
}

impl PerformanceMonitor {
    /// Create a new performance monitor with the given sample window size.
    ///
    /// # Arguments
    /// * `max_samples` - Number of frames to keep in the rolling window (default: 60).
    pub fn new() -> Self {
        Self {
            frame_times: Vec::new(),
            last_frame_time: None,
            frame_start: None,
            max_samples: 60,
        }
    }

    /// Mark the start of a new frame.
    pub fn start_frame(&mut self) {
        self.frame_start = Some(Instant::now());
    }

    /// Mark the end of the current frame and record metrics.
    pub fn end_frame(&mut self) {
        if let Some(start) = self.frame_start.take() {
            let duration = start.elapsed();
            self.frame_times.push(duration);

            // Keep only the last N samples
            if self.frame_times.len() > self.max_samples {
                self.frame_times.remove(0);
            }

            self.last_frame_time = Some(Instant::now());
        }
    }

    /// Calculate the current frames per second based on recent samples.
    pub fn current_fps(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        let avg_seconds = total.as_secs_f64() / self.frame_times.len() as f64;

        if avg_seconds == 0.0 {
            0.0
        } else {
            1.0 / avg_seconds
        }
    }

    /// Get the average frame duration in milliseconds.
    pub fn avg_frame_duration_ms(&self) -> f64 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        total.as_secs_f64() * 1000.0 / self.frame_times.len() as f64
    }

    /// Get the worst frame duration in milliseconds.
    pub fn worst_frame_duration_ms(&self) -> f64 {
        self.frame_times
            .iter()
            .max()
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0)
    }

    /// Check if the current FPS meets the target.
    ///
    /// # Arguments
    /// * `target_fps` - Target frames per second (e.g., 60.0).
    pub fn meets_target(&self, target_fps: f64) -> bool {
        self.current_fps() >= target_fps
    }

    /// Reset all performance data.
    pub fn reset(&mut self) {
        self.frame_times.clear();
        self.last_frame_time = None;
        self.frame_start = None;
    }

    /// Get the number of samples collected.
    pub fn sample_count(&self) -> usize {
        self.frame_times.len()
    }
}

impl Default for PerformanceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_fps() {
        let monitor = PerformanceMonitor::new();
        assert_eq!(monitor.current_fps(), 0.0);
    }

    #[test]
    fn test_frame_tracking() {
        let mut monitor = PerformanceMonitor::new();
        monitor.start_frame();
        std::thread::sleep(Duration::from_millis(10));
        monitor.end_frame();

        assert!(monitor.current_fps() > 0.0);
        assert!(monitor.avg_frame_duration_ms() > 0.0);
    }

    #[test]
    fn test_reset() {
        let mut monitor = PerformanceMonitor::new();
        monitor.start_frame();
        monitor.end_frame();
        monitor.reset();

        assert_eq!(monitor.current_fps(), 0.0);
        assert_eq!(monitor.sample_count(), 0);
    }

    #[test]
    fn test_meets_target() {
        let mut monitor = PerformanceMonitor::new();
        monitor.start_frame();
        monitor.end_frame();

        // Very fast frame should meet 60 FPS target
        assert!(monitor.meets_target(60.0));
    }

    #[test]
    fn test_worst_frame() {
        let mut monitor = PerformanceMonitor::new();
        monitor.start_frame();
        monitor.end_frame();

        monitor.start_frame();
        std::thread::sleep(Duration::from_millis(50));
        monitor.end_frame();

        assert!(monitor.worst_frame_duration_ms() >= monitor.avg_frame_duration_ms());
    }
}
