use std::time::Instant;

pub struct AutoHideState {
    pub current_alpha: f32,
    pub target_alpha: f32,
    pub last_update: Instant,
    pub duration_secs: f32,
    pub min_alpha: f32,
    pub max_alpha: f32,
    pub just_became_hidden: bool,
    pub just_became_visible: bool,
}

impl AutoHideState {
    pub fn new() -> Self {
        Self {
            current_alpha: 1.0,
            target_alpha: 1.0,
            last_update: Instant::now(),
            duration_secs: 0.25,
            min_alpha: 1.0, // Hardcoded to full opacity
            max_alpha: 1.0,
            just_became_hidden: false,
            just_became_visible: false,
        }
    }

    /// Proximity updates ignored while auto-hide is disabled.
    pub fn update_proximity(&mut self, _is_near: bool) {
        self.target_alpha = 1.0;
        self.current_alpha = 1.0;
        self.just_became_hidden = false;
        self.just_became_visible = false;
    }

    /// Tick always returns false (no active transition).
    pub fn tick(&mut self) -> bool {
        self.current_alpha = 1.0;
        self.target_alpha = 1.0;
        false
    }

    pub fn is_fully_hidden(&self) -> bool {
        false
    }

    /// Canvas pass-through (no alpha modulation applied).
    pub fn apply_alpha_to_canvas(&self, _canvas: &mut [u8]) {
        // No-op while auto-hide is disabled
    }
}
