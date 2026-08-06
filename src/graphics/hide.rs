// In src/graphics/hide.rs

pub const HIDDEN_ALPHA: f32 = 0.2; // Adjust target alpha here (0.2 = 80% transparency)

use std::time::Instant;

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
        if self.mode != Mode::Visible {
            self.mode = Mode::Visible;
            self.target_alpha = 1.0;
            self.just_became_visible = true;
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
        self.just_became_hidden = false;
        self.just_became_visible = false;

        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        if (self.current_alpha - self.target_alpha).abs() < 0.01 {
            self.current_alpha = self.target_alpha;
            
            if self.mode == Mode::Hiding && (self.current_alpha - HIDDEN_ALPHA).abs() < f32::EPSILON {
                self.mode = Mode::Hidden;
                self.just_became_hidden = true;
            }
            return false;
        }

        let step = dt / self.duration_secs.max(0.001);
        if self.current_alpha < self.target_alpha {
            self.current_alpha = (self.current_alpha + step).min(self.target_alpha);
        } else {
            self.current_alpha = (self.current_alpha - step).max(self.target_alpha);
        }

        true
    }

    pub fn is_fully_hidden(&self) -> bool {
        self.mode == Mode::Hidden || (self.current_alpha - HIDDEN_ALPHA).abs() < 0.01
    }

    pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
        if self.current_alpha >= 0.99 {
            return;
        }
        let alpha = self.current_alpha;
        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = (pixel[0] as f32 * alpha) as u8;
            pixel[1] = (pixel[1] as f32 * alpha) as u8;
            pixel[2] = (pixel[2] as f32 * alpha) as u8;
            pixel[3] = (pixel[3] as f32 * alpha) as u8;
        }
    }
}
