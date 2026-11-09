//! Audio monitor service.
//!
//! Monitors PipeWire/WirePlumber for audio player state changes via wpctl.

use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crate::models::{AudioAction, AudioDevice, AudioSnapshot, AudioStream};

/// Runs all PipeWire/WirePlumber CLI calls off the Wayland/render thread.
/// `wpctl` operates on PipeWire node IDs directly and is the authoritative
/// control path for a WirePlumber session.
pub fn spawn_audio_monitor(snapshot_tx: Sender<AudioSnapshot>, action_rx: Receiver<AudioAction>) {
    thread::Builder::new()
        .name("dock-pipewire-audio".into())
        .spawn(move || {
            let poll_interval = Duration::from_millis(1500);
            let action_interval = Duration::from_millis(24);
            let mut last_snapshot: Option<AudioSnapshot> = None;
            let mut pending_action: Option<AudioAction> = None;
            let mut action_due: Option<Instant> = None;
            let mut next_poll = Instant::now() + Duration::from_millis(50);

            loop {
                let now = Instant::now();
                let wait = action_due
                    .map(|due| due.saturating_duration_since(now))
                    .unwrap_or_else(|| next_poll.saturating_duration_since(now));

                match action_rx.recv_timeout(wait) {
                    Ok(action) => {
                        if action_due.is_none() {
                            action_due = Some(Instant::now() + action_interval);
                        }
                        pending_action = Some(action);
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }

                // Coalesce rapid slider-motion events. The most recent value is
                // applied at up to ~40 Hz instead of spawning a wpctl process for
                // every pointer event on the Wayland thread.
                while let Ok(action) = action_rx.try_recv() {
                    if action_due.is_none() {
                        action_due = Some(Instant::now() + action_interval);
                    }
                    pending_action = Some(action);
                }

                let now = Instant::now();
                let action_ready = action_due.is_some_and(|due| now >= due);
                let poll_ready = now >= next_poll;

                if action_ready {
                    if let Some(action) = pending_action.take() {
                        apply_action(action);
                    }
                    action_due = None;
                }

                if action_ready || poll_ready {
                    let snapshot = read_audio_snapshot();
                    if last_snapshot.as_ref() != Some(&snapshot) {
                        if snapshot_tx.send(snapshot.clone()).is_err() {
                            break;
                        }
                        last_snapshot = Some(snapshot);
                    }
                    next_poll = Instant::now() + poll_interval;
                }
            }
        })
        .expect("failed to start PipeWire audio monitor");
}

fn run_wpctl(args: &[String]) -> Result<String, String> {
    let output = Command::new("wpctl")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("could not start wpctl: {e}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn apply_action(action: AudioAction) {
    let result = match action {
        AudioAction::SetOutputVolume { node_id, volume }
        | AudioAction::SetInputVolume { node_id, volume }
        | AudioAction::SetStreamVolume { node_id, volume } => run_wpctl(&[
            "set-volume".into(),
            node_id.to_string(),
            format!("{:.3}", volume.clamp(0.0, 1.0)),
            "--limit".into(),
            "1.0".into(),
        ]),
        AudioAction::SetOutputMute { node_id, muted }
        | AudioAction::SetInputMute { node_id, muted }
        | AudioAction::SetStreamMute { node_id, muted } => run_wpctl(&[
            "set-mute".into(),
            node_id.to_string(),
            if muted { "1".into() } else { "0".into() },
        ]),
        AudioAction::SetDefaultOutput { node_id } | AudioAction::SetDefaultInput { node_id } => {
            run_wpctl(&["set-default".into(), node_id.to_string()])
        }
        AudioAction::LaunchMixer => {
            let launched = Command::new("pavucontrol")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
            launched
                .map(|_| String::new())
                .map_err(|e| format!("could not launch pavucontrol: {e}"))
        }
        AudioAction::MediaControl(control) => {
            let verb = match control {
                crate::models::MediaControl::Previous => "previous",
                crate::models::MediaControl::PlayPause => "play-pause",
                crate::models::MediaControl::Next => "next",
            };
            Command::new("playerctl")
                .arg(verb)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|_| String::new())
                .map_err(|e| format!("could not control MPRIS player with playerctl: {e}"))
        }
    };
    if let Err(error) = result {
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    None,
    Outputs,
    Inputs,
    Streams,
}

fn parse_node_id_and_payload(line: &str) -> Option<(u32, String, bool)> {
    let tokens: Vec<&str> = line.split_whitespace().collect();
    for (index, token) in tokens.iter().enumerate() {
        let candidate = token.trim_end_matches('.');
        if token.ends_with('.')
            && !candidate.is_empty()
            && candidate.chars().all(|c| c.is_ascii_digit())
        {
            let id = candidate.parse::<u32>().ok()?;
            let name = tokens
                .iter()
                .skip(index + 1)
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            let is_default = tokens.iter().take(index).any(|part| *part == "*");
            return Some((id, name, is_default));
        }
    }
    None
}

fn status_row_indent(line: &str) -> usize {
    let trimmed = line.trim_start();
    let leading = line.chars().take_while(|ch| ch.is_whitespace()).count();
    if let Some(rest) = trimmed
        .strip_prefix('│')
        .or_else(|| trimmed.strip_prefix('├'))
        .or_else(|| trimmed.strip_prefix('└'))
    {
        rest.chars().take_while(|ch| ch.is_whitespace()).count()
    } else {
        leading
    }
}

fn is_top_level_status_row(line: &str) -> bool {
    // wpctl's current tree format uses 12 leading spaces for first-level
    // stream rows and 17 for their channel/port children.
    status_row_indent(line) <= 12
}

fn parse_volume(text: &str) -> (f32, bool) {
    let volume = text
        .find("[vol:")
        .and_then(|start| {
            let tail = &text[start + 5..];
            let end = tail.find(']')?;
            tail[..end].trim().parse::<f32>().ok()
        })
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let muted = text.to_ascii_lowercase().contains("muted");
    (volume, muted)
}

fn clean_node_name(value: &str) -> String {
    let without_volume = value.split("[vol:").next().unwrap_or(value);
    let value = without_volume
        .split("[MUTED]")
        .next()
        .unwrap_or(without_volume)
        .trim();
    if value.is_empty() {
        "Unknown device".into()
    } else {
        value.to_string()
    }
}

fn parse_wpctl_list(text: &str) -> Vec<AudioDevice> {
    text.lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            if fields.len() < 2 {
                return None;
            }
            let id = fields.first()?.parse::<u32>().ok()?;
            let name = fields.get(1).copied().unwrap_or_default();
            if name.is_empty() {
                return None;
            }
            let is_default = fields.iter().any(|field| *field == "*");
            Some(AudioDevice {
                id,
                name: name.to_string(),
                volume: 1.0,
                muted: false,
                is_default,
            })
        })
        .collect()
}

fn read_device_list(kind: &str) -> Option<Vec<AudioDevice>> {
    let output = run_wpctl(&["list".into(), "audio".into(), kind.into()]).ok()?;
    let devices = parse_wpctl_list(&output);
    (!devices.is_empty()).then_some(devices)
}

fn read_node_volume(node_id: u32) -> (f32, bool) {
    let Ok(output) = run_wpctl(&["get-volume".into(), node_id.to_string()]) else {
        return (1.0, false);
    };
    let volume = output
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    (volume, output.to_ascii_lowercase().contains("muted"))
}

fn read_audio_snapshot() -> AudioSnapshot {
    let status = match run_wpctl(&["status".into(), "-n".into()]) {
        Ok(status) => status,
        Err(error) => {
            return AudioSnapshot {
                available: false,
                error: Some(error),
                ..AudioSnapshot::default()
            }
        }
    };

    let mut snapshot = AudioSnapshot {
        available: true,
        ..AudioSnapshot::default()
    };
    let mut section = Section::None;
    for line in status.lines() {
        let lower = line.to_ascii_lowercase();
        if lower.contains("sinks:") {
            section = Section::Outputs;
            continue;
        } else if lower.contains("sources:") {
            section = Section::Inputs;
            continue;
        } else if lower.contains("streams:") {
            section = Section::Streams;
            continue;
        } else if lower.contains("devices:")
            || lower.contains("filters:")
            || lower.contains("sink endpoints:")
            || lower.contains("source endpoints:")
        {
            section = Section::None;
            continue;
        }

        let Some((id, raw_name, is_default)) = parse_node_id_and_payload(line) else {
            continue;
        };
        if raw_name.is_empty() {
            continue;
        }
        let (volume, muted) = parse_volume(line);
        match section {
            Section::Outputs => snapshot.outputs.push(AudioDevice {
                id,
                name: clean_node_name(&raw_name),
                volume,
                muted,
                is_default,
            }),
            Section::Inputs => snapshot.inputs.push(AudioDevice {
                id,
                name: clean_node_name(&raw_name),
                volume,
                muted,
                is_default,
            }),
            // Stream entries are top-level children of the Streams section;
            // their channel/port rows are nested more deeply and are skipped.
            // Do not require `[vol: ...]`: some wpctl versions omit it when
            // `status --name` is used, and volume is queried by node ID below.
            Section::Streams if is_top_level_status_row(line) => {
                snapshot.streams.push(AudioStream {
                    id,
                    name: clean_node_name(&raw_name),
                    volume,
                    muted,
                })
            }
            _ => {}
        }
    }

    // WirePlumber 0.5 provides a tab-separated list command specifically for
    // application consumption. Prefer it for device discovery; the tree parser
    // above remains a compatibility fallback for older wpctl builds.
    if let Some(outputs) = read_device_list("sinks") {
        snapshot.outputs = outputs;
    }
    if let Some(inputs) = read_device_list("sources") {
        snapshot.inputs = inputs;
    }

    if let Some(device) = snapshot
        .outputs
        .iter()
        .find(|device| device.is_default)
        .or_else(|| snapshot.outputs.first())
    {
        snapshot.default_output = Some(device.id);
        (snapshot.output_volume, snapshot.output_muted) = read_node_volume(device.id);
    }
    if let Some(device) = snapshot
        .inputs
        .iter()
        .find(|device| device.is_default)
        .or_else(|| snapshot.inputs.first())
    {
        snapshot.default_input = Some(device.id);
        (snapshot.input_volume, snapshot.input_muted) = read_node_volume(device.id);
    }
    if snapshot.default_output.is_none() {
        let (volume, muted) = query_default_volume("@DEFAULT_AUDIO_SINK@");
        snapshot.output_volume = volume;
        snapshot.output_muted = muted;
    }
    if snapshot.default_input.is_none() {
        let (volume, muted) = query_default_volume("@DEFAULT_AUDIO_SOURCE@");
        snapshot.input_volume = volume;
        snapshot.input_muted = muted;
    }

    snapshot
        .outputs
        .sort_by_key(|d| (!d.is_default, d.name.to_lowercase()));
    snapshot
        .inputs
        .sort_by_key(|d| (!d.is_default, d.name.to_lowercase()));
    snapshot.streams.sort_by_key(|s| s.name.to_lowercase());
    snapshot.streams.truncate(3);
    for stream in &mut snapshot.streams {
        (stream.volume, stream.muted) = read_node_volume(stream.id);
    }
    if let Ok(status) = Command::new("playerctl").arg("status").output() {
        if status.status.success() {
            snapshot.media_status =
                Some(String::from_utf8_lossy(&status.stdout).trim().to_string());
            if let Ok(metadata) = Command::new("playerctl")
                .args([
                    "metadata",
                    "--format",
                    "{{artist}}\t{{title}}\t{{mpris:length}}",
                ])
                .output()
            {
                if metadata.status.success() {
                    let values: Vec<String> = String::from_utf8_lossy(&metadata.stdout)
                        .trim_end()
                        .split('\t')
                        .map(str::to_string)
                        .collect();
                    snapshot.media_artist = values.first().filter(|s| !s.is_empty()).cloned();
                    snapshot.media_title = values.get(1).filter(|s| !s.is_empty()).cloned();
                    snapshot.media_duration_seconds = values
                        .get(2)
                        .and_then(|s| s.parse::<f64>().ok())
                        .map(|us| us / 1_000_000.0);
                }
            }
            if let Ok(position) = Command::new("playerctl").arg("position").output() {
                if position.status.success() {
                    snapshot.media_position_seconds = String::from_utf8_lossy(&position.stdout)
                        .trim()
                        .parse()
                        .ok();
                }
            }
        }
    }
    snapshot
}

fn query_default_volume(target: &str) -> (f32, bool) {
    let Ok(output) = run_wpctl(&["get-volume".into(), target.into()]) else {
        return (1.0, false);
    };
    let volume = output
        .split_whitespace()
        .find_map(|part| part.parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    (volume, output.to_ascii_lowercase().contains("muted"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wpctl_list_rows() {
        let rows = "42\tBuilt-in Audio Analog Stereo\tAudio/Sink\t*\n43\tUSB Headset\tAudio/Sink\t";
        let parsed = parse_wpctl_list(rows);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].id, 42);
        assert!(parsed[0].is_default);
        assert_eq!(parsed[1].name, "USB Headset");
    }

    #[test]
    fn parses_stream_rows_without_volume_metadata_and_skips_ports() {
        let top_level = "            91. Firefox";
        let tree_top_level = " │  *   93. Discord";
        let child_port = "                 92. output_FL";
        let tree_child_port = " │             94. output_FR";
        assert!(is_top_level_status_row(top_level));
        assert!(is_top_level_status_row(tree_top_level));
        assert!(!is_top_level_status_row(child_port));
        assert!(!is_top_level_status_row(tree_child_port));
        let (id, name, _) = parse_node_id_and_payload(top_level).unwrap();
        assert_eq!(id, 91);
        assert_eq!(name, "Firefox");
    }

    #[test]
    fn parses_wpctl_node_rows() {
        let line = " │  *   52. alsa_output.pci-0000_00_1f.3.analog-stereo [vol: 0.65]";
        let (id, name, is_default) = parse_node_id_and_payload(line).unwrap();
        assert_eq!(id, 52);
        assert!(name.starts_with("alsa_output"));
        assert!(is_default);
        assert_eq!(parse_volume(line), (0.65, false));
    }
}
