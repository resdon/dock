use crate::state::AppState;

pub fn handle_drag_release(
    state: &mut AppState,
    dragged_id: &str,
    apps_in_dock: &[String],
    is_over_icons: bool,
    layout: (i32, i32, i32), // (box_size, spacing, start_offset_x)
) -> bool {
    let (box_size, spacing, start_offset_x) = layout;
    let mut layer_changed = false;

    if is_over_icons {
        let mut dropped_idx = None;
        let pointer_x = state.interaction.pointer_position.x as i32;

        if pointer_x < start_offset_x {
            dropped_idx = Some(0);
        } else {
            for (index, _) in apps_in_dock.iter().enumerate() {
                let start_x = start_offset_x + index as i32 * (box_size + spacing);
                let hit_start_x = start_x.saturating_sub(spacing / 2);
                let hit_end_x = start_x + box_size + (spacing / 2);

                if pointer_x >= hit_start_x && pointer_x <= hit_end_x {
                    dropped_idx = Some(index as i32);
                    break;
                }
            }

            if dropped_idx.is_none() && pointer_x >= start_offset_x {
                dropped_idx = Some(apps_in_dock.len().saturating_sub(1) as i32);
            }
        }

        if let Some(target_idx) = dropped_idx {
            if let Some(target_app_id) = apps_in_dock.get(target_idx as usize) {
                let old_idx_opt = state.pinned_apps.iter().position(|x| x == dragged_id);
                let new_idx_opt = state.pinned_apps.iter().position(|x| x == target_app_id);

                match (old_idx_opt, new_idx_opt) {
                    (Some(old_idx), Some(new_idx)) => {
                        if old_idx != new_idx {
                            let app = state.pinned_apps.remove(old_idx);
                            state.pinned_apps.insert(new_idx, app);
                            layer_changed = true;
                        }
                    }
                    (None, Some(new_idx)) => {
                        state.pinned_apps.insert(new_idx, dragged_id.to_string());
                        layer_changed = true;
                    }
                    (Some(old_idx), None) => {
                        let app = state.pinned_apps.remove(old_idx);
                        state.pinned_apps.push(app);
                        layer_changed = true;
                    }
                    (None, None) => {
                        state.pinned_apps.push(dragged_id.to_string());
                        layer_changed = true;
                    }
                }
            }
        }
        crate::cache::persistence::save_pinned_apps(&state.pinned_apps);
    } else if let Some(old_idx) = state.pinned_apps.iter().position(|x| x == dragged_id) {
        state.pinned_apps.remove(old_idx);
        crate::cache::persistence::save_pinned_apps(&state.pinned_apps);
        layer_changed = true;
    }

    layer_changed
}
