//! Dashed block surface management.
//!
//! Renders a configurable dashed border block as a separate floating layer surface
//! positioned to the right of the dock. The surface is created in init.rs
//! and this module handles per-frame rendering and commit.
//!
//! # Configuration
//!
//! All visual parameters (dimensions, colors, font sizes, positions, icon sizes)
//! are read from `DockConfig` fields prefixed with `dashed_block_`.
//! The block is shown only when `dashed_block_enabled` is true.

use smithay_client_toolkit::{dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool};
use std::os::fd::AsFd;
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;
use wayland_client::Proxy;
use crate::state::AppState;

use crate::config::DockConfig;
use crate::render::dashed_block::render_dashed_block;
use crate::render::font::FontManager;
use crate::types::DockInstance;
use wgpu_api::Renderer as WgpuRenderer;

/// Draws the dashed block widget and commits the buffer.
///
/// The layer surface is pre-created during init; this function renders the
/// dashed border pattern with the current time, date, status icons, and keyboard
/// layout indicator, then attaches the buffer each frame.
///
/// # Parameters
///
/// - `dock` — dock instance containing the dashed block layer surface
/// - `pool` — shared memory buffer pool for allocating frame buffers
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all dashed_block_* settings
pub fn draw_dashed_block(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
    mut gpu_context: Option<&mut WgpuRenderer>,
    dmabuf_state: &DmabufState,
) {
    // Skip rendering if the dashed block is disabled in config
    if !config.dashed_block_enabled {
        return;
    }

    let block = match &mut dock.dashed_block_surface {
        Some(b) => b,
        None => return,
    };

    // Use configured dimensions, falling back to defaults for the surface size constants
    let block_width = config.dashed_block_width.max(1) as i32;
    let block_height = config.dashed_block_height.max(1) as i32;
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (block_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (block_height as f64 * scale).round().max(1.0) as i32;
    let a = alpha.clamp(0.0, 1.0);

    // Try GPU rendering via DMA-BUF first
    if let Some(ref mut renderer) = gpu_context {
        let mut draw_commands = Vec::new();

        // Build dashed border draw commands
        let border_color = DockConfig::parse_hex_color(&config.dashed_block_border_color);
        let br = border_color.0 as f32 / 255.0;
        let bg = border_color.1 as f32 / 255.0;
        let bb = border_color.2 as f32 / 255.0;
        let border_col = wgpu_api::Color::rgba(br, bg, bb, a);
        let dash_len = config.dashed_block_dash_length as usize;
        let gap_len = config.dashed_block_gap_length as usize;
        let cycle = dash_len + gap_len;
        let thickness = config.dashed_block_border_thickness as usize;
        let w = block_width as usize;
        let h = block_height as usize;

        // Draw dashed border segments as rectangles
        let mut pos = 0;
        while pos < w {
            let dash_end = (pos + dash_len).min(w);
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(pos as f32, 0.0, (dash_end - pos) as f32, thickness as f32),
                color: border_col,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
            });
            pos += cycle;
        }
        pos = 0;
        while pos < w {
            let dash_end = (pos + dash_len).min(w);
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(pos as f32, (h - thickness) as f32, (dash_end - pos) as f32, thickness as f32),
                color: border_col,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
            });
            pos += cycle;
        }
        pos = 0;
        while pos < h {
            let dash_end = (pos + dash_len).min(h);
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(0.0, pos as f32, thickness as f32, (dash_end - pos) as f32),
                color: border_col,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
            });
            pos += cycle;
        }
        pos = 0;
        while pos < h {
            let dash_end = (pos + dash_len).min(h);
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new((w - thickness) as f32, pos as f32, thickness as f32, (dash_end - pos) as f32),
                color: border_col,
                radius: 0.0,
                stroke: None,
                stroke_width: 0.0,
            });
            pos += cycle;
        }

        // Render SVG icons as textures and add them as DrawCommand::Image
        let icon_size = config.dashed_block_icon_size as usize;
        let icon_gap = config.dashed_block_icon_gap as usize;
        let row_gap = config.dashed_block_row_gap as usize;
        let row_width = icon_size * 2 + icon_gap;
        let icons_start_x = (w - row_width) / 2;
        let row_offset = config.dashed_block_row_offset as usize;
        let top_row_y = config.dashed_block_top_row_y as usize;
        let bottom_row_y = top_row_y + icon_size + row_gap;
        let row_start_x = icons_start_x + row_offset;

        // Embedded SVG data for status icons (same as CPU fallback)
        let rclone_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" version=\"1\"><path style=\"fill:#00539e\" d=\"M 8,8 C 5.784,8 4,9.784 4,12 L 4,52 C 4,54.216 5.784,56 8,56 L 56,56 C 58.216,56 60,54.216 60,52 L 60,12 C 60,9.784 58.216,8 56,8 Z\"/><path style=\"fill:#ffffff\" d=\"M 20,20 L 20,44 L 28,44 L 28,36 L 36,36 L 36,44 L 44,44 L 44,20 Z\"/><path style=\"fill:#ffffff\" d=\"M 24,24 L 40,24 L 40,32 L 24,32 Z\"/><path style=\"fill:#00539e\" d=\"M 28,28 L 36,28 L 36,30 L 28,30 Z\"/></svg>";
        let bt_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"22\" height=\"22\" version=\"1\"><g><rect style=\"fill:#324090\" width=\"20\" height=\"14\" x=\"-21\" y=\"-18\" rx=\"7\" ry=\"7\" transform=\"matrix(0,-1,-1,0,0,0)\"/><path style=\"fill:none;stroke:#ffffff;stroke-linecap:square\" d=\"m 7.7066,7.7929 7,6.5 -3.2066,3.5 0,-13.5 3.2066,3.5 -7,6.5\"/><path style=\"opacity:0.1;fill:#ffffff\" d=\"M 11,1 C 7.122,1 4,4.122 4,8 l 0,0.5 c 0,-3.878 3.122,-7 7,-7 3.878,0 7,3.122 7,7 L 18,8 C 18,4.122 14.878,1 11,1 Z\"/><path style=\"opacity:0.2\" d=\"m 4,14 0,0.5 c 0,3.878 3.122,7 7,7 3.878,0 7,-3.122 7,-7 L 18,14 c 0,3.878 -3.122,7 -7,7 -3.878,0 -7,-3.122 -7,-7 z\"/></g></svg>";
        let speaker_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"#3b82f6\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"> <polygon points=\"11 5 6 9 2 9 2 15 6 15 11 19 11 5\"/> <path d=\"M15.5 8.5a5 5 0 0 1 0 7\"/> <path d=\"M19.5 4.5a9 9 0 0 1 0 15\"/> </svg>";
        let pac_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" version=\"1\"> <path style=\"opacity:0.2\" d=\"M 20,57 16,53 H 8 C 5.784,53 4,51.216 4,49 V 13 C 4,10.784 5.784,9 8,9 H 56 C 58.216,9 60,10.784 60,13 V 49 C 60,51.216 58.216,53 56,53 H 48 L 44,57 Z\"/> <path style=\"fill:#2c80c2\" d=\"M 20,56 16,52 H 8 C 5.784,52 4,50.216 4,48 V 12 C 4,9.784 5.784,8 8,8 H 56 C 58.216,8 60,9.784 60,12 V 48 C 60,50.216 58.216,52 56,52 H 48 L 44,56 Z\"/> <path style=\"opacity:0.5\" d=\"M 24,48 20,44 H 12 V 16 H 52 V 44 H 44 L 40,48 Z\"/> <g style=\"fill:#f8c44e\" transform=\"scale(1,-1)\"> <rect width=\"4\" height=\"4\" x=\"18\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"24\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"30\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"36\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"42\" y=\"-20\"/> </g> <path style=\"fill:#ffffff;opacity:0.2\" d=\"M 8 8 C 5.784 8 4 9.784 4 12 L 4 13 C 4 10.784 5.784 9 8 9 L 56 9 C 58.216 9 60 10.784 60 13 L 60 12 C 60 9.784 58.216 8 56 8 L 8 8 z\"/> <path style=\"opacity:0.2;fill:#ffffff\" d=\"M 12,44 V 45 H 20 L 24,49 H 40 L 44,45 H 52 V 44 H 44 L 40,48 H 24 L 20,44 Z\"/> </svg>";

        // Helper to render SVG to RGBA pixels and upload as GPU texture
        let raster_icon_size = (icon_size as f64 * scale).round().max(1.0) as u32;
        let mut render_svg_to_texture =
            |key: &str, svg_data: &[u8], icon_size: u32| -> Option<wgpu_api::TextureId> {
                let cache_key = format!("{key}:{icon_size}");
                if let Some(texture) = renderer.cached_texture(&cache_key) {
                    return Some(texture);
                }
                use resvg::tiny_skia::{Pixmap, Transform};
                use resvg::usvg::{Options, Tree};

                let tree = Tree::from_data(svg_data, &Options::default()).ok()?;
                let mut pixmap = Pixmap::new(icon_size, icon_size)?;
                let transform = Transform::from_scale(
                    icon_size as f32 / tree.size().width(),
                    icon_size as f32 / tree.size().height(),
                );
                resvg::render(&tree, transform, &mut pixmap.as_mut());

                Some(renderer.upload_texture_from_pixels_cached(
                    &cache_key,
                    pixmap.data(),
                    icon_size,
                    icon_size,
                )?)
            };

        // Top row: rclonetray | bluetooth
        if let Some(tex) =
            render_svg_to_texture("dashed-block:rclone", rclone_svg, raster_icon_size)
        {
            draw_commands.push(wgpu_api::DrawCommand::Image {
                dest_rect: wgpu_api::Rect::new(row_start_x as f32, top_row_y as f32, icon_size as f32, icon_size as f32),
                texture: tex,
                source_uv: wgpu_api::Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: wgpu_api::Color::rgba(1.0, 1.0, 1.0, a),
            });
        }
        if let Some(tex) = render_svg_to_texture("dashed-block:bluetooth", bt_svg, raster_icon_size)
        {
            draw_commands.push(wgpu_api::DrawCommand::Image {
                dest_rect: wgpu_api::Rect::new((row_start_x + icon_size + icon_gap) as f32, top_row_y as f32, icon_size as f32, icon_size as f32),
                texture: tex,
                source_uv: wgpu_api::Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: wgpu_api::Color::rgba(1.0, 1.0, 1.0, a),
            });
        }

        // Bottom row: speaker | pac
        if let Some(tex) = render_svg_to_texture("dashed-block:speaker", speaker_svg, raster_icon_size)
        {
            draw_commands.push(wgpu_api::DrawCommand::Image {
                dest_rect: wgpu_api::Rect::new(row_start_x as f32, bottom_row_y as f32, icon_size as f32, icon_size as f32),
                texture: tex,
                source_uv: wgpu_api::Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: wgpu_api::Color::rgba(1.0, 1.0, 1.0, a),
            });
        }
        if let Some(tex) = render_svg_to_texture("dashed-block:pac", pac_svg, raster_icon_size) {
            draw_commands.push(wgpu_api::DrawCommand::Image {
                dest_rect: wgpu_api::Rect::new((row_start_x + icon_size + icon_gap) as f32, bottom_row_y as f32, icon_size as f32, icon_size as f32),
                texture: tex,
                source_uv: wgpu_api::Rect { x: 0.0, y: 0.0, width: 1.0, height: 1.0 },
                tint: wgpu_api::Color::rgba(1.0, 1.0, 1.0, a),
            });
        }

        // Keyboard layout text — positioned to the left of the bottom row icons.
        let text_color = DockConfig::parse_hex_color(&config.dashed_block_text_color);
        let tr = text_color.0 as f32 / 255.0;
        let tg = text_color.1 as f32 / 255.0;
        let tb = text_color.2 as f32 / 255.0;
        let text_col = wgpu_api::Color::rgba(tr, tg, tb, a);
        let kb_layout = &config.dashed_block_kb_layout;
        let kb_font_size = config.dashed_block_kb_font_size as f32;
        // Estimate text width as character count * font_size * 0.6 (average glyph width ratio)
        let kb_text_width = kb_layout.len().max(1) as f32 * kb_font_size * 0.6;
        let kb_text_x = row_start_x as f32 - kb_text_width - 4.0 - 4.0;
        let kb_text_y = bottom_row_y as f32 + icon_size as f32 / 2.0 + kb_font_size / 3.0 - 10.0
            + config.dashed_block_kb_text_offset_y as f32;
        draw_commands.push(wgpu_api::DrawCommand::Text {
            content: kb_layout.clone(),
            position: wgpu_api::Point::new(kb_text_x, kb_text_y),
            font_size: kb_font_size,
            color: text_col,
            align: 0.0,
        });

        match renderer.render_to_dmabuf_scaled(
            phys_width as u32,
            phys_height as u32,
            scale as f32,
            &draw_commands,
        ) {
            Ok(export_info) => {
                match dmabuf_state.create_params(qh) {
                    Ok(params) => {
                        for (i, plane) in export_info.planes.iter().enumerate() {
                            params.add(
                                plane.fd.as_fd(),
                                i as u32,
                                plane.offset,
                                plane.stride,
                                export_info.modifier,
                            );
                        }
                        let (wl_buffer, _params) = params.create_immed(
                            phys_width as i32,
                            phys_height as i32,
                            export_info.fourcc as u32,
                            wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags::empty(),
                            qh,
                        );
                        block.surface.wl_surface().set_buffer_scale(scale.round().max(1.0) as i32);
                        block.surface.wl_surface().attach(Some(&wl_buffer), 0, 0);
                        block.surface.wl_surface().damage_buffer(0, 0, phys_width, phys_height);
                        block.surface.wl_surface().commit();
                        return;
                    }
                    Err(_) => {}
                }
            }
            Err(_) => {}
        }
    }

    // CPU fallback rendering
    fallback_dashed_block_cpu(
        dock, pool, phys_width, phys_height, scale, a,
        font_manager, config,
    );
}

/// CPU fallback rendering for the dashed block
fn fallback_dashed_block_cpu(
    dock: &mut DockInstance,
    pool: &mut SlotPool,
    phys_width: i32,
    phys_height: i32,
    scale: f64,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
) {
    let block = match &mut dock.dashed_block_surface {
        Some(b) => b,
        None => return,
    };
    let expected_len = phys_width as usize * phys_height as usize * 4;
    block.cached_content.resize(expected_len, 0);
    block.cached_content.fill(0);

    render_dashed_block(
        &mut block.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale,
        alpha,
        font_manager,
        config,
    );
    block.cached_phys_width = phys_width;
    block.cached_phys_height = phys_height;

    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(b) => b,
        Err(_) => return,
    };

    if !crate::app::copy_popup_content(canvas, &block.cached_content) {
        return;
    }

    block
        .surface
        .wl_surface()
        .set_buffer_scale(scale.round().max(1.0) as i32);
    if buffer.attach_to(block.surface.wl_surface()).is_err() {
        return;
    }
    block
        .surface
        .wl_surface()
        .damage_buffer(0, 0, phys_width, phys_height);
    block.surface.wl_surface().commit();
    block.current_buffer = Some(buffer);
}
