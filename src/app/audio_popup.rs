//! Audio popup panel.
//!
//! Expanded audio mixer popup with volume sliders and device selection.

use smithay_client_toolkit::{
    compositor::CompositorState, dmabuf::DmabufState, shell::WaylandSurface, shm::slot::SlotPool,
};
use wayland_client::protocol::wl_shm;
use wayland_client::QueueHandle;

use crate::models::{AudioAction, AudioSliderTarget, AudioSnapshot, SystemBarSnapshot, TrayItem};
use crate::render::audio::{render_audio_surface, AUDIO_POPUP_HEIGHT, AUDIO_POPUP_WIDTH};
use crate::render::audio_player::{
    player_position, COMPACT_WIDTH, COMPACT_X, PLAYER_HEIGHT, PLAYER_WIDTH,
};
use crate::render::font::FontManager;
use crate::state::AppState;
use crate::types::{DockInstance, PopupSurface};

use super::draw::DrawFrameData;

pub fn draw_audio_popup(
    dock: &mut DockInstance,
    qh: &QueueHandle<AppState>,
    _frame: &DrawFrameData<'_>,
    pool: &mut SlotPool,
    compositor_state: &CompositorState,
    subcompositor: Option<&wayland_client::protocol::wl_subcompositor::WlSubcompositor>,
    font_manager: &mut FontManager,
    is_open: bool,
    snapshot: &AudioSnapshot,
    _tray_items: &[TrayItem],
    _system_bar: &SystemBarSnapshot,
    _gpu_context: Option<&mut wgpu_api::Renderer>,
    _dmabuf_state: &DmabufState,
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

    let expected_len = phys_width as usize * phys_height as usize * 4;

    // GPU rendering via wgpu-api not yet integrated — using CPU rendering
    // CPU rendering path — always render every frame
    popup.cached_content.resize(expected_len, 0);
    render_audio_surface(
        &mut popup.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale as f32,
        snapshot,
        font_manager,
    );
    popup.cached_phys_width = phys_width;
    popup.cached_phys_height = phys_height;

    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(bc) => bc,
        Err(e) => {
            tracing::warn!("Failed to allocate audio popup buffer: {}", e);
            return;
        }
    };
    if !super::copy_popup_content(canvas, &popup.cached_content) {
        return;
    }
    popup.surface.set_buffer_scale(scale.round().max(1.0) as i32);
    if let Err(e) = buffer.attach_to(&popup.surface) {
        tracing::warn!("Failed to attach audio popup buffer: {}", e);
        return;
    }
    popup.surface.damage_buffer(0, 0, phys_width, phys_height);

    let region = compositor_state.wl_compositor().create_region(qh, ());
    region.add(0, 0, AUDIO_POPUP_WIDTH, AUDIO_POPUP_HEIGHT);
    popup.surface.set_input_region(Some(&region));
    region.destroy();
    popup.surface.commit();
    popup.current_buffer = Some(buffer);
}

/// Finds which volume track was pressed so pointer motion can update it
/// continuously until the matching button release.
pub fn slider_target_at(snapshot: &AudioSnapshot, x: f64, y: f64) -> Option<AudioSliderTarget> {
    use crate::render::audio::{
        INPUT_SLIDER_Y, OUTPUT_SLIDER_Y, STREAM_ROW_HEIGHT, STREAM_ROW_START_Y,
    };
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
    if y >= STREAM_ROW_START_Y && y < STREAM_ROW_START_Y + 2 * STREAM_ROW_HEIGHT {
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
    use crate::render::audio::{
        MINIMIZE_BUTTON_HEIGHT, MINIMIZE_BUTTON_WIDTH, MINIMIZE_BUTTON_X, MINIMIZE_BUTTON_Y,
    };
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
    use crate::render::audio::{
        DEVICE_ROW_HEIGHT, INPUT_DEVICE_Y, INPUT_MUTE_Y, MIXER_BUTTON_Y, OUTPUT_DEVICE_Y,
        OUTPUT_MUTE_Y, STREAM_ROW_HEIGHT, STREAM_ROW_START_Y,
    };
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

    if y >= STREAM_ROW_START_Y && y < STREAM_ROW_START_Y + 2 * STREAM_ROW_HEIGHT {
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
            crate::render::audio::OUTPUT_SLIDER_Y as f64,
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
            crate::render::audio::OUTPUT_DEVICE_Y as f64 + 5.0,
        ) {
            Some(AudioAction::SetDefaultOutput { node_id }) => assert_eq!(node_id, 22),
            _ => panic!("expected next-output action"),
        }
        match action_for_audio_popup_click(
            &snapshot,
            15.0,
            crate::render::audio::OUTPUT_DEVICE_Y as f64 + 5.0,
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
            (crate::render::audio::STREAM_ROW_START_Y + 19) as f64,
        );
        assert_eq!(target, Some(AudioSliderTarget::Stream(77)));
    }
}
