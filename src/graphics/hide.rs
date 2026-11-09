//! Dock hide/show animation state.
//!
//! Tracks whether the dock is visible, hidden, or transitioning.

// In src/graphics/hide.rs

pub const HIDDEN_ALPHA: f32 = 0.2; // Adjust target alpha here (0.2 = 80% transparency)

use std::time::{Duration, Instant};

pub const HIDE_FRAME_INTERVAL: Duration = Duration::from_nanos(41_666_667);

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
        if self.mode != Mode::Visible || (self.target_alpha - 1.0).abs() > f32::EPSILON {
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
            if timer.elapsed() >= std::time::Duration::from_millis(timeout_ms) {
                self.mode = Mode::Hiding;
                self.target_alpha = HIDDEN_ALPHA;
                self.hide_timer = None;
            }
        }
    }

    pub fn tick(&mut self) -> bool {
        // Hide animation disabled — always stay fully visible
        self.just_became_hidden = false;
        self.just_became_visible = false;
        self.current_alpha = 1.0;
        self.target_alpha = 1.0;
        self.mode = Mode::Visible;
        false
    }

    pub fn is_fully_hidden(&self) -> bool {
        // Hide disabled — dock is always visible
        false
    }

    pub fn apply_alpha_to_canvas(&self, _canvas: &mut [u8]) {
        // Hide animation disabled — no alpha applied, dock always fully visible
    }
}
