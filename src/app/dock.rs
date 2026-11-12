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

static GPU_BOUNCE_ANIMATIONS: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, u32>>> =
    std::sync::OnceLock::new();
static GPU_CLICK_ANIMATIONS: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, u32>>> =
    std::sync::OnceLock::new();
static GPU_MINIMIZE_ANIMATIONS: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, u32>>> =
    std::sync::OnceLock::new();

pub fn has_active_gpu_animation() -> bool {
    GPU_BOUNCE_ANIMATIONS
        .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
        .lock()
        .values()
        .any(|count| *count > 0)
        || GPU_CLICK_ANIMATIONS
            .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
            .lock()
            .values()
            .any(|count| *count > 0)
        || GPU_MINIMIZE_ANIMATIONS
            .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
            .lock()
            .values()
            .any(|count| *count > 0)
}

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
            let y = (phys_height as usize).saturating_sub(icon_base_size) / 2;
            let mut running_count = 0;
            let mut is_activated = false;
            if let Some(window_ids) = frame.running_by_app.get(app_id) {
                for id in window_ids {
                    if let Some(window) = frame
                        .windows
                        .get(&crate::models::system::WindowId(id.clone()))
                    {
                        if window.output_ids.is_empty()
                            || window
                                .output_ids
                                .iter()
                                .any(|output| &output.0 == frame.dock_output)
                        {
                            running_count += 1;
                            is_activated |= window.focused;
                        }
                    }
                }
            }
            let is_running = running_count > 0;

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

    // GPU rendering path via DMA-BUF: renders the dock through wgpu-api
    // and presents via DMA-BUF zero-copy sharing with the Wayland compositor.
    // Falls back to Slint CPU rendering if GPU is unavailable or fails.
    use std::os::fd::AsFd;

    // Build draw commands from dock state for GPU rendering.
    // Layout matches the Slint ui.slint definition exactly:
    // - Background: full dock rectangle with configurable color/border
    // - Icons: 48px base size, y=6px, 12px left padding, 8px spacing between items
    // - Running dashes: 32px span centered under each icon, y=58px, 2px height
    // - Hover highlight: colored outline behind icon when hovered
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();

    // GPU rendering: upload icon textures, build draw commands, and render
    let gpu_rendered = if let Some(gpu_renderer) = render_gpu {
        // Trigger GPU animations
        if let Some(app_id) = bounce_app_id {
            if config.bounce_animation {
                let total_frames = (config.bounce_animation_duration * 60.0) as u32;
                let mut anims = GPU_BOUNCE_ANIMATIONS
                    .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                    .lock();
                anims.insert(app_id.clone(), total_frames);
            }
        }
        if let Some(app_id) = click_app_id {
            let total_frames = ((config.click_animation_up_duration
                + config.click_animation_down_duration)
                * 60.0) as u32;
            let mut anims = GPU_CLICK_ANIMATIONS
                .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                .lock();
            anims.insert(app_id.clone(), total_frames);
        }
        if let Some(app_id) = minimize_app_id {
            let total_frames = (config.minimize_animation_duration * 60.0) as u32;
            let mut anims = GPU_MINIMIZE_ANIMATIONS
                .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                .lock();
            anims.insert(app_id.clone(), total_frames);
        }

        // Upload icon textures from cache
        let mut icon_textures: HashMap<String, wgpu_api::TextureId> = HashMap::new();
        for pin in &dock_state.pins {
            if let Some((pixels, size)) = icon_cache.get(&pin.app_id) {
                let icon_size = *size as u32;
                tracing::debug!(
                    "GPU upload {}: {}x{} pixels",
                    pin.app_id,
                    icon_size,
                    icon_size
                );
                let cache_key = format!("dock-icon:{}:{}x{}", pin.app_id, icon_size, icon_size);
                if let Some(texture_id) = gpu_renderer
                    .upload_texture_from_pixels_cached(&cache_key, pixels, icon_size, icon_size)
                {
                    icon_textures.insert(pin.app_id.clone(), texture_id);
                    tracing::debug!("  -> TextureId {}", texture_id.as_u64());
                } else {
                    tracing::warn!("  -> FAILED to upload texture for {}", pin.app_id);
                }
            } else {
                tracing::debug!("GPU cache miss for {}", pin.app_id);
            }
        }
        tracing::info!(
            "GPU: {} textures uploaded, {} draw commands",
            icon_textures.len(),
            draw_commands.len()
        );

        // Scale factor to convert logical pixels to physical pixels.
        // The GPU render target is phys_width x phys_height (physical pixels),
        // so all layout coordinates must be scaled accordingly.
        let scale = dock_scale as f32;

        // Layout constants matching ui.slint exactly (in logical pixels),
        // scaled to physical pixels for GPU rendering.
        let icon_size = frame.box_size as f32;
        let icon_scale = icon_size / 48.0;
        let icon_size_px = icon_size * scale;
        let item_spacing = frame.spacing as f32 * scale;
        let icon_y = (frame.dock_height as f32 - icon_size) * 0.5 * scale;
        let padding_x = frame.spacing as f32 * 1.5 * scale;
        let dash_y = (frame.dock_height as f32 - 2.0) * scale;
        let dash_height: f32 = 2.0 * scale;
        let dash_span = icon_size * (2.0 / 3.0);
        let dash_base_x_offset = (icon_size - dash_span) * 0.5 * scale;
        let dash_gap = 3.0 * icon_scale * scale;
        let dash_configs: [(usize, f32, f32); 5] = std::array::from_fn(|index| {
            let count = index + 1;
            let gap = if count == 1 { 0.0 } else { dash_gap };
            (
                count,
                (dash_span * scale - (count - 1) as f32 * gap) / count as f32,
                gap,
            )
        });

        // Parse colors from config
        let bg_color = crate::config::DockConfig::parse_hex_color(&config.background_color);
        let border_color = crate::config::DockConfig::parse_hex_color(&config.border_color);
        let hover_color = crate::config::DockConfig::parse_hex_color(&config.hover_color);
        let focused_dash_color =
            crate::config::DockConfig::parse_hex_color(&config.dash_focused_color);
        let inactive_dash_color =
            crate::config::DockConfig::parse_hex_color(&config.dash_inactive_color);

        // Dock background: full rectangle with configurable color and alpha
        let bg_alpha = config.background_alpha; // 0.0-1.0 range
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect {
                x: 0.0,
                y: 0.0,
                width: phys_width as f32,
                height: phys_height as f32,
            },
            color: wgpu_api::Color {
                r: bg_color.0 as f32 / 255.0,
                g: bg_color.1 as f32 / 255.0,
                b: bg_color.2 as f32 / 255.0,
                a: bg_alpha,
            },
            radius: config.border_radius as f32 * scale,
            stroke: if config.border_alpha > 0.0 {
                Some(wgpu_api::Color {
                    r: border_color.0 as f32 / 255.0,
                    g: border_color.1 as f32 / 255.0,
                    b: border_color.2 as f32 / 255.0,
                    a: config.border_alpha,
                })
            } else {
                None
            },
            stroke_width: if config.border_alpha > 0.0 {
                1.0 * scale
            } else {
                0.0
            },
        });

        // Calculate drag target slot index for sliding animation (matches slint_dock.rs)
        let item_slot_width = (frame.box_size as f32 + frame.spacing as f32) * scale;
        let padding_logical = frame.spacing as f32 * 1.5; // 12px logical padding
        let (gpu_drag_source_idx, gpu_drag_target_slot) = if is_dragging {
            if let Some(app_id) = dragged_app_id {
                let source_idx = dock_state.pins.iter().position(|p| p.app_id == *app_id);
                if let Some(si) = source_idx {
                    let cursor_x = interaction.pointer_position.x as f32 / scale; // convert to logical
                    let slot_x = cursor_x - padding_logical;
                    let target_slot =
                        (slot_x / (frame.box_size as f32 + frame.spacing as f32)).round() as i32;
                    let clamped = target_slot.max(0).min(dock_state.pins.len() as i32);
                    (si as i32, clamped)
                } else {
                    (-1, -1)
                }
            } else {
                (-1, -1)
            }
        } else {
            (-1, -1)
        };

        // Draw icons and running indicators — layout matches ui.slint exactly
        for (i, pin) in dock_state.pins.iter().enumerate() {
            // Skip the dragged icon — it will be drawn at the cursor position below
            if is_dragging && dragged_app_id.is_some_and(|id| *id == pin.app_id) {
                continue;
            }

            // Calculate horizontal offset for sliding animation during drag
            let offset_x = if is_dragging && gpu_drag_source_idx >= 0 {
                let idx = i as i32;
                if gpu_drag_target_slot > gpu_drag_source_idx {
                    // Target is to the right of source: shift icons between source+1 and target left
                    if idx > gpu_drag_source_idx && idx <= gpu_drag_target_slot {
                        -item_slot_width
                    } else {
                        0.0
                    }
                } else if gpu_drag_target_slot < gpu_drag_source_idx {
                    // Target is to the left of source: shift icons between target and source-1 right
                    if idx >= gpu_drag_target_slot && idx < gpu_drag_source_idx {
                        item_slot_width
                    } else {
                        0.0
                    }
                } else {
                    0.0
                }
            } else {
                0.0
            };

            // Icon position: 12px padding + index * (48 + 8), scaled to physical pixels
            let px = padding_x + i as f32 * (icon_size_px + item_spacing) + offset_x;
            let py = icon_y;
            let ps = icon_size_px;

            // Calculate animation scale for this icon. Lock each mutex once
            // and read/decrement in a single critical section per animation type.
            let anim_scale = {
                let mut bounce_anims = GPU_BOUNCE_ANIMATIONS
                    .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                    .lock();
                let bounce_counter = bounce_anims.get(&pin.app_id).copied().unwrap_or(0);
                if let Some(count) = bounce_anims.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let bounce_scale = if bounce_counter > 0 && config.bounce_animation {
                    let total_bounce_frames = (config.bounce_animation_duration * 60.0) as u32;
                    if total_bounce_frames == 0 {
                        1.0f32
                    } else {
                        let elapsed = total_bounce_frames - bounce_counter;
                        let progress = elapsed as f32 / total_bounce_frames as f32;
                        let angle = progress * std::f32::consts::PI;
                        1.0 + angle.sin() * (config.bounce_animation_scale - 1.0)
                    }
                } else {
                    1.0f32
                };

                let mut click_anims = GPU_CLICK_ANIMATIONS
                    .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                    .lock();
                let click_counter = click_anims.get(&pin.app_id).copied().unwrap_or(0);
                if let Some(count) = click_anims.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let click_scale = if click_counter > 0 {
                    let total_frames = ((config.click_animation_up_duration
                        + config.click_animation_down_duration)
                        * 60.0) as u32;
                    if total_frames == 0 {
                        1.0
                    } else {
                        let elapsed = total_frames - click_counter;
                        let progress = elapsed as f32 / total_frames as f32;
                        let angle = progress * std::f32::consts::PI;
                        1.0 + angle.sin() * (config.click_animation_scale - 1.0)
                    }
                } else {
                    1.0f32
                };

                let mut minimize_anims = GPU_MINIMIZE_ANIMATIONS
                    .get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
                    .lock();
                let minimize_counter = minimize_anims.get(&pin.app_id).copied().unwrap_or(0);
                if let Some(count) = minimize_anims.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let minimize_scale = if minimize_counter > 0 {
                    let total_frames = (config.minimize_animation_duration * 60.0) as u32;
                    if total_frames == 0 {
                        1.0
                    } else {
                        let elapsed = total_frames - minimize_counter;
                        let progress = elapsed as f32 / total_frames as f32;
                        let angle = progress * std::f32::consts::PI;
                        1.0 - angle.sin() * (1.0 - config.minimize_animation_scale)
                    }
                } else {
                    1.0f32
                };

                if minimize_counter > 0 {
                    minimize_scale
                } else if click_counter > 0 {
                    click_scale
                } else {
                    bounce_scale
                }
            };

            // Apply animation scale — centered on base position
            let scaled_ps = ps * anim_scale;
            let icon_x = px + (ps - scaled_ps) / 2.0;
            let icon_y = py + (ps - scaled_ps) / 2.0;

            // Hover highlight: colored outline behind icon (matches ui.slint icon_outline)
            if config.hover_highlight && dock_state.hovered_index == Some(i) {
                let thickness = scale.max(1.0);
                let outline_offset = 2.0 * thickness;
                let outline_color = wgpu_api::Color {
                    r: hover_color.0 as f32 / 255.0,
                    g: hover_color.1 as f32 / 255.0,
                    b: hover_color.2 as f32 / 255.0,
                    a: 1.0,
                };
                let outline_x = icon_x - outline_offset;
                let outline_y = icon_y - outline_offset;
                let outline_width = scaled_ps + outline_offset * 2.0;
                let outline_height = scaled_ps + outline_offset * 2.0;
                for rect in [
                    wgpu_api::Rect::new(outline_x, outline_y, outline_width, thickness),
                    wgpu_api::Rect::new(
                        outline_x,
                        outline_y + outline_height - thickness,
                        outline_width,
                        thickness,
                    ),
                    wgpu_api::Rect::new(
                        outline_x,
                        outline_y + thickness,
                        thickness,
                        outline_height - thickness * 2.0,
                    ),
                    wgpu_api::Rect::new(
                        outline_x + outline_width - thickness,
                        outline_y + thickness,
                        thickness,
                        outline_height - thickness * 2.0,
                    ),
                ] {
                    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                        rect,
                        color: outline_color,
                        radius: 0.0,
                        stroke: None,
                        stroke_width: 0.0,
                    });
                }
            }

            // Draw icon texture if available
            if let Some(texture_id) = icon_textures.get(&pin.app_id) {
                draw_commands.push(wgpu_api::DrawCommand::Image {
                    dest_rect: wgpu_api::Rect {
                        x: icon_x,
                        y: icon_y,
                        width: scaled_ps,
                        height: scaled_ps,
                    },
                    texture: *texture_id,
                    source_uv: wgpu_api::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: 1.0,
                        height: 1.0,
                    },
                    tint: wgpu_api::Color {
                        r: 1.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                });
            }

            // Running indicator dashes — match ui.slint 32px-span design exactly
            if pin.is_running {
                let window_count = pin.running_count.min(5);
                let config = dash_configs.get(window_count - 1);
                if let Some(&(count, dash_width, gap)) = config {
                    // Dash span is 32px centered within 48px icon: starts at px + 8*scale
                    let dash_base_x = px + dash_base_x_offset;
                    let (r, g, b) = if pin.is_activated {
                        (
                            focused_dash_color.0 as f32 / 255.0,
                            focused_dash_color.1 as f32 / 255.0,
                            focused_dash_color.2 as f32 / 255.0,
                        )
                    } else {
                        (
                            inactive_dash_color.0 as f32 / 255.0,
                            inactive_dash_color.1 as f32 / 255.0,
                            inactive_dash_color.2 as f32 / 255.0,
                        )
                    };
                    let a = if pin.is_activated { 1.0 } else { 1.0 };

                    for d in 0..count {
                        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                            rect: wgpu_api::Rect {
                                x: dash_base_x + d as f32 * (dash_width + gap),
                                y: dash_y,
                                width: dash_width,
                                height: dash_height,
                            },
                            color: wgpu_api::Color { r, g, b, a },
                            radius: 0.0,
                            stroke: None,
                            stroke_width: 0.0,
                        });
                    }
                }
            }
        }

        // Draw the dragged icon ghost at the cursor position.
        // The ghost follows the pointer while dragging, centered on the cursor,
        // with window count dashes below it. The pointer position is in screen
        // coordinates relative to the dock surface, so scale it to physical pixels.
        if is_dragging {
            if let Some(app_id) = dragged_app_id {
                if let Some(texture_id) = icon_textures.get(app_id) {
                    // Find the dragged item's properties for dashes
                    let dragged_pin = dock_state.pins.iter().find(|p| p.app_id == *app_id);
                    let ghost_size = icon_size_px;
                    let half = ghost_size / 2.0;

                    // Pointer position in dock-relative physical pixel coordinates.
                    // The pointer_position is in screen coordinates; the dock surface
                    // starts at (0,0) in its own coordinate space, so the pointer's
                    // dock-relative position is the pointer position itself, scaled.
                    let ptr_x = interaction.pointer_position.x as f32;
                    let ptr_y = interaction.pointer_position.y as f32;

                    // Center the ghost icon on the cursor
                    let ghost_x = ptr_x - half;
                    let ghost_y = ptr_y - half;

                    draw_commands.push(wgpu_api::DrawCommand::Image {
                        dest_rect: wgpu_api::Rect {
                            x: ghost_x,
                            y: ghost_y,
                            width: ghost_size,
                            height: ghost_size,
                        },
                        texture: *texture_id,
                        source_uv: wgpu_api::Rect {
                            x: 0.0,
                            y: 0.0,
                            width: 1.0,
                            height: 1.0,
                        },
                        tint: wgpu_api::Color {
                            r: 1.0,
                            g: 1.0,
                            b: 1.0,
                            a: 1.0,
                        },
                    });

                    // Draw window count dashes below the ghost icon
                    if let Some(pin) = dragged_pin {
                        let wc = pin.running_count.min(5);
                        if wc >= 1 {
                            let dash_config = dash_configs.get(wc - 1);
                            if let Some(&(count, dash_width, gap)) = dash_config {
                                let (r, g, b) = if pin.is_activated {
                                    (
                                        focused_dash_color.0 as f32 / 255.0,
                                        focused_dash_color.1 as f32 / 255.0,
                                        focused_dash_color.2 as f32 / 255.0,
                                    )
                                } else {
                                    (
                                        inactive_dash_color.0 as f32 / 255.0,
                                        inactive_dash_color.1 as f32 / 255.0,
                                        inactive_dash_color.2 as f32 / 255.0,
                                    )
                                };
                                // Dashes are centered below the ghost icon
                                let dash_base_x = ghost_x + dash_base_x_offset;
                                let ghost_dash_y = ghost_y + ghost_size + 2.0 * scale;
                                for d in 0..count {
                                    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                        rect: wgpu_api::Rect {
                                            x: dash_base_x + d as f32 * (dash_width + gap),
                                            y: ghost_dash_y,
                                            width: dash_width,
                                            height: dash_height,
                                        },
                                        color: wgpu_api::Color { r, g, b, a: 1.0 },
                                        radius: 0.0,
                                        stroke: None,
                                        stroke_width: 0.0,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        tracing::info!(
            "GPU render_to_dmabuf: {}x{} {} commands",
            phys_width,
            phys_height,
            draw_commands.len()
        );
        match gpu_renderer.render_to_dmabuf(phys_width, phys_height, &draw_commands) {
            Ok(export_info) => {
                tracing::info!("GPU render_to_dmabuf succeeded");
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
                        dock.surface.wl_surface().damage_buffer(
                            0,
                            0,
                            phys_width as i32,
                            phys_height as i32,
                        );
                        dock.surface.wl_surface().commit();
                        if is_animating {
                            dock.surface
                                .wl_surface()
                                .frame(qh, dock.surface.wl_surface().clone());
                        }
                        true
                    }
                    Err(e) => {
                        tracing::warn!("GPU dmabuf create_params failed: {:?}", e);
                        false
                    }
                }
            }
            Err(e) => {
                tracing::warn!("GPU render_to_dmabuf failed: {}", e);
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

    // CPU fallback: GPU rendering failed or GPU unavailable.
    // Fall back to Slint software renderer via SHM buffer.
    let stride = phys_width * 4;
    let (buffer, canvas) = match pool.create_buffer(
        phys_width as i32,
        phys_height as i32,
        stride as i32,
        wl_shm::Format::Argb8888,
    ) {
        Ok(bc) => bc,
        Err(e) => {
            tracing::warn!("Failed to allocate SHM buffer for CPU fallback: {}", e);
            return;
        }
    };

    // Use Slint renderer for CPU fallback
    if let Some(ref mut renderer) = slint_renderer {
        if let Some(app_id) = bounce_app_id {
            renderer.start_bounce(app_id, config);
        }
        if let Some(app_id) = click_app_id {
            renderer.start_click_animation(app_id, config);
        }
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
    if let Err(e) = buffer.attach_to(dock.surface.wl_surface()) {
        tracing::warn!("Buffer attach failed: {}", e);
        return;
    }
    dock.surface
        .wl_surface()
        .damage_buffer(0, 0, phys_width as i32, phys_height as i32);
    dock.surface.wl_surface().commit();
    dock.current_buffer = Some(buffer);
    if first_buffer {
        // First framebuffer committed
    }
}
