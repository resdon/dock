//! Audio popup panel.
//!
//! Expanded audio mixer popup with volume sliders and device selection.
//! Renders via wgpu GPU using DrawCommands and DMA-BUF.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface,
};
use wayland_client::{Proxy, QueueHandle};

use crate::models::{AudioAction, AudioSliderTarget, AudioSnapshot, SystemBarSnapshot, TrayItem};
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

// Layout constants for audio popup hit-testing
pub const AUDIO_POPUP_WIDTH: i32 = 180;
pub const AUDIO_POPUP_HEIGHT: i32 = 240;
const OUTPUT_SLIDER_Y: i32 = 43;
const OUTPUT_MUTE_Y: i32 = 24;
const OUTPUT_DEVICE_Y: i32 = 52;
const INPUT_SLIDER_Y: i32 = 94;
const INPUT_MUTE_Y: i32 = 75;
const INPUT_DEVICE_Y: i32 = 103;
const DEVICE_ROW_HEIGHT: i32 = 16;
const STREAMS_HEADER_Y: i32 = 137;
const STREAM_ROW_START_Y: i32 = 143;
const STREAM_ROW_HEIGHT: i32 = 31;
const MIXER_BUTTON_Y: i32 = 217;
const MINIMIZE_BUTTON_X: i32 = 153;
const MINIMIZE_BUTTON_Y: i32 = 4;
const MINIMIZE_BUTTON_WIDTH: i32 = 20;
const MINIMIZE_BUTTON_HEIGHT: i32 = 16;

// Audio player bar layout constants
const COMPACT_X: i32 = 0;
const COMPACT_WIDTH: i32 = 120;
const PLAYER_WIDTH: i32 = 240;
const PLAYER_HEIGHT: i32 = 60;

fn player_position() -> (i32, i32) {
    (COMPACT_X, 0)
}

use super::draw::DrawFrameData;

pub fn draw_audio_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _frame: &DrawFrameData<'_>,
    _pool: &mut smithay_client_toolkit::shm::slot::SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    is_open: bool,
    snapshot: &AudioSnapshot,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    mut gpu_context: Option<&mut wgpu_api::Renderer>,
    dmabuf_state: &DmabufState,
    dmabuf_pool: &crate::render::dmabuf_export::DmabufPool,
) {
    if !is_open {
        if let Some(popup) = dock.audio_popup.take() {
            popup.surface.attach(None, 0, 0);
            popup.surface.commit();
        }
        return;
    }

    let scale = dock.scale_factor.max(0.25);
    let phys_width = (AUDIO_POPUP_WIDTH as f64 * scale).round().max(1.0) as i32;
    let phys_height = (AUDIO_POPUP_HEIGHT as f64 * scale).round().max(1.0) as i32;
    let (bar_x, bar_y) = player_position();
    let player_center = if dock.audio_player_expanded {
        PLAYER_WIDTH / 2
    } else {
        COMPACT_X + COMPACT_WIDTH / 2
    };
    let anchor_x = ((bar_x + player_center) as f64 * scale).round() as i32;
    // The audio player lives left of the dock surface, so its mixer popup can
    // also use a negative parent-local x coordinate instead of jumping over
    // the app row to the dock's left edge.
    let x = anchor_x - phys_width / 2;
    let anchor_y = ((bar_y + PLAYER_HEIGHT) as f64 * scale).round() as i32;
    let y = anchor_y - phys_height;
    let logical_x = (x as f64 / scale).round() as i32;
    let logical_y = (y as f64 / scale).round() as i32;

    if dock.audio_popup.is_none() {
        let surface = compositor_state.create_surface(qh);
        let Some(sc) = subcompositor else {
            tracing::warn!("Subcompositor not available; audio popup rendering degraded");
            return;
        };
        let subsurface = sc.get_subsurface(&surface, dock.surface.wl_surface(), qh, ());
        subsurface.set_desync();
        dock.audio_popup = Some(PopupSurface {
            subsurface,
            surface,
            current_buffer: None,
            width: 0,
            height: 0,
            current_app_id: Some("audio-panel".into()),
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
    let Some(popup) = dock.audio_popup.as_mut() else {
        tracing::warn!("Audio popup not initialized");
        return;
    };

    popup.position = (logical_x, logical_y);
    popup.subsurface.set_position(logical_x, logical_y);
    popup.width = AUDIO_POPUP_WIDTH as u32;
    popup.height = AUDIO_POPUP_HEIGHT as u32;

    // GPU rendering via wgpu-api using DrawCommands
    let mut draw_commands: Vec<wgpu_api::DrawCommand> = Vec::new();
    let scale_f32 = scale as f32;
    let width = phys_width as f32;
    let height = phys_height as f32;

    // Background panel
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(0.0, 0.0, width, height),
        color: wgpu_api::Color {
            r: 45.0 / 255.0,
            g: 45.0 / 255.0,
            b: 56.0 / 255.0,
            a: 1.0,
        },
        radius: 14.0 * scale_f32,
        stroke: None,
        stroke_width: 0.0,
    });

    // Yellow accent bar at top
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(
            8.0 * scale_f32,
            0.0,
            24.0 * scale_f32,
            3.0 * scale_f32,
        ),
        color: wgpu_api::Color {
            r: 250.0 / 255.0,
            g: 204.0 / 255.0,
            b: 21.0 / 255.0,
            a: 1.0,
        },
        radius: 0.0,
        stroke: None,
        stroke_width: 0.0,
    });

    // Header text
    draw_commands.push(wgpu_api::DrawCommand::Text {
        position: wgpu_api::Point::new(8.0 * scale_f32, 17.0 * scale_f32),
        content: "Audio".to_string(),
        font_size: 12.0 * scale_f32,
        color: wgpu_api::Color {
            r: 232.0 / 255.0,
            g: 229.0 / 255.0,
            b: 219.0 / 255.0,
            a: 1.0,
        },
        align: 0.0,
    });
    draw_commands.push(wgpu_api::DrawCommand::Text {
        position: wgpu_api::Point::new(48.0 * scale_f32, 16.0 * scale_f32),
        content: "PIPEWIRE".to_string(),
        font_size: 7.0 * scale_f32,
        color: wgpu_api::Color {
            r: 160.0 / 255.0,
            g: 160.0 / 255.0,
            b: 170.0 / 255.0,
            a: 1.0,
        },
        align: 0.0,
    });

    // Separator line
    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
        rect: wgpu_api::Rect::new(8.0 * scale_f32, 23.0 * scale_f32, 164.0 * scale_f32, 1.0 * scale_f32),
        color: wgpu_api::Color {
            r: 80.0 / 255.0,
            g: 80.0 / 255.0,
            b: 90.0 / 255.0,
            a: 1.0,
        },
        radius: 0.0,
        stroke: None,
        stroke_width: 0.0,
    });

    if !snapshot.available {
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 58.0 * scale_f32),
            content: "PipeWire unavailable".to_string(),
            font_size: 10.0 * scale_f32,
            color: wgpu_api::Color {
                r: 255.0 / 255.0,
                g: 214.0 / 255.0,
                b: 105.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
    } else {
        // Output section
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 35.0 * scale_f32),
            content: "OUT".to_string(),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(124.0 * scale_f32, 35.0 * scale_f32),
            content: format!("{}%", (snapshot.output_volume * 100.0).round() as u32),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 224.0 / 255.0,
                g: 219.0 / 255.0,
                b: 201.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
        // Output volume slider track
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(
                8.0 * scale_f32,
                OUTPUT_SLIDER_Y as f32 * scale_f32 - 1.5 * scale_f32,
                111.0 * scale_f32,
                3.0 * scale_f32,
            ),
            color: wgpu_api::Color {
                r: 64.0 / 255.0,
                g: 64.0 / 255.0,
                b: 76.0 / 255.0,
                a: 1.0,
            },
            radius: 1.5 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });
        // Output volume slider fill
        let output_fill = ((111.0_f32 - 6.0).max(1.0) * snapshot.output_volume.clamp(0.0, 1.0)).round();
        if output_fill > 0.0 {
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(
                    8.0 * scale_f32,
                    OUTPUT_SLIDER_Y as f32 * scale_f32 - 1.5 * scale_f32,
                    output_fill * scale_f32,
                    3.0 * scale_f32,
                ),
                color: wgpu_api::Color {
                    r: 250.0 / 255.0,
                    g: 204.0 / 255.0,
                    b: 21.0 / 255.0,
                    a: 1.0,
                },
                radius: 1.5 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });
        }
        // Output device label
        if let Some(device_id) = snapshot.default_output {
            if let Some(device) = snapshot.outputs.iter().find(|d| d.id == device_id) {
                let short = device.name.chars().take(18).collect::<String>()
                    + if device.name.chars().count() > 18 { "..." } else { "" };
                draw_commands.push(wgpu_api::DrawCommand::Text {
                    position: wgpu_api::Point::new(8.0 * scale_f32, OUTPUT_DEVICE_Y as f32 * scale_f32),
                    content: short,
                    font_size: 7.0 * scale_f32,
                    color: wgpu_api::Color {
                        r: 160.0 / 255.0,
                        g: 160.0 / 255.0,
                        b: 170.0 / 255.0,
                        a: 1.0,
                    },
                    align: 0.0,
                });
            }
        }

        // Separator
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(8.0 * scale_f32, 72.0 * scale_f32, 164.0 * scale_f32, 1.0 * scale_f32),
            color: wgpu_api::Color {
                r: 80.0 / 255.0,
                g: 80.0 / 255.0,
                b: 90.0 / 255.0,
                a: 1.0,
            },
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });

        // Input section
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, 86.0 * scale_f32),
            content: "IN".to_string(),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(124.0 * scale_f32, 86.0 * scale_f32),
            content: format!("{}%", (snapshot.input_volume * 100.0).round() as u32),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 224.0 / 255.0,
                g: 219.0 / 255.0,
                b: 201.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
        // Input volume slider track
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(
                8.0 * scale_f32,
                INPUT_SLIDER_Y as f32 * scale_f32 - 1.5 * scale_f32,
                111.0 * scale_f32,
                3.0 * scale_f32,
            ),
            color: wgpu_api::Color {
                r: 64.0 / 255.0,
                g: 64.0 / 255.0,
                b: 76.0 / 255.0,
                a: 1.0,
            },
            radius: 1.5 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });
        // Input volume slider fill
        let input_fill = ((111.0_f32 - 6.0).max(1.0) * snapshot.input_volume.clamp(0.0, 1.0)).round();
        if input_fill > 0.0 {
            draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                rect: wgpu_api::Rect::new(
                    8.0 * scale_f32,
                    INPUT_SLIDER_Y as f32 * scale_f32 - 1.5 * scale_f32,
                    input_fill * scale_f32,
                    3.0 * scale_f32,
                ),
                color: wgpu_api::Color {
                    r: 250.0 / 255.0,
                    g: 204.0 / 255.0,
                    b: 21.0 / 255.0,
                    a: 1.0,
                },
                radius: 1.5 * scale_f32,
                stroke: None,
                stroke_width: 0.0,
            });
        }
        // Input device label
        if let Some(device_id) = snapshot.default_input {
            if let Some(device) = snapshot.inputs.iter().find(|d| d.id == device_id) {
                let short = device.name.chars().take(18).collect::<String>()
                    + if device.name.chars().count() > 18 { "..." } else { "" };
                draw_commands.push(wgpu_api::DrawCommand::Text {
                    position: wgpu_api::Point::new(8.0 * scale_f32, INPUT_DEVICE_Y as f32 * scale_f32),
                    content: short,
                    font_size: 7.0 * scale_f32,
                    color: wgpu_api::Color {
                        r: 160.0 / 255.0,
                        g: 160.0 / 255.0,
                        b: 170.0 / 255.0,
                        a: 1.0,
                    },
                    align: 0.0,
                });
            }
        }

        // Streams section
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(8.0 * scale_f32, 123.0 * scale_f32, 164.0 * scale_f32, 1.0 * scale_f32),
            color: wgpu_api::Color {
                r: 80.0 / 255.0,
                g: 80.0 / 255.0,
                b: 90.0 / 255.0,
                a: 1.0,
            },
            radius: 0.0,
            stroke: None,
            stroke_width: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(8.0 * scale_f32, STREAMS_HEADER_Y as f32 * scale_f32),
            content: "APPS".to_string(),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });

        if snapshot.streams.is_empty() {
            draw_commands.push(wgpu_api::DrawCommand::Text {
                position: wgpu_api::Point::new(8.0 * scale_f32, 159.0 * scale_f32),
                content: "No active streams".to_string(),
                font_size: 8.0 * scale_f32,
                color: wgpu_api::Color {
                    r: 160.0 / 255.0,
                    g: 160.0 / 255.0,
                    b: 170.0 / 255.0,
                    a: 1.0,
                },
                align: 0.0,
            });
        } else {
            for (index, stream) in snapshot.streams.iter().take(2).enumerate() {
                let row_y = STREAM_ROW_START_Y + index as i32 * STREAM_ROW_HEIGHT;
                let name = stream.name.chars().take(23).collect::<String>()
                    + if stream.name.chars().count() > 23 { "..." } else { "" };
                draw_commands.push(wgpu_api::DrawCommand::Text {
                    position: wgpu_api::Point::new(8.0 * scale_f32, (row_y + 9) as f32 * scale_f32),
                    content: name,
                    font_size: 8.0 * scale_f32,
                    color: wgpu_api::Color {
                        r: 232.0 / 255.0,
                        g: 229.0 / 255.0,
                        b: 219.0 / 255.0,
                        a: 1.0,
                    },
                    align: 0.0,
                });
                // Stream volume slider
                let stream_slider_y = (row_y + 19) as f32 * scale_f32;
                draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                    rect: wgpu_api::Rect::new(
                        8.0 * scale_f32,
                        stream_slider_y - 1.5 * scale_f32,
                        111.0 * scale_f32,
                        3.0 * scale_f32,
                    ),
                    color: wgpu_api::Color {
                        r: 64.0 / 255.0,
                        g: 64.0 / 255.0,
                        b: 76.0 / 255.0,
                        a: 1.0,
                    },
                    radius: 1.5 * scale_f32,
                    stroke: None,
                    stroke_width: 0.0,
                });
                let stream_fill = ((111.0_f32 - 6.0).max(1.0) * stream.volume.clamp(0.0, 1.0)).round();
                if stream_fill > 0.0 {
                    draw_commands.push(wgpu_api::DrawCommand::Rectangle {
                        rect: wgpu_api::Rect::new(
                            8.0 * scale_f32,
                            stream_slider_y - 1.5 * scale_f32,
                            stream_fill * scale_f32,
                            3.0 * scale_f32,
                        ),
                        color: wgpu_api::Color {
                            r: 250.0 / 255.0,
                            g: 204.0 / 255.0,
                            b: 21.0 / 255.0,
                            a: 1.0,
                        },
                        radius: 1.5 * scale_f32,
                        stroke: None,
                        stroke_width: 0.0,
                    });
                }
                draw_commands.push(wgpu_api::DrawCommand::Text {
                    position: wgpu_api::Point::new(124.0 * scale_f32, (row_y + 22) as f32 * scale_f32),
                    content: format!("{}%", (stream.volume * 100.0).round() as u32),
                    font_size: 7.0 * scale_f32,
                    color: wgpu_api::Color {
                        r: 224.0 / 255.0,
                        g: 219.0 / 255.0,
                        b: 201.0 / 255.0,
                        a: 1.0,
                    },
                    align: 0.0,
                });
            }
        }

        // Mixer button
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect::new(
                8.0 * scale_f32,
                MIXER_BUTTON_Y as f32 * scale_f32,
                164.0 * scale_f32,
                17.0 * scale_f32,
            ),
            color: wgpu_api::Color {
                r: 250.0 / 255.0,
                g: 204.0 / 255.0,
                b: 21.0 / 255.0,
                a: 1.0,
            },
            radius: 7.0 * scale_f32,
            stroke: None,
            stroke_width: 0.0,
        });
        draw_commands.push(wgpu_api::DrawCommand::Text {
            position: wgpu_api::Point::new(54.0 * scale_f32, 229.0 * scale_f32),
            content: "Open full mixer".to_string(),
            font_size: 8.0 * scale_f32,
            color: wgpu_api::Color {
                r: 30.0 / 255.0,
                g: 30.0 / 255.0,
                b: 30.0 / 255.0,
                a: 1.0,
            },
            align: 0.0,
        });
    }

    // Render to GPU texture via DMA-BUF pool
    let gpu_success = if let Some(ref mut renderer) = gpu_context {
        if let (Some(adapter), Some(device)) = (renderer.adapter(), renderer.device()) {
            match dmabuf_pool.acquire_texture(
                adapter, device, qh, dmabuf_state,
                phys_width as u32, phys_height as u32,
            ) {
                Ok((texture, export_info)) => {
                    if renderer.render_to_texture(
                        &texture, phys_width as u32, phys_height as u32,
                        1.0, &draw_commands,
                    ).is_err() {
                        false
                    } else {
                        match crate::render::dmabuf_export::create_buffer_from_export(
                            dmabuf_state, qh, &export_info,
                        ) {
                            Ok(wl_buffer) => {
                                dmabuf_pool.set_buffer_id(wl_buffer.id());
                                popup.surface.set_buffer_scale(scale.round().max(1.0) as i32);
                                popup.surface.attach(Some(&wl_buffer), 0, 0);
                                popup.surface.damage_buffer(0, 0, phys_width, phys_height);
                                let region = compositor_state.wl_compositor().create_region(qh, ());
                                region.add(0, 0, AUDIO_POPUP_WIDTH, AUDIO_POPUP_HEIGHT);
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

    if !gpu_success {
        tracing::warn!("GPU rendering failed for audio popup; skipping frame");
    }
}

/// Finds which volume track was pressed so pointer motion can update it
/// continuously until the matching button release.
pub fn slider_target_at(snapshot: &AudioSnapshot, x: f64, y: f64) -> Option<AudioSliderTarget> {
    let x = x.round() as i32;
    let y = y.round() as i32;
    if !(8..=120).contains(&x) {
        return None;
    }
    if (OUTPUT_SLIDER_Y - 7..=OUTPUT_SLIDER_Y + 7).contains(&y) {
        return snapshot.default_output.map(|_| AudioSliderTarget::Output);
    }
    if (INPUT_SLIDER_Y - 7..=INPUT_SLIDER_Y + 7).contains(&y) {
        return snapshot.default_input.map(|_| AudioSliderTarget::Input);
    }
    if (STREAM_ROW_START_Y..STREAM_ROW_START_Y + 2 * STREAM_ROW_HEIGHT).contains(&y) {
        let index = ((y - STREAM_ROW_START_Y) / STREAM_ROW_HEIGHT) as usize;
        let stream = snapshot.streams.get(index)?;
        let slider_y = STREAM_ROW_START_Y + index as i32 * STREAM_ROW_HEIGHT + 19;
        if (slider_y - 7..=slider_y + 7).contains(&y) {
            return Some(AudioSliderTarget::Stream(stream.id));
        }
    }
    None
}

pub fn action_for_audio_slider(
    target: AudioSliderTarget,
    snapshot: &AudioSnapshot,
    x: f64,
) -> Option<AudioAction> {
    let volume = ((x.round() as i32 - 8) as f32 / 111.0).clamp(0.0, 1.0);
    match target {
        AudioSliderTarget::Output => snapshot
            .default_output
            .map(|node_id| AudioAction::SetOutputVolume { node_id, volume }),
        AudioSliderTarget::Input => snapshot
            .default_input
            .map(|node_id| AudioAction::SetInputVolume { node_id, volume }),
        AudioSliderTarget::Stream(node_id) => snapshot
            .streams
            .iter()
            .any(|stream| stream.id == node_id)
            .then_some(AudioAction::SetStreamVolume { node_id, volume }),
    }
}

fn cycle_device_id(
    devices: &[crate::models::AudioDevice],
    current: Option<u32>,
    direction: i32,
) -> Option<u32> {
    if devices.is_empty() {
        return None;
    }
    let current_index = current
        .and_then(|id| devices.iter().position(|device| device.id == id))
        .unwrap_or(if direction >= 0 { devices.len() - 1 } else { 0 });
    let next_index =
        (current_index as isize + direction as isize).rem_euclid(devices.len() as isize) as usize;
    Some(devices[next_index].id)
}

/// True when the small header control should hide the audio panel.
pub fn minimize_button_at(x: f64, y: f64) -> bool {
    let x = x.round() as i32;
    let y = y.round() as i32;
    (MINIMIZE_BUTTON_X..MINIMIZE_BUTTON_X + MINIMIZE_BUTTON_WIDTH).contains(&x)
        && (MINIMIZE_BUTTON_Y..MINIMIZE_BUTTON_Y + MINIMIZE_BUTTON_HEIGHT).contains(&y)
}

/// Converts a click in audio popup-local logical coordinates to a PipeWire
/// control command. Device selector arrows cycle through every available node.
pub fn action_for_audio_popup_click(
    snapshot: &AudioSnapshot,
    x: f64,
    y: f64,
) -> Option<AudioAction> {
    if let Some(target) = slider_target_at(snapshot, x, y) {
        return action_for_audio_slider(target, snapshot, x);
    }
    let x = x.round() as i32;
    let y = y.round() as i32;

    if (OUTPUT_MUTE_Y..OUTPUT_MUTE_Y + 17).contains(&y) && (151..=173).contains(&x) {
        let id = snapshot.default_output?;
        return Some(AudioAction::SetOutputMute {
            node_id: id,
            muted: !snapshot.output_muted,
        });
    }
    if (OUTPUT_DEVICE_Y..OUTPUT_DEVICE_Y + DEVICE_ROW_HEIGHT).contains(&y) {
        let direction = if (8..=25).contains(&x) {
            -1
        } else if (155..=172).contains(&x) {
            1
        } else {
            0
        };
        if direction != 0 {
            let node_id = cycle_device_id(&snapshot.outputs, snapshot.default_output, direction)?;
            return Some(AudioAction::SetDefaultOutput { node_id });
        }
    }

    if (INPUT_MUTE_Y..INPUT_MUTE_Y + 17).contains(&y) && (151..=173).contains(&x) {
        let id = snapshot.default_input?;
        return Some(AudioAction::SetInputMute {
            node_id: id,
            muted: !snapshot.input_muted,
        });
    }
    if (INPUT_DEVICE_Y..INPUT_DEVICE_Y + DEVICE_ROW_HEIGHT).contains(&y) {
        let direction = if (8..=25).contains(&x) {
            -1
        } else if (155..=172).contains(&x) {
            1
        } else {
            0
        };
        if direction != 0 {
            let node_id = cycle_device_id(&snapshot.inputs, snapshot.default_input, direction)?;
            return Some(AudioAction::SetDefaultInput { node_id });
        }
    }

    if (STREAM_ROW_START_Y..STREAM_ROW_START_Y + 2 * STREAM_ROW_HEIGHT).contains(&y) {
        let index = ((y - STREAM_ROW_START_Y) / STREAM_ROW_HEIGHT) as usize;
        let stream = snapshot.streams.get(index)?;
        let row_y = STREAM_ROW_START_Y + index as i32 * STREAM_ROW_HEIGHT;
        if (row_y..row_y + 17).contains(&y) && (151..=173).contains(&x) {
            return Some(AudioAction::SetStreamMute {
                node_id: stream.id,
                muted: !stream.muted,
            });
        }
    }

    if (MIXER_BUTTON_Y..MIXER_BUTTON_Y + 17).contains(&y) && (8..=172).contains(&x) {
        return Some(AudioAction::LaunchMixer);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AudioStream;

    #[test]
    fn output_slider_maps_pointer_position_to_volume_action() {
        let snapshot = AudioSnapshot {
            available: true,
            default_output: Some(42),
            ..AudioSnapshot::default()
        };
        let target = slider_target_at(
            &snapshot,
            63.5,
            OUTPUT_SLIDER_Y as f64,
        );
        assert_eq!(target, Some(AudioSliderTarget::Output));
        let action = action_for_audio_slider(target.unwrap(), &snapshot, 63.5).unwrap();
        match action {
            AudioAction::SetOutputVolume { node_id, volume } => {
                assert_eq!(node_id, 42);
                assert!((volume - 0.5).abs() < 0.01);
            }
            _ => panic!("unexpected audio action for output slider"),
        }
    }

    #[test]
    fn minimize_button_only_hits_the_header_control() {
        assert!(minimize_button_at(160.0, 10.0));
        assert!(!minimize_button_at(140.0, 10.0));
        assert!(!minimize_button_at(160.0, 30.0));
    }

    #[test]
    fn output_device_arrows_cycle_through_all_devices() {
        use crate::models::AudioDevice;
        let snapshot = AudioSnapshot {
            outputs: vec![
                AudioDevice {
                    id: 11,
                    name: "Speakers".into(),
                    volume: 0.5,
                    muted: false,
                    is_default: true,
                },
                AudioDevice {
                    id: 22,
                    name: "Headphones".into(),
                    volume: 0.5,
                    muted: false,
                    is_default: false,
                },
                AudioDevice {
                    id: 33,
                    name: "HDMI".into(),
                    volume: 0.5,
                    muted: false,
                    is_default: false,
                },
            ],
            default_output: Some(11),
            ..AudioSnapshot::default()
        };
        match action_for_audio_popup_click(
            &snapshot,
            165.0,
            OUTPUT_DEVICE_Y as f64 + 5.0,
        ) {
            Some(AudioAction::SetDefaultOutput { node_id }) => assert_eq!(node_id, 22),
            _ => panic!("expected next-output action"),
        }
        match action_for_audio_popup_click(
            &snapshot,
            15.0,
            OUTPUT_DEVICE_Y as f64 + 5.0,
        ) {
            Some(AudioAction::SetDefaultOutput { node_id }) => assert_eq!(node_id, 33),
            _ => panic!("expected previous-output action"),
        }
    }

    #[test]
    fn stream_slider_keeps_node_id_when_the_list_changes_order() {
        let snapshot = AudioSnapshot {
            streams: vec![
                AudioStream {
                    id: 77,
                    name: "Browser".into(),
                    volume: 0.5,
                    muted: false,
                },
                AudioStream {
                    id: 88,
                    name: "Player".into(),
                    volume: 0.5,
                    muted: false,
                },
            ],
            ..AudioSnapshot::default()
        };
        let target = slider_target_at(
            &snapshot,
            63.0,
            (STREAM_ROW_START_Y + 19) as f64,
        );
        assert_eq!(target, Some(AudioSliderTarget::Stream(77)));
    }
}
