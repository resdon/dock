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

use wayland_client::Proxy;

use smithay_client_toolkit::dmabuf::DmabufState;
use smithay_client_toolkit::{
    compositor::CompositorState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::config::DockConfig;
use crate::models::system::WindowId;
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
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    let show_window_list = window_list_state.is_open;
    if show_window_list {
        if let Some(target_app_id) = &window_list_state.target_app_id {
            // Primary: use foreign_toplevel windows filtered by the hovered app's app_id.
            // These are the actual windows belonging to the app.
            let mut app_windows: Vec<&WindowState> = frame
                .running_by_app
                .get(target_app_id)
                .into_iter()
                .flatten()
                .filter_map(|id| frame.windows.get(&WindowId(id.clone())))
                .filter(|window| {
                    window.output_ids.is_empty()
                        || window
                            .output_ids
                            .iter()
                            .any(|output| &output.0 == frame.dock_output)
                })
                .collect();
            app_windows.sort_by(|a, b| a.title.cmp(&b.title));

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

                        let mut matched: Vec<(WindowState, Option<usize>)> =
                            Vec::with_capacity(app_windows.len());
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
                        let title_to_stream: Vec<(String, usize)> = matched
                            .iter()
                            .filter_map(|e| {
                                if e.1.is_some() && !e.0.title.is_empty() {
                                    if let Some(idx) = e.1 {
                                        Some((e.0.title.clone(), idx))
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                }
                            })
                            .collect();
                        for entry in matched.iter_mut() {
                            if entry.1.is_none() && !entry.0.title.is_empty() {
                                if let Some(idx) = title_to_stream
                                    .iter()
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
                        let mut m: Vec<(WindowState, Option<usize>)> =
                            app_windows.iter().map(|w| ((*w).clone(), None)).collect();
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
                    let first_stream_idx = matched_windows
                        .iter()
                        .find_map(|(_, idx)| *idx)
                        .unwrap_or(0);
                    if let Some((orig_w, orig_h)) = service.get_stream_dimensions(first_stream_idx)
                    {
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

                if dock.window_list_popup.is_none() {
                    let Some(sc) = subcompositor else {
                        tracing::warn!("Subcompositor not available; popup rendering degraded");
                        return;
                    };
                    let surf = compositor_state.create_surface(qh);
                    let sub = sc.get_subsurface(&surf, dock.surface.wl_surface(), qh, ());
                    sub.set_desync();

                    dock.window_list_popup = Some(PopupSurface {
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
                        // FIX (v1.5.5): Initialize last_content_hash to 0 so the first frame
                        // always renders. The hash will be updated after the first render.
                        last_content_hash: 0,
                        last_texture_generation: 0,
                    });
                }

                let Some(popup) = dock.window_list_popup.as_mut() else {
                    tracing::warn!("Window list popup not initialized");
                    return;
                };

                popup.current_app_id = Some(target_app_id.clone());

                let popup_x = (geom.x as f64 / frame.dock_scale).round() as i32;
                let popup_y = (geom.y as f64 / frame.dock_scale).round() as i32;
                popup.position = (popup_x, popup_y);
                popup.subsurface.set_position(popup_x, popup_y);

                popup.width = geom.logical_width as u32;
                popup.height = geom.logical_height as u32;

                if geom.phys_width > 0 && geom.phys_height > 0 {
                    // CPU rendering path — GPU path not yet integrated
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
                    let pointer_on_row = local_ptr_x >= 0
                        && local_ptr_x < geom.logical_width as i32
                        && local_ptr_y >= frame_logical_h
                        && local_ptr_y < frame_logical_h + rows_logical_h;
                    // Clamp the selection used for capture switching even outside a row.
                    let hovered_item = if local_ptr_x >= 0
                        && local_ptr_x < geom.logical_width as i32
                        && local_ptr_y >= frame_logical_h
                        && local_ptr_y < frame_logical_h + rows_logical_h
                    {
                        ((local_ptr_y - frame_logical_h) / item_h)
                            .clamp(0, item_count_usize.saturating_sub(1) as i32)
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
                        let phys_item_h = (frame.config.popup_row_height as f32
                            * frame.dock_scale as f32)
                            .round() as i32;
                        let frame_phys_h =
                            (frame_logical_h as f32 * frame.dock_scale as f32).round() as i32;
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

                    // Set the active row based on hovered item for PipeWire capture switching.
                    // Only set active row if we have a valid PipeWire stream match.
                    // Validate hovered_item is within bounds before accessing matched_windows.
                    let has_valid_stream =
                        if use_matched && (hovered_item as usize) < matched_windows.len() {
                            matched_windows[hovered_item as usize].1.is_some()
                        } else {
                            false
                        };
                    if let Some(service) = pipewire_capture {
                        if has_valid_stream
                            && use_matched
                            && (hovered_item as usize) < matched_windows.len()
                        {
                            if let Some(stream_idx) = matched_windows[hovered_item as usize].1 {
                                service.set_active_row(stream_idx);
                            }
                        }
                    }

                    // Calculate the target frame display dimensions (inner area, excluding border).
                    // These are passed to get_active_frame() so it can downscale the capture
                    // to match the display size exactly, instead of using a fixed scale factor.
                    let frame_border_val = 8.0; // 4px on each side, matches popup.rs
                    let target_frame_w = (popup_width as f32 - frame_border_val).max(1.0) as u32;
                    let target_frame_h = (frame_logical_h as f32 - frame_border_val).max(1.0) as u32;

                    // Capture frame data will be fetched below when rendering.
                    let mut capture_frame_data: Option<CaptureFrameData> = None;

                    // GPU rendering path: build draw commands and render via DMA-BUF
                    let windows: Vec<WindowState> = if use_matched {
                        matched_windows.iter().map(|(w, _)| w.clone()).collect()
                    } else {
                        synthetic_windows.clone()
                    };

                    // FIX (v1.5.5): Compute a content hash so we can skip rendering when nothing
                    // changed. The hash combines: window count, window titles, hovered item,
                    // pointer position, and the PipeWire capture generation counter.
                    //
                    // Before this fix, the popup rendered unconditionally every frame, rebuilding
                    // draw commands, replacing GPU textures, creating new DMABUF buffers, and
                    // committing the Wayland surface — even when the underlying window hadn't
                    // changed. This wasted CPU, GPU, and memory bandwidth.
                    //
                    // Including the pointer position ensures the highlight updates when the user
                    // moves the mouse, even if the hovered row and frame generation are unchanged.
                    // Without the pointer position, the highlight would freeze when the captured
                    // window was static and the generation counter did not increment.
                    let mut content_hash: u64 = windows.len() as u64;
                    for w in &windows {
                        for b in w.title.bytes() {
                            content_hash = content_hash.wrapping_mul(31).wrapping_add(b as u64);
                        }
                    }
                    content_hash = content_hash.wrapping_mul(37).wrapping_add(hovered_item as u64);

                    // FIX (v1.5.5): Include the rounded pointer position so that mouse movement
                    // within the popup always triggers a re-render. This prevents the highlight
                    // from freezing when the captured window is static and the generation counter
                    // does not increment.
                    //
                    // The pointer position is already rounded to i32 (local_ptr_x, local_ptr_y),
                    // so sub-pixel movements within the same pixel don't trigger renders.
                    content_hash = content_hash.wrapping_mul(43)
                        .wrapping_add(local_ptr_x as u64)
                        .wrapping_add((local_ptr_y as i64) as u64);

                    // FIX (v1.5.5): Include the PipeWire capture generation counter so we detect
                    // when the underlying window's content has changed, even if the title hasn't.
                    // The generation counter is incremented by the stream thread each time a new
                    // frame buffer arrives. When the generation matches the previous frame's
                    // generation, the content hash will match and rendering is skipped.
                    //
                    // If the generation is unavailable (mutex timeout), we force a re-render
                    // rather than risking a stale hash that skips rendering forever. This was
                    // the root cause of the "popup freezes" bug: when try_lock() failed, the
                    // generation read returned None, and without this fallback the popup would
                    // skip rendering on a stale hash indefinitely.
                    let (capture_generation, gen_available) = if has_valid_stream
                        && use_matched
                        && (hovered_item as usize) < matched_windows.len()
                    {
                        if let Some(stream_idx) = matched_windows[hovered_item as usize].1 {
                            if let Some(gen) = pipewire_capture
                                .and_then(|service| service.get_stream_generation(stream_idx))
                            {
                                (gen, true)
                            } else {
                                // Generation unavailable — force re-render
                                (0, false)
                            }
                        } else {
                            (0, true)
                        }
                    } else {
                        (0, true)
                    };
                    content_hash = content_hash.wrapping_mul(41).wrapping_add(capture_generation);

                    // FIX (v1.5.5): Skip rendering if the content hash matches the previous frame.
                    // This avoids rebuilding draw commands, replacing GPU textures, creating
                    // new DMABUF buffers, and committing the surface when nothing changed.
                    //
                    // But always render when the generation counter was unavailable, to avoid
                    // freezing the popup on a stale hash. This safeguard ensures the popup
                    // never freezes even under heavy mutex contention.
                    if gen_available && popup.last_content_hash == content_hash {
                        return;
                    }
                    popup.last_content_hash = content_hash;

                    let expected_len = geom.phys_width as usize * geom.phys_height as usize * 4;

                    // Build draw commands for the window list popup
                    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
                    let config = frame.config;
                    let scale = frame.dock_scale as f32;
                    let width = geom.phys_width as f32;
                    let _height = geom.phys_height as f32;
                    let item_count = windows.len();

                    // Parse colors from config
                    let text_color = DockConfig::parse_hex_color(&config.popup_text_color);
                    let border_color = DockConfig::parse_hex_color(&config.popup_border_color);
                    let close_button_color =
                        DockConfig::parse_hex_color(&config.popup_close_button_color);

                    // Calculate frame and rows area
                    let frame_border = config.popup_frame_border as f32 * scale;
                    let frame_phys_h = frame_logical_h as f32 * scale;
                    let rows_area_y = frame_phys_h;
                    let phys_item_h = config.popup_row_height as f32 * scale;

                    // FIX (v1.5.5): Draw window rows first, before the expensive PipeWire frame
                    // fetch. The dashed perimeter highlight must render immediately when switching
                    // hovered rows — it should not be delayed by frame mutex locks, RGBA
                    // conversion, or GPU texture upload.
                    //
                    // Before this fix, the PipeWire frame fetch ran before the window rows were
                    // drawn. When switching hovered rows, the highlight was delayed by the time
                    // it took to acquire mutex locks, convert the frame to RGBA, and upload the
                    // texture to the GPU — causing a visible 1-second lag on hover.
                    for (i, window) in windows.iter().enumerate() {
                        let item_top = rows_area_y + (i as f32 * phys_item_h);
                        let item_h = phys_item_h;

                        if pointer_on_row && i == hovered_item as usize {
                            let highlight_color = wgpu_api::Color {
                                r: border_color.0 as f32 / 255.0,
                                g: border_color.1 as f32 / 255.0,
                                b: border_color.2 as f32 / 255.0,
                                a: 1.0,
                            };
                            let thickness = scale.max(1.0);
                            let dash_length = (config.popup_dash_gap.max(1) as f32 * scale)
                                .round()
                                .max(1.0);
                            let dash_period = dash_length * 2.0;
                            let row_left = 1.0;
                            let row_right = (width - thickness - 1.0).max(row_left);
                            let top = item_top + 1.0;
                            let bottom = (item_top + item_h - thickness - 1.0).max(top);

                            let mut x = row_left;
                            while x <= row_right {
                                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                    rect: wgpu_api::Rect {
                                        x,
                                        y: top,
                                        width: dash_length.min(row_right + thickness - x),
                                        height: thickness,
                                    },
                                    color: highlight_color,
                                    radius: 0.0,
                                    stroke: None,
                                    stroke_width: 0.0,
                                });
                                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                    rect: wgpu_api::Rect {
                                        x,
                                        y: bottom,
                                        width: dash_length.min(row_right + thickness - x),
                                        height: thickness,
                                    },
                                    color: highlight_color,
                                    radius: 0.0,
                                    stroke: None,
                                    stroke_width: 0.0,
                                });
                                x += dash_period;
                            }

                            let mut y = top + thickness;
                            while y < bottom {
                                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                    rect: wgpu_api::Rect {
                                        x: row_left,
                                        y,
                                        width: thickness,
                                        height: dash_length.min(bottom - y),
                                    },
                                    color: highlight_color,
                                    radius: 0.0,
                                    stroke: None,
                                    stroke_width: 0.0,
                                });
                                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                    rect: wgpu_api::Rect {
                                        x: row_right,
                                        y,
                                        width: thickness,
                                        height: dash_length.min(bottom - y),
                                    },
                                    color: highlight_color,
                                    radius: 0.0,
                                    stroke: None,
                                    stroke_width: 0.0,
                                });
                                y += dash_period;
                            }
                        }

                        // Draw window title text
                        let text_y = item_top
                            + item_h / 2.0
                            - 1.0 * scale
                            + (config.popup_font_size as f32 * 2.0 * scale) / 2.0;
                        let text_x = config.popup_text_padding as f32 * scale;

                        // Truncate title so it doesn't overflow the close button
                        let truncate_len = config.popup_text_truncate as usize;
                        let title = if window.title.chars().count() > truncate_len {
                            format!(
                                "{}...",
                                window.title
                                    .chars()
                                    .take(truncate_len.saturating_sub(3))
                                    .collect::<String>()
                            )
                        } else {
                            window.title.clone()
                        };

                        draw_commands.push(wgpu_api::DrawCommand::Text {
                            position: wgpu_api::Point::new(text_x, text_y),
                            content: title,
                            font_size: config.popup_font_size as f32 * 2.0 * scale,
                            color: wgpu_api::Color {
                                r: text_color.0 as f32 / 255.0,
                                g: text_color.1 as f32 / 255.0,
                                b: text_color.2 as f32 / 255.0,
                                a: 1.0,
                            },
                            align: 0.0,
                        });

                        // Draw close button (X)
                        let close_btn_size = config.popup_close_button_size as f32 * scale;
                        let close_margin = config.popup_close_button_margin as f32 * scale;
                        let close_btn_x = width - close_btn_size - close_margin;
                        let close_btn_y = item_top + (item_h - close_btn_size) / 2.0;
                        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                            rect: wgpu_api::Rect {
                                x: close_btn_x,
                                y: close_btn_y,
                                width: close_btn_size,
                                height: close_btn_size,
                            },
                            color: wgpu_api::Color {
                                r: close_button_color.0 as f32 / 255.0,
                                g: close_button_color.1 as f32 / 255.0,
                                b: close_button_color.2 as f32 / 255.0,
                                a: 0.5,
                            },
                            radius: close_btn_size / 4.0,
                            stroke: None,
                            stroke_width: 0.0,
                        });
                    }

                    // Draw horizontal dashed separators between rows
                    let separator_color_f = wgpu_api::Color {
                        r: border_color.0 as f32 / 255.0,
                        g: border_color.1 as f32 / 255.0,
                        b: border_color.2 as f32 / 255.0,
                        a: 1.0,
                    };
                    let dash_len = config.popup_dash_gap.max(1) as f32;
                    let dash_period = dash_len * 2.0;
                    let line_thickness = scale.max(1.0);
                    for i in 1..item_count {
                        let line_y = rows_area_y + (i as f32 * phys_item_h);
                        let mut x = 0.0f32;
                        while x < width {
                            let seg_w = dash_len.min(width - x);
                            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                                rect: wgpu_api::Rect {
                                    x,
                                    y: line_y,
                                    width: seg_w,
                                    height: line_thickness,
                                },
                                color: separator_color_f,
                                radius: 0.0,
                                stroke: None,
                                stroke_width: 0.0,
                            });
                            x += dash_period;
                        }
                    }

                    // FIX (v1.5.5): Draw PipeWire capture frame as a textured quad if available.
                    // Only fetch and upload a new GPU texture when the PipeWire generation counter
                    // has actually changed. The content hash includes the pointer position, so any
                    // mouse movement triggers a re-render — but the underlying frame hasn't changed,
                    // so we reuse the cached GPU texture instead of re-uploading it.
                    //
                    // Before this fix, `replace_cached_texture_from_pixels()` was called on every
                    // redraw, even when only the pointer position changed. This wasted GPU bandwidth
                    // uploading the same frame data repeatedly.
                    if let Some(ref mut renderer) = gpu_context {
                        let active_stream_idx = pipewire_capture
                            .map(|service| *service.active_row.lock())
                            .unwrap_or(0);
                        let cache_key = format!(
                            "window-list-capture:{:?}:{}",
                            dock.output.id(),
                            active_stream_idx
                        );

                        // Check if the PipeWire frame has actually changed since the last texture
                        // upload. If the generation is the same, skip the expensive frame fetch,
                        // RGBA conversion, and GPU texture upload — just reuse the cached texture.
                        let generation_changed = if has_valid_stream
                            && use_matched
                            && (hovered_item as usize) < matched_windows.len()
                        {
                            if let Some(stream_idx) = matched_windows[hovered_item as usize].1 {
                                if let Some(gen) = pipewire_capture
                                    .and_then(|service| service.get_stream_generation(stream_idx))
                                {
                                    gen != popup.last_texture_generation
                                } else {
                                    // Generation unavailable — assume changed to avoid stale texture
                                    true
                                }
                            } else {
                                false
                            }
                        } else {
                            false
                        };

                        let texture_id = if generation_changed {
                            // Fetch the latest frame data and upload it, replacing any cached texture.
                            // FIX (v1.5.5): Try BGRx direct upload first to eliminate two CPU
                            // conversions per frame (BGRx→RGBA and RGBA→BGRA). The GPU downscales
                            // via linear sampling by adjusting UV coordinates. Falls back to the
                            // RGBA conversion path for non-BGRx formats.
                            let mut bgfx_uploaded = false;
                            if has_valid_stream {
                                if let Some(service) = pipewire_capture.as_ref() {
                                    let row = *service.active_row.lock();
                                    let (orig_w, orig_h) =
                                        service.get_stream_dimensions(row).unwrap_or((0, 0));
                                    // Try raw BGRx upload — no CPU conversion needed
                                    if let Some((w, h, bgrx)) = service.get_active_frame_bgfx() {
                                        capture_frame_data = Some(CaptureFrameData {
                                            width: w,
                                            height: h,
                                            rgba: bgrx.clone(),
                                            original_width: w,
                                            original_height: h,
                                        });
                                        // FIX (v1.5.5): Upload BGRx bytes directly — no CPU conversion
                                        if renderer.replace_cached_texture_from_bgfx(
                                            &cache_key,
                                            &bgrx,
                                            w as u32,
                                            h as u32,
                                        ).is_some() {
                                            bgfx_uploaded = true;
                                        }
                                    } else {
                                        // Fall back to RGBA conversion for non-BGRx formats
                                        let frame_data = service
                                            .get_active_frame(Some(target_frame_w), Some(target_frame_h))
                                            .map(|(w, h, rgba)| CaptureFrameData {
                                                width: w,
                                                height: h,
                                                rgba,
                                                original_width: orig_w,
                                                original_height: orig_h,
                                            });
                                        capture_frame_data = frame_data;
                                    }
                                }
                            }
                            if !bgfx_uploaded {
                                // Fall back to RGBA upload path if BGRx upload didn't happen
                                capture_frame_data.as_ref().and_then(|fd| {
                                    renderer.replace_cached_texture_from_pixels(
                                        &cache_key,
                                        &fd.rgba,
                                        fd.width as u32,
                                        fd.height as u32,
                                    )
                                })
                            } else {
                                // BGRx was uploaded — get the cached texture
                                renderer.cached_texture(&cache_key)
                            }
                        } else {
                            // Reuse the existing cached texture — frame hasn't changed.
                            renderer.cached_texture(&cache_key)
                        };
                        // Update the last texture generation after a successful upload.
                        if generation_changed {
                            if let Some(stream_idx) = matched_windows.get(hovered_item as usize)
                                .and_then(|w| w.1)
                            {
                                if let Some(gen) = pipewire_capture
                                    .and_then(|service| service.get_stream_generation(stream_idx))
                                {
                                    popup.last_texture_generation = gen;
                                }
                            }
                        }
                        if let Some(texture_id) = texture_id {
                            let frame_inner_w = width - frame_border * 2.0;
                            let frame_inner_h = frame_phys_h - frame_border * 2.0;
                            draw_commands.push(wgpu_api::DrawCommand::Image {
                                dest_rect: wgpu_api::Rect {
                                    x: frame_border,
                                    y: frame_border / 2.0,
                                    width: frame_inner_w,
                                    height: frame_inner_h,
                                },
                                texture: texture_id,
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
                    }

                    // FIX (v1.5.5): Use pooled DMABUF texture instead of allocating a new one each frame.
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
                                                let region =
                                                    compositor_state.wl_compositor().create_region(qh, ());
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
                        popup.cached_content.resize(expected_len, 0);
                        let windows_refs: Vec<&WindowState> = windows.iter().collect();
                        let active_row_idx = pipewire_capture
                            .as_ref()
                            .map_or(0, |service| *service.active_row.lock());
                        let active_window_row = if use_matched {
                            matched_windows
                                .iter()
                                .position(|(_, si)| *si == Some(active_row_idx))
                                .unwrap_or(0)
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

                        let (buffer, canvas) = match pool.create_buffer(
                            geom.phys_width,
                            geom.phys_height,
                            geom.phys_width * 4,
                            wl_shm::Format::Argb8888,
                        ) {
                            Ok(bc) => bc,
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to allocate window list popup buffer: {}",
                                    e
                                );
                                return;
                            }
                        };
                        if !super::copy_popup_content(canvas, &popup.cached_content) {
                            return;
                        }

                        popup.surface.set_buffer_scale(scale_i32);
                        if let Err(e) = buffer.attach_to(&popup.surface) {
                            tracing::warn!("Failed to attach window list buffer: {}", e);
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
