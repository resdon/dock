//! System-level events (keyboard layout, network rates).

use crate::models::system::{OutputId, SurfaceId, WindowId};

/// Platform/runtime events that are useful to the application pipeline but are
/// not window identity events themselves. No Wayland protocol objects cross
/// this boundary.
#[derive(Clone, Debug)]
pub enum SystemEvent {
    SurfaceEnter {
        output: OutputId,
    },
    SurfaceLeave {
        output: OutputId,
    },
    FrameRequested,
    LayerConfigured {
        surface_id: SurfaceId,
        width: u32,
        height: u32,
    },
    LayerClosed {
        surface_id: SurfaceId,
    },
    FractionalScaleChanged {
        output: OutputId,
        scale: i32,
    },
    WindowSurfaceChanged {
        window: WindowId,
    },
}
