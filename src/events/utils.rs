//! Event utility functions.
//!
//! Helpers for creating and dispatching domain events.

use std::path::PathBuf;

/// Parses raw byte flags from foreign toplevel state into (activated, minimized).
pub fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;

    for chunk in state_bytes.chunks_exact(4) {
        let value = u32::from_ne_bytes(chunk.try_into().unwrap());
        match value {
            2 => activated = true,
            1 => minimized = true,
            _ => {}
        }
    }

    (activated, minimized)
}

/// Parses a `text/uri-list` string into valid local PathBuf instances.
pub fn parse_uri_list(buffer: &str) -> Vec<PathBuf> {
    buffer
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let path_str = if let Some(stripped) = line.strip_prefix("file://localhost") {
                stripped
            } else if let Some(stripped) = line.strip_prefix("file://") {
                stripped
            } else {
                line
            };

            percent_decode(path_str).map(PathBuf::from)
        })
        .collect()
}

/// Decodes percent-encoded character sequences in file paths.
pub fn percent_decode(input: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut chars = input.bytes();

    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next()?;
            let h2 = chars.next()?;

            let hex_bytes = [h1, h2];
            let hex_str = std::str::from_utf8(&hex_bytes).ok()?;
            let byte = u8::from_str_radix(hex_str, 16).ok()?;

            bytes.push(byte);
        } else {
            bytes.push(b);
        }
    }

    String::from_utf8(bytes).ok()
}
