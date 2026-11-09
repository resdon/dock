//! Dock runtime state.
//!
//! Runtime metrics and state for the dock component.

use crate::geometry::Rect;
use crate::models::system::AppId;

#[derive(Clone, Debug, Default)]
pub struct DockRuntimeState {
    pub ordered_apps: Vec<AppId>,
    pub hovered_app: Option<AppId>,
    pub hovered_index: Option<usize>,
    pub pinned: Vec<AppId>,
    pub running: Vec<AppId>,
    pub panel_rect: Option<Rect>,
    pub visible: bool,
    pub alpha: f32,
    pub input_enabled: bool,
    pub dirty: bool,
}

impl DockRuntimeState {
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }
}
