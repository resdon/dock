use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use crate::AppState;
use crate::handlers::dock::get_windows_for_app;
use super::proximity;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HoverResult {
    pub visible: bool,
    pub app_id: Option<String>,
}

/// Evaluates hovered app state using proximity helper routines.
pub fn update_hover(
    state: &AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> HoverResult {
    if state.menu_state.is_open {
        return HoverResult {
            visible: false,
            app_id: None,
        };
    }

    let (dock_height, _box_size, spacing, _start_offset_x) = layout;

    let icon_app_id = proximity::pointer_on_icon(state, apps_in_dock, layout);

    let candidate_app_id = icon_app_id.clone().or_else(|| {
        if state.hover_state.is_visible {
            state.hover_state.app_id.clone()
        } else {
            None
        }
    });

    let mut should_be_visible = false;
    let mut final_app_id: Option<String> = None;

    if let Some(app_id) = candidate_app_id {
        let windows = get_windows_for_app(&app_id, running_by_app, &state.open_windows);
        let is_over_icon = icon_app_id.as_ref() == Some(&app_id);

        let is_over_window_list = proximity::pointer_on_window_list(
            state,
            &app_id,
            apps_in_dock,
            running_by_app,
            scale_factor,
            dock_height,
        );

        let in_leeway = proximity::pointer_between_icon_and_window(
            state,
            &app_id,
            apps_in_dock,
            running_by_app,
            scale_factor,
            dock_height,
            spacing,
        );

        if is_over_icon || is_over_window_list || in_leeway {
            if !windows.is_empty() {
                should_be_visible = true;
                final_app_id = Some(app_id);
            }
        }
    }

    HoverResult {
        visible: should_be_visible,
        app_id: if should_be_visible { final_app_id } else { None },
    }
}
