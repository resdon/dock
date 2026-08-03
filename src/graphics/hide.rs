use std::time::Instant;

pub struct AutoHideState {
    pub current_alpha: f32, // 0.25 (75% transparent) -> 1.0 (fully opaque)
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
            duration_secs: 3.0,
            min_alpha: 0.25, // 25% opacity when hidden
            max_alpha: 1.0,  // 100% opacity when active
            just_became_hidden: false,
            just_became_visible: false,
        }
    }

    /// Call this whenever mouse proximity changes.
    pub fn update_proximity(&mut self, is_near: bool) {
        let was_hidden = self.target_alpha <= self.min_alpha + f32::EPSILON;

        if is_near {
            self.target_alpha = self.max_alpha;
        } else {
            self.target_alpha = self.min_alpha;
        }

        let is_hidden = self.target_alpha <= self.min_alpha + f32::EPSILON;

        // Reset timer baseline to prevent delta time jumps after idle periods
        if was_hidden != is_hidden {
            self.last_update = Instant::now();
        }

        // Set transition flags when target state shifts
        if !was_hidden && is_hidden {
            self.just_became_hidden = true;
            self.just_became_visible = false;
        } else if was_hidden && !is_hidden {
            self.just_became_visible = true;
            self.just_became_hidden = false;
        }
    }

    /// Advances the linear animation frame.
    /// Returns `true` if the animation is still actively progressing.
    pub fn tick(&mut self) -> bool {
        if (self.current_alpha - self.target_alpha).abs() < 0.001 {
            self.current_alpha = self.target_alpha;
            return false; // Animation idle
        }

        let now = Instant::now();
        let delta = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // Calculate step rate based on active alpha range (0.75 span)
        let alpha_range = self.max_alpha - self.min_alpha;
        let step = (alpha_range / self.duration_secs) * delta;

        if self.current_alpha < self.target_alpha {
            self.current_alpha = (self.current_alpha + step).min(self.target_alpha);
        } else {
            self.current_alpha = (self.current_alpha - step).max(self.target_alpha);
        }

        true // Animation in progress
    }

    pub fn is_fully_hidden(&self) -> bool {
        self.current_alpha <= self.min_alpha + 0.001
    }

    /// Modulates SHM canvas buffer by opacity factor.
    /// Wayland SHM ARGB8888 expects premultiplied alpha.
    pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
        if (self.current_alpha - 1.0).abs() < f32::EPSILON {
            return; // Fully opaque, skip post-processing
        }

        let factor = self.current_alpha;

        for pixel in canvas.chunks_exact_mut(4) {
            pixel[0] = (pixel[0] as f32 * factor) as u8;
            pixel[1] = (pixel[1] as f32 * factor) as u8;
            pixel[2] = (pixel[2] as f32 * factor) as u8;
            pixel[3] = (pixel[3] as f32 * factor) as u8;
        }
    }
}