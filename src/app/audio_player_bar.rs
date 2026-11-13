//! Audio player bar widget.
//!
//! Always-visible media player card showing the current track and controls.
//! Renders via wgpu GPU using DrawCommands and DMA-BUF.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface,
};
use wayland_client::{Proxy, QueueHandle};

use crate::{
    models::AudioSnapshot,
    state::AppState,
    types::{DockInstance, PopupSurface},
};

// Audio player bar layout constants
const COMPACT_X: i32 = 0;
const COMPACT_WIDTH: i32 = 120;
const PLAYER_WIDTH: i32 = 240;
const PLAYER_HEIGHT: i32 = 60;

fn player_position() -> (i32, i32) {
    (COMPACT_X, 0)
}

pub fn draw_audio_player_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _pool: &mut smithay_client_toolkit::shm::slot::SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    snapshot: &AudioSnapshot,
    visible: bool,
    alpha: f32,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    if dock.audio_player_bar.is_none() {
        let surface = compositor.create_surface(qh);
        let Some(sc) = subcompositor else {
            tracing::warn!("Subcompositor not available; audio player bar rendering degraded");
            return;
        };
        let subsurface = sc.get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
        subsurface.set_desync();
        dock.audio_player_bar = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("audio-player-card".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: u64::MAX,
            // FIX (v1.5.5): Initialize last_content_hash to 0 for content-hash caching.
            last_content_hash: 0,
            last_texture_generation: 0,
        });
    }

    let (x, y) = player_position();
    let scale = dock.scale_factor.max(0.25);
    let pw = (PLAYER_WIDTH as f64 * scale).round().max(1.0) as i32;
    let ph = (PLAYER_HEIGHT as f64 * scale)
        .round()
        .max(1.0) as i32;
    let expanded = dock.audio_player_expanded;
    let Some(bar) = dock.audio_player_bar.as_mut() else {
        tracing::warn!("Audio player bar not initialized");
        return;
    };
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = PLAYER_WIDTH as u32;
    bar.height = PLAYER_HEIGHT as u32;
    // GPU rendering via wgpu-api using DrawCommands
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
    let scale_f32 = scale as f32;

    // Background panel
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(0.0, 0.0, pw as f32, ph as f32),
        color: wgpu_api::Color {
            r: 45.0 / 255.0,
            g: 45.0 / 255.0,
            b: 56.0 / 255.0,
            a: alpha,
        },
        radius: 14.0 * scale_f32,
        stroke: None,
        stroke_width: 0.0,
    });

    // Track info
    if let Some(ref title) = snapshot.media_title {
        let short = title.chars().take(20).collect::<String>()
            + if title.chars().count() > 20 { "..." } else { "" };
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 16.0 * scale_f32),
            content: short,
            font_size: 10.0 * scale_f32,
            color: wgpu_api::Color {
                r: 232.0 / 255.0,
                g: 229.0 / 255.0,
                b: 219.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });
    }
    if let Some(ref artist) = snapshot.media_artist {
        let short = artist.chars().take(20).collect::<String>()
            + if artist.chars().count() > 20 { "..." } else { "" };
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 30.0 * scale_f32),
            content: short,
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 160.0 / 255.0,
                g: 160.0 / 255.0,
                b: 170.0 / 255.0,
                a: alpha,
            },
            align: 0.0,
        });
    }

    // Render to GPU texture via DMA-BUF pool
    let gpu_success = if let Some(ref mut renderer) = gpu_context {
        if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
            match dmabuf_pool.acquire_texture(
                adapter, device, qh, dmabuf_state,
                pw as u32, ph as u32,
            ) {
                Ok((texture, export_info, slot_handle)) => {
                    if renderer.render_to_texture(
                        &texture, pw as u32, ph as u32,
                        1.0, &draw_commands,
                    ).is_err() {
                        false
                    } else {
                        match crate::render::dmabuf_export::create_buffer_from_export(
                            dmabuf_state, qh, &export_info,
                        ) {
                            Ok(wl_buffer) => {
                                match slot_handle {
                                    Some((k, idx)) => dmabuf_pool.set_buffer_id(k, idx, wl_buffer.id()),
                                    None => dmabuf_pool.set_fallback_buffer_id(
                                        wl_buffer.id(),
                                        (pw as u64) * (ph as u64) * 4,
                                    ),
                                }
                                bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                                bar.surface.attach(Some(&wl_buffer), 0, 0);
                                bar.surface.damage_buffer(0, 0, pw, ph);
                                true
                            }
                            Err(_) => false,
                        }
                    }
                }
                Err(_) => false,
            }
        } else {
            false
        }
    } else {
        false
    };

    if !gpu_success {
        tracing::warn!("GPU rendering failed for audio player bar; skipping frame");
        return;
    }
    let region_key = if !visible {
        0
    } else if expanded {
        2
    } else {
        1
    };
    if bar.input_region_key != region_key {
        if region_key == 0 {
            let region = compositor.wl_compositor().create_region(qh, ());
            bar.surface.set_input_region(Some(&region));
            region.destroy();
        } else {
            let region = compositor.wl_compositor().create_region(qh, ());
            if expanded {
                region.add(0, 0, PLAYER_WIDTH, PLAYER_HEIGHT);
            } else {
                region.add(COMPACT_X, 0, COMPACT_WIDTH, PLAYER_HEIGHT);
            }
            bar.surface.set_input_region(Some(&region));
            region.destroy();
        }
        bar.input_region_key = region_key;
    }
    bar.surface.commit();
}
