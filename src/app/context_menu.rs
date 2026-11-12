//! Context menu rendering.
//!
//! Builds and renders the right-click context menu popup with items
//! for Launch, Focus, Unpin, and Quit actions.

use wayland_client::Proxy;

use smithay_client_toolkit::dmabuf::DmabufState;
use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::state::AppState;
use crate::types::{DockInstance, MenuItemType, MenuState, PopupSurface};

use super::draw::DrawFrameData;

pub fn draw_context_menu_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut crate::render::font::FontManager,
    menu_state: &MenuState,
    interaction: &crate::types::InteractionState,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    let show_menu = !dock.menu_fade.is_fully_hidden() && !menu_state.items.is_empty();

    if show_menu {
        let total_apps = frame.apps_in_dock.len();

        // Copy the identifiers before mutably borrowing `context_menu_popup`.
        // Keeping an `as_ref()` borrow alive across `get_or_insert_with()` causes
        // E0502 and is unnecessary because these are small owned identifiers.
        let popup_owner = dock
            .context_menu_popup
            .as_ref()
            .and_then(|popup| popup.current_app_id.clone());
        let target_app_id = menu_state.target_app_id.clone().or(popup_owner);

        let target_index = target_app_id
            .as_ref()
            .and_then(|app_id| frame.apps_in_dock.iter().position(|id| id == app_id))
            .unwrap_or(0);

        let scale = frame.dock_scale as f32;
        let icon_base_size = (frame.box_size as f32 * scale) as i32;
        let icon_spacing = (frame.spacing as f32 * scale) as i32;
        let total_icons_width =
            total_apps as i32 * icon_base_size + (total_apps as i32 - 1).max(0) * icon_spacing;
        let start_x_offset = (frame.phys_width as i32 - total_icons_width) / 2;

        let (anchor_x, anchor_y) = if let Some((x, y)) = menu_state.anchor_override {
            (
                (x as f64 * frame.dock_scale).round() as i32,
                (y as f64 * frame.dock_scale).round() as i32,
            )
        } else {
            let anchor_x = if total_apps > 0 {
                let pin_x = start_x_offset + target_index as i32 * (icon_base_size + icon_spacing);
                pin_x + icon_base_size / 2
            } else {
                frame.phys_width as i32 / 2
            };
            let anchor_y = 0;
            (anchor_x, anchor_y)
        };

        let item_count = menu_state.items.len() as i32;
        let phys_dock_height = (frame.dock_height as f64 * frame.dock_scale).round() as i32;

        let geom = PopupGeometry::compute_bounds(
            PopupType::ContextMenu,
            anchor_x,
            anchor_y,
            frame.phys_width as i32,
            phys_dock_height,
            item_count,
            frame.dock_scale as f32,
        );

        let scale_i32 = frame.dock_scale.round() as i32;

        if dock.context_menu_popup.is_none() {
            let Some(sc) = subcompositor else {
                tracing::warn!("Subcompositor not available; context menu rendering degraded");
                return;
            };
            let surf = compositor_state.create_surface(qh);
            let sub = sc.get_subsurface(&surf, dock.surface.wl_surface(), qh, ());
            sub.set_desync();
            dock.context_menu_popup = Some(PopupSurface {
                subsurface: sub,
                surface: surf,
                current_buffer: None,
                width: 0,
                height: 0,
                current_app_id: None,
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
        let Some(popup) = dock.context_menu_popup.as_mut() else {
            tracing::warn!("Context menu popup not initialized");
            return;
        };

        if let Some(tray_key) = menu_state.target_tray_item.as_ref() {
            popup.current_app_id = Some(format!("tray:{tray_key}"));
        } else if let Some(app_id) = menu_state.target_app_id.clone() {
            popup.current_app_id = Some(app_id);
        }

        let popup_x = (geom.x as f64 / frame.dock_scale).round() as i32;
        let popup_y = (geom.y as f64 / frame.dock_scale).round() as i32;
        popup.position = (popup_x, popup_y);
        popup.subsurface.set_position(popup_x, popup_y);

        popup.width = geom.logical_width as u32;
        popup.height = geom.logical_height as u32;

        if geom.phys_width > 0 && geom.phys_height > 0 {
            // GPU rendering path: build draw commands and render via DMA-BUF
            let popup_x_logical = geom.x as f64 / frame.dock_scale;
            let popup_y_logical = geom.y as f64 / frame.dock_scale;
            let local_ptr_x = (interaction.pointer_position.x - popup_x_logical).round() as i32;
            let local_ptr_y = (interaction.pointer_position.y - popup_y_logical).round() as i32;
            let item_h = (geom.logical_height / item_count.max(1)).max(1) as i32;
            let hovered_item = if local_ptr_x >= 0
                && local_ptr_x < geom.logical_width as i32
                && local_ptr_y >= 0
                && local_ptr_y < geom.logical_height as i32
            {
                (local_ptr_y / item_h).clamp(0, item_count.saturating_sub(1))
            } else {
                -1
            };

            // Build draw commands for the context menu
            let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
            let config = frame.config;
            let scale = frame.dock_scale as f32;
            let width = geom.phys_width as f32;
            let height = geom.phys_height as f32;
            let item_count_usize = menu_state.items.len();

            // Parse colors from config
            let text_color = DockConfig::parse_hex_color(&config.popup_text_color);
            let border_color = DockConfig::parse_hex_color(&config.popup_border_color);
            let focus_color = DockConfig::parse_hex_color(&config.popup_focus_color);
            let unpin_color = DockConfig::parse_hex_color(&config.popup_unpin_color);
            let launch_color = DockConfig::parse_hex_color(&config.popup_launch_color);
            let quit_color = DockConfig::parse_hex_color(&config.popup_quit_color);

            for (i, item) in menu_state.items.iter().enumerate() {
                let item_top = (i as f32 * height) / item_count_usize as f32;
                let next_item_top = ((i as f32 + 1.0) * height) / item_count_usize as f32;
                let item_h = next_item_top - item_top;

                // Draw border highlight on hovered row
                if hovered_item == i as i32 {
                    let color = wgpu_api::Color {
                        r: border_color.0 as f32 / 255.0,
                        g: border_color.1 as f32 / 255.0,
                        b: border_color.2 as f32 / 255.0,
                        a: 1.0,
                    };
                    let dash_size = (config.popup_dash_gap.max(1) as f32 * scale)
                        .round()
                        .max(1.0);
                    let dash_period = dash_size * 2.0;
                    let thickness = scale.max(1.0);

                    let mut x = 0.0;
                    while x < width {
                        let segment = dash_size.min(width - x);
                        for y in [item_top, item_top + item_h - thickness] {
                            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                rect: wgpu_api::Rect::new(x, y, segment, thickness),
                                color,
                                radius: 0.0,
                                stroke: None,
                                stroke_width: 0.0,
                            });
                        }
                        x += dash_period;
                    }

                    let mut y = item_top + thickness;
                    while y < item_top + item_h - thickness {
                        let segment = dash_size.min(item_top + item_h - thickness - y);
                        for x in [0.0, (width - thickness).max(0.0)] {
                            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                rect: wgpu_api::Rect::new(x, y, thickness, segment),
                                color,
                                radius: 0.0,
                                stroke: None,
                                stroke_width: 0.0,
                            });
                        }
                        y += dash_period;
                    }
                }

                // Text color based on menu item type
                let color = match &item.item_type {
                    MenuItemType::CloseApp => quit_color,
                    MenuItemType::LaunchNew => launch_color,
                    MenuItemType::Focus => focus_color,
                    MenuItemType::TogglePin => unpin_color,
                    _ => text_color,
                };

                // Text position (centered vertically in the item)
                let text_y =
                    item_top + item_h / 2.0 + config.context_menu_text_offset_y as f32 * scale;
                let text_x = config.popup_text_padding as f32 * scale;

                // Add text draw command (size controlled by config)
                draw_commands.push(wgpu_api::DrawCommand::Text {
                    position: wgpu_api::Point::new(text_x, text_y),
                    content: item.label.clone(),
                    font_size: config.popup_font_size as f32 * config.context_menu_text_scale * scale,
                    color: wgpu_api::Color {
                        r: color.0 as f32 / 255.0,
                        g: color.1 as f32 / 255.0,
                        b: color.2 as f32 / 255.0,
                        a: 1.0,
                    },
                    align: 0.0,
                });
            }

            // FIX (v1.5.5): Use pooled DMABUF texture
            let gpu_success = if let Some(ref mut renderer) = gpu_context {
                if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
                    match dmabuf_pool.acquire_texture(
                        adapter, device, qh, dmabuf_state,
                        geom.phys_width as u32, geom.phys_height as u32,
                    ) {
                        Ok((texture, export_info)) => {
                            if renderer.render_to_texture(
                                &texture, geom.phys_width as u32, geom.phys_height as u32,
                                1.0, &draw_commands,
                            ).is_err() {
                                false
                            } else {
                                match crate::render::dmabuf_export::create_buffer_from_export(
                                    dmabuf_state, qh, &export_info,
                                ) {
                                    Ok(wl_buffer) => {
                                        dmabuf_pool.set_buffer_id(wl_buffer.id());
                                        popup.surface.set_buffer_scale(scale_i32);
                                        popup.surface.attach(Some(&wl_buffer), 0, 0);
                                        popup.surface.damage_buffer(
                                            0, 0, geom.phys_width, geom.phys_height,
                                        );
                                        let region = compositor_state.wl_compositor().create_region(qh, ());
                                        region.add(0, 0, geom.logical_width, geom.logical_height);
                                        popup.surface.set_input_region(Some(&region));
                                        region.destroy();
                                        popup.surface.commit();
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

            // Fallback: if GPU rendering failed, use CPU rendering
            if !gpu_success {
                use crate::render::popup::render_context_menu_surface;
                let expected_len = geom.phys_width as usize * geom.phys_height as usize * 4;
                popup.cached_content.resize(expected_len, 0);
                render_context_menu_surface(
                    &mut popup.cached_content,
                    &geom,
                    frame.dock_scale as f32,
                    menu_state,
                    font_manager,
                    local_ptr_x,
                    local_ptr_y,
                    frame.config,
                );
                popup.cached_phys_width = geom.phys_width;
                popup.cached_phys_height = geom.phys_height;

                let (buffer, canvas) = match pool.create_buffer(
                    geom.phys_width,
                    geom.phys_height,
                    geom.phys_width * 4,
                    wl_shm::Format::Argb8888,
                ) {
                    Ok(bc) => bc,
                    Err(e) => {
                        tracing::warn!("Failed to allocate context menu popup buffer: {}", e);
                        return;
                    }
                };
                if !super::copy_popup_content(canvas, &popup.cached_content) {
                    return;
                }

                popup.surface.set_buffer_scale(scale_i32);
                if let Err(e) = buffer.attach_to(&popup.surface) {
                    tracing::warn!("Failed to attach context menu buffer: {}", e);
                    return;
                }
                popup
                    .surface
                    .damage_buffer(0, 0, geom.phys_width, geom.phys_height);

                let region = compositor_state.wl_compositor().create_region(qh, ());
                region.add(0, 0, geom.logical_width, geom.logical_height);
                popup.surface.set_input_region(Some(&region));
                region.destroy();

                popup.surface.commit();
                popup.current_buffer = Some(buffer);
            }
        } else if let Some(popup) = dock.context_menu_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
    } else if let Some(popup) = dock.context_menu_popup.take() {
        popup.surface.attach(None, 0, 0);
        popup.surface.commit();
    }
}
