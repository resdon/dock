// src/graphics/fade.rs

use std::time::Instant;

#[derive(Debug, Clone)]
pub struct FadeAnimation {
    pub current_alpha: f32,
    pub target_alpha: f32,
    pub last_update: Instant,
    pub duration_secs: f32,
    pub min_alpha: f32,
    pub max_alpha: f32,
    pub just_became_hidden: bool,
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

    pub fn show(&mut self) {
        if self.target_alpha != self.max_alpha {
            self.target_alpha = self.max_alpha;
            self.last_update = Instant::now();
        }
    }

    pub fn hide(&mut self) {
        if self.target_alpha != self.min_alpha {
            self.target_alpha = self.min_alpha;
            self.last_update = Instant::now();
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        if visible {
            self.show();
        } else {
            self.hide();
        }
    }

    /// Advances the animation frame based on elapsed time.
    /// Returns `true` if alpha changed and a frame redraw is required.
    pub fn tick(&mut self) -> bool {
        self.just_became_hidden = false;
        self.just_became_visible = false;

        if (self.current_alpha - self.target_alpha).abs() < f32::EPSILON {
            return false;
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f32();
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

        // Trigger edge-state transitions
        if (self.current_alpha - self.min_alpha).abs() < f32::EPSILON
            && self.target_alpha == self.min_alpha
        {
            self.just_became_hidden = true;
        } else if (self.current_alpha - self.max_alpha).abs() < f32::EPSILON
            && self.target_alpha == self.max_alpha
        {
            self.just_became_visible = true;
        }

        true
    }

    pub fn is_fully_hidden(&self) -> bool {
        (self.current_alpha - self.min_alpha).abs() < f32::EPSILON
    }

    pub fn is_fully_visible(&self) -> bool {
        (self.current_alpha - self.max_alpha).abs() < f32::EPSILON
    }

    /// Multiplies the pixel buffer's alpha channel (byte 3 of RGBA/BGRA) by current_alpha.
    pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
        if (self.current_alpha - 1.0).abs() < f32::EPSILON {
            return; // Fast path: full opacity
        }

        let factor = self.current_alpha.clamp(0.0, 1.0);
        for chunk in canvas.chunks_exact_mut(4) {
            chunk[3] = (chunk[3] as f32 * factor) as u8;
        }
    }
}
