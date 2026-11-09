//! Rendering modules.
//!
//! CPU and GPU renderers for dock, popups, context menus, and widgets.

pub mod audio;
pub mod audio_player;
pub mod badges;
pub mod bluetooth;
pub mod debug_overlay;
pub mod dmabuf_export;
pub mod dock;
pub mod femtovg_wgpu;
pub mod font;
pub mod popup;
pub mod text;
pub mod theme;
pub mod tray;
pub mod utils;
pub mod widget_bar;

pub use badges::{draw_canvas_badge, draw_dock_item_badge};
pub use dock::{
    render_dock_background, render_dock_items, render_dock_surface, render_dragged_icon,
    DockRenderResources,
};

pub use popup::*;
pub use text::draw_text;
pub use utils::hsl_to_rgb;
pub mod processes;
pub mod rclone;
pub mod slint_dock;
