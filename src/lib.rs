//! Dock application library.
//!
//! Re-exports all public modules for use by the binary crate.

#![allow(clippy::manual_strip)]
// src/lib.rs

use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::path::Path;

pub mod app;
pub mod cache;
pub mod config;
pub mod events;
pub mod geometry;
pub mod graphics;
pub mod handlers;
pub mod init;
pub mod interaction;
pub mod listeners;
pub mod models;
pub mod pointer;
pub mod registry;
pub mod render;
pub mod resolvers;
pub mod services;
pub mod state;
pub mod types;
pub mod error;
pub mod errors;
pub mod diagnostics;
pub mod recovery;

pub use geometry::*;

// Re-exports for public crate API backwards-compatibility
pub use graphics::animation::{self as animations, Frame, IconAnimation};
pub use models::{
    AudioAction, AudioDevice, AudioSliderTarget, AudioSnapshot, AudioStream, BadgeUpdate,
    BluetoothAction, BluetoothDevice, BluetoothSnapshot, DockItem, MediaControl, MediaSource,
    MediaWindowLink, ProcessEntry, ProcessSnapshot, ProcessSort, RcloneMount, RcloneSnapshot,
    SystemBarSnapshot, TrayAction, TrayActionKind, TrayIcon, TrayItem, TraySnapshot,
};
pub use registry::SystemRegistry;
pub use resolvers::desktop::{
    clean_exec_field, find_desktop_file_by_exec, find_desktop_file_by_name, get_desktop_actions,
    parse_desktop_actions, DesktopAction,
};
pub use resolvers::icon::{self as icon_utils, extract_icon_name, get_icon_path};
pub use resolvers::steam::resolve_steam_game_details;
pub use state::AppState;
pub use terminal_graphics::load_image_raw_rgba;
pub use types::{
    DockInstance, DockRenderState, InteractionState, MediaWindowState, MenuState, PopupSurface,
    WindowListState,
};

pub mod terminal_graphics {
    use super::*;
    use std::fs;
    

    /// Pre-cache directory for rasterized SVG icons.
    /// SVGs are rasterized once to PNG and cached here, avoiding per-frame CPU rasterization.
    const SVG_CACHE_DIR: &str = ".icon_cache";

    fn svg_cache_path(svg_path: &Path, target_size: u32) -> Option<String> {
        let stem = svg_path.file_stem()?.to_str()?;
        let cache_dir = dirs::cache_dir()?.join("dock").join(SVG_CACHE_DIR);
        fs::create_dir_all(&cache_dir).ok()?;
        Some(cache_dir.join(format!("{}_{}.png", stem, target_size)).to_string_lossy().into_owned())
    }

    pub fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
        let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

        if extension == "svg" {
            // Try to load pre-cached PNG first
            if let Some(cache_path) = svg_cache_path(path, target_size) {
                if let Ok(img) = image::open(&cache_path) {
                    let rgba = img.to_rgba8();
                    return Some((rgba.width(), rgba.height(), rgba.into_raw()));
                }
            }

            // Rasterize SVG and cache as PNG
            let svg_data = fs::read(path).ok()?;
            let tree =
                resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;

            let transform = resvg::tiny_skia::Transform::from_scale(
                target_size as f32 / tree.size().width(),
                target_size as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            // Cache the rasterized SVG as PNG for future loads
            if let Some(cache_path) = svg_cache_path(path, target_size) {
                let img: image::ImageBuffer<image::Rgba<u8>, Vec<u8>> =
                    image::ImageBuffer::from_raw(target_size, target_size, pixmap.data().to_vec())?;
                let _ = img.save(&cache_path);
            }

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
