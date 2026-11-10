//! # Window List Popup
//!
//! Calculates popup dimensions, matches Wayland windows to PipeWire capture
//! streams, and provides the data for rendering the window list popup.
//!
//! ## Window-to-Stream Matching
//!
//! Each Wayland window is matched to a PipeWire capture stream so the popup
//! can show a live thumbnail. The matching strategy (in priority order):
//!
//! 1. **Persistent binding registry**: if the window ID has a bound stream
//!    index, use it directly (fastest, most reliable)
//! 2. **Window ID scan**: scan all streams for a matching `window_id` field
//! 3. **Title matching**: multi-strategy title comparison (exact, partial,
//!    prefix, word-based) as a last resort
//!
//! A `used_streams` set tracks already-assigned streams so two windows from
//! the same app don't both map to the same stream. This ensures each
//! PipeWire stream is assigned to at most one Wayland window.

use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::render::font::FontManager;
use crate::render::popup::{render_window_list_surface, CaptureFrameData};
use crate::services::pipewire_capture::PipewireCaptureService;
use crate::state::AppState;
use crate::state::WindowState;
use crate::types::{DockInstance, PopupSurface, WindowListState};

use super::draw::DrawFrameData;

/// Renders the window list popup for a hovered dock icon.
///
/// Matches Wayland windows to PipeWire streams, calculates popup geometry,
/// and renders the capture frame with window rows below it. Windows are
/// sorted by title for stable ordering that matches the click handler.
///
/// # Arguments
/// * `dock` - Dock instance with popup surface
/// * `qh` - Wayland queue handle for creating surfaces
/// * `frame` - Draw frame data with running windows and config
/// * `pool` - Shared memory pool for buffer allocation
/// * `compositor_state` - Wayland compositor state
/// * `subcompositor` - Optional subcompositor for popup subsurface
/// * `font_manager` - Font manager for text rendering
/// * `interaction` - Current pointer interaction state
/// * `window_list_state` - Popup state (open/closed, target app)
/// * `pipewire_capture` - Optional PipeWire capture service for streams
pub fn draw_window_list_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    interaction: &crate::types::InteractionState,
    window_list_state: &WindowListState,
    pipewire_capture: Option<&std::sync::Arc<PipewireCaptureService>>,
) {
    let show_window_list = window_list_state.is_open;
    if show_window_list {
        if let Some(target_app_id) = &window_list_state.target_app_id {
            // Primary: use foreign_toplevel windows filtered by the hovered app's app_id.
            // These are the actual windows belonging to the app.
            let app_windows: Vec<&WindowState> = frame
                .running_by_app
                .get(target_app_id)
                .cloned()
                .unwrap_or_default();

            // Match foreign_toplevel windows to PipeWire streams.
            // Each window gets a (WindowState, pipewire_stream_index) pair.
            // Track which streams have already been matched so two windows from the
            // same app don't both map to the same stream.
            // Priority: match by stable WindowId first (bound at capture time),
            // then fall back to title matching for streams without a bound ID.
            let matched_windows: Vec<(WindowState, Option<usize>)> = if !app_windows.is_empty() {
                pipewire_capture
                    .map(|service| {
                        // Track which streams have already been assigned to prevent
                        // multiple windows from matching the same PipeWire stream.
                        let mut used_streams: Vec<usize> = Vec::new();

                        let mut matched: Vec<(WindowState, Option<usize>)> = Vec::with_capacity(app_windows.len());
                        for w in &app_windows {
                            let window_id = w.id.0.protocol_id();
                            let stream_idx = if w.title.is_empty() {
                                None
                            } else {
                                // Primary: check persistent binding registry
                                // Once a stream is tied to a window, it stays tied
                                let bound = service.get_bound_stream(window_id);
                                if let Some(b) = bound {
                                    if !used_streams.contains(&b) {
                                        Some(b)
                                    } else {
                                        // Bound stream already used by another window,
                                        // fall back to title match
                                        service.find_stream_by_title(&w.title, &used_streams)
                                    }
                                } else {
                                    // No binding yet, fall back to title match
                                    service.find_stream_by_title(&w.title, &used_streams)
                                }
                            };

                            // Mark this stream as used so the next window can't claim it
                            if let Some(idx) = stream_idx {
                                used_streams.push(idx);
                            }
                            matched.push(((*w).clone(), stream_idx));
                        }

                        // Third pass: if multiple windows share the same title and one has
                        // a stream, the others can share it (e.g., 3 pcmanfm "dock" windows
                        // but only 2 PipeWire streams because portal can't distinguish them).
                        let title_to_stream: Vec<(String, usize)> = matched.iter()
                            .filter_map(|e| {
                                if e.1.is_some() && !e.0.title.is_empty() {
                                    Some((e.0.title.clone(), e.1.unwrap()))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        for entry in matched.iter_mut() {
                            if entry.1.is_none() && !entry.0.title.is_empty() {
                                if let Some(idx) = title_to_stream.iter()
                                    .find(|(t, _)| *t == entry.0.title)
                                    .map(|(_, i)| *i)
                                {
                                    entry.1 = Some(idx);
                                }
                            }
                        }

                        // Sort by title so the popup row order is stable and matches
                        // the click handler in click.rs which also sorts by title.
                        matched.sort_by(|(a, _), (b, _)| a.title.cmp(&b.title));
                        matched
                    })
                    .unwrap_or_else(|| {
                        let mut m: Vec<(WindowState, Option<usize>)> = app_windows.iter().map(|w| ((*w).clone(), None)).collect();
                        m.sort_by(|(a, _), (b, _)| a.title.cmp(&b.title));
                        m
                    })
            } else {
                Vec::new()
            };

            // Use matched foreign_toplevel windows if available, otherwise synthetic fallback.
            // Never fall back to showing all PipeWire streams — that would show unrelated windows.
            let use_matched = !matched_windows.is_empty();

            // Always show the popup if the app is in the dock
            if true {
                let total_apps = frame.apps_in_dock.len();
                let target_index = frame
                    .apps_in_dock
                    .iter()
                    .position(|id| id == target_app_id)
                    .unwrap_or(0);

                let scale = frame.dock_scale as f32;
                let icon_base_size = (frame.box_size as f32 * scale) as i32;
                let icon_spacing = (frame.spacing as f32 * scale) as i32;
                let total_icons_width = total_apps as i32 * icon_base_size
                    + (total_apps as i32 - 1).max(0) * icon_spacing;
                let start_x_offset = (frame.phys_width as i32 - total_icons_width) / 2;

                let anchor_x = if total_apps > 0 {
                    let pin_x =
                        start_x_offset + target_index as i32 * (icon_base_size + icon_spacing);
                    pin_x + icon_base_size / 2
                } else {
                    frame.phys_width as i32 / 2
                };
                let anchor_y = 0;

                // Build synthetic fallback when app has no foreign_toplevel windows
                let use_synthetic_fallback = !use_matched;
                let synthetic_windows: Vec<WindowState> = if use_synthetic_fallback {
                    let mut w = WindowState::new(crate::models::system::WindowId(
                        wayland_client::backend::ObjectId::null(),
                    ));
                    w.title = target_app_id
                        .split('-')
                        .map(|s| {
                            let mut c = s.chars();
                            match c.next() {
                                None => String::new(),
                                Some(ch) => ch.to_uppercase().to_string() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");
                    vec![w]
                } else {
                    Vec::new()
                };

                let item_count = if use_matched {
                    matched_windows.len() as i32
                } else {
                    synthetic_windows.len() as i32
                };
                // Use config values for popup dimensions
                let popup_width = frame.config.popup_width as i32;
                let popup_row_height = frame.config.popup_row_height as i32;
                let popup_frame_height = frame.config.popup_frame_height as i32;

                // Calculate the frame height based on the first matched stream's aspect ratio.
                // Use the first stream's dimensions so the popup geometry stays stable when
                // hovering between rows — only the capture image changes, not the layout.
                let frame_logical_h = if let Some(service) = pipewire_capture {
                    // Find the first stream index from matched windows to use for frame sizing
                    let first_stream_idx = matched_windows.iter()
                        .find_map(|(_, idx)| *idx)
                        .unwrap_or(0);
                    if let Some((orig_w, orig_h)) = service.get_stream_dimensions(first_stream_idx) {
                        let frame_border = 8.0; // 4px on each side
                        let frame_inner_w = popup_width as f32 - frame_border;
                        let aspect = orig_w as f32 / orig_h as f32;
                        let frame_h = frame_inner_w / aspect + frame_border;
                        frame_h as i32
                    } else {
                        popup_frame_height
                    }
                } else {
                    popup_frame_height
                };

                // Total popup height: frame + fixed row heights
                let scale = frame.dock_scale as f32;
                let total_logical_h = frame_logical_h + item_count * popup_row_height;
                let total_phys_h = (total_logical_h as f32 * scale).round() as i32;
                let total_phys_w = (popup_width as f32 * scale).round() as i32;

                // Position the popup above the dock
                let menu_y = anchor_y as i32 - total_phys_h;
                let max_x = (frame.phys_width as i32 - total_phys_w).max(0);
                let menu_x = (anchor_x as i32 - total_phys_w / 2).clamp(0, max_x);

                let geom = crate::geometry::popup::SurfaceGeometry {
                    x: menu_x,
                    y: menu_y,
                    phys_width: total_phys_w,
                    phys_height: total_phys_h,
                    logical_width: popup_width,
                    logical_height: total_logical_h,
                };

                let scale_i32 = frame.dock_scale.round() as i32;

                let popup = dock.window_list_popup.get_or_insert_with(|| {
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
                        current_app_id: Some(target_app_id.clone()),
                        position: (0, 0),
                        cached_content: Vec::new(),
                        cached_phys_width: 0,
                        cached_phys_height: 0,
            input_region_key: 0,
                    }
                });

                popup.current_app_id = Some(target_app_id.clone());

                let popup_x = (geom.x as f64 / frame.dock_scale).round() as i32;
                let popup_y = (geom.y as f64 / frame.dock_scale).round() as i32;
                popup.position = (popup_x, popup_y);
                popup.subsurface.set_position(popup_x, popup_y);

                popup.width = geom.logical_width as u32;
                popup.height = geom.logical_height as u32;

                if geom.phys_width > 0 && geom.phys_height > 0 {
                    // CPU rendering path — GPU path (dmabuf) has issues with FemtoVG image rendering
                    // for the PipeWire capture frame, so use CPU (shm) which works correctly.
                    let popup_x_logical = geom.x as f64 / frame.dock_scale;
                    let popup_y_logical = geom.y as f64 / frame.dock_scale;
                    let local_ptr_x =
                        (interaction.pointer_position.x - popup_x_logical).round() as i32;
                    let local_ptr_y =
                        (interaction.pointer_position.y - popup_y_logical).round() as i32;

                    let item_count_usize = if use_matched {
                        matched_windows.len()
                    } else {
                        synthetic_windows.len()
                    };
                    // Frame area is at the top of the popup, rows below it.
                    // Row height from config.
                    let item_h = frame.config.popup_row_height as i32;
                    let rows_logical_h = item_count_usize as i32 * item_h;
                    // Calculate hovered item; clamp to valid range so the frame
                    // keeps updating even when the cursor briefly leaves the popup.
                    let hovered_item = if local_ptr_x >= 0
                        && local_ptr_x < geom.logical_width as i32
                        && local_ptr_y >= frame_logical_h
                        && local_ptr_y < frame_logical_h + rows_logical_h
                    {
                        ((local_ptr_y - frame_logical_h) / item_h).clamp(0, item_count_usize.saturating_sub(1) as i32)
                    } else if local_ptr_y < frame_logical_h {
                        // Cursor is in the frame area — default to row 0
                        0
                    } else {
                        // Cursor is below the popup — clamp to last row
                        item_count_usize.saturating_sub(1) as i32
                    };
                    let close_size = (16.0 * frame.dock_scale as f32).round() as i32;
                    let close_x = geom.phys_width
                        - close_size
                        - (8.0 * frame.dock_scale as f32).round() as i32;
                    let _hovered_close = if hovered_item >= 0 {
                        let phys_item_h = (frame.config.popup_row_height as f32 * frame.dock_scale as f32).round() as i32;
                        let frame_phys_h = (frame_logical_h as f32 * frame.dock_scale as f32).round() as i32;
                        let top = frame_phys_h + hovered_item * phys_item_h;
                        let close_y = top + (phys_item_h - close_size) / 2;
                        let px = (local_ptr_x as f32 * frame.dock_scale as f32).round() as i32;
                        let py = (local_ptr_y as f32 * frame.dock_scale as f32).round() as i32;
                        px >= close_x
                            && px < close_x + close_size
                            && py >= close_y
                            && py < close_y + close_size
                    } else {
                        false
                    };

                    // Always render every frame — no content hash caching.
                    let expected_len = geom.phys_width as usize * geom.phys_height as usize * 4;

                    // Set the active row based on hovered item for PipeWire capture switching.
                    // Only set active row if we have a valid PipeWire stream match.
                    // Validate hovered_item is within bounds before accessing matched_windows.
                    let has_valid_stream = if use_matched
                        && (hovered_item as usize) < matched_windows.len()
                    {
                        matched_windows[hovered_item as usize].1.is_some()
                    } else {
                        false
                    };
                    if let Some(service) = pipewire_capture {
                        if has_valid_stream && use_matched
                            && (hovered_item as usize) < matched_windows.len()
                        {
                            let stream_idx = matched_windows[hovered_item as usize].1.unwrap();
                            service.set_active_row(stream_idx);
                        }
                    }

                    // Calculate the target frame display dimensions (inner area, excluding border).
                    // These are passed to get_active_frame() so it can downscale the capture
                    // to match the display size exactly, instead of using a fixed scale factor.
                    let frame_border = 8.0; // 4px on each side, matches popup.rs
                    let target_frame_w = (popup_width as f32 - frame_border).max(1.0) as u32;
                    let target_frame_h = (frame_logical_h as f32 - frame_border).max(1.0) as u32;

                    // Get the capture frame for the active row with original dimensions.
                    // Only show frame data if the hovered row has a valid PipeWire stream match.
                    let capture_frame_data = if has_valid_stream {
                        pipewire_capture.and_then(|service| {
                            let row = *service.active_row.lock();
                            let (orig_w, orig_h) = service.get_stream_dimensions(row)
                                .unwrap_or((0, 0));
                            service.get_active_frame(Some(target_frame_w), Some(target_frame_h))
                                .map(|(w, h, rgba)| {
                                    CaptureFrameData { width: w, height: h, rgba, original_width: orig_w, original_height: orig_h }
                                })
                        })
                    } else {
                        // No matching stream for this window — show a placeholder frame
                        // The renderer will draw a gray placeholder when capture_frame_data is None
                        None
                    };

                    popup.cached_content.resize(expected_len, 0);
                    let windows: Vec<WindowState> = if use_matched {
                        matched_windows.iter().map(|(w, _)| w.clone()).collect()
                    } else {
                        synthetic_windows.clone()
                    };
                    let windows_refs: Vec<&WindowState> = windows.iter().collect();
                    // Get the active row index for blue highlight on the active stream's row
                    let active_row_idx = pipewire_capture.as_ref().map_or(0, |service| {
                        *service.active_row.lock()
                    });
                    // Map stream index to window row index: find which matched window
                    // owns the active stream
                    let active_window_row = if use_matched {
                        matched_windows.iter().position(|(_, si)| {
                            *si == Some(active_row_idx)
                        }).unwrap_or(0)
                    } else {
                        0
                    };
                    render_window_list_surface(
                        &mut popup.cached_content,
                        &geom,
                        frame.dock_scale as f32,
                        &windows_refs,
                        font_manager,
                        local_ptr_x,
                        local_ptr_y,
                        capture_frame_data.as_ref(),
                        active_window_row,
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
                        .expect("Failed to allocate window list popup buffer");
                    if !super::copy_popup_content(canvas, &popup.cached_content) {
                        return;
                    }
                    // Fade animations are disabled, skip alpha application
                    // dock.window_list_fade.apply_alpha_to_canvas(canvas);

                    popup.surface.set_buffer_scale(scale_i32);
                    buffer
                        .attach_to(&popup.surface)
                        .expect("Failed to attach window list buffer");
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

                } else if let Some(popup) = dock.window_list_popup.take() {
                    popup.surface.attach(None, 0, 0);
                    popup.surface.commit();
                }
            } else if let Some(popup) = dock.window_list_popup.take() {
                popup.surface.attach(None, 0, 0);
                popup.surface.commit();
            }
        } else if let Some(popup) = dock.window_list_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
    } else if let Some(popup) = dock.window_list_popup.take() {
        popup.surface.attach(None, 0, 0);
        popup.surface.commit();
    }
}
