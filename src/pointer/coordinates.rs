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

    // Pointer events on a popup subsurface are local to that popup. The popup
    // stores the exact dock-surface position used by set_position(), so use
    // that position directly instead of reconstructing geometry from screen
    // dimensions. This keeps rendering and hit-testing in the same coordinate
    // space and avoids the old screen-width/dock-width mismatch.
    for dock in &state.docks {
        if let Some(popup) = &dock.context_menu_popup {
            if event.surface == popup.surface {
                return (px + popup.position.0 as f32, py + popup.position.1 as f32);
            }
        }
        if let Some(popup) = &dock.window_list_popup {
            if event.surface == popup.surface {
                return (px + popup.position.0 as f32, py + popup.position.1 as f32);
            }
        }
        if let Some(popup) = &dock.hover_popup {
            if event.surface == popup.surface {
                return (px + popup.position.0 as f32, py + popup.position.1 as f32);
            }
        }
    }

    if dock_surface_ptr.is_some_and(|surface| event.surface == *surface) {
        return (px, py);
    }

    (px, py)
}

/// Calculates physical horizontal anchor coordinate for popups.
pub fn calculate_popup_anchor(
    state: &AppState,
    target_app: &str,
    apps_in_dock: &[String],
    phys_width: i32,
    dock_scale: f64,
    layout: (i32, i32, i32, i32),
) -> i32 {
    let (_dock_height, box_size, spacing, start_offset_x) = layout;

    // 1. Check pinned items
    for d in &state.docks {
        if let Some(ref dock_state) = d.dock_state {
            if let Some(pin) = dock_state.pins.iter().find(|p| p.app_id == target_app) {
                let pin_center_logical = pin.x as f32 + (pin.size as f32 / 2.0);
                return (pin_center_logical * dock_scale as f32).round() as i32;
            }
        }
    }

    // 2. Fallback for unpinned running items
    let pinned_count = state
        .docks
        .first()
        .and_then(|d| d.dock_state.as_ref())
        .map(|ds| ds.pins.len())
        .unwrap_or(0);

    if let Some(idx) = apps_in_dock.iter().position(|id| id == target_app) {
        let is_full_list = state
            .docks
            .first()
            .and_then(|d| d.dock_state.as_ref())
            .and_then(|ds| ds.pins.first())
            .is_some_and(|first_pin| apps_in_dock.first() == Some(&first_pin.app_id));

        let slot_idx = if is_full_list {
            idx
        } else {
            pinned_count + idx
        };
        let start_x = start_offset_x + spacing + slot_idx as i32 * (box_size + spacing);
        let pin_center_logical = start_x as f32 + (box_size as f32 / 2.0);
        return (pin_center_logical * dock_scale as f32).round() as i32;
    }

    phys_width / 2
}
