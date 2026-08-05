pub mod badges;
pub mod context_menu;
pub mod dock;
pub mod font;
pub mod text;
pub mod utils;
pub mod window_list;

use std::collections::HashMap;

pub use crate::geometry::context_menu::{
    BASE_ITEM_HEIGHT, BASE_MENU_WIDTH, SurfaceGeometry,
};

use crate::graphics::fade::FadeAnimation;
use crate::state::dock::DOCK_HEIGHT;
use crate::models::{BadgeUpdate, WindowDiagnostics};
use crate::FontManager;

pub use window_list::{
    get_hover_menu_bounds, prepare_window_list, render_window_list_surface, PreparedWindowList,
};

pub use badges::{draw_canvas_badge, draw_dock_item_badge};
pub use dock::{
    render_dock_background, render_dock_items, render_dock_surface, render_dragged_icon,
    DockRenderResources,
};
pub use text::draw_text;
pub use utils::hsl_to_rgb;

#[allow(clippy::too_many_arguments)]
pub fn render_dock_surface_legacy(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale_factor: f64,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    pinned_apps: &[String],
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    font_manager: &mut FontManager,
    is_dragging: bool,
    dragged_app_id: Option<&String>,
    pointer_x: i32,
    pointer_y: i32,
    fallback_anim: &dockman_lib::animations::IconAnimation,
    badges: &HashMap<String, BadgeUpdate>,
) {
    let dock_height = (DOCK_HEIGHT as f64 * scale_factor).round() as usize;
    let window_list = prepare_window_list(pinned_apps, open_windows);

    let mut running_by_app: HashMap<String, Vec<&WindowDiagnostics>> = HashMap::new();
    for win in open_windows.values() {
        let id = if !win.app_id.is_empty() {
            win.app_id.clone()
        } else if !win.title.is_empty() {
            win.title.clone()
        } else {
            "Unknown".to_string()
        };
        running_by_app.entry(id).or_default().push(win);
    }

    let scale = scale_factor as f32;
    let base_size = (48.0 * scale) as usize;
    let spacing = (8.0 * scale) as usize;
    let total_icons = window_list.apps_in_dock.len();
    let total_width = total_icons * base_size + (total_icons.saturating_sub(1)) * spacing;
    let start_x_offset = (phys_width as usize).saturating_sub(total_width) / 2;
    let dock_y = (phys_height as usize).saturating_sub(dock_height);

    let pins: Vec<crate::app::types::PinItem> = window_list
        .apps_in_dock
        .iter()
        .enumerate()
        .map(|(i, app_id)| {
            let x = start_x_offset + i * (base_size + spacing);
            let y = dock_y + (dock_height.saturating_sub(base_size)) / 2;
            let running_windows = running_by_app.get(app_id);
            let is_running = running_windows.map_or(false, |w| !w.is_empty());
            let running_count = running_windows.map_or(0, |w| w.len());

            crate::app::types::PinItem {
                app_id: app_id.clone(),
                x: x as i32,
                y: y as i32,
                size: base_size,
                scale_factor: scale,
                is_running,
                is_activated: false,
                running_count,
                badge_count: None,
            }
        })
        .collect();

    let dock_state = crate::app::types::DockState {
        pins,
        hovered_index: None,
        current_visibility_alpha: 1.0,
        panel_rect: crate::Rect {
            x: 0.0,
            y: dock_y as f64,
            width: phys_width as f64,
            height: dock_height as f64,
        },
        fade: FadeAnimation::new(0.0, 1.0, 0.25),
    };

    let mut render_res = DockRenderResources {
        phys_width,
        phys_height,
        running_by_app: &running_by_app,
        icon_cache,
        badges,
        font_manager,
        fallback_anim,
        is_dragging,
        dragged_app_id,
        pointer_position: (pointer_x, pointer_y),
    };

    render_dock_surface(canvas, &dock_state, &mut render_res);
}
