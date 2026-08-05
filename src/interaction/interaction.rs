use std::collections::HashMap;
use wayland_client::backend::ObjectId;
use crate::AppState;
use crate::hover::update_hover;
use crate::fade::{update_hover_fade, update_menu_fade};
use crate::proximity::{pointer_on_context_menu, pointer_in_context_menu_leeway};

/// Top-level coordinator that updates context menus, hover states, and animations.
pub fn update(
    state: &mut AppState,
    apps_in_dock: &[String],
    running_by_app: &HashMap<String, Vec<ObjectId>>,
    scale_factor: f32,
    layout: (i32, i32, i32, i32),
) -> bool {
    let (dock_height, _, spacing, _) = layout;
    let mut layer_changed = false;

    // 1. Context Menu Lifecycle & Fade
    if state.menu_state.is_open && !state.menu_state.items.is_empty() {
        let menu_active = pointer_on_context_menu(state, scale_factor, dock_height)
            || pointer_in_context_menu_leeway(state, scale_factor, dock_height, spacing);

        update_menu_fade(state, menu_active);

        if !menu_active {
            state.menu_state.is_open = false;
            state.menu_state.target_app_id = None;
            layer_changed = true;
        }
    }

    // 2. Hover Calculation & Fade
    let hover_res = update_hover(state, apps_in_dock, running_by_app, scale_factor, layout);

    let visibility_changed = hover_res.visible != state.hover_state.is_visible;
    let app_changed = hover_res.visible && (hover_res.app_id != state.hover_state.app_id);

    if visibility_changed || app_changed {
        state.hover_state.is_visible = hover_res.visible;
        state.hover_state.app_id = hover_res.app_id;
        layer_changed = true;

        update_hover_fade(state, hover_res.visible);
    }

    layer_changed
}
