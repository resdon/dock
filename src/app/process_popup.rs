//! Process popup panel.
//!
//! Paginated list of top processes sorted by CPU or memory usage.

use crate::render::femtovg_wgpu::GpuContext;

use smithay_client_toolkit::{
    dmabuf::DmabufState,
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use super::draw::DrawFrameData;
use crate::models::{ProcessSnapshot, ProcessSort, SystemBarSnapshot, TrayItem};
use crate::render::font::FontManager;
use crate::render::processes::{render_process_surface, PROCESS_POPUP_HEIGHT, PROCESS_POPUP_WIDTH};
use crate::render::widget_bar::{button_center_x, widget_bar_position, PROCESS_INDEX};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

pub fn draw_process_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    is_open: bool,
    snapshot: &ProcessSnapshot,
    sort: ProcessSort,
    page: usize,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    gpu_context: Option<&mut GpuContext>,
    dmabuf_state: &DmabufState,
) {
    if !is_open {
        if let Some(popup) = dock.process_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
        return;
    }
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (PROCESS_POPUP_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (PROCESS_POPUP_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let (bar_x, bar_y) = widget_bar_position(
        frame.dock_width as i32,
        _tray_items.len(),
        frame.surface_height as i32,
    );
    let anchor_x = ((bar_x + button_center_x(PROCESS_INDEX)) as f64 * scale).round() as i32;
    let x = (anchor_x - phys_width / 2).max(0);
    let anchor_y = (bar_y as f64 * scale).round() as i32;
    let y = anchor_y - phys_height;
    let logical_x = (x as f64 / scale).round() as i32;
    let logical_y = (y as f64 / scale).round() as i32;

    if dock.process_popup.is_none() {
        let surface = compositor_state.create_surface(qh);
        let subsurface = subcompositor
            .expect("Subcompositor global not bound")
            .get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
        subsurface.set_desync();
        dock.process_popup = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("process-panel".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: 0,
        });
    }
    let popup = dock.process_popup.as_mut().expect("process popup created");
    popup.position = (logical_x, logical_y);
    popup.subsurface.set_position(logical_x, logical_y);
    popup.width = PROCESS_POPUP_WIDTH as u32;
    popup.height = PROCESS_POPUP_HEIGHT as u32;

    let expected_len = phys_width as usize * phys_height as usize * 4;
    popup.cached_content.resize(expected_len, 0);
    render_process_surface(
        &mut popup.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale as f32,
        snapshot,
        sort,
        page,
        font_manager,
    );
    popup.cached_phys_width = phys_width;
    popup.cached_phys_height = phys_height;

    // GPU rendering path
    if let Some(gpu) = gpu_context {
        let alpha = dock.hover_fade.current_alpha;
        let processes: Vec<(&str, f32, f32)> = snapshot
            .processes
            .iter()
            .map(|p| (p.name.as_str(), p.cpu_percent, p.memory_bytes as f32))
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
                crate::render::femtovg_wgpu::render_process_popup_femtovg(
                    &mut gpu.canvas,
                    phys_width as f32,
                    phys_height as f32,
                    scale as f32,
                    &processes,
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
                region.add(0, 0, PROCESS_POPUP_WIDTH, PROCESS_POPUP_HEIGHT);
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
        .expect("Failed to allocate process popup buffer");
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup
        .surface
        .set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&popup.surface)
        .expect("Failed to attach process popup buffer");
    popup.surface.damage_buffer(0, 0, phys_width, phys_height);
    let region = compositor_state.wl_compositor().create_region(qh, ());
    region.add(0, 0, PROCESS_POPUP_WIDTH, PROCESS_POPUP_HEIGHT);
    popup.surface.set_input_region(Some(&region));
    region.destroy();
    popup.surface.commit();
    popup.current_buffer = Some(buffer);
}

pub fn minimize_button_at(x: f64, y: f64) -> bool {
    crate::render::processes::minimize_button_at(x, y)
}
pub fn page_delta_at(x: f64, y: f64) -> Option<i32> {
    crate::render::processes::page_delta_at(x, y)
}
pub fn sort_at(x: f64, y: f64) -> Option<ProcessSort> {
    crate::render::processes::sort_at(x, y)
}
