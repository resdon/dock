//! Dock hide/show animation state.
//!
//! Tracks whether the dock is visible, hidden, or transitioning.

// In src/graphics/hide.rs

pub const HIDDEN_ALPHA: f32 = 0.2; // Adjust target alpha here (0.2 = 80% transparency)

use std::time::{Duration, Instant};

pub const HIDE_FRAME_INTERVAL: Duration = Duration::from_nanos(41_666_667);
const ALPHA_EPSILON: f32 = 1e-3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Visible,
    Hiding,
    Hidden,
}

pub struct AutoHideState {
    pub mode: Mode,
    pub current_alpha: f32,
    pub target_alpha: f32,
    pub last_update: Instant,
    pub duration_secs: f32,
    pub just_became_hidden: bool,
    pub just_became_visible: bool,
    pub hide_timer: Option<Instant>,
}

impl Default for AutoHideState {
    fn default() -> Self {
        Self::new()
    }
}

impl AutoHideState {
    pub fn new() -> Self {
        Self {
            mode: Mode::Visible,
            current_alpha: 1.0,
            target_alpha: 1.0,
            last_update: Instant::now(),
            duration_secs: 1.0, // 0.25 default time of fade
            just_became_hidden: false,
            just_became_visible: false,
            hide_timer: None,
        }
    }

    pub fn show(&mut self) {
        self.hide_timer = None;
        if self.mode == Mode::Hidden || self.mode == Mode::Hiding || (self.target_alpha - 1.0).abs() > ALPHA_EPSILON {
            self.mode = Mode::Visible;
            self.target_alpha = 1.0;
            self.last_update = Instant::now() - HIDE_FRAME_INTERVAL;
        }
    }

    pub fn start_hide_timer(&mut self) {
        if self.mode == Mode::Visible && self.hide_timer.is_none() {
            self.hide_timer = Some(Instant::now());
        }
    }

    pub fn update_timer(&mut self, timeout_ms: u64) {
        if let Some(timer) = self.hide_timer {
            if timer.elapsed() >= Duration::from_millis(timeout_ms) {
                self.mode = Mode::Hiding;
                self.target_alpha = HIDDEN_ALPHA;
                self.last_update = Instant::now() - HIDE_FRAME_INTERVAL;
                self.hide_timer = None;
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        self.just_became_hidden = false;
        self.just_became_visible = false;

        if (self.current_alpha - self.target_alpha).abs() <= ALPHA_EPSILON {
            self.current_alpha = self.target_alpha;
            if self.target_alpha <= HIDDEN_ALPHA + ALPHA_EPSILON {
                self.mode = Mode::Hidden;
            } else if self.target_alpha >= 1.0 - ALPHA_EPSILON {
                self.mode = Mode::Visible;
            }
            return false;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update);
        if elapsed < HIDE_FRAME_INTERVAL {
            // Keep animation progressing on subsequent ticks.
            return true;
        }
        let dt = elapsed.as_secs_f32();
        self.last_update = now;

        if self.duration_secs <= 0.0 {
            self.current_alpha = self.target_alpha;
        } else {
            let range = 1.0 - HIDDEN_ALPHA;
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

        if (self.current_alpha - HIDDEN_ALPHA).abs() <= ALPHA_EPSILON
            && self.target_alpha <= HIDDEN_ALPHA + ALPHA_EPSILON
        {
            self.mode = Mode::Hidden;
            self.just_became_hidden = true;
        } else if (self.current_alpha - 1.0).abs() <= ALPHA_EPSILON
            && self.target_alpha >= 1.0 - ALPHA_EPSILON
        {
            self.mode = Mode::Visible;
            self.just_became_visible = true;
        } else if self.target_alpha < self.current_alpha {
            self.mode = Mode::Hiding;
        } else {
            self.mode = Mode::Visible;
        }

        true
    }

    pub fn is_fully_hidden(&self) -> bool {
        self.mode == Mode::Hidden
            && self.target_alpha <= HIDDEN_ALPHA + ALPHA_EPSILON
            && (self.current_alpha - HIDDEN_ALPHA).abs() <= ALPHA_EPSILON
    }

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
            chunk[0] = ((chunk[0] as u16 * alpha + 127) / 255) as u8;
            chunk[1] = ((chunk[1] as u16 * alpha + 127) / 255) as u8;
            chunk[2] = ((chunk[2] as u16 * alpha + 127) / 255) as u8;
            chunk[3] = ((chunk[3] as u16 * alpha + 127) / 255) as u8;
        }
    }
}
