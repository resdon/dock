use std::time::Instant;

pub struct AutoHideState {
    pub current_alpha: f32, // 0.25 (75% transparent) -> 1.0 (0% transparent / fully opaque)
    pub target_alpha: f32,
    pub last_update: Instant,
    pub duration_secs: f32,
    pub min_alpha: f32,
    pub max_alpha: f32,
}

impl AutoHideState {
    pub fn new() -> Self {
        Self {
            current_alpha: 1.0,
            target_alpha: 1.0,
            last_update: Instant::now(),
            duration_secs: 3.0,
            min_alpha: 0.25, // 75% transparent = 25% opacity
            max_alpha: 1.0,  // 0% transparent = 100% opacity
        }
    }

    /// Call this whenever the mouse pointer moves or enters/leaves proximity.
    pub fn update_proximity(&mut self, is_near: bool) {
        let new_target = if is_near { self.max_alpha } else { self.min_alpha };
        
        if (self.target_alpha - new_target).abs() > f32::EPSILON {
            self.target_alpha = new_target;
            self.last_update = Instant::now(); // Reset time delta baseline
        }
    }

    /// Advances the linear animation frame.
    /// Returns `true` if the animation is still actively progressing (requests a redraw).
    pub fn tick(&mut self) -> bool {
        if (self.current_alpha - self.target_alpha).abs() < 0.001 {
            self.current_alpha = self.target_alpha;
            return false; // Animation idle
        }

        let now = Instant::now();
        let delta = now.duration_since(self.last_update).as_secs_f32();
        self.last_update = now;

        // Calculate step rate for 3.0s total transition duration
        let step = (1.0 / self.duration_secs) * delta;

        if self.current_alpha < self.target_alpha {
            self.current_alpha = (self.current_alpha + step).min(self.target_alpha);
        } else {
            self.current_alpha = (self.current_alpha - step).max(self.target_alpha);
        }

        true // Animation in progress
    }

    /// Modulates the entire SHM canvas buffer by the active opacity factor.
    /// Note: Wayland SHM ARGB8888 buffers expect premultiplied alpha, 
    /// so we scale both the color bytes and the alpha byte.
    pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
        if (self.current_alpha - 1.0).abs() < f32::EPSILON {
            return; // Fully opaque, skip post-process
        }

        let factor = self.current_alpha;

        for pixel in canvas.chunks_exact_mut(4) {
            // [B, G, R, A] or [ARGB] - scale all channels to preserve premultiplied alpha
            pixel[0] = (pixel[0] as f32 * factor) as u8;
            pixel[1] = (pixel[1] as f32 * factor) as u8;
            pixel[2] = (pixel[2] as f32 * factor) as u8;
            pixel[3] = (pixel[3] as f32 * factor) as u8;
        }
    }
}