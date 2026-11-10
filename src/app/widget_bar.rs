//! Widget bar app component.
//!
//! Right-side utility widget cluster: rclone, process monitor, audio, Bluetooth.

use smithay_client_toolkit::{
    dmabuf::DmabufState,
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use crate::models::BluetoothSnapshot;
use crate::render::widget_bar::{
    render_status_widget_bar, widget_bar_position, WIDGET_BAR_HEIGHT, WIDGET_BAR_WIDTH,
};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

/// Draws the four utility controls in a persistent subsurface to the right of
/// the tray app box.
pub fn draw_status_widget_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    bluetooth: &BluetoothSnapshot,
    tray_item_count: usize,
    visible: bool,
    alpha: f32,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
) {
    if dock.status_widget_bar.is_none() {
        let surface = compositor_state.create_surface(qh);
        let parent_surface = dock.surface.wl_surface().clone();
        let subsurface = subcompositor
            .expect("Subcompositor global not bound")
            .get_subsurface(&surface, &parent_surface, qh, ());
        subsurface.set_desync();
        dock.status_widget_bar = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("status-widget-bar".into()),
            position: (0, 0),
            cached_content: Vec::new(),
            cached_phys_width: 0,
            cached_phys_height: 0,
            input_region_key: u64::MAX,
        });
    }

    let (x, y) = widget_bar_position(dock.width as i32, tray_item_count, dock.height as i32);
    let bar = dock
        .status_widget_bar
        .as_mut()
        .expect("widget bar initialized");
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = WIDGET_BAR_WIDTH as u32;
    bar.height = WIDGET_BAR_HEIGHT as u32;

    let scale = dock.scale_factor.max(0.25);
    let phys_width = (WIDGET_BAR_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (WIDGET_BAR_HEIGHT as f64 * scale).round().max(1.0) as i32;
    // Use the same hide-state alpha as the main dock and left tray. A hidden
    // dock settles at HIDDEN_ALPHA (currently 0.20), rather than becoming
    // fully transparent. The input region is handled independently below.
    let alpha = alpha.clamp(0.0, 1.0);

    let expected_len = phys_width as usize * phys_height as usize * 4;
    bar.cached_content.resize(expected_len, 0);
    // The renderer uses premultiplied-alpha blending. Clear the old frame first
    // so a previous opaque frame cannot keep the new frame at alpha 255.
    bar.cached_content.fill(0);
    render_status_widget_bar(
        &mut bar.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale,
        alpha,
        bluetooth,
    );
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
    // CPU rendering path
    let (buffer, canvas) = pool
        .create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate status widget bar buffer");
    if !super::copy_popup_content(canvas, &bar.cached_content) {
        return;
    }

    bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&bar.surface)
        .expect("Failed to attach status widget bar buffer");
    bar.surface.damage_buffer(0, 0, phys_width, phys_height);

    let region_key = if visible { 1 } else { 0 };
    if bar.input_region_key != region_key {
        if visible {
            bar.surface.set_input_region(None);
        } else {
            // Empty input region means the hidden sidecar cannot intercept
            // clicks or prevent the dock's 3px reveal strip from working.
            let region = compositor_state.wl_compositor().create_region(qh, ());
            bar.surface.set_input_region(Some(&region));
            region.destroy();
        }
        bar.input_region_key = region_key;
    }

    bar.surface.commit();
    bar.current_buffer = Some(buffer);
}
