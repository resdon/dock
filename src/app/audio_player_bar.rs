//! Audio player bar widget.
//!
//! Always-visible media player card showing the current track and controls.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use crate::{
    models::AudioSnapshot,
    render::{audio_player, font::FontManager},
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
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
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
        });
    }

    let (x, y) = audio_player::player_position();
    let scale = dock.scale_factor.max(0.25);
    let pw = (audio_player::PLAYER_WIDTH as f64 * scale).round().max(1.0) as i32;
    let ph = (audio_player::PLAYER_HEIGHT as f64 * scale)
        .round()
        .max(1.0) as i32;
    let expanded = dock.audio_player_expanded;
    let Some(bar) = dock.audio_player_bar.as_mut() else {
        tracing::warn!("Audio player bar not initialized");
        return;
    };
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = audio_player::PLAYER_WIDTH as u32;
    bar.height = audio_player::PLAYER_HEIGHT as u32;
    bar.cached_content.resize(pw as usize * ph as usize * 4, 0);
    bar.cached_content.fill(0);

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
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

    let (buffer, canvas) = match pool.create_buffer(pw, ph, pw * 4, wl_shm::Format::Argb8888) {
        Ok(bc) => bc,
        Err(e) => {
            tracing::warn!("Failed to allocate audio player buffer: {}", e);
            return;
        }
    };
    if !super::copy_popup_content(canvas, &bar.cached_content) {
        return;
    }
    bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    if let Err(e) = buffer.attach_to(&bar.surface) {
        tracing::warn!("Failed to attach audio player buffer: {}", e);
        return;
    }
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
