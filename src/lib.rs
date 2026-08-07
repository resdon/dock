#![allow(clippy::manual_strip)]
// src/lib.rs

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::path::Path;

pub mod graphics;
pub mod listeners;
pub mod models;
pub mod resolvers;

// Re-exports for public crate API backwards-compatibility
pub use graphics::animation::{self as animations, Frame, IconAnimation};
pub use models::{BadgeUpdate, DockItem, LastState, WindowDiagnostics};
pub use resolvers::desktop::{
    clean_exec_field, find_desktop_file_by_exec, find_desktop_file_by_name, get_desktop_actions,
    parse_desktop_actions, DesktopAction,
};
pub use resolvers::icon::{self as icon_utils, extract_icon_name, get_icon_path};
pub use resolvers::steam::resolve_steam_game_details;
pub use terminal_graphics::load_image_raw_rgba;

pub mod terminal_graphics {
    use super::*;

    pub fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
        let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

        if extension == "svg" {
            let svg_data = std::fs::read(path).ok()?;
            let tree =
                resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;

            let transform = resvg::tiny_skia::Transform::from_scale(
                target_size as f32 / tree.size().width(),
                target_size as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            Some((target_size, target_size, pixmap.data().to_vec()))
        } else {
            let img = image::open(path).ok()?;
            let scaled = img.resize_exact(
                target_size,
                target_size,
                image::imageops::FilterType::Lanczos3,
            );
            let rgba = scaled.to_rgba8();
            Some((rgba.width(), rgba.height(), rgba.into_raw()))
        }
    }

    pub fn generate_terminal_image_string(app_id: &str, target_size: u32) -> String {
        let icon_path = match get_icon_path(app_id) {
            Some(path) => path,
            None => return "📁 [No Icon Found]".to_string(),
        };

        if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, target_size) {
            let b64_data = STANDARD.encode(&raw_bytes);
            format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data)
        } else {
            "📁 [Rasterize Error]".to_string()
        }
    }
}
