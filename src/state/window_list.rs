//! Window list runtime state.
//!
//! Runtime state for the window list popup component.

use crate::models::system::{AppId, WindowId};

#[derive(Clone, Debug, Default)]
pub struct WindowListRuntimeState {
    pub visible: bool,
    pub target_app: Option<AppId>,
    pub windows: Vec<WindowId>,
    pub selected: Option<WindowId>,
    pub hovered: Option<WindowId>,
    pub dirty: bool,
}

impl WindowListRuntimeState {
    pub fn set_windows(&mut self, app: Option<AppId>, windows: Vec<WindowId>) {
        self.target_app = app;
        self.windows = windows;
        self.selected = None;
        self.hovered = None;
        self.dirty = true;
    }
}
