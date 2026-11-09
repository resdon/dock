//! PipeWire window capture service for the dock.
//!
//! Captures all open windows via XDG Desktop Portal and provides frame data
//! for the window_list popup. Each window has its own PipeWire stream thread.

use std::collections::{HashMap, HashSet};
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::Proxy;

use pipewire as pw;
use pw::properties::properties;
use pw::spa::pod::Pod;

    /// Frame data from a single PipeWire stream
pub struct StreamFrameData {
    /// Latest frame: (width, height, format, raw bytes)
    pub frame: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat, Vec<u8>)>>>,
    /// Negotiated format info: (width, height, format)
    pub format_info: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat)>>>,
    /// Whether format negotiation has completed
    pub ready: Arc<AtomicBool>,
    /// Window title for this stream
    pub title: String,
    /// Stable Wayland window ID (protocol_id from ObjectId), set at capture time.
    /// Once set, this stream is bound to this window regardless of title changes.
    /// Stored in a Mutex so the monitor task can update it after stream retention.
    pub window_id: Arc<Mutex<Option<u32>>>,
}

/// Capture state for all windows
pub struct CaptureState {
    /// PipeWire file descriptors (one per portal session)
    pub pw_fds: Vec<OwnedFd>,
    /// Node IDs for each captured window
    pub node_ids: Vec<u32>,
    /// Window titles for each captured window
    pub titles: Vec<String>,
    /// Number of captured windows
    pub count: usize,
}

/// Persistent binding from a Wayland window ID to a PipeWire stream index.
/// Once set, the window always uses this stream regardless of title changes.
#[derive(Clone, Debug)]
pub struct StreamBinding {
    pub window_id: u32,
    pub stream_index: usize,
    pub title_at_bind: String,
}

/// Shared state for the capture service
pub struct PipewireCaptureService {
    pub capture_state: Arc<Mutex<CaptureState>>,
    pub stream_frames: Arc<Mutex<Vec<Arc<StreamFrameData>>>>,
    pub active_row: Arc<Mutex<usize>>,
    pub running: Arc<AtomicBool>,
    /// Signal to probe for new windows (set by event_compat on window create)
    pub probe_signal: Arc<tokio::sync::Notify>,
    /// Last probe time for debouncing
    pub last_probe_time: Arc<Mutex<Instant>>,
    /// Shared map from window title to stable Wayland window IDs (protocol_id).
    /// Multiple windows can have the same title, so we store a Vec of IDs.
    /// Updated by the main draw loop so the monitor task can bind streams to window IDs.
    pub window_ids: Arc<Mutex<HashMap<String, Vec<u32>>>>,
    /// Persistent binding registry: window_id → stream_index.
    /// Once a PipeWire stream is tied to a Wayland window, it stays tied
    /// until the window closes or the stream is removed.
    /// This eliminates the need for title matching on every frame.
    pub bindings: Arc<Mutex<HashMap<u32, usize>>>,
    /// XDG Desktop Portal restore token for screen capture permission.
    /// Saved after the first successful capture and reused on subsequent starts
    /// to bypass the portal confirmation dialog.
    pub restore_token: Arc<Mutex<Option<String>>>,
}

impl PipewireCaptureService {
    pub fn new() -> Self {
        Self {
            capture_state: Arc::new(Mutex::new(CaptureState {
                pw_fds: Vec::new(),
                node_ids: Vec::new(),
                titles: Vec::new(),
                count: 0,
            })),
            stream_frames: Arc::new(Mutex::new(Vec::new())),
            active_row: Arc::new(Mutex::new(0)),
            running: Arc::new(AtomicBool::new(false)),
            probe_signal: Arc::new(tokio::sync::Notify::new()),
            last_probe_time: Arc::new(Mutex::new(Instant::now())),
            window_ids: Arc::new(Mutex::new(HashMap::new())),
            bindings: Arc::new(Mutex::new(HashMap::new())),
            restore_token: Arc::new(Mutex::new(load_restore_token())),
        }
    }

    /// Update the shared window title-to-IDs map from the current Wayland window list.
    /// Called by the main draw loop so the monitor task can bind streams to stable IDs.
    /// Multiple windows can have the same title, so each title maps to a Vec of IDs.
    pub fn update_window_ids(&self, windows: &[(String, u32)]) {
        let mut map = self.window_ids.lock().unwrap();
        map.clear();
        for (title, id) in windows {
            map.entry(title.clone()).or_insert_with(Vec::new).push(*id);
        }
    }

    /// Get the persistently bound stream index for a window ID.
    /// Returns None if the window has no binding or the stream no longer exists.
    pub fn get_bound_stream(&self, window_id: u32) -> Option<usize> {
        let bindings = self.bindings.lock().unwrap();
        if let Some(&stream_idx) = bindings.get(&window_id) {
            let frames = self.stream_frames.lock().unwrap();
            if stream_idx < frames.len() {
                return Some(stream_idx);
            }
        }
        None
    }

    /// Persistently bind a window ID to a stream index.
    /// Once bound, the window always uses this stream regardless of title changes.
    pub fn bind_window_to_stream(&self, window_id: u32, stream_idx: usize, title: &str) {
        let mut bindings = self.bindings.lock().unwrap();
        bindings.insert(window_id, stream_idx);
    }

    /// Unbind a window ID (e.g., when the window closes)
    pub fn unbind_window(&self, window_id: u32) {
        let mut bindings = self.bindings.lock().unwrap();
        let _ = bindings.remove(&window_id);
    }

    /// Unbind all windows bound to a specific stream index, then remap bindings
    /// for streams that shifted after removal.
    pub fn unbind_and_remap_stream(&self, removed_stream_idx: usize) {
        let mut bindings = self.bindings.lock().unwrap();
        // Remove bindings to the deleted stream
        bindings.retain(|_window_id, stream_idx| {
            if *stream_idx == removed_stream_idx {
                false // stream gone, unbind
            } else if *stream_idx > removed_stream_idx {
                // Stream shifted down by 1 after removal
                *stream_idx -= 1;
                true
            } else {
                true
            }
        });
    }

    /// Check if a window already has a persistent binding
    pub fn is_window_bound(&self, window_id: u32) -> bool {
        let bindings = self.bindings.lock().unwrap();
        bindings.contains_key(&window_id)
    }

    /// Reverse lookup: find the window ID bound to a given stream index.
    /// Used when clicking a dock icon to find which window's stream is currently displayed.
    pub fn find_window_by_stream_idx(&self, stream_idx: usize) -> Option<u32> {
        let bindings = self.bindings.lock().unwrap();
        for (&window_id, &idx) in bindings.iter() {
            if idx == stream_idx {
                return Some(window_id);
            }
        }
        None
    }

    /// Find the PipeWire stream index that matches a window by its stable Wayland ID.
    /// First checks the persistent binding registry, then falls back to scanning streams.
    pub fn find_stream_by_window_id(&self, window_id: u32, exclude: &[usize]) -> Option<usize> {
        // Fast path: check persistent binding registry
        if let Some(idx) = self.get_bound_stream(window_id) {
            if !exclude.contains(&idx) {
                return Some(idx);
            }
        }
        // Slow path: scan streams by window_id field (for streams not yet in registry)
        let frames = self.stream_frames.lock().unwrap();
        for (i, stream) in frames.iter().enumerate() {
            if exclude.contains(&i) {
                continue;
            }
            if let Some(sid) = stream.window_id.lock().unwrap().as_ref().copied() {
                if sid == window_id {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Signal the monitor to probe for new windows (debounced to 500ms)
    pub fn request_probe(&self) {
        let now = Instant::now();
        let mut last = self.last_probe_time.lock().unwrap();
        if now.duration_since(*last) >= std::time::Duration::from_millis(500) {
            *last = now;
            self.probe_signal.notify_one();
        }
    }

    /// Check if a window title has a matching PipeWire stream.
    /// Returns true if the title matches any stream.
    pub fn has_stream_for_title(&self, title: &str) -> bool {
        self.find_stream_by_title(title, &[]).is_some()
    }

    /// Set the active row (hovered window index)
    pub fn set_active_row(&self, row: usize) {
        let mut r = self.active_row.lock().unwrap();
        *r = row;
    }

    /// Get the latest frame for the active row, converted to RGBA
    pub fn get_active_frame(&self) -> Option<(u32, u32, Vec<u8>)> {
        let row = *self.active_row.lock().unwrap();
        let frames = self.stream_frames.lock().unwrap();
        if row >= frames.len() {
            return None;
        }
        let stream = &frames[row];
        let frame_data = stream.frame.lock().unwrap().clone();
        match frame_data {
            Some((w, h, fmt, bytes)) => Some(convert_to_rgba(w, h, fmt, &bytes, 0.25)),
            None => None,
        }
    }

    /// Get the number of captured windows
    pub fn window_count(&self) -> usize {
        self.capture_state.lock().unwrap().count
    }

    /// Get the title for a window index
    pub fn window_title(&self, idx: usize) -> String {
        let state = self.capture_state.lock().unwrap();
        if idx < state.titles.len() {
            state.titles[idx].clone()
        } else {
            format!("Window {}", idx + 1)
        }
    }

    /// Get the original frame dimensions (width, height) for a stream index.
    /// Returns None if the stream hasn't negotiated format yet.
    pub fn get_stream_dimensions(&self, idx: usize) -> Option<(u32, u32)> {
        let frames = self.stream_frames.lock().unwrap();
        if idx >= frames.len() {
            return None;
        }
        let stream = &frames[idx];
        let fmt = stream.format_info.lock().unwrap().clone();
        fmt.map(|(w, h, _)| (w, h))
    }


    /// Strip the portal hash suffix from a PipeWire stream title.
    /// The portal appends a unique hex ID in parentheses, e.g. "dock (8f06968d...)".
    /// Matching should use the title without this suffix so "dock" matches "dock (hash)".
    fn strip_portal_hash(title: &str) -> &str {
        // Find the last " (" that starts a hex-only parenthesized suffix
        if let Some(pos) = title.rfind(" (") {
            let suffix = &title[pos + 2..];
            if suffix.ends_with(')') {
                let inner = &suffix[..suffix.len() - 1];
                // Check if it looks like a hex hash (32 hex chars = SHA-1 token from portal)
                if inner.len() == 32 && inner.chars().all(|c| c.is_ascii_hexdigit()) {
                    return &title[..pos];
                }
            }
        }
        title
    }

    /// Find the PipeWire stream index that matches a window title from foreign_toplevel.
    /// Returns None if no match is found.
    /// Uses stream_frames titles (from selected title file) which are more accurate than capture_state.titles.
    /// `exclude` is a set of stream indices that have already been matched to another window
    /// and should be skipped — this prevents two windows from the same app from matching
    /// the same stream.
    pub fn find_stream_by_title(&self, title: &str, exclude: &[usize]) -> Option<usize> {
        if title.is_empty() {
            return None;
        }
        let frames = self.stream_frames.lock().unwrap();
        let title_lower = title.to_lowercase();

        // Try exact match first (compare against the title-without-hash)
        for (i, stream) in frames.iter().enumerate() {
            if exclude.contains(&i) {
                continue;
            }
            let pw_title = Self::strip_portal_hash(&stream.title);
            if pw_title == title {
                return Some(i);
            }
        }
        // Try case-insensitive partial match (skip empty titles).
        // For short single-word titles (<=4 chars), skip partial match to avoid false positives
        // like "dock" matching "[1/2] resdon@archlinux:~/m.2/Rust/dock" where "dock" appears
        // as a substring in a file path rather than as the actual window title.
        let title_words: Vec<&str> = title_lower.split_whitespace().collect();
        let is_short_single_word = title_words.len() <= 1 && title.len() <= 4;
        if !is_short_single_word {
            for (i, stream) in frames.iter().enumerate() {
                if exclude.contains(&i) {
                    continue;
                }
                if stream.title.is_empty() {
                    continue;
                }
                let pw_title = Self::strip_portal_hash(&stream.title);
                let pw_lower = pw_title.to_lowercase();
                if pw_lower.contains(&title_lower) || title_lower.contains(&pw_lower) {
                    return Some(i);
                }
            }
        }
        // Try prefix matching: compare first N words to handle truncated titles.
        // Only use prefix matching when the title has at least 3 words (short titles like
        // "Untitled" would falsely match anything).
        if title_words.len() >= 3 {
            let prefix_count = title_words.len().min(3);
            for (i, stream) in frames.iter().enumerate() {
                if exclude.contains(&i) {
                    continue;
                }
                if stream.title.is_empty() {
                    continue;
                }
                let pw_title = Self::strip_portal_hash(&stream.title);
                let pw_lower = pw_title.to_lowercase();
                let pw_words: Vec<&str> = pw_lower.split_whitespace().collect();
                if pw_words.is_empty() {
                    continue;
                }
                let pw_prefix_count = pw_words.len().min(prefix_count);
                if pw_prefix_count >= 2 {
                    let mut matches = 0;
                    for j in 0..pw_prefix_count {
                        if title_words[j] == pw_words[j] {
                            matches += 1;
                        }
                    }
                    if matches >= 2 {
                        return Some(i);
                    }
                }
            }
        }
        // Try matching by checking if any word from the title appears in the PipeWire title.
        // Only use words longer than 4 chars to avoid false matches on common short words
        // like "the", "tab", "new", etc.
        let title_words: Vec<&str> = title_lower.split_whitespace().filter(|w| w.len() > 4).collect();
        if !title_words.is_empty() {
            for (i, stream) in frames.iter().enumerate() {
                if exclude.contains(&i) {
                    continue;
                }
                if stream.title.is_empty() {
                    continue;
                }
                let pw_title = Self::strip_portal_hash(&stream.title);
                let pw_lower = pw_title.to_lowercase();
                for word in &title_words {
                    if pw_lower.contains(word) {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

}

/// Initialize and start the capture service
pub async fn init_capture_service(
    service: &PipewireCaptureService,
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    max_windows: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    pw::init();

    // Get the saved restore token to bypass the portal dialog
    let restore_token = {
        let token = service.restore_token.lock().unwrap();
        token.clone()
    };
    // Capture all windows
    let results = capture_windows_range(connection, desktop, 0, max_windows, restore_token).await?;

    if results.is_empty() {
        // If capture failed, clear the stale restore token
        {
            let mut token = service.restore_token.lock().unwrap();
            *token = None;
        }
        return Ok(());
    }

    // Update the in-memory restore token from disk (capture_windows_range saves it)
    {
        let mut token = service.restore_token.lock().unwrap();
        *token = load_restore_token();
    }

    let pw_fds: Vec<OwnedFd> = results.iter().map(|(fd, _, _)| fd.try_clone().unwrap()).collect();
    let node_ids: Vec<u32> = results.iter().map(|(_, nid, _)| *nid).collect();
    let titles: Vec<String> = results.iter().map(|(_, _, t)| t.clone()).collect();
    let count = results.len();

    // Update capture state
    {
        let mut state = service.capture_state.lock().unwrap();
        state.pw_fds = pw_fds;
        state.node_ids = node_ids;
        state.titles = titles;
        state.count = count;
    }

    // Spawn stream threads and bind them to the persistent registry
    for (idx, (fd, node_id, title)) in results.into_iter().enumerate() {
        // Look up the stable Wayland window ID for this title (may not be set yet on init).
        // Strip the portal hash suffix so "dock (hash)" matches the Wayland title "dock".
        // Pick an ID that isn't already bound to another stream.
        let window_id = {
            let ids = service.window_ids.lock().unwrap();
            let bindings = service.bindings.lock().unwrap();
            let clean_title = PipewireCaptureService::strip_portal_hash(&title);
            ids.get(clean_title).and_then(|v| {
                v.iter().find(|id| !bindings.contains_key(id)).copied()
            })
        };
        let stream_data = Arc::new(StreamFrameData {
            frame: Arc::new(Mutex::new(None)),
            format_info: Arc::new(Mutex::new(None)),
            ready: Arc::new(AtomicBool::new(false)),
            title: title.clone(),
            window_id: Arc::new(Mutex::new(window_id)),
        });

        {
            let mut frames = service.stream_frames.lock().unwrap();
            if idx < frames.len() {
                frames[idx] = stream_data.clone();
            } else {
                frames.push(stream_data.clone());
            }
        }

        // Bind the initial stream to the window in the persistent registry
        if let Some(wid) = window_id {
            service.bind_window_to_stream(wid, idx, &title);
        }

        spawn_stream_thread(fd, node_id, idx, stream_data.clone());
    }

    // Wait for streams to negotiate format
    for i in 0..100 {
        let frames = service.stream_frames.lock().unwrap();
        let ready = frames.iter().filter(|s| s.ready.load(Ordering::Acquire)).count();
        if ready == count {
            break;
        }
        drop(frames);
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    service.running.store(true, Ordering::Release);
    Ok(())
}

fn next_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dock_{}", n)
}

/// Path where the XDG Desktop Portal restore token is stored
const RESTORE_TOKEN_PATH: &str = ".config/dock/pipewire_restore_token";

/// Load the saved restore token from disk
fn load_restore_token() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = format!("{}/{}", home, RESTORE_TOKEN_PATH);
    std::fs::read_to_string(&path).ok()
}

/// Save the restore token to disk for future use
fn save_restore_token(token: &str) {
    let home = match std::env::var("HOME") {
        Ok(h) => h,
        Err(_) => return,
    };
    let dir = format!("{}/.config/dock", home);
    if let Err(_) = std::fs::create_dir_all(&dir) {
        return;
    }
    let _ = std::fs::write(format!("{}/pipewire_restore_token", dir), token);
}

async fn create_session(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
) -> Result<OwnedObjectPath, Box<dyn std::error::Error>> {
    let handle_token = next_token();
    let session_handle_token = next_token();

    let unique = connection.unique_name().unwrap().as_str().trim_start_matches(':');
    let bus_name = unique.replace('.', "_");
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        bus_name, handle_token
    );

    let request = Proxy::new(
        connection,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    ).await?;

    let mut signal = request.receive_signal("Response").await?;

    let options: HashMap<String, Value<'_>> = HashMap::from([
        ("handle_token".to_string(), Value::Str(handle_token.as_str().into())),
        ("session_handle_token".to_string(), Value::Str(session_handle_token.as_str().into())),
        ("desktop_file".to_string(), Value::Str("list-widget.desktop".into())),
    ]);

    desktop.call_method("CreateSession", &(options,)).await?;

    let sig_opt = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        signal.next()
    ).await.expect("Timeout waiting for Response signal");
    let sig = sig_opt.ok_or("Signal stream ended")?;

    let (response, session_path): (u32, OwnedObjectPath) = sig.body().deserialize().unwrap_or_else(|_| {
        let (resp, props): (u32, HashMap<String, OwnedValue>) = sig.body().deserialize().expect("fallback failed");
        if let Some(val) = props.get("session_handle") {
            if let Value::Str(s) = &**val {
                let path = OwnedObjectPath::try_from(s.as_str()).expect("invalid session path");
                return (resp, path);
            }
        }
        if let Some(val) = props.get("session") {
            if let Value::ObjectPath(path) = &**val {
                return (resp, OwnedObjectPath::from(path.clone()));
            }
        }
        panic!("No session path in response dict");
    });
    if response != 0 {
        return Err(format!("Session creation failed: {}", response).into());
    }
    Ok(session_path)
}

async fn select_sources(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
) -> Result<(), Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let unique = connection.unique_name().unwrap().as_str().trim_start_matches(':');
    let bus_name = unique.replace('.', "_");
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        bus_name, handle_token
    );

    let request = Proxy::new(
        connection,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    ).await?;

    let mut signal = request.receive_signal("Response").await?;

    let options: HashMap<String, Value<'_>> = HashMap::from([
        ("handle_token".to_string(), Value::Str(handle_token.as_str().into())),
        ("types".to_string(), Value::U32(2)),  // 2 = windows only
        ("multiple".to_string(), Value::Bool(true)),
        ("cursor_mode".to_string(), Value::U32(1)),
        ("persist_mode".to_string(), Value::U32(2)),
    ]);

    desktop.call_method("SelectSources", &(session, options)).await?;

    // Timeout after 3 seconds — the portal dialog may never appear on some compositors
    let sig = match tokio::time::timeout(std::time::Duration::from_secs(3), signal.next()).await {
        Ok(Some(sig)) => sig,
        _ => return Err("SelectSources timed out (dialog did not appear)".into()),
    };
    let response: u32 = if let Ok((r,)) = sig.body().deserialize::<(u32,)>() {
        r
    } else {
        let (r, _props): (u32, HashMap<String, OwnedValue>) = sig.body().deserialize()
            .expect("SelectSources response");
        r
    };
    if response != 0 {
        return Err(format!("SelectSources canceled (response={})", response).into());
    }
    Ok(())
}

async fn start_capture(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
    restore_token: Option<&str>,
) -> Result<(Vec<(u32, HashMap<String, String>)>, Option<String>), Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let unique = connection.unique_name().unwrap().as_str().trim_start_matches(':');
    let bus_name = unique.replace('.', "_");
    let request_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        bus_name, handle_token
    );

    let request = Proxy::new(
        connection,
        "org.freedesktop.portal.Desktop",
        request_path.as_str(),
        "org.freedesktop.portal.Request",
    ).await?;

    let mut signal = request.receive_signal("Response").await?;

    // Build options with handle_token and optionally the restore_token
    let mut options: HashMap<String, Value<'_>> = HashMap::from([
        ("handle_token".to_string(), Value::Str(handle_token.as_str().into())),
    ]);
    if let Some(token) = restore_token {
        options.insert("restore_token".to_string(), Value::Str(token.into()));
    }

    desktop.call_method("Start", &(session, handle_token.as_str(), options)).await?;

    // Timeout after 3 seconds
    let sig = match tokio::time::timeout(std::time::Duration::from_secs(3), signal.next()).await {
        Ok(Some(sig)) => sig,
        _ => return Err("Start timed out".into()),
    };
    let (response, props): (u32, HashMap<String, OwnedValue>) = sig.body().deserialize()?;

    if response != 0 {
        return Err(format!("Start failed: {}", response).into());
    }

    let mut streams = Vec::new();
    let mut restore_token = None;

    for (key, val) in &props {
        if key == "streams" {
            if let Value::Array(arr) = &**val {
                for item in arr.inner() {
                    if let Value::Structure(tup) = item {
                        let mut node_id = 0u32;
                        let mut stream_props = HashMap::new();
                        for (i, field) in tup.fields().into_iter().enumerate() {
                            match field {
                                Value::U32(id) if i == 0 => {
                                    node_id = *id;
                                }
                                Value::Dict(dict) if i == 1 => {
                                    for (k, v) in dict.iter() {
                                        if let Value::Str(key_str) = k {
                                            stream_props.insert(key_str.to_string(), format!("{}", v));
                                        }
                                    }
                                }
                                Value::Array(dict_arr) if i == 1 => {
                                    for entry in dict_arr.inner() {
                                        if let Value::Structure(entry_tup) = entry {
                                            let mut entry_key = String::new();
                                            let mut entry_val = String::new();
                                            for (j, f) in entry_tup.fields().into_iter().enumerate() {
                                                match f {
                                                    Value::Str(s) if j == 0 => {
                                                        entry_key = s.to_string();
                                                    }
                                                    _ if j == 1 => {
                                                        entry_val = format!("{}", f);
                                                    }
                                                    _ => {}
                                                }
                                            }
                                            if !entry_key.is_empty() {
                                                stream_props.insert(entry_key, entry_val);
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        streams.push((node_id, stream_props));
                    }
                }
            }
        } else if key == "restore_token" {
            if let Value::Str(s) = &**val {
                restore_token = Some(s.to_string());
            }
        }
    }

    Ok((streams, restore_token))
}

async fn open_pipewire_remote(
    _connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
) -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let options: HashMap<String, Value<'_>> = HashMap::from([
        ("handle_token".to_string(), Value::Str(handle_token.as_str().into())),
    ]);

    let msg = desktop.call_method("OpenPipeWireRemote", &(session, options)).await?;

    let body = msg.body();
    let fd_val: zvariant::Fd<'_> = body.deserialize().map_err(|e| {
        "Failed to deserialize PipeWire FD"
    })?;

    let owned_fd = match fd_val {
        zvariant::Fd::Borrowed(borrowed) => {
            borrowed.try_clone_to_owned().map_err(|e| {
                "Failed to clone file descriptor"
            })?
        }
        zvariant::Fd::Owned(owned) => owned,
    };

    Ok(owned_fd)
}

async fn capture_windows_range(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    start_idx: usize,
    end_idx: usize,
    restore_token: Option<String>,
) -> Result<Vec<(OwnedFd, u32, String)>, Box<dyn std::error::Error>> {
    let mut results: Vec<(OwnedFd, u32, String)> = Vec::new();
    let mut last_restore_token = restore_token;

    for idx in start_idx..end_idx {
        // Tell the chooser which window to select
        std::fs::write("/tmp/chooser-index.txt", idx.to_string()).ok();

        let session = create_session(connection, desktop).await?;

        // Call SelectSources to invoke the chooser script
        if let Err(_) = select_sources(connection, desktop, &session).await {
            break;
        }

        let token_for_start = last_restore_token.clone();
        let (streams, new_restore_token) = match start_capture(connection, desktop, &session, token_for_start.as_deref()).await {
            Ok(s) => s,
            Err(_) => {
                break;
            }
        };

        // Save the restore token for the next window and for future sessions
        if let Some(ref token) = new_restore_token {
            save_restore_token(token);
        }
        last_restore_token = new_restore_token;

        if streams.is_empty() {
            break;
        }

        let fd = open_pipewire_remote(connection, desktop, &session).await?;

        // Read the selected window's title from the chooser script.
        // The chooser saves the exact title of the selected window to
        // /tmp/chooser-selected-title.txt, so we know which window was captured.
        let selected_title = std::fs::read_to_string("/tmp/chooser-selected-title.txt")
            .ok()
            .and_then(|content| {
                let title = content.trim().to_string();
                if title.is_empty() { None } else { Some(title) }
            });

        for (node_id, _props) in &streams {
            // Keep the full title — truncation prevents distinguishing multiple windows
            // from the same app (e.g. two wezterm tabs with different titles).
            let title = match selected_title.clone() {
                Some(t) => t,
                None => {
                    format!("Window {}", results.len() + 1)
                }
            };
            results.push((fd.try_clone().unwrap(), *node_id, title));
        }
    }

    Ok(results)
}

fn spawn_stream_thread(fd: OwnedFd, node_id: u32, idx: usize, stream_data: Arc<StreamFrameData>) {
    std::thread::spawn(move || {
        let mainloop = match pw::main_loop::MainLoopRc::new(None) {
            Ok(ml) => ml,
            Err(_) => { return; }
        };
        let context = match pw::context::ContextRc::new(&mainloop, None) {
            Ok(ctx) => ctx,
            Err(_) => { return; }
        };
        let loop_ = mainloop.loop_();

        let core = match context.connect_fd_rc(fd, None) {
            Ok(c) => c,
            Err(_) => { return; }
        };

        let stream = match pw::stream::StreamRc::new(
            core.clone(),
            &format!("dock-capture-{}", idx),
            properties! {
                *pw::keys::MEDIA_TYPE => "Video",
                *pw::keys::MEDIA_CATEGORY => "Capture",
                *pw::keys::MEDIA_ROLE => "Camera",
            },
        ) {
            Ok(s) => s,
            Err(_) => { return; }
        };

        let frame_data_for_process = Arc::clone(&stream_data.frame);
        let format_info_for_process = Arc::clone(&stream_data.format_info);
        let format_info_for_param = Arc::clone(&stream_data.format_info);
        let ready_flag_for_param = Arc::clone(&stream_data.ready);

        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |_stream, _user_data: &mut (), _old, _new| {
            })
            .param_changed(move |stream, _user_data: &mut (), id, param| {
                let Some(param) = param else { return; };
                if id != pw::spa::param::ParamType::Format.as_raw() { return; }

                let (media_type, media_subtype) = match pw::spa::param::format_utils::parse_format(param) {
                    Ok(v) => v,
                    Err(_) => return,
                };

                if media_type != pw::spa::param::format::MediaType::Video
                    || media_subtype != pw::spa::param::format::MediaSubtype::Raw
                {
                    return;
                }

                let mut format = pw::spa::param::video::VideoInfoRaw::new();
                if format.parse(param).is_ok() {
                    format_info_for_param.lock().unwrap().replace((
                        format.size().width, format.size().height, format.format()
                    ));
                    if stream.set_active(true).is_ok() {
                        ready_flag_for_param.store(true, Ordering::Release);
                    }
                }
            })
            .process(move |stream, _| {
                match stream.dequeue_buffer() {
                    None => {}
                    Some(mut buffer) => {
                        let datas = buffer.datas_mut();
                        if datas.is_empty() { return; }

                        let data = &mut datas[0];
                        let chunk = data.chunk();
                        let size = chunk.size() as usize;

                        if let Some(samples) = data.data() {
                            if size > 0 {
                                let (w, h, fmt) = {
                                    let fi = format_info_for_process.lock().unwrap();
                                    match fi.as_ref() {
                                        Some(f) => (f.0, f.1, f.2),
                                        None => (0, 0, pw::spa::param::video::VideoFormat::RGB),
                                    }
                                };
                                if w > 0 && h > 0 {
                                    let bytes = samples[0..size.min(samples.len())].to_vec();
                                    frame_data_for_process.lock().unwrap().replace((w, h, fmt, bytes));
                                }
                            }
                        }
                    }
                }
            })
            .register()
            .unwrap();

        // Build format parameter bytes
        let obj = pw::spa::pod::object!(
            pw::spa::utils::SpaTypes::ObjectParamFormat,
            pw::spa::param::ParamType::EnumFormat,
            pw::spa::pod::property!(
                pw::spa::param::format::FormatProperties::MediaType,
                Id,
                pw::spa::param::format::MediaType::Video
            ),
            pw::spa::pod::property!(
                pw::spa::param::format::FormatProperties::MediaSubtype,
                Id,
                pw::spa::param::format::MediaSubtype::Raw
            ),
            pw::spa::pod::property!(
                pw::spa::param::format::FormatProperties::VideoFormat,
                Choice,
                Enum,
                Id,
                pw::spa::param::video::VideoFormat::BGRx,
                pw::spa::param::video::VideoFormat::BGRA,
                pw::spa::param::video::VideoFormat::RGBA,
                pw::spa::param::video::VideoFormat::RGB,
            ),
            pw::spa::pod::property!(
                pw::spa::param::format::FormatProperties::VideoSize,
                Choice,
                Range,
                Rectangle,
                pw::spa::utils::Rectangle { width: 1920, height: 1080 },
                pw::spa::utils::Rectangle { width: 1, height: 1 },
                pw::spa::utils::Rectangle { width: 7680, height: 4320 }
            ),
            pw::spa::pod::property!(
                pw::spa::param::format::FormatProperties::VideoFramerate,
                Choice,
                Range,
                Fraction,
                pw::spa::utils::Fraction { num: 30, denom: 1 },
                pw::spa::utils::Fraction { num: 0, denom: 1 },
                pw::spa::utils::Fraction { num: 60, denom: 1 }
            ),
        );
        let values: Vec<u8> = pw::spa::pod::serialize::PodSerializer::serialize(
            std::io::Cursor::new(Vec::new()),
            &pw::spa::pod::Value::Object(obj),
        ).unwrap().0.into_inner();

        let mut stream_params = vec![Pod::from_bytes(&values).unwrap()];
        if stream.connect(
            pw::spa::utils::Direction::Input,
            Some(node_id),
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::RT_PROCESS,
            &mut stream_params,
        ).is_err() {
            return;
        }

        // Run the main loop persistently
        loop {
            let timeout = pw::loop_::Timeout::Finite(std::time::Duration::from_millis(16));
            loop_.iterate(timeout);
        }
    });
}

/// Start a background monitor task that waits for probe signals (event-driven)
/// and incrementally adds new PipeWire streams without disrupting existing ones.
/// Must be called from within a tokio runtime context.
pub async fn start_monitor_inner(
    service: &PipewireCaptureService,
    connection: zbus::Connection,
    desktop: Proxy<'static>,
    max_windows: usize,
) {
    let service_state = service.capture_state.clone();
    let service_frames = service.stream_frames.clone();
    let probe_signal = service.probe_signal.clone();
    let initial_count = service.capture_state.lock().unwrap().count;

    let mut last_count = initial_count;
    loop {
        // Wait for a probe signal instead of polling on a timer
        probe_signal.notified().await;

        // First, trigger a full portal session (SelectSources + Start) to refresh the chooser file
        // with all current window titles. The chooser script only runs during Start, so we must
        // complete the full cycle. We discard the resulting stream FD and node ID.
        {
            let _ = std::fs::write("/tmp/chooser-index.txt", "0");
            let session = match create_session(&connection, &desktop).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            if let Err(_) = select_sources(&connection, &desktop, &session).await {
                continue;
            }
            // Start the capture to trigger the chooser script, then discard the stream
            let portal_restore_token = {
                let token = service.restore_token.lock().unwrap();
                token.clone()
            };
            let _ = start_capture(&connection, &desktop, &session, portal_restore_token.as_deref()).await;
        }

        // Small async delay to let the chooser script finish writing the file
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Read all window titles from the chooser file to detect new windows.
        // The chooser file is updated by chooser-all.sh on every portal call.
        // Keep full titles — truncation prevents distinguishing multiple windows
        // from the same app (e.g. two wezterm tabs with different titles).
        let chooser_titles: Vec<String> = if let Ok(content) = std::fs::read_to_string("/tmp/chooser-windows.txt") {
            content.lines()
                .filter(|l| !l.is_empty())
                .map(|l| l.trim().to_string())
                .collect()
        } else {
            continue;
        };

        // Truncated titles for debug logging only
        let chooser_titles_truncated: Vec<String> = chooser_titles.iter().map(|t| {
            if t.len() > 20 {
                format!("{}...", &t[..17])
            } else {
                t.clone()
            }
        }).collect();

        let new_count = chooser_titles.len();

        if new_count == 0 {
            continue;
        }

        // --- Phase 1: Remove stale streams whose titles no longer exist in the Portal ---
        // Use a count-based approach: if a title appears N times in the Portal list,
        // keep at most N streams with that title. This handles duplicate titles from
        // the same app (e.g. two Edge "New tab" windows).
        // Strip portal hashes before comparing so "dock (hash1)" matches "dock (hash2)".
        // After removal, update the persistent binding registry to reflect the changes.
        
        // Save the active row's window_id and stream count BEFORE retain() removes streams.
        // After removal, we'll remap active_row to the new index of this window.
        let (active_window_id_before_removal, stream_count_before) = {
            let row = *service.active_row.lock().unwrap();
            let frames = service_frames.lock().unwrap();
            let wid = frames.get(row).and_then(|s| *s.window_id.lock().unwrap());
            (wid, frames.len())
        };
        
        {
            let mut frames = service_frames.lock().unwrap();
            let before = frames.len();

            // Count how many times each portal title appears (hash-stripped)
            let mut portal_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for ct in &chooser_titles {
                let clean = PipewireCaptureService::strip_portal_hash(ct).to_string();
                *portal_counts.entry(clean).or_insert(0) += 1;
            }

            // Track how many of each title we've kept (hash-stripped)
            let mut kept_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            frames.retain(|stream| {
                let clean_title = PipewireCaptureService::strip_portal_hash(&stream.title).to_string();
                let max_keep = *portal_counts.get(&clean_title).unwrap_or(&0);
                let current_kept = *kept_counts.get(&clean_title).unwrap_or(&0);
                if current_kept < max_keep {
                    *kept_counts.entry(clean_title).or_insert(0) += 1;
                    true
                } else {
                    false
                }
            });

            let removed = before - frames.len();
            if removed > 0 {
                // Unbind removed streams from the registry and remap remaining bindings
                // Since retain removes from the end first conceptually, we need to figure out
                // which indices were removed. The simplest approach: unbind all and rebuild.
                let window_ids_map = service.window_ids.lock().unwrap();
                let mut bindings = service.bindings.lock().unwrap();
                
                // Build a reverse map: stream_index → window_id for retained streams
                let mut retained_bindings: std::collections::HashMap<usize, u32> = std::collections::HashMap::new();
                for (i, stream) in frames.iter().enumerate() {
                    if let Some(sid) = *stream.window_id.lock().unwrap() {
                        retained_bindings.insert(i, sid);
                    }
                }
                
                // Clear and rebuild bindings from retained streams
                bindings.clear();
                for (stream_idx, window_id) in retained_bindings {
                    bindings.insert(window_id, stream_idx);
                }
                drop(window_ids_map);
            }

            // Phase 1b: For retained streams without a window_id, try to bind them now.
            // Streams that already have a binding in the registry keep it.
            {
                let window_ids_map = service.window_ids.lock().unwrap();
                let bindings = service.bindings.lock().unwrap();
                
                // Collect which window IDs are already bound
                let bound_window_ids: HashSet<u32> = bindings.keys().copied().collect();
                drop(bindings);
                
                // For each unbound stream, try to assign a window ID
                let mut newly_bound: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
                for (i, stream) in frames.iter().enumerate() {
                    let current_id = *stream.window_id.lock().unwrap();
                    if current_id.is_some() {
                        continue;
                    }
                    let clean_title = PipewireCaptureService::strip_portal_hash(&stream.title).to_string();
                    if let Some(ids) = window_ids_map.get(&clean_title) {
                        for &id in ids {
                            if !bound_window_ids.contains(&id) {
                                *stream.window_id.lock().unwrap() = Some(id);
                                newly_bound.insert(id, i);
                                break;
                            }
                        }
                    }
                }
                drop(window_ids_map);
                
                // Register the new bindings
                for (window_id, stream_idx) in newly_bound {
                    let title = frames[stream_idx].title.clone();
                    service.bind_window_to_stream(window_id, stream_idx, &title);
                }
            }
        }

        // Remap active_row after Phase 1 removal: if the previously active window
        // still has a stream, point active_row to its new index.
        let stream_count_after = {
            let frames = service_frames.lock().unwrap();
            frames.len()
        };
        let streams_removed = stream_count_before > stream_count_after;
        
        if let Some(wid) = active_window_id_before_removal {
            if let Some(new_idx) = service.get_bound_stream(wid) {
                service.set_active_row(new_idx);
            }
            // If the active window's stream was removed, fall through to clamp below
        } else if streams_removed {
            // No window_id bound and streams were removed — the active_row may now
            // point to a different stream. Reset to 0 so the popup shows the first.
            service.set_active_row(0);
        }
        // Always clamp active_row to the valid range after removal
        {
            let frames = service_frames.lock().unwrap();
            let mut row = service.active_row.lock().unwrap();
            if frames.is_empty() {
                *row = 0;
            } else if *row >= frames.len() {
                *row = frames.len() - 1;
            }
        }

        // Re-read existing titles after removal (collect before any .await)
        let existing_titles: Vec<String> = {
            let frames = service_frames.lock().unwrap();
            frames.iter().map(|f| f.title.clone()).collect()
        };
        let current_stream_count = existing_titles.len();

        // --- Phase 2: Add new streams for windows that don't have one yet ---
        // Use count-based matching: if a Portal title appears N times but only M streams
        // exist with that title (M < N), we need to capture N - M more.
        // Strip portal hashes before comparing so "dock (hash1)" matches "dock (hash2)".
        let mut to_capture: Vec<usize> = Vec::new();
        {
            // Count existing streams per title (hash-stripped)
            let mut stream_title_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for t in &existing_titles {
                let clean = PipewireCaptureService::strip_portal_hash(t).to_string();
                *stream_title_counts.entry(clean).or_insert(0) += 1;
            }

            // Count portal windows per title (hash-stripped)
            let mut portal_title_counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for ct in &chooser_titles {
                let clean = PipewireCaptureService::strip_portal_hash(ct).to_string();
                *portal_title_counts.entry(clean).or_insert(0) += 1;
            }

            // Track how many streams we already have per title (for incremental matching)
            let mut matched_so_far: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

            for (i, title) in chooser_titles.iter().enumerate() {
                let clean_title = PipewireCaptureService::strip_portal_hash(title).to_string();
                let streams_with_title = *stream_title_counts.get(&clean_title).unwrap_or(&0);
                let portal_with_title = *portal_title_counts.get(&clean_title).unwrap_or(&0);
                let already_matched = *matched_so_far.get(&clean_title).unwrap_or(&0);

                // Total streams we should have for this title = portal count
                // Already have = streams_with_title (existing) + already_matched (from this loop)
                let total_have = streams_with_title + already_matched;
                if total_have < portal_with_title {
                    // Need to capture this one
                    if to_capture.len() >= max_windows - current_stream_count {
                        break;
                    }
                    to_capture.push(i);
                    *matched_so_far.entry(clean_title).or_insert(0) += 1;
                }
            }
        }

        if to_capture.is_empty() {
            // Update titles in capture state even if nothing changed structurally
            {
                let mut state = service_state.lock().unwrap();
                state.titles = chooser_titles.clone();
                state.count = current_stream_count;
            }
            last_count = current_stream_count;
            probe_signal.notified().await;
            continue;
        }

        // Capture each new window
        let mut added_count = 0;
        let current_restore_token = {
            let token = service.restore_token.lock().unwrap();
            token.clone()
        };
        for (idx, portal_idx) in to_capture.into_iter().enumerate() {
            match capture_windows_range(&connection, &desktop, portal_idx, portal_idx + 1, current_restore_token.clone()).await {
                Ok(new_streams) => {
                    for (fd, node_id, captured_title) in new_streams {
                        let stream_idx = current_stream_count + idx + added_count;
                        // Look up the stable Wayland window ID for this title.
                        // Strip the portal hash suffix so "dock (hash)" matches Wayland title "dock".
                        // Pick an ID that isn't already bound to another stream.
                        let window_id = {
                            let ids = service.window_ids.lock().unwrap();
                            let bindings = service.bindings.lock().unwrap();
                            let clean_title = PipewireCaptureService::strip_portal_hash(&captured_title);
                            // Find an unbound window ID for this title
                            ids.get(clean_title).and_then(|v| {
                                v.iter().find(|id| !bindings.contains_key(id)).copied()
                            })
                        };
                        let stream_data = Arc::new(StreamFrameData {
                            frame: Arc::new(Mutex::new(None)),
                            format_info: Arc::new(Mutex::new(None)),
                            ready: Arc::new(AtomicBool::new(false)),
                            title: captured_title.clone(),
                            window_id: Arc::new(Mutex::new(window_id)),
                        });

                        // Add to shared stream_frames vector (lock guard dropped before next .await)
                        {
                            let mut frames = service_frames.lock().unwrap();
                            frames.push(stream_data.clone());
                        }

                        // Bind the new stream to the window in the persistent registry
                        if let Some(wid) = window_id {
                            service.bind_window_to_stream(wid, stream_idx, &captured_title);
                        }

                        spawn_stream_thread(fd, node_id, stream_idx, stream_data.clone());
                        added_count += 1;
                    }
                }
                Err(_) => {}
            }
        }

        // Update capture state
        {
            let mut state = service_state.lock().unwrap();
            state.count = current_stream_count + added_count;
            state.titles = chooser_titles.clone();
        }
        last_count = current_stream_count + added_count;
    }
}

/// Convert a raw video frame to RGBA with optional downscaling
fn convert_to_rgba(
    w: u32, h: u32, fmt: pw::spa::param::video::VideoFormat, bytes: &[u8],
    scale_factor: f64,
) -> (u32, u32, Vec<u8>) {
    let out_w = if scale_factor < 1.0 {
        (w as f64 * scale_factor).max(1.0) as u32
    } else {
        w
    };
    let out_h = if scale_factor < 1.0 {
        (h as f64 * scale_factor).max(1.0) as u32
    } else {
        h
    };

    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];

    match fmt {
        pw::spa::param::video::VideoFormat::BGRx | pw::spa::param::video::VideoFormat::BGRA => {
            if scale_factor < 1.0 {
                let step_x = (w as f64 / out_w as f64) as usize;
                let step_y = (h as f64 / out_h as f64) as usize;
                for y in 0..out_h as usize {
                    let src_row = y * step_y * w as usize * 4;
                    for x in 0..out_w as usize {
                        let src = src_row + x * step_x * 4;
                        let dst = (y * out_w as usize + x) * 4;
                        if src + 3 < bytes.len() && dst + 3 < rgba.len() {
                            rgba[dst] = bytes[src + 2];     // R
                            rgba[dst + 1] = bytes[src + 1]; // G
                            rgba[dst + 2] = bytes[src];     // B
                            rgba[dst + 3] = 255;            // A
                        }
                    }
                }
            } else {
                for chunk in bytes.chunks_exact(4) {
                    let dst = ((chunk.as_ptr() as usize - bytes.as_ptr() as usize) / 4) * 4;
                    if dst + 3 < rgba.len() {
                        rgba[dst] = chunk[2];
                        rgba[dst + 1] = chunk[1];
                        rgba[dst + 2] = chunk[0];
                        rgba[dst + 3] = 255;
                    }
                }
            }
        }
        pw::spa::param::video::VideoFormat::RGBA => {
            if scale_factor < 1.0 {
                let step_x = (w as f64 / out_w as f64) as usize;
                let step_y = (h as f64 / out_h as f64) as usize;
                for y in 0..out_h as usize {
                    let src_row = y * step_y * w as usize * 4;
                    for x in 0..out_w as usize {
                        let src = src_row + x * step_x * 4;
                        let dst = (y * out_w as usize + x) * 4;
                        if src + 3 < bytes.len() && dst + 3 < rgba.len() {
                            rgba[dst..dst + 4].copy_from_slice(&bytes[src..src + 4]);
                        }
                    }
                }
            } else {
                let len = bytes.len().min(rgba.len());
                rgba[..len].copy_from_slice(&bytes[..len]);
            }
        }
        pw::spa::param::video::VideoFormat::RGB => {
            if scale_factor < 1.0 {
                let step_x = (w as f64 / out_w as f64) as usize;
                let step_y = (h as f64 / out_h as f64) as usize;
                for y in 0..out_h as usize {
                    let src_row = y * step_y * w as usize * 3;
                    for x in 0..out_w as usize {
                        let src = src_row + x * step_x * 3;
                        let dst = (y * out_w as usize + x) * 4;
                        if src + 2 < bytes.len() && dst + 3 < rgba.len() {
                            rgba[dst] = bytes[src];
                            rgba[dst + 1] = bytes[src + 1];
                            rgba[dst + 2] = bytes[src + 2];
                            rgba[dst + 3] = 255;
                        }
                    }
                }
            } else {
                for chunk in bytes.chunks_exact(3) {
                    let dst = ((chunk.as_ptr() as usize - bytes.as_ptr() as usize) / 3) * 4;
                    if dst + 3 < rgba.len() {
                        rgba[dst] = chunk[0];
                        rgba[dst + 1] = chunk[1];
                        rgba[dst + 2] = chunk[2];
                        rgba[dst + 3] = 255;
                    }
                }
            }
        }
        _ => {
            // Fallback: treat as BGRx
            if scale_factor < 1.0 {
                let step_x = (w as f64 / out_w as f64) as usize;
                let step_y = (h as f64 / out_h as f64) as usize;
                for y in 0..out_h as usize {
                    let src_row = y * step_y * w as usize * 4;
                    for x in 0..out_w as usize {
                        let src = src_row + x * step_x * 4;
                        let dst = (y * out_w as usize + x) * 4;
                        if src + 3 < bytes.len() && dst + 3 < rgba.len() {
                            rgba[dst] = bytes[src + 2];
                            rgba[dst + 1] = bytes[src + 1];
                            rgba[dst + 2] = bytes[src];
                            rgba[dst + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    (out_w, out_h, rgba)
}
