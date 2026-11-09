//! Audio player bar widget.
//!
//! Always-visible media player card showing the current track and controls.

use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
    dmabuf::DmabufState,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use crate::{
    models::AudioSnapshot,
    render::{audio_player, dmabuf_export::attach_and_commit, femtovg_wgpu::{GpuContext, render_audio_player_bar_femtovg}, font::FontManager},
    state::AppState,
    types::{DockInstance, PopupSurface},
};

pub fn draw_audio_player_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    snapshot: &AudioSnapshot,
    visible: bool,
    alpha: f32,
    font: &mut FontManager,
    gpu_context: Option<&mut GpuContext>,
    dmabuf_state: &DmabufState,
) {
    if dock.audio_player_bar.is_none() {
        let surface = compositor.create_surface(qh);
        let subsurface = subcompositor
            .expect("Subcompositor global not bound")
            .get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
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
        });
    }

    let (x, y) = audio_player::player_position();
    let scale = dock.scale_factor.max(0.25);
    let pw = (audio_player::PLAYER_WIDTH as f64 * scale).round().max(1.0) as i32;
    let ph = (audio_player::PLAYER_HEIGHT as f64 * scale)
        .round()
        .max(1.0) as i32;
    let expanded = dock.audio_player_expanded;
    let bar = dock
        .audio_player_bar
        .as_mut()
        .expect("audio player surface initialized");
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = audio_player::PLAYER_WIDTH as u32;
    bar.height = audio_player::PLAYER_HEIGHT as u32;
    bar.cached_content.resize(pw as usize * ph as usize * 4, 0);
    bar.cached_content.fill(0);

    // GPU path: FemtoVG + dmabuf
    if let Some(gpu) = gpu_context {
        gpu.canvas.save();
        gpu.canvas.set_global_alpha(alpha);
        render_audio_player_bar_femtovg(&mut gpu.canvas, pw as f32, ph as f32, scale as f32, snapshot, alpha, expanded);
        gpu.canvas.restore();

        match gpu.dmabuf_pool.get_or_create(&gpu.adapter, &gpu.device, qh, dmabuf_state, pw as u32, ph as u32) {
            Ok(target) => {
                if let Some(cmd_buf) = gpu.flush_to_texture(&target.texture) {
                    gpu.queue.submit(std::iter::once(cmd_buf));
                }
                bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                attach_and_commit(&bar.surface, &target.buffer);
                let region_key = if !visible { 0 } else if expanded { 2 } else { 1 };
                if bar.input_region_key != region_key {
                    if region_key == 0 {
                        let region = compositor.wl_compositor().create_region(qh, ());
                        bar.surface.set_input_region(Some(&region));
                        region.destroy();
                    } else {
                        let region = compositor.wl_compositor().create_region(qh, ());
                        if expanded {
                            region.add(0, 0, audio_player::PLAYER_WIDTH, audio_player::PLAYER_HEIGHT);
                        } else {
                            region.add(audio_player::COMPACT_X, audio_player::COMPACT_Y - 45, audio_player::COMPACT_WIDTH, audio_player::PLAYER_HEIGHT - (audio_player::COMPACT_Y - 45));
                        }
                        bar.surface.set_input_region(Some(&region));
                        region.destroy();
                    }
                    bar.input_region_key = region_key;
                }
                return;
            }
            Err(_) => {}
        }
    }

    // CPU fallback path
    audio_player::render(
        &mut bar.cached_content,
        pw as u32,
        ph as u32,
        scale,
        alpha,
        snapshot,
        font,
        expanded,
    );
    bar.cached_phys_width = pw;
    bar.cached_phys_height = ph;

    let (buffer, canvas) = pool
        .create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888)
        .expect("Failed to allocate audio player buffer");
    if !super::copy_popup_content(canvas, &bar.cached_content) {
        return;
    }
    bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&bar.surface)
        .expect("Failed to attach audio player buffer");
    bar.surface.damage_buffer(0, 0, pw, ph);
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
                region.add(
                    0,
                    0,
                    audio_player::PLAYER_WIDTH,
                    audio_player::PLAYER_HEIGHT,
                );
            } else {
                region.add(
                    audio_player::COMPACT_X,
                    audio_player::COMPACT_Y - 45,
                    audio_player::COMPACT_WIDTH,
                    audio_player::PLAYER_HEIGHT - (audio_player::COMPACT_Y - 45),
                );
            }
            bar.surface.set_input_region(Some(&region));
            region.destroy();
        }
        bar.input_region_key = region_key;
    }
    bar.surface.commit();
    bar.current_buffer = Some(buffer);
}
