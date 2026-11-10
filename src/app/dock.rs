//! Dock app component.
//!
//! High-level dock component that coordinates icon layout and state.

use std::collections::HashMap;
use std::collections::HashSet;

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::graphics::hide::AutoHideState;
use crate::render::slint_dock::SlintDockRenderer;
use crate::state::AppState;
use crate::types::{DockInstance, DockState, PinItem};
use crate::Rect;

use super::draw::DrawFrameData;

use crate::models::{BadgeUpdate, BluetoothSnapshot, SystemBarSnapshot, TrayItem};
use crate::render::font::FontManager;

pub fn draw_main_dock(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    _subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    _font_manager: &mut FontManager,
    hide_state: &mut AutoHideState,
    icon_cache: &HashMap<String, (Vec<u8>, u32)>,
    _loading_apps: &HashSet<String>,
    _badges: &HashMap<String, BadgeUpdate>,
    is_dragging: bool,
    dragged_app_id: Option<&String>,
    interaction: &crate::types::InteractionState,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    _bluetooth_snapshot: &BluetoothSnapshot,
    _sys_scanner: &mut sysinfo::System,
    _render_status: bool,
    render_gpu: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    is_animating: bool,
    scale_int: i32,
    mut slint_renderer: Option<&mut SlintDockRenderer>,
    bounce_app_id: Option<&String>,
    click_app_id: Option<&String>,
    minimize_app_id: Option<&String>,
    config: &crate::config::DockConfig,
) {
    let dock_height: u32 = frame.dock_height;
    let dock_scale = dock.scale_factor;
    let phys_width = frame.phys_width;
    let phys_height = frame.phys_height;

    // Build PinItem list from frame data
    let pins: Vec<PinItem> = frame
        .apps_in_dock
        .iter()
        .enumerate()
        .map(|(i, app_id)| {
            let icon_base_size = (frame.box_size as f64 * dock_scale).round() as usize;
            let icon_spacing = (frame.spacing as f64 * dock_scale).round() as usize;
            let total_icons = frame.apps_in_dock.len();
            let total_icons_width =
                total_icons * icon_base_size + (total_icons.saturating_sub(1)) * icon_spacing;
            let start_x_offset = (phys_width as usize).saturating_sub(total_icons_width) / 2;
            let x = start_x_offset + i * (icon_base_size + icon_spacing);
            let y = 0usize + ((dock_height as usize).saturating_sub(icon_base_size)) / 2;
            let running_windows = frame.running_by_app.get(app_id);
            let is_running = running_windows.is_some_and(|w| !w.is_empty());
            let running_count = running_windows.map_or(0, |w| w.len());
            let is_activated = running_windows.map_or(false, |w| w.iter().any(|win| win.focused));

            PinItem {
                app_id: app_id.clone(),
                x: x as i32,
                y: y as i32,
                size: icon_base_size,
                scale_factor: dock_scale as f32,
                is_running,
                is_activated,
                running_count,
                badge_count: None,
            }
        })
        .collect();

    // Animations disabled — always fully visible
    let dock_state = DockState {
        pins,
        hovered_index: interaction.hovered_icon,
        current_visibility_alpha: 1.0,
        panel_rect: Rect {
            x: 0.0,
            y: 0.0,
            width: phys_width as f64,
            height: (dock_height as f64 * dock_scale).round(),
        },
        fade: crate::graphics::fade::FadeAnimation::new(1.0, 1.0, 0.0),
    };

    dock.dock_state = Some(dock_state.clone());

    // Allocate SHM buffer
    let stride = phys_width * 4;
    let (buffer, canvas) = pool
        .create_buffer(
            phys_width as i32,
            phys_height as i32,
            stride as i32,
            wl_shm::Format::Argb8888,
        )
        .expect("Failed to allocate SHM buffer");

    // Use Slint renderer if available, otherwise clear buffer
    if let Some(ref mut renderer) = slint_renderer {
        // Trigger bounce animation if an app was just launched
        if let Some(app_id) = bounce_app_id {
            renderer.start_bounce(app_id, config);
        }
        // Trigger click animation if an icon was clicked
        if let Some(app_id) = click_app_id {
            renderer.start_click_animation(app_id, config);
        }
        // Trigger minimize animation if a window was minimized
        if let Some(app_id) = minimize_app_id {
            renderer.start_minimize_animation(app_id, config);
        }
        renderer.render(
            canvas,
            &dock_state,
            phys_width,
            phys_height,
            icon_cache,
            interaction.hovered_icon,
            is_dragging,
            dragged_app_id,
            interaction.pointer_position.x as i32,
            interaction.pointer_position.y as i32,
            config,
        );
    } else {
        canvas.fill(0);
    }

    // GPU rendering path via DMA-BUF: renders the dock through wgpu-api
    // and presents via DMA-BUF zero-copy sharing with the Wayland compositor.
    // Falls back to Slint CPU rendering if GPU is unavailable or fails.
    use std::os::fd::AsFd;

    // Build draw commands from dock state for GPU rendering
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();

    // Background rounded rectangle
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect {
            x: 0.0,
            y: 0.0,
            width: phys_width as f32,
            height: phys_height as f32,
        },
        color: wgpu_api::Color {
            r: 0.05,
            g: 0.05,
            b: 0.08,
            a: 0.85,
        },
        radius: (phys_height as f32) / 2.0, // Pill shape
        stroke: None,
        stroke_width: 0.0,
    });

    // Draw icon placeholders and running indicators
    for (i, pin) in dock_state.pins.iter().enumerate() {
        let px = pin.x as f32;
        let py = pin.y as f32;
        let ps = pin.size as f32;

        // Icon background (subtle highlight for running apps)
        if pin.is_running {
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect {
                    x: px,
                    y: py,
                    width: ps,
                    height: ps,
                },
                color: wgpu_api::Color {
                    r: 0.15,
                    g: 0.15,
                    b: 0.2,
                    a: 0.5,
                },
                radius: ps / 4.0,
                stroke: None,
                stroke_width: 0.0,
            });
        }

        // Running indicator dots at the bottom
        if pin.is_running {
            let dot_count = pin.running_count.min(3);
            let dot_size = 3.0 * dock_scale as f32;
            let dot_spacing = 6.0 * dock_scale as f32;
            let dots_total_width = dot_count as f32 * dot_spacing;
            let dots_x = px + (ps - dots_total_width) / 2.0;
            let dots_y = py + ps - dot_size - 2.0 * dock_scale as f32;

            for d in 0..dot_count {
                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                    rect: wgpu_api::Rect {
                        x: dots_x + d as f32 * dot_spacing,
                        y: dots_y,
                        width: dot_size,
                        height: dot_size,
                    },
                    color: wgpu_api::Color {
                        r: 0.9,
                        g: 0.9,
                        b: 0.95,
                        a: if pin.is_activated { 1.0 } else { 0.6 },
                    },
                    radius: dot_size / 2.0,
                    stroke: None,
                    stroke_width: 0.0,
                });
            }
        }
    }

    // Hover highlight
    if let Some(hovered_idx) = dock_state.hovered_index {
        if let Some(pin) = dock_state.pins.get(hovered_idx) {
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect {
                    x: pin.x as f32 - 2.0 * dock_scale as f32,
                    y: pin.y as f32 - 2.0 * dock_scale as f32,
                    width: pin.size as f32 + 4.0 * dock_scale as f32,
                    height: pin.size as f32 + 4.0 * dock_scale as f32,
                },
                color: wgpu_api::Color {
                    r: 0.3,
                    g: 0.5,
                    b: 0.8,
                    a: 0.3,
                },
                radius: (pin.size as f32) / 3.0,
                stroke: None,
                stroke_width: 0.0,
            });
        }
    }

    let gpu_rendered = if let Some(gpu_renderer) = render_gpu {
        match gpu_renderer.render_to_dmabuf(phys_width, phys_height, &draw_commands) {
            Ok(export_info) => {
                // Create Wayland buffer from DMA-BUF planes
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
                        // Attach DMA-BUF buffer to Wayland surface
                        dock.surface.wl_surface().set_buffer_scale(scale_int.max(1));
                        dock.surface.wl_surface().attach(Some(&wl_buffer), 0, 0);
                        dock.surface.wl_surface()
                            .damage_buffer(0, 0, phys_width as i32, phys_height as i32);
                        dock.surface.wl_surface().commit();
                        if is_animating {
                            dock.surface.wl_surface()
                                .frame(qh, dock.surface.wl_surface().clone());
                        }
                        true
                    }
                    Err(e) => {
                        eprintln!("DMA-BUF params creation failed: {}, falling back to CPU", e);
                        false
                    }
                }
            }
            Err(e) => {
                eprintln!("DMA-BUF rendering failed: {}, falling back to CPU", e);
                false
            }
        }
    } else {
        false
    };

    // If GPU rendering succeeded, skip the CPU SHM path entirely.
    if gpu_rendered {
        return;
    }

    // Tray, utility widgets, and system indicators are rendered on sibling
    // subsurfaces, not into the application-row framebuffer.

    hide_state.apply_alpha_to_canvas(canvas);

    let container_start = 0;
    let container_w = dock.width as i32;

    let is_hidden = hide_state.is_fully_hidden();
    dock.update_input_region(
        compositor_state,
        is_hidden,
        container_start,
        container_w,
        qh,
    );

    if is_animating {
        dock.surface
            .wl_surface()
            .frame(qh, dock.surface.wl_surface().clone());
    }

    let first_buffer = dock.current_buffer.is_none();
    dock.surface.wl_surface().set_buffer_scale(scale_int.max(1));
    buffer
        .attach_to(dock.surface.wl_surface())
        .expect("Buffer attach failed");
    dock.surface
        .wl_surface()
        .damage_buffer(0, 0, phys_width as i32, phys_height as i32);
    dock.surface.wl_surface().commit();
    dock.current_buffer = Some(buffer);
    if first_buffer {
        // First framebuffer committed
    }
}
