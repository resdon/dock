//! Process popup panel.
//!
//! Paginated list of top processes sorted by CPU or memory usage.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
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
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
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
        let Some(sc) = subcompositor else {
            tracing::warn!("Subcompositor not available; process popup rendering degraded");
            return;
        };
        let surface = compositor_state.create_surface(qh);
        let subsurface = sc.get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
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
            // FIX (v1.5.5): Initialize last_content_hash to 0 for content-hash caching.
            last_content_hash: 0,
            last_texture_generation: 0,
        });
    }
    let Some(popup) = dock.process_popup.as_mut() else {
        tracing::warn!("Process popup not initialized");
        return;
    };
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

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
    // CPU rendering path
    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(bc) => bc,
        Err(e) => {
            tracing::warn!("Failed to allocate process popup buffer: {}", e);
            return;
        }
    };
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup
        .surface
        .set_buffer_scale(scale.round().max(1.0) as i32);
    if let Err(e) = buffer.attach_to(&popup.surface) {
        tracing::warn!("Failed to attach process popup buffer: {}", e);
        return;
    }
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
