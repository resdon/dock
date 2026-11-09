//! Context menu runtime state.

use crate::models::system::{AppId, WindowId};

#[derive(Clone, Debug, Default)]
pub struct ContextMenuRuntimeState {
    pub visible: bool,
    pub target_app: Option<AppId>,
    pub target_window: Option<WindowId>,
    pub hovered_item: Option<usize>,
    pub dirty: bool,
}

impl ContextMenuRuntimeState {
    pub fn open_for_app(&mut self, app: AppId) {
        self.visible = true;
        self.target_app = Some(app);
        self.target_window = None;
        self.dirty = true;
    }

    pub fn open_for_window(&mut self, window: WindowId) {
        self.visible = true;
        self.target_window = Some(window);
        self.target_app = None;
        self.dirty = true;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.target_app = None;
        self.target_window = None;
        self.hovered_item = None;
        self.dirty = true;
    }
}
