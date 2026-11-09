//! System output state.
//!
//! Tracks Wayland output (monitor) state and capabilities.

use std::time::{Duration, Instant};

use crate::models::system::OutputId;

/// Live session/system state derived from listeners and platform services.
#[derive(Clone, Debug)]
pub struct SystemState {
    pub locked: bool,
    pub active: bool,
    pub idle: bool,
    pub outputs: Vec<OutputId>,
    pub last_update: Option<Instant>,
}

impl Default for SystemState {
    fn default() -> Self {
        Self {
            locked: false,
            active: true,
            idle: false,
            outputs: Vec::new(),
            last_update: None,
        }
    }
}

impl SystemState {
    pub fn touch(&mut self) {
        self.last_update = Some(Instant::now());
    }

    pub fn is_interaction_blocked(&self) -> bool {
        self.locked || !self.active
    }
}

#[derive(Clone, Debug, Default)]
pub struct LiveOutputState {
    pub output_id: Option<OutputId>,
    pub connected: bool,
    pub scale_factor: f64,
    pub width: u32,
    pub height: u32,
    pub refresh_hz: Option<f32>,
    pub last_frame: Option<Instant>,
    pub frame_budget: Option<Duration>,
}
