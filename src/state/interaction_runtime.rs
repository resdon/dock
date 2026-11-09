//! Interaction runtime state.
//!
//! Tracks input targets and drop targets for interaction.

use std::time::Instant;

use crate::geometry::Point;
use crate::models::system::{AppId, WindowId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputTarget {
    Dock,
    DockIcon(AppId),
    Window(WindowId),
    WindowInput(WindowId),
    WindowList(AppId),
    ContextMenu,
    Outside,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropTarget {
    App(AppId),
    Window(WindowId),
    Dock,
    WindowList(AppId),
    ContextMenu,
    None,
}

impl Default for DropTarget {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone, Debug)]
pub struct PointerRuntimeState {
    pub position: Point,
    pub previous_position: Option<Point>,
    pub target: InputTarget,
    pub inside: bool,
    pub moved: bool,
    pub last_motion: Option<Instant>,
    pub entered_target_at: Option<Instant>,
}

impl Default for PointerRuntimeState {
    fn default() -> Self {
        Self {
            position: Point { x: 0.0, y: 0.0 },
            previous_position: None,
            target: InputTarget::Outside,
            inside: false,
            moved: false,
            last_motion: None,
            entered_target_at: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct DragRuntimeState {
    pub active: bool,
    pub app_id: Option<AppId>,
    pub source_window: Option<WindowId>,
    pub start_position: Option<Point>,
    pub current_position: Option<Point>,
    pub target: DropTarget,
    pub can_drop: bool,
}

#[derive(Clone, Debug, Default)]
pub struct InteractionRuntimeState {
    pub pointer: PointerRuntimeState,
    pub drag: DragRuntimeState,
    pub hovered_app: Option<AppId>,
    pub hovered_window: Option<WindowId>,
    pub active_target: Option<InputTarget>,
    pub drop_target: DropTarget,
}

impl InteractionRuntimeState {
    pub fn set_pointer_target(&mut self, target: InputTarget, now: Instant) {
        if self.pointer.target != target {
            self.pointer.entered_target_at = Some(now);
        }
        self.pointer.target = target.clone();
        self.active_target = Some(target);
    }

    pub fn begin_drag(
        &mut self,
        app_id: Option<AppId>,
        source_window: Option<WindowId>,
        position: Point,
    ) {
        self.drag.active = true;
        self.drag.app_id = app_id;
        self.drag.source_window = source_window;
        self.drag.start_position = Some(position);
        self.drag.current_position = Some(position);
        self.drag.target = DropTarget::None;
        self.drag.can_drop = false;
    }

    pub fn end_drag(&mut self) {
        self.drag = DragRuntimeState::default();
    }
}
