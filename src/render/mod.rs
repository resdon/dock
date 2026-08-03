// src/render/mod.rs

pub mod badges;
pub mod context_menu;
pub mod dock;
pub mod font;
pub mod text;
pub mod utils;
pub mod window_list;

use std::collections::HashMap;
use crate::models::{BadgeUpdate, WindowDiagnostics};
use crate::{FontManager, HoverState, MenuState};

// Import dock rendering functions
use dock::{render_dock_background, render_dock_items, render_dragged_icon};

// Re-export geometry and surface renderers so your compositor/main loop can call them
pub use context_menu::{
    BASE_ITEM_HEIGHT, BASE_MENU_WIDTH, DOCK_HEIGHT,
    HOVER_ITEM_HEIGHT, MENU_WIDTH,
    calculate_context_menu_geometry, render_context_menu_surface, SurfaceGeometry,
};
pub use window_list::{
    get_hover_menu_bounds, prepare_window_list, render_window_list_surface,
};

pub use badges::{draw_canvas_badge, draw_dock_item_badge};
pub use text::draw_text;
pub use utils::hsl_to_rgb;

/// Renders ONLY the main dock surface (background, items, badges, and dragged icons).
pub fn render_dock_surface(
    canvas: &mut [u8],
    phys_width: u32,
    phys_height: u32,
    scale_factor: f64,
    open_windows: &HashMap<wayland_client::backend::ObjectId, WindowDiagnostics>,
    pinned_apps: &Vec<String>,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    font_manager: &FontManager,
    is_dragging: bool,
    dragged_app_id: Option<&String>,
    pointer_x: i32,
    pointer_y: i32,
    fallback_anim: &dockman_lib::animations::IconAnimation,
    badges: &HashMap<String, BadgeUpdate>,
) {
    let dock_height = (DOCK_HEIGHT as f64 * scale_factor).round() as usize;

    // 1. Clear background
    render_dock_background(canvas, phys_width, phys_height, dock_height);

    // 2. Group pinned & open windows
    let window_list = prepare_window_list(pinned_apps, open_windows);

    // 3. Render dock icons & indicator dashes
    render_dock_items(
        canvas,
        phys_width,
        phys_height,
        scale_factor,
        dock_height,
        &window_list.apps_in_dock,
        &window_list.running_by_app,
        icon_cache,
        font_manager,
        fallback_anim,
        badges,
    );

    // 4. Dragged Icon Rendering (if active)
    if is_dragging {
        if let Some(drag_id) = dragged_app_id {
            render_dragged_icon(
                canvas,
                phys_width,
                phys_height,
                scale_factor,
                drag_id,
                &window_list.running_by_app,
                icon_cache,
                pointer_x,
                pointer_y,
                font_manager,
            );
        }
    }
}