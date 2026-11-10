//! Pointer coordinate conversion.
//!
//! Converts between screen, dock, and popup coordinate spaces.

use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

use crate::AppState;

pub fn map_coordinates(
    event: &PointerEvent,
    state: &AppState,
    dock_surface_ptr: Option<&WlSurface>,
    _scale_factor: f32,
) -> (f32, f32) {
    let (px, py) = (event.position.0 as f32, event.position.1 as f32);

    // Pointer events on a side-box or popup subsurface are local to that child.
    // Map child-local coordinates back to main dock space using the stored
    // parent-local subsurface position.
    if let Some((ox, oy)) = state.surface_offset_in_dock_space(&event.surface) {
        return (px + ox, py + oy);
    }

    if dock_surface_ptr.is_some_and(|surface| event.surface == *surface) {
        return (px, py);
    }

    (px, py)
}

/// Calculates physical horizontal anchor coordinate for popups.
pub fn calculate_popup_anchor(
    _state: &AppState,
    target_app: &str,
    apps_in_dock: &[String],
    phys_width: i32,
    dock_scale: f64,
    layout: (i32, i32, i32, i32),
) -> i32 {
    if apps_in_dock.is_empty() || dock_scale <= 0.0 {
        return phys_width / 2;
    }

    let (_dock_height, box_size, spacing, _start_offset_x) = layout;
    let logical_width = phys_width as f64 / dock_scale;
    let total_icons_width = apps_in_dock.len() as f64 * box_size as f64
        + apps_in_dock.len().saturating_sub(1) as f64 * spacing as f64;
    let start_x = ((logical_width - total_icons_width) / 2.0).max(0.0);

    if let Some(index) = apps_in_dock.iter().position(|id| id == target_app) {
        let icon_x = start_x + index as f64 * (box_size + spacing) as f64;
        let center = icon_x + box_size as f64 / 2.0;
        return (center * dock_scale).round() as i32;
    }

    phys_width / 2
}
