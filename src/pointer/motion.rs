use crate::pointer::coordinates::map_coordinates;
use crate::AppState;
use smithay_client_toolkit::seat::pointer::PointerEvent;
use wayland_client::protocol::wl_surface::WlSurface;

pub struct IconLayout {
    pub box_size: f32,
    pub spacing: f32,
    pub dock_height: f32,
    pub scale: f32,
}

pub fn get_hovered_app<'a>(
    apps: &'a [&str],
    pointer_pos: (f64, f64),
    pointer_inside: bool,
    phys_dock_width: u32,
    phys_dock_height: u32,
    layout: &IconLayout,
) -> Option<(usize, &'a str)> {
    if !pointer_inside {
        return None;
    }

    let (px, py) = pointer_pos;

    if px < 0.0 || py < 0.0 || px > phys_dock_width as f64 || py > phys_dock_height as f64 {
        return None;
    }

    let box_size = (layout.box_size * layout.scale) as f64;
    let spacing = (layout.spacing * layout.scale) as f64;
    let start_offset_x = spacing;

    for (i, app_id) in apps.iter().enumerate() {
        let item_x = start_offset_x + i as f64 * (box_size + spacing);
        let item_w = box_size;

        if px >= item_x && px <= item_x + item_w {
            return Some((i, *app_id));
        }
    }

    None
}

/// Handles pointer motion, entry, and leave events.
pub fn handle_motion_events(
    state: &mut AppState,
    events: &[PointerEvent],
    dock_surface_ptr: Option<&WlSurface>,
    scale_factor: f32,
) -> bool {
    let mut layer_changed = false;

    for event in events {
        let is_popup_surface = state.docks.iter().any(|dock| {
            dock.window_list_popup
                .as_ref()
                .is_some_and(|popup| popup.surface == event.surface)
                || dock
                    .context_menu_popup
                    .as_ref()
                    .is_some_and(|popup| popup.surface == event.surface)
                || dock
                    .hover_popup
                    .as_ref()
                    .is_some_and(|popup| popup.surface == event.surface)
        });

        match event.kind {
            smithay_client_toolkit::seat::pointer::PointerEventKind::Leave { .. } => {
                let is_dock_surface = dock_surface_ptr == Some(&event.surface);

                if is_popup_surface {
                    state.window_list_state.pointer_inside_popup = false;
                    // A popup is still part of the dock interaction domain.
                    // Do not turn this into a dock Leave, otherwise the
                    // fade/hover state can oscillate while crossing the
                    // popup boundary.
                    continue;
                }

                if is_dock_surface && state.window_list_state.is_open {
                    continue;
                }

                if state.interaction.pointer_inside {
                    state.interaction.pointer_inside = false;
                    layer_changed = true;
                }
            }
            smithay_client_toolkit::seat::pointer::PointerEventKind::Enter { .. }
            | smithay_client_toolkit::seat::pointer::PointerEventKind::Motion { .. } => {
                if is_popup_surface {
                    state.window_list_state.pointer_inside_popup = true;
                }

                state.menu_state.cursor_moved = true;
                state.window_list_state.cursor_moved = true;

                if !state.interaction.pointer_inside && !is_popup_surface {
                    state.interaction.pointer_inside = true;
                    layer_changed = true;
                }

                let (mapped_x, mapped_y) =
                    map_coordinates(event, state, dock_surface_ptr, scale_factor);
                let (new_x, new_y) = (mapped_x as f64, mapped_y as f64);

                if state.interaction.pointer_position.x != new_x
                    || state.interaction.pointer_position.y != new_y
                {
                    state.interaction.pointer_position.x = new_x;
                    state.interaction.pointer_position.y = new_y;
                    layer_changed = true;
                }

                if state.hide_state.is_fully_hidden() {
                    state.hover_state.is_visible = false;
                    state.hover_state.app_id = None;
                    layer_changed = true;
                }

                if state.dragged_app_id.is_some() && !state.is_dragging {
                    let dx = mapped_x - state.drag_start_x as f32;
                    let dy = mapped_y - state.drag_start_y as f32;
                    if (dx * dx + dy * dy) > 25.0 {
                        state.is_dragging = true;
                        layer_changed = true;
                    }
                }

                // Popup surfaces own their pointer events. In particular, a
                // context-menu event must never be allowed to re-target the
                // window list through dock hover logic.
                if !is_popup_surface && !state.menu_state.is_open {
                    let hovered_app = if let Some(dock) = state.docks.first() {
                        let apps_in_dock = state.get_apps_in_dock();
                        let apps_refs: Vec<&str> =
                            apps_in_dock.iter().map(|s| s.as_str()).collect();

                        let dock_scale = dock.scale_factor;
                        let phys_dock_width = (dock.width as f64 * dock_scale).round() as u32;
                        let phys_dock_height = (dock.height as f64 * dock_scale).round() as u32;

                        let phys_ptr_x = new_x * dock_scale;
                        let phys_ptr_y = new_y * dock_scale;

                        let layout = IconLayout {
                            box_size: 48.0,
                            spacing: 8.0,
                            dock_height: dock.height as f32,
                            scale: dock_scale as f32,
                        };

                        get_hovered_app(
                            &apps_refs,
                            (phys_ptr_x, phys_ptr_y),
                            state.interaction.pointer_inside,
                            phys_dock_width,
                            phys_dock_height,
                            &layout,
                        )
                        .map(|(_idx, app_id)| app_id.to_string())
                    } else {
                        None
                    };

                    if let Some(dock) = state.docks.first_mut() {
                        if let Some(ref app_id) = hovered_app {
                            if state.window_list_state.target_app_id.as_deref() != Some(app_id) {
                                state.window_list_state.target_app_id = Some(app_id.clone());
                                state.window_list_state.is_open = true;
                                dock.window_list_fade.set_visible(true);
                                dock.window_list_fade.show();
                                layer_changed = true;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    layer_changed
}
