//! Window state resolver.
//!
//! Resolves Wayland window IDs to app IDs and titles.

use super::context::WindowHints;
use crate::models::system::{AppId, WindowId};
use crate::models::system::{
    ResolutionConfidence, ResolutionEvidence, WindowCapabilities, WindowGeometry, WindowLayer,
    WindowPriority, WindowRecord, WindowStatus,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct WindowResolver;

impl WindowResolver {
    pub fn resolve(
        &self,
        id: WindowId,
        app_id: AppId,
        title: String,
        hints: &WindowHints,
        process_id: Option<crate::models::system::ProcessId>,
    ) -> WindowRecord {
        let mut window = WindowRecord::new(id.clone(), app_id);
        window.process_id = process_id;
        window.title = title;
        window.status = if hints.activated {
            WindowStatus::Active
        } else if hints.minimized {
            WindowStatus::Minimized
        } else {
            WindowStatus::Background
        };
        window.priority = if hints.activated {
            WindowPriority::Foreground
        } else if hints.minimized {
            WindowPriority::Background
        } else {
            WindowPriority::Normal
        };
        window.order_index = id.0.protocol_id() as u64;
        window.layer = WindowLayer::Normal;
        window.outputs = hints.outputs.clone();
        window.is_activated = hints.activated;
        window.is_minimized = hints.minimized;
        window.is_foreground = hints.activated;
        window.is_background = !hints.activated && !hints.minimized;
        window.geometry = WindowGeometry::default();
        window.resolution_confidence = ResolutionConfidence::Probable;
        window.resolution_trace.strengthen(
            ResolutionConfidence::Probable,
            [
                ResolutionEvidence::WaylandState,
                ResolutionEvidence::OutputAssociation,
            ],
        );
        window.capabilities = WindowCapabilities {
            can_activate: true,
            can_minimize: true,
            can_close: true,
            can_move: true,
            can_raise: true,
            can_pin: true,
            can_move_to_output: !hints.outputs.is_empty(),
        };
        window
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::system::{AppId, WindowId};
    use wayland_client::backend::ObjectId;

    // The compositor object cannot be constructed in a unit test without a
    // live backend, so the test surface for this stage is intentionally kept
    // to compilation/type checking in normal builds.
    #[test]
    fn resolver_type_is_constructible() {
        let _ = WindowResolver;
        let _ = std::mem::size_of::<WindowGeometry>();
        let _ = std::mem::size_of::<WindowId>();
        let _ = std::mem::size_of::<AppId>();
        let _ = std::mem::size_of::<ObjectId>();
    }
}
