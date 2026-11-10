//! Side bar components.
//!
//! Left tray (system tray icons) and bottom system bar (clock, keyboard, network).

use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
    dmabuf::DmabufState,
};
use wayland_client::{protocol::wl_shm, QueueHandle};

use crate::models::SystemBarSnapshot;
use crate::render::tray::{
    render_system_status_into_canvas, render_tray_icons_into_canvas, system_bar_logical_width,
    system_bar_position, tray_bar_position, tray_logical_size, SYSTEM_BAR_HEIGHT,
};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

fn ensure_surface(
    slot: &mut Option<PopupSurface>,
    parent_surface: &wayland_client::protocol::wl_surface::WlSurface,
    qh: &QueueHandle<AppState>,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    app_id: &'static str,
) {
    if slot.is_some() {
        return;
    }
    let surface = compositor.create_surface(qh);
    let subsurface = subcompositor
        .expect("Subcompositor global not bound")
        .get_subsurface(&surface, parent_surface, qh, ());
    subsurface.set_desync();
    *slot = Some(PopupSurface {
        subsurface,
        surface,
        current_buffer: None,
        width: 0,
        height: 0,
        current_app_id: Some(app_id.into()),
        position: (0, 0),
        cached_content: Vec::new(),
        cached_phys_width: 0,
        cached_phys_height: 0,
        input_region_key: u64::MAX,
    });
}

fn set_interactive_region(
    bar: &mut PopupSurface,
    visible: bool,
    compositor: &CompositorState,
    qh: &QueueHandle<AppState>,
) {
    let key = if visible { 1 } else { 0 };
    if bar.input_region_key == key {
        return;
    }
    if visible {
        bar.surface.set_input_region(None);
    } else {
        let region = compositor.wl_compositor().create_region(qh, ());
        bar.surface.set_input_region(Some(&region));
        region.destroy();
    }
    bar.input_region_key = key;
}

/// StatusNotifierItem icons in an independent rounded box to the right of the
/// centered app row. It matches the utility box for up to four icons,
/// then grows vertically in two-column rows as more SNI items arrive.
pub fn draw_left_tray_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    items: &[crate::models::TrayItem],
    visible: bool,
    alpha: f32,
    font_manager: &mut crate::render::font::FontManager,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
) {
    let parent_surface = dock.surface.wl_surface().clone();
    ensure_surface(
        &mut dock.tray_bar,
        &parent_surface,
        qh,
        compositor,
        subcompositor,
        "left-status-tray",
    );
    let (logical_width, logical_height) = tray_logical_size(items.len());
    let buffer_width = logical_width.max(1) as u32;
    let (x, y) = tray_bar_position(dock.width as i32, items.len());
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (buffer_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (logical_height as f64 * scale).round().max(1.0) as i32;
    let expected = phys_width as usize * phys_height as usize * 4;

    let bar = dock.tray_bar.as_mut().expect("tray bar initialized");
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = buffer_width;
    bar.height = logical_height as u32;
    bar.cached_content.resize(expected, 0);
    bar.cached_content.fill(0);

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
    if !items.is_empty() {
        render_tray_icons_into_canvas(
            &mut bar.cached_content,
            phys_width as u32,
            phys_height as u32,
            scale,
            items,
            1.0,
            font_manager,
            std::env::var_os("DOCK_RENDER_DEBUG").is_some(),
        );
        apply_surface_alpha(&mut bar.cached_content, alpha);
    }
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    let (buffer, canvas) = pool
        .create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate left tray buffer");
    if !super::copy_popup_content(canvas, &bar.cached_content) {
        return;
    }
    bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&bar.surface)
        .expect("Failed to attach left tray buffer");
    bar.surface.damage_buffer(0, 0, phys_width, phys_height);
    set_interactive_region(bar, visible && !items.is_empty(), compositor, qh);
    bar.surface.commit();
    bar.current_buffer = Some(buffer);
}

/// Keyboard layout, network rates and clock in a passive, always-opaque strip
/// directly under the dock row. It deliberately does not follow the dock fade;
/// its empty input region lets pointer input pass through to the desktop.
pub fn draw_bottom_system_bar(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    pool: &mut SlotPool,
    compositor: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    snapshot: &SystemBarSnapshot,
    visible: bool,
    alpha: f32,
    font_manager: &mut crate::render::font::FontManager,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
) {
    let parent_surface = dock.surface.wl_surface().clone();
    ensure_surface(
        &mut dock.system_status_bar,
        &parent_surface,
        qh,
        compositor,
        subcompositor,
        "bottom-system-status",
    );
    let scale = dock.scale_factor.max(0.25);
    let logical_width = system_bar_logical_width(dock.width as i32).max(1) as u32;
    let phys_width = (logical_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (SYSTEM_BAR_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let expected = phys_width as usize * phys_height as usize * 4;
    let (x, y) = system_bar_position(dock.width as i32);

    let bar = dock
        .system_status_bar
        .as_mut()
        .expect("system status bar initialized");
    bar.position = (x, y);
    bar.subsurface.set_position(x, y);
    bar.width = logical_width;
    bar.height = SYSTEM_BAR_HEIGHT as u32;
    bar.cached_content.resize(expected, 0);
    bar.cached_content.fill(0);

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
    if visible {
        render_system_status_into_canvas(
            &mut bar.cached_content,
            phys_width as u32,
            phys_height as u32,
            scale,
            snapshot,
            1.0,
            font_manager,
            std::env::var_os("DOCK_RENDER_DEBUG").is_some(),
        );
        apply_surface_alpha(&mut bar.cached_content, alpha);
    }
    bar.cached_phys_width = phys_width;
    bar.cached_phys_height = phys_height;

    let (buffer, canvas) = pool
        .create_buffer(
            phys_width,
            phys_height,
            phys_width * 4,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate bottom system status buffer");
    if !super::copy_popup_content(canvas, &bar.cached_content) {
        return;
    }
    bar.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    buffer
        .attach_to(&bar.surface)
        .expect("Failed to attach bottom system status buffer");
    bar.surface.damage_buffer(0, 0, phys_width, phys_height);
    set_interactive_region(bar, false, compositor, qh);
    bar.surface.commit();
    bar.current_buffer = Some(buffer);
}

fn apply_surface_alpha(pixels: &mut [u8], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha >= 0.999 {
        return;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        for channel in 0..4 {
            pixel[channel] = (pixel[channel] as f32 * alpha).round() as u8;
        }
    }
}
