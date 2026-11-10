//! System bar monitor.
//!
//! Samples keyboard layout, network rates, and clock for the bottom status bar.

use std::fs;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use crate::models::SystemBarSnapshot;

/// Starts a small, bounded sampler for the top status bar. It never runs in the
/// Wayland callback/render path. Network counters are sampled every two seconds
/// to keep the display current without forcing a full dock redraw each second;
/// keyboard-layout discovery is refreshed less often because it may invoke a
/// compositor IPC command.
pub fn spawn_system_bar_monitor(tx: Sender<SystemBarSnapshot>) {
    thread::Builder::new()
        .name("dock-system-bar".into())
        .spawn(move || {
            let mut previous_totals = read_network_totals();
            let mut previous_sample = Instant::now();
            let mut keyboard_layout = detect_keyboard_layout();
            // The startup lookup above is the first sample; do not immediately
            // spawn the same compositor IPC commands again two seconds later.
            let mut last_layout_check = Instant::now();
            let (initial_date, initial_time) = local_date_and_time();
            let initial_snapshot = SystemBarSnapshot {
                keyboard_layout: if keyboard_layout.is_empty() {
                    "--".to_string()
                } else {
                    keyboard_layout.clone()
                },
                download_rate: "0B/s".to_string(),
                upload_rate: "0B/s".to_string(),
                date: initial_date,
                time: initial_time,
            };
            if tx.send(initial_snapshot.clone()).is_err() {
                return;
            }
            let mut last_sent: Option<SystemBarSnapshot> = Some(initial_snapshot);

            loop {
                thread::sleep(Duration::from_secs(2));
                let now = Instant::now();
                let totals = read_network_totals();
                let elapsed = now
                    .saturating_duration_since(previous_sample)
                    .as_secs_f64()
                    .max(0.001);
                let download = totals.0.saturating_sub(previous_totals.0);
                let upload = totals.1.saturating_sub(previous_totals.1);
                previous_totals = totals;
                previous_sample = now;

                if last_layout_check.elapsed() >= Duration::from_secs(8) {
                    let detected = detect_keyboard_layout();
                    if !detected.is_empty() {
                        keyboard_layout = detected;
                    }
                    last_layout_check = now;
                }

                let (date, time) = local_date_and_time();
                let snapshot = SystemBarSnapshot {
                    keyboard_layout: if keyboard_layout.is_empty() {
                        "--".to_string()
                    } else {
                        keyboard_layout.clone()
                    },
                    download_rate: format_rate((download as f64 / elapsed) as u64),
                    upload_rate: format_rate((upload as f64 / elapsed) as u64),
                    date,
                    time,
                };

                if last_sent.as_ref() != Some(&snapshot) {
                    if tx.send(snapshot.clone()).is_err() {
                        break;
                    }
                    last_sent = Some(snapshot);
                }
            }
        })
        .ok();
}

fn read_network_totals() -> (u64, u64) {
    let Ok(contents) = fs::read_to_string("/proc/net/dev") else {
        return (0, 0);
    };

    let mut received = 0u64;
    let mut transmitted = 0u64;
    for line in contents.lines().skip(2) {
        let Some((interface, stats)) = line.split_once(':') else {
            continue;
        };
        if interface.trim() == "lo" {
            continue;
        }
        let fields: Vec<&str> = stats.split_whitespace().collect();
        if fields.len() < 9 {
            continue;
        }
        received = received.saturating_add(fields[0].parse::<u64>().unwrap_or(0));
        transmitted = transmitted.saturating_add(fields[8].parse::<u64>().unwrap_or(0));
    }
    (received, transmitted)
}

fn format_rate(bytes_per_second: u64) -> String {
    if bytes_per_second >= 100 * 1024 * 1024 * 1024 {
        format!(
            "{:.0}G/s",
            bytes_per_second as f64 / (1024.0 * 1024.0 * 1024.0)
        )
    } else if bytes_per_second >= 1024 * 1024 * 1024 {
        format!(
            "{:.1}G/s",
            bytes_per_second as f64 / (1024.0 * 1024.0 * 1024.0)
        )
    } else if bytes_per_second >= 100 * 1024 * 1024 {
        format!("{:.0}M/s", bytes_per_second as f64 / (1024.0 * 1024.0))
    } else if bytes_per_second >= 1024 * 1024 {
        format!("{:.1}M/s", bytes_per_second as f64 / (1024.0 * 1024.0))
    } else if bytes_per_second >= 1024 {
        format!("{}K/s", (bytes_per_second + 512) / 1024)
    } else {
        format!("{}B/s", bytes_per_second)
    }
}

fn detect_keyboard_layout() -> String {
    // Hyprland exposes the active keymap directly in its device snapshot.
    if let Some(output) = command_output("hyprctl", &["devices", "-j"]) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(keyboards) = value.get("keyboards").and_then(|v| v.as_array()) {
                for keyboard in keyboards {
                    if let Some(layout) = keyboard
                        .get("active_keymap")
                        .and_then(|v| v.as_str())
                        .and_then(normalize_layout_label)
                    {
                        return layout;
                    }
                }
            }
        }
    }

    // Sway/wlroots IPC exposes xkb_active_layout_name per keyboard.
    if let Some(output) = command_output("swaymsg", &["-t", "get_inputs", "-r"]) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&output) {
            if let Some(inputs) = value.as_array() {
                for input in inputs {
                    if input.get("type").and_then(|v| v.as_str()) != Some("keyboard") {
                        continue;
                    }
                    if let Some(layout) = input
                        .get("xkb_active_layout_name")
                        .and_then(|v| v.as_str())
                        .and_then(normalize_layout_label)
                    {
                        return layout;
                    }
                }
            }
        }
    }

    // Xwayland/setxkbmap can still report the current configured layout on
    // sessions that expose the compatibility command.
    if let Some(output) = command_output("setxkbmap", &["-query"]) {
        for line in output.lines() {
            if let Some((key, value)) = line.split_once(':') {
                if key.trim() == "layout" {
                    if let Some(layout) = value
                        .trim()
                        .split(',')
                        .next()
                        .and_then(normalize_layout_label)
                    {
                        return layout;
                    }
                }
            }
        }
    }

    for variable in ["XKB_DEFAULT_LAYOUT", "XKB_LAYOUT"] {
        if let Ok(value) = std::env::var(variable) {
            if let Some(layout) = value.split(',').next().and_then(normalize_layout_label) {
                return layout;
            }
        }
    }

    // Last-resort static system configuration. It will still update when the
    // compositor query above is available, without reading files per frame.
    for path in ["/etc/default/keyboard", "/etc/vconsole.conf"] {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        for line in contents.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            if key.trim() == "XKBLAYOUT" || key.trim() == "KEYMAP" {
                let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
                if let Some(layout) = value.split(',').next().and_then(normalize_layout_label) {
                    return layout;
                }
            }
        }
    }

    "--".to_string()
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn normalize_layout_label(label: &str) -> Option<String> {
    let label = label.trim();
    if label.is_empty() {
        return None;
    }

    // Prefer a standard country/layout code in parentheses, e.g. "English
    // (US)". Layout variant names such as "Bulgarian (Phonetic)" are not
    // country codes, so fall through and identify the language instead of
    // incorrectly displaying "PHO".
    if let Some((_, suffix)) = label.rsplit_once('(') {
        let suffix = suffix.trim_end_matches(')').trim();
        let suffix_code = suffix
            .split(|c: char| !c.is_ascii_alphanumeric())
            .find(|part| !part.is_empty())
            .unwrap_or("");
        if let Some(code) = known_layout_code(suffix_code) {
            return Some(code.to_string());
        }
    }

    let first = label
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|part| !part.is_empty())?;
    if let Some(code) = known_layout_code(first) {
        return Some(code.to_string());
    }

    Some(
        first
            .chars()
            .take(3)
            .collect::<String>()
            .to_ascii_uppercase(),
    )
}

fn known_layout_code(value: &str) -> Option<&'static str> {
    match value.to_ascii_lowercase().as_str() {
        "english" => Some("EN"),
        "bulgarian" => Some("BG"),
        "german" => Some("DE"),
        "french" => Some("FR"),
        "russian" => Some("RU"),
        "spanish" => Some("ES"),
        "italian" => Some("IT"),
        "us" => Some("US"),
        "gb" | "uk" => Some("GB"),
        "bg" => Some("BG"),
        "de" => Some("DE"),
        "fr" => Some("FR"),
        "ru" => Some("RU"),
        "es" => Some("ES"),
        "it" => Some("IT"),
        "ua" => Some("UA"),
        "pl" => Some("PL"),
        "tr" => Some("TR"),
        _ => None,
    }
}

/// Format a libc::tm using strftime.
///
/// # Safety
/// Calls libc::strftime which requires a valid tm struct and format string.
fn format_tm(local: &libc::tm, format: &'static [u8]) -> String {
    let mut output = [0i8; 64];
    let written = unsafe {
        libc::strftime(
            output.as_mut_ptr(),
            output.len(),
            format.as_ptr() as *const libc::c_char,
            local as *const libc::tm,
        )
    };
    if written == 0 {
        return String::new();
    }
    let bytes = output[..written as usize]
        .iter()
        .map(|c| *c as u8)
        .collect::<Vec<_>>();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Get the current local date and time as formatted strings.
///
/// # Safety
/// Calls libc::time and libc::localtime_r which require proper initialization.
fn local_date_and_time() -> (String, String) {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    if now == libc::time_t::MAX {
        return (String::new(), String::new());
    }
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&now, &mut local) }.is_null() {
        return (String::new(), String::new());
    }
    (format_tm(&local, b"%d %b\0"), format_tm(&local, b"%H:%M\0"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rates_are_compact() {
        assert_eq!(format_rate(0), "0B/s");
        assert_eq!(format_rate(1536), "2K/s");
        assert_eq!(format_rate(1_572_864), "1.5M/s");
        assert_eq!(format_rate(1_610_612_736), "1.5G/s");
    }

    #[test]
    fn layout_names_are_short() {
        assert_eq!(normalize_layout_label("English (US)"), Some("US".into()));
        assert_eq!(normalize_layout_label("Bulgarian"), Some("BG".into()));
        assert_eq!(
            normalize_layout_label("Bulgarian (Phonetic)"),
            Some("BG".into())
        );
        assert_eq!(
            normalize_layout_label("English (Colemak)"),
            Some("EN".into())
        );
    }
}
