//! Icon animation state.
//!
//! Manages click and bounce animation state per app.

use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationKind {
    DockFade,
    DockHide,
    ContextMenuFade,
    WindowListFade,
    Hover,
    Icon,
}

#[derive(Clone, Debug)]
pub struct AnimationState {
    pub active: bool,
    pub dirty: bool,
    pub started_at: Option<Instant>,
    pub last_frame: Option<Instant>,
    pub frame_count: u64,
    pub target_fps: u32,
    pub kind: Option<AnimationKind>,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            active: false,
            dirty: false,
            started_at: None,
            last_frame: None,
            frame_count: 0,
            target_fps: 60,
            kind: None,
        }
    }
}

impl AnimationState {
    pub fn begin(&mut self, kind: AnimationKind, now: Instant) {
        self.active = true;
        self.dirty = true;
        self.started_at = Some(now);
        self.last_frame = None;
        self.kind = Some(kind);
    }

    pub fn frame(&mut self, now: Instant) {
        self.active = true;
        self.dirty = false;
        self.last_frame = Some(now);
        self.frame_count = self.frame_count.saturating_add(1);
    }

    pub fn finish(&mut self) {
        self.active = false;
        self.dirty = false;
        self.kind = None;
    }
}
