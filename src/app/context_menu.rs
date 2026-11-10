//! Context menu rendering.
//!
//! Builds and renders the right-click context menu popup with items
//! for Launch, Focus, Unpin, and Quit actions.

use smithay_client_toolkit::dmabuf::DmabufState;
use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::geometry::popup::{PopupGeometry, PopupType};
use crate::render::font::FontManager;
use crate::render::popup::render_context_menu_surface;
use crate::state::AppState;
use crate::types::{DockInstance, MenuState, PopupSurface};

use super::draw::DrawFrameData;

pub fn draw_context_menu_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    menu_state: &MenuState,
    interaction: &crate::types::InteractionState,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
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

        let popup = dock.context_menu_popup.get_or_insert_with(|| {
            let surf = compositor_state.create_surface(qh);
            let sub = subcompositor
                .expect("Subcompositor global not bound")
                .get_subsurface(&surf, dock.surface.wl_surface(), qh, ());
            sub.set_desync();

            PopupSurface {
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
            }
        });

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
            // CPU rendering path — uses same text rendering as window list popup (smooth anti-aliased text)
            let popup_x_logical = geom.x as f64 / frame.dock_scale;
            let popup_y_logical = geom.y as f64 / frame.dock_scale;
            let local_ptr_x = (interaction.pointer_position.x - popup_x_logical).round() as i32;
            let local_ptr_y = (interaction.pointer_position.y - popup_y_logical).round() as i32;
            let item_h = (geom.logical_height / item_count.max(1)).max(1) as i32;
            let _hovered_item = if local_ptr_x >= 0
                && local_ptr_x < geom.logical_width as i32
                && local_ptr_y >= 0
                && local_ptr_y < geom.logical_height as i32
            {
                (local_ptr_y / item_h).clamp(0, item_count.saturating_sub(1))
            } else {
                -1
            };

            popup
                .cached_content
                .resize(geom.phys_width as usize * geom.phys_height as usize * 4, 0);
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

            let (buffer, canvas) = pool
                .create_buffer(
                    geom.phys_width,
                    geom.phys_height,
                    geom.phys_width * 4,
                    wl_shm::Format::Argb8888,
                )
                .expect("Failed to allocate context menu popup buffer");
            if !super::copy_popup_content(canvas, &popup.cached_content) {
                return;
            }
            dock.menu_fade.apply_alpha_to_canvas(canvas);

            popup.surface.set_buffer_scale(scale_i32);
            buffer
                .attach_to(&popup.surface)
                .expect("Failed to attach context menu buffer");
            popup
                .surface
                .damage_buffer(0, 0, geom.phys_width, geom.phys_height);

            // wl_region coordinates are in the surface's logical coordinate
            // space. Buffer dimensions are physical pixels and must not be used
            // for input regions on a scaled surface.
            let region = compositor_state.wl_compositor().create_region(qh, ());
            region.add(0, 0, geom.logical_width, geom.logical_height);
            popup.surface.set_input_region(Some(&region));
            region.destroy();

            popup.surface.commit();
            popup.current_buffer = Some(buffer);
        } else if let Some(popup) = dock.context_menu_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
    } else if let Some(popup) = dock.context_menu_popup.take() {
        popup.surface.attach(None, 0, 0);
        popup.surface.commit();
    }
}
