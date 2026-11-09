//! Rclone popup panel.
//!
//! Paginated list of rclone mounts with transfer rate information.

use crate::render::femtovg_wgpu::GpuContext;

use smithay_client_toolkit::{
    dmabuf::DmabufState,
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use super::draw::DrawFrameData;
use crate::models::{RcloneSnapshot, SystemBarSnapshot, TrayItem};
use crate::render::font::FontManager;
use crate::render::rclone::{render_rclone_surface, RCLONE_POPUP_HEIGHT, RCLONE_POPUP_WIDTH};
use crate::render::widget_bar::{button_center_x, widget_bar_position, RCLONE_INDEX};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

pub fn draw_rclone_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    is_open: bool,
    snapshot: &RcloneSnapshot,
    page: usize,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    gpu_context: Option<&mut GpuContext>,
    dmabuf_state: &DmabufState,
) {
    if !is_open {
        if let Some(popup) = dock.rclone_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
        return;
    }
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (RCLONE_POPUP_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (RCLONE_POPUP_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let (bar_x, bar_y) = widget_bar_position(
        frame.dock_width as i32,
        _tray_items.len(),
        frame.surface_height as i32,
    );
    let anchor_x = ((bar_x + button_center_x(RCLONE_INDEX)) as f64 * scale).round() as i32;
    let x = (anchor_x - phys_width / 2).max(0);
    let anchor_y = (bar_y as f64 * scale).round() as i32;
    let y = anchor_y - phys_height;
    let logical_x = (x as f64 / scale).round() as i32;
    let logical_y = (y as f64 / scale).round() as i32;

    if dock.rclone_popup.is_none() {
        let surface = compositor_state.create_surface(qh);
        let subsurface = subcompositor
            .expect("Subcompositor global not bound")
            .get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
        subsurface.set_desync();
        dock.rclone_popup = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("rclone-panel".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: 0,
        });
    }
    let popup = dock.rclone_popup.as_mut().expect("rclone popup created");
    popup.position = (logical_x, logical_y);
    popup.subsurface.set_position(logical_x, logical_y);
    popup.width = RCLONE_POPUP_WIDTH as u32;
    popup.height = RCLONE_POPUP_HEIGHT as u32;

    let expected_len = phys_width as usize * phys_height as usize * 4;
    popup.cached_content.resize(expected_len, 0);
    render_rclone_surface(
        &mut popup.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale as f32,
        snapshot,
        page,
        font_manager,
    );
    popup.cached_phys_width = phys_width;
    popup.cached_phys_height = phys_height;

    // GPU rendering path
    if let Some(gpu) = gpu_context {
        let alpha = dock.hover_fade.current_alpha;
        let transfers: Vec<(&str, f32, String)> = snapshot
            .mounts
            .iter()
            .map(|m| {
                let pct = if m.total_bytes > 0 {
                    (m.used_bytes as f32 / m.total_bytes as f32) * 100.0
                } else {
                    0.0
                };
                let speed = format!("{:.1}%", pct);
                (m.mount_point.as_str(), pct, speed)
            })
            .collect();
        match gpu.dmabuf_pool.get_or_create(
            &gpu.adapter,
            &gpu.device,
            qh,
            dmabuf_state,
            phys_width as u32,
            phys_height as u32,
        ) {
            Ok(target) => {
                gpu.resize(phys_width as u32, phys_height as u32, (scale * 96.0) as f32);
                crate::render::femtovg_wgpu::render_rclone_popup_femtovg(
                    &mut gpu.canvas,
                    phys_width as f32,
                    phys_height as f32,
                    scale as f32,
                    &transfers,
                    alpha,
                );
                let output = femtovg::renderer::WGPURenderOutput::from(&target.texture);
                if let Some(cmd_buf) = gpu.canvas.flush_to_output(output) {
                    gpu.queue.submit(std::iter::once(cmd_buf));
                }
                popup.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                crate::render::dmabuf_export::attach_and_commit(&popup.surface, &target.buffer);
                popup.surface.damage_buffer(0, 0, phys_width, phys_height);
                let region = compositor_state.wl_compositor().create_region(qh, ());
                region.add(0, 0, RCLONE_POPUP_WIDTH, RCLONE_POPUP_HEIGHT);
                popup.surface.set_input_region(Some(&region));
                region.destroy();
                popup.surface.commit();
                popup.current_buffer = None;
                return;
            }
            Err(_) => {
                // Fall through to CPU path
            }
        }
    }

    // CPU rendering path
    let (buffer, canvas) = pool
        .create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate rclone popup buffer");
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup
        .surface
        .set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&popup.surface)
        .expect("Failed to attach rclone popup buffer");
    popup.surface.damage_buffer(0, 0, phys_width, phys_height);
    let region = compositor_state.wl_compositor().create_region(qh, ());
    region.add(0, 0, RCLONE_POPUP_WIDTH, RCLONE_POPUP_HEIGHT);
    popup.surface.set_input_region(Some(&region));
    region.destroy();
    popup.surface.commit();
    popup.current_buffer = Some(buffer);
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    crate::render::rclone::minimize_button_at(x, y)
}
pub fn page_delta_at(x: f64, y: f64) -> Option<i32> {
    crate::render::rclone::page_delta_at(x, y)
}
