//! Wayland window change events.
//!
//! Events emitted when windows are added, removed, or change state.

use crate::models::system::{AppId, OutputId, WindowId};

/// Domain-level window events emitted by platform listeners.
///
/// This module intentionally contains no `AppState`, Wayland protocol objects,
/// registry mutation, rendering, or resolver logic. It is only the event data
/// crossing the listener -> resolver/registry boundary.
#[derive(Clone, Debug)]
pub enum WindowEvent {
    Created {
        window_id: WindowId,
    },
    OutputEntered {
        window_id: WindowId,
        output: OutputId,
    },
    OutputLeft {
        window_id: WindowId,
        output: OutputId,
    },
    Closed {
        window_id: WindowId,
    },
    Done {
        window_id: WindowId,
    },
    TitleChanged {
        window_id: WindowId,
        title: String,
    },
    AppIdChanged {
        window_id: WindowId,
        app_id: AppId,
    },
    StateChanged {
        window_id: WindowId,
        activated: bool,
        minimized: bool,
    },
}
