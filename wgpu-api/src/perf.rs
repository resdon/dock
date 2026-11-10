//! Performance profiling and timing utilities.
//!
//! This module provides high-resolution timing, performance scope tracking,
//! frame rate analysis, and stress testing utilities for the rendering pipeline.
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────────┐
//! │  Profiler                                                    │
//! │                                                              │
//! │  ┌──────────────┐  ┌───────────────┐  ┌──────────────────┐  │
//! │  │  TimingScope │  │  FrameRate    │  │  Performance     │  │
//! │  │  (phases)    │  │  Analyzer     │  │  Counters        │  │
//! │  └──────┬───────┘  └───────┬───────┘  └────────┬─────────┘  │
//! │         └──────────────────┴───────────────────┘             │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                        │
//! │              │  ProfileSnapshot     │                        │
//! │              │  (aggregated)        │                        │
//! │              └──────────┬───────────┘                        │
//! │                         ▼                                    │
//! │              ┌──────────────────────┐                        │
//! │              │  StressTestResult    │                        │
//! │              │  (validation)        │                        │
//! │              └──────────────────────┘                        │
//! └──────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Integration Points
//!
//! - **renderer.rs**: Renderer uses Profiler to time each render phase
//! - **gpu.rs**: GPU backend reports timing for encoding and submission
//! - **monitor.rs**: ResourceMonitor provides memory data; Profiler provides timing
//! - **video.rs**: VideoManager reports frame acquisition timing

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// High-resolution timer
// ---------------------------------------------------------------------------

/// A high-resolution stopwatch for measuring code block duration.
#[derive(Debug)]
pub struct Stopwatch {
    /// When the stopwatch was started (if running).
    start: RefCell<Option<Instant>>,
    /// Accumulated elapsed time from previous runs.
    elapsed: RefCell<Duration>,
}

impl Stopwatch {
    /// Create a new stopped stopwatch.
    pub fn new() -> Self {
        Self {
            start: RefCell::new(None),
            elapsed: RefCell::new(Duration::ZERO),
        }
    }

    /// Start the stopwatch.
    pub fn start(&self) {
        *self.start.borrow_mut() = Some(Instant::now());
    }

    /// Stop the stopwatch and accumulate the elapsed time.
    pub fn stop(&self) {
        if let Some(start) = self.start.borrow_mut().take() {
            let mut elapsed = self.elapsed.borrow_mut();
            *elapsed = *elapsed + start.elapsed();
        }
    }

    /// Reset the stopwatch to zero.
    pub fn reset(&self) {
        *self.start.borrow_mut() = None;
        *self.elapsed.borrow_mut() = Duration::ZERO;
    }

    /// Get the current elapsed time (including any active run).
    pub fn elapsed(&self) -> Duration {
        let mut total = *self.elapsed.borrow();
        if let Some(start) = *self.start.borrow() {
            total = total + start.elapsed();
        }
        total
    }

    /// Get the elapsed time in milliseconds.
    pub fn elapsed_ms(&self) -> f64 {
        self.elapsed().as_secs_f64() * 1000.0
    }

    /// Get the elapsed time in microseconds.
    pub fn elapsed_us(&self) -> f64 {
        self.elapsed().as_secs_f64() * 1_000_000.0
    }

    /// Get the elapsed time in nanoseconds.
    pub fn elapsed_ns(&self) -> f64 {
        self.elapsed().as_secs_f64() * 1_000_000_000.0
    }
}

impl Default for Stopwatch {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Timing scope (RAII guard)
// ---------------------------------------------------------------------------

/// An RAII guard that measures the duration of a code block.
///
/// ```
/// use wgpu_api::perf::TimingScope;
///
/// let scope = TimingScope::new("my_operation");
/// // ... do work ...
/// let duration = scope.finish();
/// ```
#[derive(Debug)]
pub struct TimingScope {
    /// Name of the timed operation.
    name: String,
    /// When the scope was created.
    start: Instant,
}

impl TimingScope {
    /// Create a new timing scope with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start: Instant::now(),
        }
    }

    /// Finish the scope and return the elapsed duration.
    pub fn finish(self) -> Duration {
        self.start.elapsed()
    }

    /// Get the current elapsed time without finishing.
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Get the name of this scope.
    pub fn name(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// Render phase timing
// ---------------------------------------------------------------------------

/// A phase within a render frame.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum RenderPhase {
    /// CPU-side scene traversal and command building.
    #[default]
    CpuPreparation,
    /// GPU command encoding.
    GpuEncoding,
    /// GPU command submission.
    GpuSubmission,
    /// Frame presentation.
    Presentation,
    /// Texture uploads.
    TextureUpload,
    /// Glyph rasterization.
    GlyphRasterization,
    /// Text shaping.
    TextShaping,
    /// Animation updates.
    AnimationUpdate,
    /// Video frame acquisition.
    VideoAcquisition,
    /// Batching and sorting.
    Batching,
    /// Resource management (eviction, cleanup).
    ResourceManagement,
    /// Custom phase.
    Custom,
}

impl RenderPhase {
    /// Get a human-readable name for this phase.
    pub fn name(&self) -> &'static str {
        match self {
            Self::CpuPreparation => "CPU Prep",
            Self::GpuEncoding => "GPU Encode",
            Self::GpuSubmission => "GPU Submit",
            Self::Presentation => "Present",
            Self::TextureUpload => "Tex Upload",
            Self::GlyphRasterization => "Glyph Rast",
            Self::TextShaping => "Text Shape",
            Self::AnimationUpdate => "Animation",
            Self::VideoAcquisition => "Video Acq",
            Self::Batching => "Batching",
            Self::ResourceManagement => "Res Mgmt",
            Self::Custom => "Custom",
        }
    }
}

/// Timing data for a single render phase.
#[derive(Debug, Clone, Copy, Default)]
pub struct PhaseTiming {
    /// Phase identifier.
    pub phase: RenderPhase,
    /// Elapsed time in milliseconds.
    pub duration_ms: f64,
    /// Number of times this phase was executed.
    pub call_count: u32,
    /// Minimum duration observed.
    pub min_ms: f64,
    /// Maximum duration observed.
    pub max_ms: f64,
    /// Total accumulated time.
    pub total_ms: f64,
}

impl PhaseTiming {
    /// Record a new timing sample.
    pub fn record(&mut self, duration_ms: f64) {
        self.call_count += 1;
        self.total_ms += duration_ms;

        if self.call_count == 1 || duration_ms < self.min_ms {
            self.min_ms = duration_ms;
        }
        if duration_ms > self.max_ms {
            self.max_ms = duration_ms;
        }
    }

    /// Compute the average duration.
    pub fn avg_ms(&self) -> f64 {
        if self.call_count == 0 {
            0.0
        } else {
            self.total_ms / self.call_count as f64
        }
    }

    /// Format as a report string.
    pub fn report(&self) -> String {
        if self.call_count == 0 {
            format!("{}: -", self.phase.name())
        } else {
            format!(
                "{}: {:.2}ms (avg {:.2}, min {:.2}, max {:.2}, {} calls)",
                self.phase.name(),
                self.duration_ms,
                self.avg_ms(),
                self.min_ms,
                self.max_ms,
                self.call_count
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Frame rate analyzer
// ---------------------------------------------------------------------------

/// Analyzes frame rate over time, providing statistics and stability metrics.
#[derive(Debug, Clone)]
pub struct FrameRateAnalyzer {
    /// Frame times in milliseconds (circular buffer).
    frame_times: RefCell<Vec<f64>>,
    /// Maximum number of frame times to retain.
    max_samples: usize,
    /// Total frames recorded.
    total_frames: RefCell<u64>,
    /// Target FPS.
    target_fps: f64,
}

impl FrameRateAnalyzer {
    /// Create a new analyzer targeting the given FPS.
    pub fn new(target_fps: f64) -> Self {
        Self {
            frame_times: RefCell::new(Vec::with_capacity(360)),
            max_samples: 360, // ~6 seconds at 60fps
            total_frames: RefCell::new(0),
            target_fps,
        }
    }

    /// Create a new analyzer targeting 60 FPS.
    pub fn new_60fps() -> Self {
        Self::new(60.0)
    }

    /// Record a new frame time.
    pub fn record_frame(&self, frame_time_ms: f64) {
        let mut times = self.frame_times.borrow_mut();
        times.push(frame_time_ms);
        if times.len() > self.max_samples {
            times.remove(0);
        }
        *self.total_frames.borrow_mut() += 1;
    }

    /// Compute the current FPS from recent samples.
    pub fn current_fps(&self) -> f64 {
        let times = self.frame_times.borrow();
        if times.is_empty() {
            return 0.0;
        }
        let avg: f64 = times.iter().sum::<f64>() / times.len() as f64;
        if avg <= 0.0 {
            return 0.0;
        }
        1000.0 / avg
    }

    /// Compute the average FPS over all samples.
    pub fn avg_fps(&self) -> f64 {
        self.current_fps()
    }

    /// Compute the minimum FPS (from the slowest frame).
    pub fn min_fps(&self) -> f64 {
        let times = self.frame_times.borrow();
        let max_time = times.iter().cloned().fold(0.0, f64::max);
        if max_time <= 0.0 {
            return 0.0;
        }
        1000.0 / max_time
    }

    /// Compute the maximum FPS (from the fastest frame).
    pub fn max_fps(&self) -> f64 {
        let times = self.frame_times.borrow();
        let min_time = times.iter().cloned().fold(f64::MAX, f64::min).max(0.001);
        1000.0 / min_time
    }

    /// Compute the 1% low FPS (frames worse than this are in the bottom 1%).
    pub fn percentile_1_low_fps(&self) -> f64 {
        let mut times: Vec<f64> = self.frame_times.borrow().iter().cloned().collect();
        if times.is_empty() {
            return 0.0;
        }
        times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        // Sort descending to get slowest frames first
        times.reverse();
        let idx = (times.len() as f64 * 0.01) as usize;
        let idx = idx.min(times.len() - 1);
        let slow_time = times[idx];
        if slow_time <= 0.0 {
            return 0.0;
        }
        1000.0 / slow_time
    }

    /// Compute the frame time standard deviation.
    pub fn frame_time_stddev(&self) -> f64 {
        let times = self.frame_times.borrow();
        if times.len() < 2 {
            return 0.0;
        }
        let mean: f64 = times.iter().sum::<f64>() / times.len() as f64;
        let variance: f64 =
            times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / times.len() as f64;
        variance.sqrt()
    }

    /// Check if the frame rate is meeting the target.
    pub fn meets_target(&self) -> bool {
        self.current_fps() >= self.target_fps * 0.9
    }

    /// Check if the frame rate is stable (low jitter).
    pub fn is_stable(&self) -> bool {
        let stddev = self.frame_time_stddev();
        let times = self.frame_times.borrow();
        if times.is_empty() {
            return true;
        }
        let mean: f64 = times.iter().sum::<f64>() / times.len() as f64;
        if mean <= 0.0 {
            return true;
        }
        // Coefficient of variation < 0.1 (10%)
        stddev / mean < 0.1
    }

    /// Get the number of samples.
    pub fn sample_count(&self) -> usize {
        self.frame_times.borrow().len()
    }

    /// Get the total number of frames recorded.
    pub fn total_frames(&self) -> u64 {
        *self.total_frames.borrow()
    }

    /// Get the target FPS.
    pub fn target_fps(&self) -> f64 {
        self.target_fps
    }

    /// Reset all data.
    pub fn reset(&self) {
        self.frame_times.borrow_mut().clear();
        *self.total_frames.borrow_mut() = 0;
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "FPS: {:.1} (target {:.0}, min {:.1}, max {:.1}, 1%low {:.1}, stddev {:.2}ms)",
            self.current_fps(),
            self.target_fps,
            self.min_fps(),
            self.max_fps(),
            self.percentile_1_low_fps(),
            self.frame_time_stddev()
        )
    }
}

impl Default for FrameRateAnalyzer {
    fn default() -> Self {
        Self::new_60fps()
    }
}

// ---------------------------------------------------------------------------
// Performance counters
// ---------------------------------------------------------------------------

/// A named performance counter for tracking per-frame metrics.
#[derive(Debug, Clone)]
pub struct PerfCounter {
    /// Counter name.
    name: String,
    /// Current value.
    current: RefCell<f64>,
    /// Minimum value observed.
    min: RefCell<f64>,
    /// Maximum value observed.
    max: RefCell<f64>,
    /// Sum of all values.
    sum: RefCell<f64>,
    /// Number of samples.
    count: RefCell<u64>,
}

impl PerfCounter {
    /// Create a new counter with the given name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            current: RefCell::new(0.0),
            min: RefCell::new(f64::MAX),
            max: RefCell::new(f64::MIN),
            sum: RefCell::new(0.0),
            count: RefCell::new(0),
        }
    }

    /// Set the current value.
    pub fn set(&self, value: f64) {
        *self.current.borrow_mut() = value;
        *self.sum.borrow_mut() += value;
        *self.count.borrow_mut() += 1;

        let mut min = self.min.borrow_mut();
        if value < *min {
            *min = value;
        }
        let mut max = self.max.borrow_mut();
        if value > *max {
            *max = value;
        }
    }

    /// Get the current value.
    pub fn get(&self) -> f64 {
        *self.current.borrow()
    }

    /// Get the average value.
    pub fn avg(&self) -> f64 {
        let count = *self.count.borrow();
        if count == 0 {
            return 0.0;
        }
        *self.sum.borrow() / count as f64
    }

    /// Get the minimum value.
    pub fn min(&self) -> f64 {
        let min = *self.min.borrow();
        if min == f64::MAX {
            0.0
        } else {
            min
        }
    }

    /// Get the maximum value.
    pub fn max(&self) -> f64 {
        *self.max.borrow()
    }

    /// Get the name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Reset the counter.
    pub fn reset(&self) {
        *self.current.borrow_mut() = 0.0;
        *self.min.borrow_mut() = f64::MAX;
        *self.max.borrow_mut() = f64::MIN;
        *self.sum.borrow_mut() = 0.0;
        *self.count.borrow_mut() = 0;
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "{}: {:.2} (avg {:.2}, min {:.2}, max {:.2})",
            self.name,
            self.get(),
            self.avg(),
            self.min(),
            self.max()
        )
    }
}

// ---------------------------------------------------------------------------
// Profile snapshot
// ---------------------------------------------------------------------------

/// A snapshot of performance profile data at a point in time.
#[derive(Debug, Clone)]
pub struct ProfileSnapshot {
    /// Timestamp.
    pub timestamp: Instant,
    /// Phase timings.
    pub phases: HashMap<RenderPhase, PhaseTiming>,
    /// Frame rate data.
    pub fps: FrameRateData,
    /// Custom counters.
    pub counters: HashMap<String, f64>,
}

impl ProfileSnapshot {
    /// Create a new empty snapshot.
    pub fn new() -> Self {
        Self {
            timestamp: Instant::now(),
            phases: HashMap::new(),
            fps: FrameRateData::default(),
            counters: HashMap::new(),
        }
    }

    /// Compute the total render time across all phases.
    pub fn total_time_ms(&self) -> f64 {
        self.phases.values().map(|p| p.duration_ms).sum()
    }

    /// Find the slowest phase.
    pub fn slowest_phase(&self) -> Option<(RenderPhase, f64)> {
        self.phases
            .iter()
            .max_by(|a, b| {
                a.1.duration_ms
                    .partial_cmp(&b.1.duration_ms)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(&phase, timing)| (phase, timing.duration_ms))
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        let total = self.total_time_ms();
        let slowest = self
            .slowest_phase()
            .map(|(p, ms)| format!("{} ({:.2}ms)", p.name(), ms))
            .unwrap_or_else(|| "-".to_string());

        format!(
            "Profile: {:.2}ms total, slowest={}, {}",
            total,
            slowest,
            self.fps.report()
        )
    }
}

impl Default for ProfileSnapshot {
    fn default() -> Self {
        Self::new()
    }
}

/// Frame rate data for a profile snapshot.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameRateData {
    /// Current FPS.
    pub current_fps: f64,
    /// Average FPS.
    pub avg_fps: f64,
    /// Frame time in ms.
    pub frame_time_ms: f64,
    /// Whether the target FPS is met.
    pub meets_target: bool,
    /// Frame time jitter in ms.
    pub jitter_ms: f64,
}

impl FrameRateData {
    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "{:.1}fps (avg {:.1}, {:.2}ms, jitter {:.2}ms)",
            self.current_fps, self.avg_fps, self.frame_time_ms, self.jitter_ms
        )
    }
}

// ---------------------------------------------------------------------------
// Profiler (main aggregator)
// ---------------------------------------------------------------------------

/// The main performance profiler.
///
/// Tracks render phase timings, frame rate, and custom performance counters.
pub struct Profiler {
    /// Phase timings.
    phases: RefCell<HashMap<RenderPhase, PhaseTiming>>,
    /// Frame rate analyzer.
    frame_rate: FrameRateAnalyzer,
    /// Custom performance counters.
    counters: RefCell<HashMap<String, PerfCounter>>,
    /// Current active phase timer.
    active_phase: RefCell<Option<(RenderPhase, Instant)>>,
    /// Whether profiling is enabled.
    enabled: RefCell<bool>,
    /// Frame count.
    frame_count: RefCell<u64>,
}

impl Profiler {
    /// Create a new profiler targeting 60 FPS.
    pub fn new() -> Self {
        Self {
            phases: RefCell::new(HashMap::new()),
            frame_rate: FrameRateAnalyzer::new_60fps(),
            counters: RefCell::new(HashMap::new()),
            active_phase: RefCell::new(None),
            enabled: RefCell::new(true),
            frame_count: RefCell::new(0),
        }
    }

    /// Create a profiler targeting a specific FPS.
    pub fn with_target_fps(target_fps: f64) -> Self {
        Self {
            phases: RefCell::new(HashMap::new()),
            frame_rate: FrameRateAnalyzer::new(target_fps),
            counters: RefCell::new(HashMap::new()),
            active_phase: RefCell::new(None),
            enabled: RefCell::new(true),
            frame_count: RefCell::new(0),
        }
    }

    /// Enable or disable profiling.
    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.borrow_mut() = enabled;
    }

    /// Check if profiling is enabled.
    pub fn is_enabled(&self) -> bool {
        *self.enabled.borrow()
    }

    /// Begin timing a render phase.
    pub fn begin_phase(&self, phase: RenderPhase) {
        if !*self.enabled.borrow() {
            return;
        }
        *self.active_phase.borrow_mut() = Some((phase, Instant::now()));
    }

    /// End timing a render phase.
    pub fn end_phase(&self) {
        if !*self.enabled.borrow() {
            return;
        }
        if let Some((phase, start)) = self.active_phase.borrow_mut().take() {
            let duration_ms = start.elapsed().as_secs_f64() * 1000.0;
            let mut phases = self.phases.borrow_mut();
            let timing = phases.entry(phase).or_insert_with(|| PhaseTiming {
                phase,
                ..Default::default()
            });
            timing.duration_ms = duration_ms;
            timing.record(duration_ms);
        }
    }

    /// Time a render phase using a closure.
    pub fn time_phase<F>(&self, phase: RenderPhase, f: F)
    where
        F: FnOnce(),
    {
        self.begin_phase(phase);
        f();
        self.end_phase();
    }

    /// Record a frame completion.
    pub fn record_frame(&self, frame_time_ms: f64) {
        if !*self.enabled.borrow() {
            return;
        }
        self.frame_rate.record_frame(frame_time_ms);
        *self.frame_count.borrow_mut() += 1;
    }

    /// Get or create a performance counter.
    pub fn counter(&self, name: &str) -> PerfCounter {
        let mut counters = self.counters.borrow_mut();
        counters
            .entry(name.to_string())
            .or_insert_with(|| PerfCounter::new(name))
            .clone()
    }

    /// Set a counter value (updates the stored counter directly).
    pub fn set_counter(&self, name: &str, value: f64) {
        if !*self.enabled.borrow() {
            return;
        }
        let mut counters = self.counters.borrow_mut();
        let counter = counters
            .entry(name.to_string())
            .or_insert_with(|| PerfCounter::new(name));
        counter.set(value);
    }

    /// Get the current FPS.
    pub fn current_fps(&self) -> f64 {
        self.frame_rate.current_fps()
    }

    /// Get the average FPS.
    pub fn avg_fps(&self) -> f64 {
        self.frame_rate.avg_fps()
    }

    /// Get the frame rate analyzer.
    pub fn frame_rate_analyzer(&self) -> &FrameRateAnalyzer {
        &self.frame_rate
    }

    /// Get the phase timing for a specific phase.
    pub fn phase_timing(&self, phase: RenderPhase) -> PhaseTiming {
        self.phases
            .borrow()
            .get(&phase)
            .cloned()
            .unwrap_or_else(|| PhaseTiming {
                phase,
                ..Default::default()
            })
    }

    /// Get all phase timings.
    pub fn all_phases(&self) -> Vec<PhaseTiming> {
        self.phases.borrow().values().cloned().collect()
    }

    /// Get the frame count.
    pub fn frame_count(&self) -> u64 {
        *self.frame_count.borrow()
    }

    /// Take a profile snapshot.
    pub fn snapshot(&self) -> ProfileSnapshot {
        ProfileSnapshot {
            timestamp: Instant::now(),
            phases: self.phases.borrow().clone(),
            fps: FrameRateData {
                current_fps: self.frame_rate.current_fps(),
                avg_fps: self.frame_rate.avg_fps(),
                frame_time_ms: 0.0,
                meets_target: self.frame_rate.meets_target(),
                jitter_ms: self.frame_rate.frame_time_stddev(),
            },
            counters: self
                .counters
                .borrow()
                .iter()
                .map(|(name, counter)| (name.clone(), counter.get()))
                .collect(),
        }
    }

    /// Reset all profiling data.
    pub fn reset(&self) {
        self.phases.borrow_mut().clear();
        self.frame_rate.reset();
        self.counters.borrow_mut().clear();
        *self.frame_count.borrow_mut() = 0;
        *self.active_phase.borrow_mut() = None;
    }

    /// Format a comprehensive report.
    pub fn report(&self) -> String {
        let phases: Vec<String> = self.phases.borrow().values().map(|p| p.report()).collect();

        let counters: Vec<String> = self
            .counters
            .borrow()
            .values()
            .map(|c| c.report())
            .collect();

        format!(
            "=== Profiler ===\n{}\nFPS: {}\nFrames: {}\n{}",
            phases.join(", "),
            self.frame_rate.report(),
            self.frame_count.borrow(),
            counters.join(", ")
        )
    }
}

impl Default for Profiler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Stress test utilities
// ---------------------------------------------------------------------------

/// Configuration for a stress test.
#[derive(Debug, Clone)]
pub struct StressTestConfig {
    /// Number of frames to render.
    pub frame_count: u32,
    /// Number of nodes per frame.
    pub nodes_per_frame: u32,
    /// Number of textures per frame.
    pub textures_per_frame: u32,
    /// Number of animated nodes.
    pub animated_nodes: u32,
    /// Whether to enable animations.
    pub enable_animations: bool,
    /// Whether to enable video.
    pub enable_video: bool,
    /// Target FPS.
    pub target_fps: f64,
}

impl StressTestConfig {
    /// Create a default stress test config.
    pub fn default_config() -> Self {
        Self {
            frame_count: 600,
            nodes_per_frame: 100,
            textures_per_frame: 20,
            animated_nodes: 5,
            enable_animations: true,
            enable_video: false,
            target_fps: 60.0,
        }
    }

    /// Create a light stress test config.
    pub fn light() -> Self {
        Self {
            frame_count: 300,
            nodes_per_frame: 50,
            textures_per_frame: 10,
            animated_nodes: 2,
            enable_animations: true,
            enable_video: false,
            target_fps: 60.0,
        }
    }

    /// Create a heavy stress test config.
    pub fn heavy() -> Self {
        Self {
            frame_count: 1200,
            nodes_per_frame: 500,
            textures_per_frame: 50,
            animated_nodes: 20,
            enable_animations: true,
            enable_video: true,
            target_fps: 60.0,
        }
    }
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

/// Result of a stress test.
#[derive(Debug, Clone)]
pub struct StressTestResult {
    /// Configuration used.
    pub config: StressTestConfig,
    /// Total time in milliseconds.
    pub total_time_ms: f64,
    /// Average FPS achieved.
    pub avg_fps: f64,
    /// Minimum FPS achieved.
    pub min_fps: f64,
    /// Maximum FPS achieved.
    pub max_fps: f64,
    /// 1% low FPS.
    pub percentile_1_low_fps: f64,
    /// Frame time standard deviation.
    pub frame_time_stddev_ms: f64,
    /// Whether the target FPS was met.
    pub target_met: bool,
    /// Number of frame drops (frames exceeding target time).
    pub frame_drops: u32,
    /// Peak memory usage.
    pub peak_memory_mb: f64,
    /// Errors encountered.
    pub errors: Vec<String>,
}

impl StressTestResult {
    /// Check if the stress test passed.
    pub fn passed(&self) -> bool {
        self.target_met && self.errors.is_empty()
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "StressTest: {} frames, {:.1}fps avg (min {:.1}, 1%low {:.1}), \
             {} drops, {} errors, peakMem {:.1}MB, passed={}",
            self.config.frame_count,
            self.avg_fps,
            self.min_fps,
            self.percentile_1_low_fps,
            self.frame_drops,
            self.errors.len(),
            self.peak_memory_mb,
            self.passed()
        )
    }
}

// ---------------------------------------------------------------------------
// Jitter analysis
// ---------------------------------------------------------------------------

/// Analyzes frame time jitter (variability).
#[derive(Debug, Clone)]
pub struct JitterAnalyzer {
    /// Recent frame times.
    frame_times: RefCell<Vec<f64>>,
    /// Maximum samples.
    max_samples: usize,
}

impl JitterAnalyzer {
    /// Create a new jitter analyzer.
    pub fn new() -> Self {
        Self {
            frame_times: RefCell::new(Vec::with_capacity(60)),
            max_samples: 60,
        }
    }

    /// Record a new frame time.
    pub fn record(&self, frame_time_ms: f64) {
        let mut times = self.frame_times.borrow_mut();
        times.push(frame_time_ms);
        if times.len() > self.max_samples {
            times.remove(0);
        }
    }

    /// Compute the jitter (standard deviation of frame times).
    pub fn jitter_ms(&self) -> f64 {
        let times = self.frame_times.borrow();
        if times.len() < 2 {
            return 0.0;
        }
        let mean: f64 = times.iter().sum::<f64>() / times.len() as f64;
        let variance: f64 =
            times.iter().map(|t| (t - mean).powi(2)).sum::<f64>() / times.len() as f64;
        variance.sqrt()
    }

    /// Compute the maximum frame-to-frame delta.
    pub fn max_delta_ms(&self) -> f64 {
        let times = self.frame_times.borrow();
        if times.len() < 2 {
            return 0.0;
        }
        let mut max_delta = 0.0;
        for window in times.windows(2) {
            let delta = (window[0] - window[1]).abs();
            if delta > max_delta {
                max_delta = delta;
            }
        }
        max_delta
    }

    /// Check if the frame rate is smooth (low jitter).
    pub fn is_smooth(&self) -> bool {
        let jitter = self.jitter_ms();
        // Jitter < 2ms is considered smooth
        jitter < 2.0
    }

    /// Reset.
    pub fn reset(&self) {
        self.frame_times.borrow_mut().clear();
    }

    /// Format as a report.
    pub fn report(&self) -> String {
        format!(
            "Jitter: {:.2}ms stddev, {:.2}ms maxDelta, smooth={}",
            self.jitter_ms(),
            self.max_delta_ms(),
            self.is_smooth()
        )
    }
}

impl Default for JitterAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stopwatch() {
        let sw = Stopwatch::new();
        assert!((sw.elapsed_ms() - 0.0).abs() < 1.0);

        sw.start();
        std::thread::sleep(Duration::from_millis(10));
        sw.stop();

        assert!(sw.elapsed_ms() > 5.0);
        sw.reset();
        assert!((sw.elapsed_ms() - 0.0).abs() < 1.0);
    }

    #[test]
    fn test_timing_scope() {
        let scope = TimingScope::new("test");
        std::thread::sleep(Duration::from_millis(5));
        let duration = scope.finish();
        assert!(duration.as_millis() >= 5);
    }

    #[test]
    fn test_render_phase_name() {
        assert_eq!(RenderPhase::CpuPreparation.name(), "CPU Prep");
        assert_eq!(RenderPhase::GpuEncoding.name(), "GPU Encode");
        assert_eq!(RenderPhase::Presentation.name(), "Present");
    }

    #[test]
    fn test_phase_timing() {
        let mut timing = PhaseTiming {
            phase: RenderPhase::CpuPreparation,
            ..Default::default()
        };
        timing.record(1.0);
        timing.record(2.0);
        timing.record(3.0);

        assert_eq!(timing.call_count, 3);
        assert!((timing.avg_ms() - 2.0).abs() < 0.01);
        assert!((timing.min_ms - 1.0).abs() < 0.01);
        assert!((timing.max_ms - 3.0).abs() < 0.01);
    }

    #[test]
    fn test_phase_timing_empty() {
        let timing = PhaseTiming::default();
        assert_eq!(timing.avg_ms(), 0.0);
    }

    #[test]
    fn test_frame_rate_analyzer() {
        let analyzer = FrameRateAnalyzer::new_60fps();
        assert_eq!(analyzer.current_fps(), 0.0);

        // Simulate 60fps frames
        for _ in 0..60 {
            analyzer.record_frame(16.67);
        }

        let fps = analyzer.current_fps();
        assert!(fps > 55.0 && fps < 65.0);
        assert!(analyzer.meets_target());
    }

    #[test]
    fn test_frame_rate_analyzer_min_max() {
        let analyzer = FrameRateAnalyzer::new_60fps();
        analyzer.record_frame(10.0); // 100fps
        analyzer.record_frame(33.33); // 30fps
        analyzer.record_frame(16.67); // 60fps

        assert!(analyzer.min_fps() < 35.0);
        assert!(analyzer.max_fps() > 90.0);
    }

    #[test]
    fn test_frame_rate_analyzer_stability() {
        let analyzer = FrameRateAnalyzer::new_60fps();
        // Consistent frame times = stable
        for _ in 0..30 {
            analyzer.record_frame(16.0);
        }
        assert!(analyzer.is_stable());
    }

    #[test]
    fn test_frame_rate_analyzer_unstable() {
        let analyzer = FrameRateAnalyzer::new_60fps();
        // Varying frame times = unstable
        for i in 0..30 {
            analyzer.record_frame(10.0 + (i % 3) as f64 * 10.0);
        }
        assert!(!analyzer.is_stable());
    }

    #[test]
    fn test_perf_counter() {
        let counter = PerfCounter::new("draw_calls");
        counter.set(5.0);
        counter.set(10.0);
        counter.set(15.0);

        assert!((counter.get() - 15.0).abs() < 0.01);
        assert!((counter.avg() - 10.0).abs() < 0.01);
        assert!((counter.min() - 5.0).abs() < 0.01);
        assert!((counter.max() - 15.0).abs() < 0.01);
    }

    #[test]
    fn test_perf_counter_reset() {
        let counter = PerfCounter::new("test");
        counter.set(10.0);
        counter.reset();
        assert!((counter.get() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_profile_snapshot() {
        let mut snapshot = ProfileSnapshot::new();
        snapshot.phases.insert(
            RenderPhase::CpuPreparation,
            PhaseTiming {
                phase: RenderPhase::CpuPreparation,
                duration_ms: 5.0,
                ..Default::default()
            },
        );

        assert!((snapshot.total_time_ms() - 5.0).abs() < 0.01);
        let slowest = snapshot.slowest_phase();
        assert!(slowest.is_some());
    }

    #[test]
    fn test_profiler_phases() {
        let profiler = Profiler::new();
        profiler.begin_phase(RenderPhase::CpuPreparation);
        std::thread::sleep(Duration::from_millis(5));
        profiler.end_phase();

        let timing = profiler.phase_timing(RenderPhase::CpuPreparation);
        assert!(timing.duration_ms > 0.0);
    }

    #[test]
    fn test_profiler_time_phase() {
        let profiler = Profiler::new();
        let mut value = 0;
        profiler.time_phase(RenderPhase::Batching, || {
            value = 42;
        });
        assert_eq!(value, 42);
    }

    #[test]
    fn test_profiler_disabled() {
        let profiler = Profiler::new();
        profiler.set_enabled(false);

        profiler.begin_phase(RenderPhase::CpuPreparation);
        profiler.end_phase();

        let timing = profiler.phase_timing(RenderPhase::CpuPreparation);
        assert_eq!(timing.duration_ms, 0.0);
    }

    #[test]
    fn test_profiler_counters() {
        let profiler = Profiler::new();
        profiler.set_counter("draw_calls", 5.0);
        profiler.set_counter("draw_calls", 10.0);

        // The internal counter tracks both values
        let counter = profiler.counter("draw_calls");
        // Counter has 2 samples: 5.0 and 10.0
        assert!((counter.get() - 10.0).abs() < 0.01);
        assert!((counter.avg() - 7.5).abs() < 0.01);
    }

    #[test]
    fn test_profiler_frame_rate() {
        let profiler = Profiler::new();
        for _ in 0..60 {
            profiler.record_frame(16.67);
        }
        assert!(profiler.current_fps() > 55.0);
        assert_eq!(profiler.frame_count(), 60);
    }

    #[test]
    fn test_profiler_reset() {
        let profiler = Profiler::new();
        profiler.record_frame(16.0);
        profiler.reset();
        assert_eq!(profiler.frame_count(), 0);
    }

    #[test]
    fn test_stress_test_config() {
        let config = StressTestConfig::default_config();
        assert_eq!(config.frame_count, 600);
        assert_eq!(config.target_fps, 60.0);
    }

    #[test]
    fn test_stress_test_light() {
        let config = StressTestConfig::light();
        assert!(config.frame_count < StressTestConfig::default_config().frame_count);
    }

    #[test]
    fn test_stress_test_heavy() {
        let config = StressTestConfig::heavy();
        assert!(config.frame_count > StressTestConfig::default_config().frame_count);
    }

    #[test]
    fn test_stress_test_result() {
        let result = StressTestResult {
            config: StressTestConfig::default_config(),
            total_time_ms: 10000.0,
            avg_fps: 60.0,
            min_fps: 55.0,
            max_fps: 65.0,
            percentile_1_low_fps: 50.0,
            frame_time_stddev_ms: 1.0,
            target_met: true,
            frame_drops: 0,
            peak_memory_mb: 100.0,
            errors: vec![],
        };
        assert!(result.passed());
    }

    #[test]
    fn test_stress_test_result_failed() {
        let result = StressTestResult {
            config: StressTestConfig::default_config(),
            total_time_ms: 10000.0,
            avg_fps: 30.0,
            min_fps: 20.0,
            max_fps: 40.0,
            percentile_1_low_fps: 15.0,
            frame_time_stddev_ms: 5.0,
            target_met: false,
            frame_drops: 100,
            peak_memory_mb: 200.0,
            errors: vec!["timeout".to_string()],
        };
        assert!(!result.passed());
    }

    #[test]
    fn test_jitter_analyzer() {
        let analyzer = JitterAnalyzer::new();
        for _ in 0..30 {
            analyzer.record(16.0);
        }
        assert!(analyzer.is_smooth());
        assert!((analyzer.jitter_ms() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_jitter_analyzer_unsmooth() {
        let analyzer = JitterAnalyzer::new();
        for i in 0..30 {
            analyzer.record(10.0 + (i % 3) as f64 * 10.0);
        }
        assert!(!analyzer.is_smooth());
    }

    #[test]
    fn test_jitter_max_delta() {
        let analyzer = JitterAnalyzer::new();
        analyzer.record(10.0);
        analyzer.record(30.0);
        analyzer.record(10.0);
        assert!((analyzer.max_delta_ms() - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_jitter_reset() {
        let analyzer = JitterAnalyzer::new();
        analyzer.record(10.0);
        analyzer.reset();
        assert!((analyzer.jitter_ms() - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_frame_rate_percentile_1_low() {
        let analyzer = FrameRateAnalyzer::new_60fps();
        // 100 frames at 16ms (62.5fps)
        for _ in 0..99 {
            analyzer.record_frame(16.0);
        }
        // 1 frame at 50ms (20fps)
        analyzer.record_frame(50.0);

        let p1 = analyzer.percentile_1_low_fps();
        // With 100 samples, idx=1 (2nd slowest after reverse sort).
        // The slowest (50ms) is at index 0, index 1 is 16ms → 62.5fps.
        // So p1 should be around 62.5fps, which is > 30fps.
        assert!(p1 > 50.0);
    }

    #[test]
    fn test_stopwatch_elapsed_us() {
        let sw = Stopwatch::new();
        sw.start();
        sw.stop();
        assert!(sw.elapsed_us() >= 0.0);
    }

    #[test]
    fn test_stopwatch_elapsed_ns() {
        let sw = Stopwatch::new();
        sw.start();
        sw.stop();
        assert!(sw.elapsed_ns() >= 0.0);
    }

    #[test]
    fn test_timing_scope_name() {
        let scope = TimingScope::new("my_test");
        assert_eq!(scope.name(), "my_test");
    }

    #[test]
    fn test_timing_scope_elapsed() {
        let scope = TimingScope::new("test");
        std::thread::sleep(Duration::from_millis(5));
        let elapsed = scope.elapsed();
        assert!(elapsed.as_millis() >= 5);
        // Can still finish after checking elapsed
        let finished = scope.finish();
        assert!(finished.as_millis() >= 5);
    }

    #[test]
    fn test_phase_timing_report() {
        let timing = PhaseTiming {
            phase: RenderPhase::GpuEncoding,
            duration_ms: 2.5,
            call_count: 10,
            min_ms: 1.0,
            max_ms: 5.0,
            total_ms: 25.0,
        };
        let report = timing.report();
        assert!(report.contains("GPU Encode"));
        assert!(report.contains("2.50"));
    }

    #[test]
    fn test_frame_rate_data_report() {
        let data = FrameRateData {
            current_fps: 60.0,
            avg_fps: 58.0,
            frame_time_ms: 16.67,
            meets_target: true,
            jitter_ms: 0.5,
        };
        let report = data.report();
        assert!(report.contains("60.0"));
    }

    #[test]
    fn test_profiler_with_target_fps() {
        let profiler = Profiler::with_target_fps(144.0);
        assert!((profiler.frame_rate_analyzer().target_fps() - 144.0).abs() < 0.01);
    }

    #[test]
    fn test_all_render_phases() {
        let phases = [
            RenderPhase::CpuPreparation,
            RenderPhase::GpuEncoding,
            RenderPhase::GpuSubmission,
            RenderPhase::Presentation,
            RenderPhase::TextureUpload,
            RenderPhase::GlyphRasterization,
            RenderPhase::TextShaping,
            RenderPhase::AnimationUpdate,
            RenderPhase::VideoAcquisition,
            RenderPhase::Batching,
            RenderPhase::ResourceManagement,
            RenderPhase::Custom,
        ];
        assert_eq!(phases.len(), 12);
        for phase in &phases {
            assert!(!phase.name().is_empty());
        }
    }
}
