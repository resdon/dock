//! Fade animation.
//!
//! Smooth alpha interpolation from current value toward a target value.

// src/graphics/fade.rs

use std::time::{Duration, Instant};

/// Fade presentation cadence. State may be sampled more often by input, but
/// actual fade frames are emitted at most 24 times per second.
pub const FADE_FRAME_INTERVAL: Duration = Duration::from_nanos(41_666_667);
const ALPHA_EPSILON: f32 = 1e-3;

/// Smooth alpha interpolation animation for show/hide transitions.
///
/// Interpolates `current_alpha` toward `target_alpha` over `duration_secs`
/// using time-based stepping (not frame-dependent). Fires edge-state flags
/// (`just_became_hidden`, `just_became_visible`) when the animation reaches
/// its bounds so the main loop can trigger cleanup or redraw.
///
/// # Invariants
/// - `min_alpha <= current_alpha <= max_alpha`
/// - `min_alpha <= target_alpha <= max_alpha`
/// - Fade frames are throttled to ~24 FPS regardless of display refresh rate
#[derive(Debug, Clone)]
pub struct FadeAnimation {
    /// Current interpolated alpha value (0.0 = fully transparent).
    pub current_alpha: f32,
    /// Target alpha value the animation is moving toward.
    pub target_alpha: f32,
    /// Last time the animation was updated.
    pub last_update: Instant,
    /// Total duration of the fade animation in seconds.
    pub duration_secs: f32,
    /// Minimum alpha bound (usually 0.0 for fully hidden).
    pub min_alpha: f32,
    /// Maximum alpha bound (usually 1.0 for fully visible).
    pub max_alpha: f32,
    /// Set to `true` for one frame when animation reaches `min_alpha`.
    pub just_became_hidden: bool,
    /// Set to `true` for one frame when animation reaches `max_alpha`.
    pub just_became_visible: bool,
}

impl FadeAnimation {
    pub fn new(min_alpha: f32, max_alpha: f32, duration_secs: f32) -> Self {
        Self {
            current_alpha: max_alpha,
            target_alpha: max_alpha,
            last_update: Instant::now(),
            duration_secs,
            min_alpha,
            max_alpha,
            just_became_hidden: false,
            just_became_visible: false,
        }
    }

    /// Creates a fade animation in its hidden state without scheduling an
    /// artificial fade-out. This is important for popups that do not exist at
    /// startup: initializing them visible and hiding them on the first update
    /// incorrectly makes the main loop treat startup as popup-only animation.
    pub fn new_hidden(min_alpha: f32, max_alpha: f32, duration_secs: f32) -> Self {
        let mut animation = Self::new(min_alpha, max_alpha, duration_secs);
        animation.current_alpha = min_alpha;
        animation.target_alpha = min_alpha;
        animation
    }

    pub fn show(&mut self) {
        if (self.target_alpha - self.max_alpha).abs() > ALPHA_EPSILON
            || (self.current_alpha - self.max_alpha).abs() > ALPHA_EPSILON
        {
            self.target_alpha = self.max_alpha;
            self.last_update = Instant::now() - FADE_FRAME_INTERVAL;
        }
    }

    pub fn show_with_duration(&mut self, duration_secs: f32) {
        self.duration_secs = duration_secs;
        self.show();
    }

    pub fn hide(&mut self) {
        if (self.target_alpha - self.min_alpha).abs() > ALPHA_EPSILON
            || (self.current_alpha - self.min_alpha).abs() > ALPHA_EPSILON
        {
            self.target_alpha = self.min_alpha;
            self.last_update = Instant::now() - FADE_FRAME_INTERVAL;
        }
    }

    pub fn hide_with_duration(&mut self, duration_secs: f32) {
        self.duration_secs = duration_secs;
        self.hide();
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible {
            self.show();
        } else {
            self.hide();
        }
    }

    pub fn set_visible_with_duration(&mut self, visible: bool, duration_secs: f32) {
        self.duration_secs = duration_secs;
        self.set_visible(visible);
    }

    /// Advances the animation frame based on elapsed time.
    /// Returns `true` if alpha changed and a frame redraw is required.
    pub fn tick(&mut self) -> bool {
        self.just_became_hidden = false;
        self.just_became_visible = false;

        if (self.current_alpha - self.target_alpha).abs() <= ALPHA_EPSILON {
            self.current_alpha = self.target_alpha;
            return false;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        if elapsed < FADE_FRAME_INTERVAL {
            // **FIX (v1.5.5)**: Return false — alpha hasn't changed yet.
            // The frame scheduler will wake us again when the next frame is due.
            return false;
        }
        let dt = elapsed.as_secs_f32();
        self.last_update = now;

        if self.duration_secs <= 0.0 {
            self.current_alpha = self.target_alpha;
        } else {
            let range = self.max_alpha - self.min_alpha;
            let step = (range / self.duration_secs) * dt;

            if self.target_alpha > self.current_alpha {
                self.current_alpha = (self.current_alpha + step).min(self.target_alpha);
            } else {
                self.current_alpha = (self.current_alpha - step).max(self.target_alpha);
            }
        }

        if (self.current_alpha - self.target_alpha).abs() <= ALPHA_EPSILON {
            self.current_alpha = self.target_alpha;
        }

        // Trigger edge-state transitions
        if (self.current_alpha - self.min_alpha).abs() <= ALPHA_EPSILON
            && (self.target_alpha - self.min_alpha).abs() <= ALPHA_EPSILON
        {
            self.just_became_hidden = true;
        } else if (self.current_alpha - self.max_alpha).abs() <= ALPHA_EPSILON
            && (self.target_alpha - self.max_alpha).abs() <= ALPHA_EPSILON
        {
            self.just_became_visible = true;
        }

        true
    }

    pub fn is_fully_hidden(&self) -> bool {
        (self.target_alpha - self.min_alpha).abs() <= ALPHA_EPSILON
            && (self.current_alpha - self.min_alpha).abs() <= ALPHA_EPSILON
    }

    pub fn is_fully_visible(&self) -> bool {
        (self.current_alpha - self.max_alpha).abs() <= ALPHA_EPSILON
            && (self.target_alpha - self.max_alpha).abs() <= ALPHA_EPSILON
    }

    /// Applies popup opacity without invoking floating-point math for every pixel.
    pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
        let alpha = (self.current_alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as u16;

        if alpha >= 255 {
            return;
        }
        if alpha == 0 {
            canvas.fill(0);
            return;
        }

        for chunk in canvas.chunks_exact_mut(4) {
            chunk[3] = ((chunk[3] as u16 * alpha + 127) / 255) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_fade_starts_idle_without_a_fade_out() {
        let mut fade = FadeAnimation::new_hidden(0.0, 1.0, 1.0);
        assert_eq!(fade.current_alpha, 0.0);
        assert_eq!(fade.target_alpha, 0.0);
        assert!(!fade.tick());
    }

    #[test]
    fn hidden_fade_can_be_shown_normally() {
        let mut fade = FadeAnimation::new_hidden(0.0, 1.0, 1.0);
        fade.show();
        assert_eq!(fade.target_alpha, 1.0);
        assert!(fade.tick());
    }
}
