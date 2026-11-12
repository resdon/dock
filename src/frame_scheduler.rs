//! Frame scheduler and invalidation system.
//!
//! Enforces a maximum frame rate for animation-driven redraws and eliminates
//! unnecessary renders by tracking whether visual content actually changed.
//!
//! # Design
//!
//! The main event loop previously ran at up to ~125 FPS (8ms poll timeout)
//! whenever `needs_redraw` was true, even when no visual content had changed.
//! This module introduces two mechanisms:
//!
//! 1. **Frame rate limiter**: Caps animation-driven redraws to a configurable
//!    maximum FPS (default 75). Between frames, the scheduler computes the
//!    remaining time until the next frame is due and returns it as the poll
//!    timeout, so the loop sleeps instead of spinning.
//!
//! 2. **Invalidation tracking**: Tracks whether any visual content actually
//!    changed since the last render. If `needs_redraw` is true but nothing
//!    visual changed (e.g., a background state update that was already rendered),
//!    the scheduler skips the draw call and clears the flag.
//!
//! # Coordinate Space
//! N/A — this module operates on time and boolean flags only.
//!
//! # Units
//! Time is measured in milliseconds and `std::time::Duration`.
//!
//! # Invariants
//! - The frame interval is always positive (FPS >= 1).
//! - `last_frame_time` is updated only after a frame is actually rendered.
//! - `content_changed` is reset after each render decision.

use std::time::{Duration, Instant};

/// Default maximum frame rate for animation-driven redraws.
/// 75 FPS balances smoothness with CPU efficiency.
const DEFAULT_MAX_FPS: f64 = 75.0;

/// Minimum poll timeout in milliseconds when idle.
/// Prevents busy-waiting when no events are expected.
const IDLE_TIMEOUT_MS: i32 = 1000;

/// Minimum poll timeout in milliseconds when waiting for animation frames.
/// Ensures the loop doesn't spin faster than the frame rate limit.
const ANIMATION_MIN_TIMEOUT_MS: i32 = 1;

/// Frame scheduler that enforces a maximum frame rate and tracks content changes.
///
/// The scheduler maintains two key pieces of state:
/// - `last_frame_time`: when the last frame was rendered
/// - `content_changed`: whether visual content changed since the last render decision
///
/// # Usage
///
/// ```rust,ignore
/// let mut scheduler = FrameScheduler::new();
///
/// // In the main loop:
/// if some_visual_change {
///     scheduler.mark_content_changed();
/// }
///
/// // Before drawing:
/// if scheduler.should_render() {
///     // ... draw ...
///     scheduler.on_frame_rendered();
/// }
///
/// // For poll timeout:
/// let timeout_ms = scheduler.poll_timeout(needs_redraw, animations_active);
/// ```
pub struct FrameScheduler {
    /// Maximum frames per second for animation-driven redraws.
    #[allow(dead_code)]
    max_fps: f64,
    
    /// Duration between frames at max_fps.
    frame_interval: Duration,
    
    /// When the last frame was actually rendered.
    last_frame_time: Option<Instant>,
    
    /// Whether visual content changed since the last render decision.
    /// Reset after each `should_render()` call.
    content_changed: bool,
    
    /// Whether any animations are currently active (fades, bounces, etc.).
    /// Used to distinguish animation-driven redraws from event-driven ones.
    animations_active: bool,
}

impl FrameScheduler {
    /// Create a new frame scheduler with the default 75 FPS limit.
    pub fn new() -> Self {
        let max_fps = DEFAULT_MAX_FPS;
        let frame_interval = Duration::from_secs_f64(1.0 / max_fps);
        Self {
            max_fps,
            frame_interval,
            last_frame_time: None,
            content_changed: false,
            animations_active: false,
        }
    }

    /// Create a new frame scheduler with a custom maximum FPS.
    pub fn with_max_fps(max_fps: f64) -> Self {
        let max_fps = max_fps.max(1.0).min(240.0);
        let frame_interval = Duration::from_secs_f64(1.0 / max_fps);
        Self {
            max_fps,
            frame_interval,
            last_frame_time: None,
            content_changed: false,
            animations_active: false,
        }
    }

    /// Mark that visual content has changed since the last render.
    ///
    /// Call this whenever a state change would affect what is displayed:
    /// icon changes, fade alpha changes, hover state changes, etc.
    pub fn mark_content_changed(&mut self) {
        self.content_changed = true;
    }

    /// Update the animation active state.
    ///
    /// Call this each loop iteration to reflect whether animations (fades,
    /// bounces, etc.) are currently in progress.
    pub fn set_animations_active(&mut self, active: bool) {
        self.animations_active = active;
    }

    /// Check whether a frame should be rendered now.
    ///
    /// Returns `true` if:
    /// 1. Content has changed since the last render, OR
    /// 2. Enough time has elapsed since the last frame (respecting max_fps),
    ///    AND animations are still active
    ///
    /// On the first call (no previous frame), always returns `true` to ensure
    /// the initial frame renders immediately.
    ///
    /// This method resets `content_changed` to `false` after checking, so
    /// subsequent calls within the same loop iteration return `false` unless
    /// new changes are marked.
    pub fn should_render(&mut self, needs_redraw: bool) -> bool {
        // If nothing needs a redraw, skip rendering
        if !needs_redraw {
            self.content_changed = false;
            return false;
        }

        // If content actually changed, render immediately
        if self.content_changed {
            self.content_changed = false;
            return true;
        }

        // For animation-driven redraws, respect the frame rate limit
        if self.animations_active {
            match self.last_frame_time {
                Some(last) => {
                    let elapsed = last.elapsed();
                    if elapsed >= self.frame_interval {
                        return true;
                    }
                }
                None => {
                    // First frame — render immediately
                    return true;
                }
            }
        }

        // No content change and not animation-driven — skip render
        self.content_changed = false;
        false
    }

    /// Call after a frame has been rendered to update the timing state.
    ///
    /// This records the current time as the last frame time, which is used
    /// for subsequent frame rate limiting.
    pub fn on_frame_rendered(&mut self) {
        self.last_frame_time = Some(Instant::now());
    }

    /// Compute the poll timeout in milliseconds for the main event loop.
    ///
    /// Returns the optimal timeout that balances responsiveness with CPU efficiency:
    /// - When content has changed: minimal timeout for immediate redraw
    /// - When animations are active: time until the next frame is due (max_fps)
    /// - When idle: longer timeout to reduce CPU usage
    ///
    /// # Arguments
    /// * `needs_redraw` - Whether a redraw has been requested
    /// * `animations_active` - Whether animations (fades, bounces) are in progress
    ///
    /// # Returns
    /// Timeout in milliseconds for `poll()`
    pub fn poll_timeout(&self, needs_redraw: bool, animations_active: bool) -> i32 {
        if !needs_redraw && !animations_active {
            // Fully idle — sleep until an event arrives
            return IDLE_TIMEOUT_MS;
        }

        if self.content_changed || needs_redraw {
            // Content changed — render as soon as possible
            return ANIMATION_MIN_TIMEOUT_MS;
        }

        if animations_active {
            // Animation-driven — respect frame rate limit
            if let Some(last) = self.last_frame_time {
                let elapsed = last.elapsed();
                if elapsed >= self.frame_interval {
                    return ANIMATION_MIN_TIMEOUT_MS;
                }
                let remaining = self.frame_interval.saturating_sub(elapsed);
                let ms = remaining.as_millis().max(1) as i32;
                return ms.min(33); // Cap at ~30 FPS minimum wake rate
            }
            // No previous frame — render immediately
            return ANIMATION_MIN_TIMEOUT_MS;
        }

        // Fallback: idle timeout
        IDLE_TIMEOUT_MS
    }

    /// Reset the scheduler state. Useful when the display configuration changes
    /// or after a long idle period.
    pub fn reset(&mut self) {
        self.last_frame_time = None;
        self.content_changed = false;
        self.animations_active = false;
    }
}

impl Default for FrameScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_always_renders() {
        let mut scheduler = FrameScheduler::new();
        scheduler.mark_content_changed();
        assert!(scheduler.should_render(true));
    }

    #[test]
    fn content_change_triggers_render() {
        let mut scheduler = FrameScheduler::new();
        scheduler.on_frame_rendered();
        std::thread::sleep(Duration::from_millis(10));
        
        scheduler.mark_content_changed();
        assert!(scheduler.should_render(true));
    }

    #[test]
    fn no_change_skips_render() {
        let mut scheduler = FrameScheduler::new();
        scheduler.on_frame_rendered();
        std::thread::sleep(Duration::from_millis(10));
        
        // No content change, no animations
        assert!(!scheduler.should_render(true));
    }

    #[test]
    fn animation_respects_frame_rate() {
        let mut scheduler = FrameScheduler::with_max_fps(60.0);
        scheduler.set_animations_active(true);
        scheduler.on_frame_rendered();
        
        // Too soon — should not render
        assert!(!scheduler.should_render(true));
        
        // Wait for frame interval
        std::thread::sleep(scheduler.frame_interval + Duration::from_millis(1));
        assert!(scheduler.should_render(true));
    }

    #[test]
    fn idle_timeout_is_long() {
        let scheduler = FrameScheduler::new();
        let timeout = scheduler.poll_timeout(false, false);
        assert_eq!(timeout, IDLE_TIMEOUT_MS);
    }

    #[test]
    fn content_changed_timeout_is_short() {
        let mut scheduler = FrameScheduler::new();
        scheduler.mark_content_changed();
        let timeout = scheduler.poll_timeout(true, false);
        assert_eq!(timeout, ANIMATION_MIN_TIMEOUT_MS);
    }
}
