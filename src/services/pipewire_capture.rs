//! # PipeWire Window Capture Service
//!
//! Captures live video frames from all open Wayland windows for the dock's
//! window list popup. This module bridges three subsystems:
//!
//! 1. **XDG Desktop Portal** (`org.freedesktop.portal.ScreenCast`) — requests
//!    screen capture permissions from the compositor via D-Bus (`zbus`). The
//!    portal creates sessions, selects source windows, and returns PipeWire
//!    node IDs and file descriptors.
//! 2. **PipeWire SPA streams** — each captured window gets a dedicated
//!    `std::thread` with its own `MainLoopRc` that continuously receives
//!    video buffers from the PipeWire node. Format negotiation requests
//!    BGRx/BGRA/RGBA/RGB at up to 60fps and 8K resolution.
//! 3. **Frame conversion** — raw video buffers are converted to RGBA with
//!    configurable downscaling (`pipewire_scale_factor`) for the popup thumbnails.
//!
//! ## Architecture
//!
//! ```text
//!  ┌─────────────┐     D-Bus      ┌──────────────────┐
//!  │  Dock App   │ ──────────────► │ XDG Desktop      │
//!  │  (main.rs)  │                 │ Portal (wlr)     │
//!  └──────┬──────┘                 └────────┬─────────┘
//!         │                                 │
//!         │  spawn_stream_thread()          │  OpenPipeWireRemote FD
//!         │  per window                     │
//!         ▼                                 ▼
//!  ┌──────────────┐              ┌──────────────────┐
//!  │ Pipewire-    │◄─────────────│ PipeWire Daemon  │
//!  │ Capture      │  SPA buffers │ (nodes per       │
//!  │ Service      │              │  window)         │
//!  └──────┬───────┘              └──────────────────┘
//!         │
//!         │  get_active_frame() → RGBA thumbnail
//!         ▼
//!  ┌──────────────┐
//!  │ Window List  │
//!  │ Popup Grid   │
//!  └──────────────┘
//! ```
//!
//! ## Window-to-Stream Matching
//!
//! Each PipeWire stream is bound to a stable Wayland window ID
//! (`protocol_id` from `foreign_toplevel`) at capture time. The binding
//! registry (`bindings: HashMap<u32, usize>`) persists across monitor
//! cycles so windows are matched by ID, not by title. Title matching
//! is only a fallback for unbound streams.
//!
//! ## Portal Integration
//!
//! - A chooser script (`chooser-all.sh`) auto-selects windows by index
//!   without showing a dialog.
//! - `persist_mode=2` makes the wlr portal remember the permission,
//!   so subsequent runs skip the chooser dialog.
//! - The wlr portal does not return a `restore_token` from `Start()` —
//!   it uses the PermissionStore internally. The in-memory `restore_token`
//!   field remains `None`, but the dialog is suppressed by the portal's
//!   built-in permission persistence.
//!
//! ## Monitor Task
//!
//! `start_monitor_inner()` runs as a background tokio task that reacts to
//! `probe_signal` notifications (fired on `WindowEvent::Created`). It
//! incrementally adds/removes streams when windows open/close, without
//! disrupting existing capture streams. On startup, a 1-second timeout
//! ensures the monitor runs at least once even if no probe fires.
//!
//! ## Dependencies
//!
//! - `pipewire = "0.10"` — Rust bindings for PipeWire
//! - `pipewire-sys = "0.10"` — Low-level FFI bindings
//! - `libspa-sys = "0.10"` — SPA (Simple Plugin API) bindings
//! - `zbus` — D-Bus communication with XDG Desktop Portal
//! - `futures-util` — async stream handling

use std::collections::{HashMap, HashSet};
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use parking_lot::Mutex;

use futures_util::StreamExt;
use zbus::zvariant::{OwnedObjectPath, OwnedVariant, Variant};
use zbus::Proxy;

use pipewire as pw;
use pw::properties::properties;
use pw::spa::pod::Pod;

/// Frame data from a single PipeWire capture stream.
///
/// Each captured window has one `StreamFrameData` instance, updated continuously
/// by its dedicated stream thread. The frame buffer holds the latest raw video
/// data, which is converted to RGBA on demand by `get_active_frame()`.
pub struct StreamFrameData {
    /// Latest captured frame: (width, height, video format, raw pixel bytes).
    /// Updated by the stream thread's `process` callback on every buffer.
    /// Protected by a Mutex so the render thread can read it safely.
    pub frame: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat, Vec<u8>)>>>,
    /// Negotiated video format: (width, height, format), set once during
    /// format negotiation when the stream receives its first `ParamType::Format` event.
    pub format_info: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat)>>>,
    /// Set to `true` after format negotiation completes and the stream is activated.
    /// Used by `init_capture_service` to wait for all streams to be ready.
    pub ready: Arc<AtomicBool>,
    /// Human-readable window title, read from the chooser file after capture.
    /// May include a portal hash suffix (e.g., "dock (8f06968d...)") that is
    /// stripped by `strip_portal_hash()` for matching.
    pub title: String,
    /// Stable Wayland window ID (`protocol_id` from `foreign_toplevel`), set at
    /// capture time. Once bound, this stream is tied to this window regardless
    /// of title changes. Stored in a Mutex so the monitor task can update it
    /// after stream retention during incremental updates.
    pub window_id: Arc<Mutex<Option<u32>>>,
}

/// Snapshot of the current capture state: file descriptors, node IDs, and titles.
///
/// Updated by `init_capture_service` on startup and by `start_monitor_inner`
/// during incremental monitor cycles. Used primarily for debugging and state
/// inspection; the render path reads from `stream_frames` directly.
pub struct CaptureState {
    /// PipeWire remote file descriptors, one per portal session.
    /// Each FD is passed to `spawn_stream_thread` to connect to the PipeWire core.
    pub pw_fds: Vec<OwnedFd>,
    /// PipeWire node IDs for each captured window, returned by the portal's
    /// `Start` method. Used to identify which node to connect the stream to.
    pub node_ids: Vec<u32>,
    /// Window titles for each captured window, read from the chooser file.
    pub titles: Vec<String>,
    /// Total number of captured windows.
    pub count: usize,
}

/// Persistent binding from a Wayland window ID to a PipeWire stream index.
///
/// Once a PipeWire stream is bound to a Wayland window, the binding persists
/// across monitor cycles. This eliminates the need for title-based matching
/// on every frame, which is fragile when multiple windows share the same title.
/// Bindings are rebuilt when streams are removed during monitor cycles.
#[derive(Clone, Debug)]
pub struct StreamBinding {
    /// Stable Wayland window ID (`protocol_id` from `foreign_toplevel`).
    pub window_id: u32,
    /// Index into `PipewireCaptureService::stream_frames`.
    pub stream_index: usize,
    /// Window title at the time the binding was created, for debugging.
    pub title_at_bind: String,
}

/// Shared, thread-safe state for the PipeWire capture service.
///
/// All fields are wrapped in `Arc<Mutex<>>` or `Arc<AtomicBool>` so they can
/// be accessed from multiple contexts:
/// - **Main thread**: reads frames via `get_active_frame()` for rendering
/// - **Stream threads**: write raw frame data continuously
/// - **Monitor task** (tokio): adds/removes streams incrementally
/// - **Draw loop**: updates `window_ids` map for stream-to-window binding
pub struct PipewireCaptureService {
    /// Current capture state: file descriptors, node IDs, titles, and count.
    /// Updated on startup and during monitor cycles.
    pub capture_state: Arc<Mutex<CaptureState>>,
    /// Vector of per-stream frame data, indexed by stream position.
    /// Each element corresponds to one captured window's video stream.
    /// The render thread reads from this vector to get thumbnails.
    pub stream_frames: Arc<Mutex<Vec<Arc<StreamFrameData>>>>,
    /// Index of the currently hovered window in the popup grid.
    /// Updated by the window list popup on pointer motion; used by
    /// `get_active_frame()` to return the frame for the hovered row.
    pub active_row: Arc<Mutex<usize>>,
    /// Set to `true` after `init_capture_service` completes successfully.
    /// Indicates that at least one stream thread is running.
    pub running: Arc<AtomicBool>,
    /// Async notification signal fired when a new Wayland window is created.
    /// The monitor task waits on this signal instead of polling, so it only
    /// runs when windows actually change. Debounced to 500ms via `last_probe_time`.
    pub probe_signal: Arc<tokio::sync::Notify>,
    /// Timestamp of the last probe signal, used to debounce rapid window
    /// creation events so the monitor doesn't re-run on every transient window.
    pub last_probe_time: Arc<Mutex<Instant>>,
    /// Shared map from window title to stable Wayland window IDs (`protocol_id`).
    /// Multiple windows can have the same title (e.g., multiple "New tab" windows),
    /// so each title maps to a `Vec<u32>` of IDs. Updated by the main draw loop
    /// every frame so the monitor task can bind new streams to the correct window.
    pub window_ids: Arc<Mutex<HashMap<String, Vec<u32>>>>,
    /// Persistent binding registry: Wayland window ID → PipeWire stream index.
    /// Once a PipeWire stream is tied to a Wayland window, it stays tied until
    /// the window closes or the stream is removed. This is the primary matching
    /// mechanism — title matching is only a fallback for unbound streams.
    /// Rebuilt during monitor cycles when streams are removed.
    pub bindings: Arc<Mutex<HashMap<u32, usize>>>,
    /// XDG Desktop Portal restore token for screen capture permission.
    /// Saved after the first successful capture and reused on subsequent starts
    /// to bypass the portal confirmation dialog. Note: the wlr portal does not
    /// return a restore_token from `Start()` — it uses PermissionStore internally
    /// with `persist_mode=2`. This field may remain `None` even when the dialog
    /// is suppressed by the portal's built-in permission persistence.
    pub restore_token: Arc<Mutex<Option<String>>>,
    /// Downscale factor for capture frame thumbnails (0.01–1.0).
    /// A value of 0.25 means a 1920×1080 window becomes a 480×270 thumbnail.
    /// Configured via `pipewire_scale_factor` in the dock config (`config.json`).
    /// Lower values reduce memory usage and rendering cost at the expense of quality.
    pub scale_factor: Arc<Mutex<f32>>,
}

impl PipewireCaptureService {
    /// Create a new capture service with empty state.
    ///
    /// Loads any saved restore token from disk and initializes the default
    /// scale factor to 0.5 (overridden by config in `main.rs`).
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
            scale_factor: Arc::new(Mutex::new(0.5)),
        }
    }

    /// Timeout (in milliseconds) for acquiring Mutex locks on per-stream data.
    /// If a stream thread blocks a lock (e.g., pcmanfm-qt), we skip that stream
    /// rather than freezing the dock.
    const LOCK_TIMEOUT_MS: u64 = 50;

    /// Set the downscale factor for capture frame thumbnails.
    ///
    /// Called from `main.rs` with the value from the dock config.
    /// Clamps the value to the valid range [0.01, 1.0] to prevent
    /// division by zero or meaningless full-resolution captures.
    pub fn set_scale_factor(&self, factor: f32) {
        let mut f = self.scale_factor.lock();
        *f = factor.clamp(0.01, 1.0);
    }

    /// Update the shared window title-to-IDs map from the current Wayland window list.
    ///
    /// Called by the main draw loop every frame so the monitor task can bind
    /// new streams to stable Wayland window IDs. Multiple windows can have the
    /// same title (e.g., multiple "New tab" windows), so each title maps to
    /// a `Vec<u32>` of IDs. The IDs are `protocol_id` values from `foreign_toplevel`.
    ///
    /// Uses a timeout lock so the draw thread never freezes if the monitor task
    /// holds the lock for an extended period.
    pub fn update_window_ids(&self, windows: &[(String, u32)]) {
        // Use timeout — if the monitor holds the lock, skip this frame's update
        let Some(mut map) = self
            .window_ids
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return;
        };
        map.clear();
        for (title, id) in windows {
            map.entry(title.clone()).or_insert_with(Vec::new).push(*id);
        }
    }

    /// Get the persistently bound stream index for a Wayland window ID.
    ///
    /// Looks up the binding registry and verifies the stream still exists
    /// in `stream_frames`. Returns `None` if the window has no binding or
    /// the stream was removed during a monitor cycle.
    ///
    /// Uses timeout locks so the draw thread never freezes if the monitor task
    /// holds either lock for an extended period.
    pub fn get_bound_stream(&self, window_id: u32) -> Option<usize> {
        // Use timeout — if the monitor holds bindings, skip
        let Some(bindings) = self
            .bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return None;
        };
        let stream_idx = bindings.get(&window_id).copied()?;
        // Release bindings before acquiring stream_frames to avoid ABBA deadlock
        drop(bindings);
        let Some(frames) = self
            .stream_frames
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return None;
        };
        if stream_idx < frames.len() {
            Some(stream_idx)
        } else {
            None
        }
    }

    /// Persistently bind a Wayland window ID to a PipeWire stream index.
    ///
    /// Once bound, the window always uses this stream regardless of title changes.
    /// The `title` parameter is stored for debugging but not used for matching.
    /// Called during initial capture and when the monitor adds new streams.
    ///
    /// Uses a timeout lock so the draw thread never freezes if the monitor task
    /// holds the lock for an extended period.
    pub fn bind_window_to_stream(&self, window_id: u32, stream_idx: usize, _title: &str) {
        let Some(mut bindings) = self
            .bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return;
        };
        bindings.insert(window_id, stream_idx);
    }

    /// Unbind a Wayland window ID from its PipeWire stream.
    ///
    /// Called when a window closes so the binding doesn't point to a stale stream.
    pub fn unbind_window(&self, window_id: u32) {
        let Some(mut bindings) = self
            .bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return;
        };
        let _ = bindings.remove(&window_id);
    }

    /// Unbind all windows bound to a specific stream index, then remap bindings
    /// for streams that shifted after removal.
    ///
    /// When a stream is removed from `stream_frames`, all subsequent streams
    /// shift down by 1. This method updates the binding registry accordingly.
    pub fn unbind_and_remap_stream(&self, removed_stream_idx: usize) {
        let Some(mut bindings) = self
            .bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return;
        };
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

    /// Check if a Wayland window already has a persistent binding to a stream.
    pub fn is_window_bound(&self, window_id: u32) -> bool {
        self.bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            .map(|b| b.contains_key(&window_id))
            .unwrap_or(false)
    }

    /// Reverse lookup: find the Wayland window ID bound to a given stream index.
    ///
    /// Used when clicking a dock icon to find which window's stream is currently
    /// displayed, so the correct window can be activated.
    ///
    /// Uses a timeout lock so the draw thread never freezes.
    pub fn find_window_by_stream_idx(&self, stream_idx: usize) -> Option<u32> {
        let bindings = self
            .bindings
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))?;
        for (&window_id, &idx) in bindings.iter() {
            if idx == stream_idx {
                return Some(window_id);
            }
        }
        None
    }

    /// Find the PipeWire stream index that matches a window by its stable Wayland ID.
    ///
    /// Two-pass strategy:
    /// 1. **Fast path**: check the persistent binding registry (`bindings`)
    /// 2. **Slow path**: scan all streams by their `window_id` field
    ///
    /// The `exclude` list prevents re-matching a stream already assigned to
    /// another window (important for duplicate-title scenarios).
    pub fn find_stream_by_window_id(&self, window_id: u32, exclude: &[usize]) -> Option<usize> {
        // Fast path: check persistent binding registry
        if let Some(idx) = self.get_bound_stream(window_id) {
            if !exclude.contains(&idx) {
                return Some(idx);
            }
        }
        // Slow path: scan streams by window_id field (for streams not yet in registry)
        let Some(frames) = self
            .stream_frames
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return None;
        };
        for (i, stream) in frames.iter().enumerate() {
            if exclude.contains(&i) {
                continue;
            }
            // Use timeout lock — if a stream thread blocks window_id, skip it
            if let Some(wid_guard) = stream
                .window_id
                .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            {
                if let Some(sid) = wid_guard.as_ref().copied() {
                    if sid == window_id {
                        return Some(i);
                    }
                }
            }
        }
        None
    }

    /// Signal the monitor task to probe for new windows.
    ///
    /// Debounced to 1000ms to avoid re-running the monitor on every transient
    /// window creation event or title change. Window title changes (e.g., pcmanfm-qt
    /// navigating folders) can fire multiple events in quick succession — the longer
    /// debounce prevents the monitor from running while titles are still changing,
    /// which would cause unnecessary stream removal and recapture cycles.
    /// Called by `event_compat` when `WindowEvent::Created` fires.
    pub fn request_probe(&self) {
        let now = Instant::now();
        // Use timeout — if monitor holds the lock, just notify anyway
        let Some(mut last) = self
            .last_probe_time
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            self.probe_signal.notify_one();
            return;
        };
        if now.duration_since(*last) >= std::time::Duration::from_millis(1000) {
            *last = now;
            self.probe_signal.notify_one();
        }
    }

    /// Check if a window title has a matching PipeWire stream.
    ///
    /// Convenience wrapper around `find_stream_by_title` that returns a boolean.
    pub fn has_stream_for_title(&self, title: &str) -> bool {
        self.find_stream_by_title(title, &[]).is_some()
    }

    /// Get the number of currently captured windows.
    ///
    /// Uses a timeout lock so the draw thread never freezes.
    pub fn window_count(&self) -> usize {
        self.capture_state
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            .map(|s| s.count)
            .unwrap_or(0)
    }

    /// Get the title for a captured window by its stream index.
    ///
    /// Returns a fallback "Window N" title if the index is out of bounds or the lock times out.
    pub fn window_title(&self, idx: usize) -> String {
        let state = match self
            .capture_state
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        {
            Some(g) => g,
            None => return format!("Window {}", idx + 1),
        };
        if idx < state.titles.len() {
            state.titles[idx].clone()
        } else {
            format!("Window {}", idx + 1)
        }
    }

    /// Set the active row (hovered window index in the popup grid).
    ///
    /// Updated by the window list popup on pointer motion. The next call to
    /// `get_active_frame()` will return the frame for this row.
    pub fn set_active_row(&self, row: usize) {
        let Some(mut r) = self
            .active_row
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return;
        };
        *r = row;
    }

    /// Get the latest captured frame for the active (hovered) row, converted to RGBA.
    ///
    /// Accepts optional target display dimensions `(target_w, target_h)` in pixels.
    /// If provided, the frame is downscaled only when the source dimensions exceed
    /// the target — the output matches the display area exactly, avoiding wasted
    /// memory on oversized frames. If the source is the same size or smaller than
    /// the target, no downscaling is applied (scale factor 1.0).
    ///
    /// The configurable `scale_factor` acts as a maximum quality cap: even when
    /// the target is large, the frame won't exceed `source × scale_factor`.
    ///
    /// Returns `None` if the row is out of bounds or the stream hasn't received
    /// a frame yet. Called by the render thread to get the thumbnail for the
    /// hovered popup row.
    ///
    /// Uses timeout-based lock acquisition so a blocked stream thread (e.g.,
    /// pcmanfm-qt) cannot freeze the dock. If the lock times out, returns `None`
    /// for that frame.
    pub fn get_active_frame(
        &self,
        target_w: Option<u32>,
        target_h: Option<u32>,
    ) -> Option<(u32, u32, Vec<u8>)> {
        let row = match self
            .active_row
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        {
            Some(g) => *g,
            None => {
                static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                if COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                    eprintln!("[PIPEWIRE] get_active_frame: active_row lock timeout");
                }
                return None;
            }
        };
        let max_scale = match self
            .scale_factor
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        {
            Some(g) => *g,
            None => return None,
        };

        // Lock stream_frames briefly to get the stream reference, then release it
        // before acquiring the per-stream frame lock. This prevents deadlock where
        // the draw thread holds stream_frames while a stream thread holds frame.
        let stream = {
            let Some(frames) = self
                .stream_frames
                .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            else {
                static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                if COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                    eprintln!("[PIPEWIRE] get_active_frame: stream_frames lock timeout");
                }
                return None;
            };
            if row >= frames.len() {
                return None;
            }
            frames[row].clone()
        };

        // Use timeout lock — if a stream thread blocks frame, skip it rather than freezing
        let frame_data = match stream
            .frame
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        {
            Some(guard) => (*guard).clone(),
            None => {
                static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                if COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 120 == 0 {
                    eprintln!(
                        "[PIPEWIRE] get_active_frame: per-stream frame lock timeout, row={}",
                        row
                    );
                }
                return None;
            }
        };

        match frame_data {
            Some((w, h, fmt, bytes)) => {
                // Compute adaptive scale: downscale only if source exceeds target
                let scale = if let (Some(tw), Some(th)) = (target_w, target_h) {
                    if w > tw || h > th {
                        // Source is larger than target — compute scale to fit within target
                        let scale_w = tw as f32 / w as f32;
                        let scale_h = th as f32 / h as f32;
                        (scale_w.min(scale_h)).min(max_scale)
                    } else {
                        // Source fits within target — no downscaling needed
                        1.0
                    }
                } else {
                    // No target provided — use the configured max scale factor
                    max_scale
                };
                Some(convert_to_rgba(w, h, fmt, &bytes, scale))
            }
            None => None,
        }
    }

    /// Get the original (pre-downscale) frame dimensions for a stream index.
    ///
    /// Returns `None` if the stream hasn't negotiated its video format yet.
    /// Used by the popup renderer to calculate aspect-ratio-correct thumbnails.
    pub fn get_stream_dimensions(&self, idx: usize) -> Option<(u32, u32)> {
        // Get the format_info Arc without holding stream_frames lock
        let format_info = {
            let Some(frames) = self
                .stream_frames
                .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            else {
                return None;
            };
            if idx >= frames.len() {
                return None;
            }
            frames[idx].format_info.clone()
        };

        // Use timeout lock — if a stream thread blocks format_info, skip it
        format_info
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
            .map(|guard| (*guard).clone())
            .and_then(|fmt| fmt.map(|(w, h, _)| (w, h)))
    }

    /// Strip the XDG Desktop Portal hash suffix from a PipeWire stream title.
    ///
    /// The portal appends a unique 32-character hex ID in parentheses to distinguish
    /// windows with identical titles, e.g. "dock (8f06968d...)". Title matching
    /// strips this suffix so "dock" matches "dock (hash)".
    ///
    /// Returns the original title if no hash suffix is found.
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

    /// Find the PipeWire stream index that matches a Wayland window title.
    ///
    /// Multi-strategy matching, tried in order:
    /// 1. **Exact match**: compare against the title-without-hash
    /// 2. **Case-insensitive partial match**: skip for short single-word titles (≤4 chars)
    ///    to avoid false positives like "dock" matching a file path
    /// 3. **Prefix matching**: compare first 3 words (only for titles with ≥3 words)
    /// 4. **Word-based match**: check if any word >4 chars appears in the stream title
    ///
    /// Uses `stream_frames` titles (from the chooser file) which are more accurate
    /// than `capture_state.titles`. The `exclude` list prevents two windows from
    /// the same app from matching the same stream.
    ///
    /// Uses a timeout lock so the draw thread never freezes if the monitor task
    /// holds the lock for an extended period.
    pub fn find_stream_by_title(&self, title: &str, exclude: &[usize]) -> Option<usize> {
        if title.is_empty() {
            return None;
        }
        // Use timeout — if the monitor holds stream_frames, skip
        let Some(frames) = self
            .stream_frames
            .try_lock_for(std::time::Duration::from_millis(Self::LOCK_TIMEOUT_MS))
        else {
            return None;
        };
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
        let title_words: Vec<&str> = title_lower
            .split_whitespace()
            .filter(|w| w.len() > 4)
            .collect();
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

/// Initialize and start the PipeWire capture service.
///
/// This is the entry point called from `main.rs`. It:
/// 1. Initializes the PipeWire library
/// 2. Probes the portal to discover the actual window count
/// 3. Captures all windows via `capture_windows_range`
/// 4. Spawns a dedicated stream thread per window
/// 5. Binds streams to Wayland window IDs in the persistent registry
/// 6. Waits for format negotiation to complete
///
/// Runs asynchronously on the tokio runtime. If capture fails, the service
/// remains in a clean state with zero streams.
pub async fn init_capture_service(
    service: &PipewireCaptureService,
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    max_windows: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    pw::init();

    // Get the saved restore token to bypass the portal dialog
    let restore_token = {
        let token = service.restore_token.lock();
        token.clone()
    };
    // Capture all windows
    let (results, new_restore_token) =
        match capture_windows_range(connection, desktop, 0, max_windows, restore_token.clone())
            .await
        {
            Ok(r) => r,
            Err(_e) => (Vec::new(), None),
        };

    // Update the in-memory restore token from the portal response
    if let Some(ref token) = new_restore_token {
        {
            let mut saved_token = service.restore_token.lock();
            *saved_token = Some(token.clone());
        }
    }

    if results.is_empty() {
        // If capture failed, clear the stale restore token
        {
            let mut token = service.restore_token.lock();
            *token = None;
        }
        return Ok(());
    }

    let pw_fds: Vec<OwnedFd> = results
        .iter()
        .map(|(fd, _, _)| fd.try_clone().unwrap())
        .collect();
    let node_ids: Vec<u32> = results.iter().map(|(_, nid, _)| *nid).collect();
    let titles: Vec<String> = results.iter().map(|(_, _, t)| t.clone()).collect();
    let count = results.len();

    // Update capture state
    {
        let mut state = service.capture_state.lock();
        state.pw_fds = pw_fds;
        state.node_ids = node_ids;
        state.titles = titles;
        state.count = count;
    }

    // Read the chooser file to get the portal's window order.
    // The chooser file lists windows in the same order as the portal provides them,
    // which matches the stream capture order. This is essential for correctly binding
    // same-title windows to their streams — without it, the binding order is arbitrary.
    let chooser_titles: Vec<String> = std::fs::read_to_string("/tmp/chooser-windows.txt")
        .ok()
        .map(|content| {
            content
                .lines()
                .filter(|l| !l.is_empty())
                .map(|s| s.trim().to_string())
                .collect()
        })
        .unwrap_or_default();

    // Spawn stream threads and bind them to the persistent registry
    for (idx, (fd, node_id, title)) in results.into_iter().enumerate() {
        // Look up the stable Wayland window ID for this stream.
        // Strip the portal hash suffix so "dock (hash)" matches the Wayland title "dock".
        //
        // For same-title windows, use the chooser file order to determine which window
        // gets which stream. The chooser file lists windows in portal order, which matches
        // the stream index order. We count how many windows with the same title appear
        // before this index in the chooser file, then pick the corresponding unbound ID.
        let window_id = {
            // Release window_ids before bindings to avoid lock ordering issues
            let ids = {
                let map = service.window_ids.lock();
                let clean_title = PipewireCaptureService::strip_portal_hash(&title);
                map.get(clean_title).cloned()
            };
            ids.and_then(|v| {
                let bindings = service.bindings.lock();
                if v.len() <= 1 {
                    // Only one window with this title — pick it directly
                    v.iter().find(|id| !bindings.contains_key(id)).copied()
                } else {
                    // Multiple windows with the same title — use chooser file order.
                    // Count how many streams with this title (hash-stripped) appear
                    // before this index in the chooser file. That count tells us which
                    // unbound ID to pick from the sorted window_ids vec.
                    let clean_title_for_count =
                        PipewireCaptureService::strip_portal_hash(&title).to_string();
                    let mut same_title_count_before = 0;
                    for ci in 0..idx {
                        if let Some(chooser_title) = chooser_titles.get(ci) {
                            let chooser_clean =
                                PipewireCaptureService::strip_portal_hash(chooser_title)
                                    .to_string();
                            if chooser_clean == clean_title_for_count {
                                same_title_count_before += 1;
                            }
                        }
                    }
                    // Pick the unbound ID at the same position as this stream among same-title streams
                    v.iter()
                        .filter(|id| !bindings.contains_key(id))
                        .nth(same_title_count_before)
                        .copied()
                }
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
            let mut frames = service.stream_frames.lock();
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

    // Wait for streams to negotiate format (up to 5 seconds).
    // If some streams never become ready (e.g., pcmanfm-qt windows that
    // refuse to negotiate), we continue with the streams that did become ready
    // instead of blocking indefinitely.
    for _ in 0..50 {
        // Use timeout lock so the draw thread isn't blocked while we wait
        let ready = service
            .stream_frames
            .try_lock_for(std::time::Duration::from_millis(50))
            .map(|frames| {
                frames
                    .iter()
                    .filter(|s| s.ready.load(Ordering::Acquire))
                    .count()
            })
            .unwrap_or(0);
        if ready == count {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    service.running.store(true, Ordering::Release);
    Ok(())
}

/// Generate a unique handle token for D-Bus portal requests.
///
/// Each portal method call requires a unique handle token to correlate
/// requests with response signals. Uses a monotonically increasing counter.
fn next_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("dock_{}", n)
}

/// Path where the XDG Desktop Portal restore token is stored.
///
/// The token allows the portal to remember screen capture permission across
/// restarts, bypassing the confirmation dialog. Note: the wlr portal uses
/// `persist_mode=2` internally, so this file may not be used in practice.
const RESTORE_TOKEN_PATH: &str = ".config/dock/pipewire_restore_token";

/// Load the saved restore token from `~/.config/dock/pipewire_restore_token`.
///
/// Returns `None` if the file doesn't exist or can't be read.
fn load_restore_token() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let path = format!("{}/{}", home, RESTORE_TOKEN_PATH);
    std::fs::read_to_string(&path).ok()
}

/// Save the restore token to `~/.config/dock/pipewire_restore_token`.
///
/// Creates the directory if it doesn't exist. Silently ignores errors.
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

/// Create a new XDG Desktop Portal session via D-Bus.
///
/// A session is the first step in the ScreenCast portal flow. It returns an
/// object path that is used by `select_sources` and `start_capture`.
///
/// The session is associated with `list-widget.desktop` so the portal knows
/// which application is requesting capture permission.
async fn create_session(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
) -> Result<OwnedObjectPath, Box<dyn std::error::Error>> {
    let handle_token = next_token();
    let session_handle_token = next_token();

    let unique = connection
        .unique_name()
        .unwrap()
        .as_str()
        .trim_start_matches(':');
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
    )
    .await?;

    let mut signal = request.receive_signal("Response").await?;

    let options: HashMap<String, Variant<'_>> = HashMap::from([
        (
            "handle_token".to_string(),
            Variant::Str(handle_token.as_str().into()),
        ),
        (
            "session_handle_token".to_string(),
            Variant::Str(session_handle_token.as_str().into()),
        ),
        (
            "desktop_file".to_string(),
            Variant::Str("list-widget.desktop".into()),
        ),
    ]);

    desktop.call_method("CreateSession", &(options,)).await?;

    let sig_opt = tokio::time::timeout(std::time::Duration::from_secs(10), signal.next())
        .await
        .map_err(|_| "CreateSession timed out waiting for Response signal")?;
    let sig = sig_opt.ok_or("Signal stream ended")?;

    let (response, session_path): (u32, OwnedObjectPath) =
        match sig.body().deserialize() {
            Ok(result) => result,
            Err(_) => {
                let result = match sig.body().deserialize::<(u32, HashMap<String, OwnedVariant>)>() {
                    Ok((resp, props)) => {
                        if let Some(val) = props.get("session_handle") {
                            if let Variant::Str(s) = &**val {
                                if let Ok(path) = OwnedObjectPath::try_from(s.as_str()) {
                                    return (resp, path);
                                } else {
                                    tracing::warn!("Invalid session path in portal response");
                                }
                            }
                        }
                        if let Some(val) = props.get("session") {
                            if let Variant::ObjectPath(path) = &**val {
                                return (resp, OwnedObjectPath::from(path.clone()));
                            }
                        }
                        return Err("No session path in portal response dict".into());
                    }
                    Err(e) => {
                        tracing::warn!("Failed to deserialize portal response: {}", e);
                        return Err("Failed to parse portal CreateSession response".into());
                    }
                };
                return Err("Failed to deserialize portal response".into());
            }
        };
    if response != 0 {
        return Err(format!("Session creation failed: {}", response).into());
    }
    Ok(session_path)
}

/// Call `SelectSources` on the XDG Desktop Portal to select capture sources.
///
/// This is the second step in the ScreenCast portal flow. Configures the
/// session to capture individual windows (type=3 = window + viewport) with
/// multiple sources enabled. The `restore_token` is passed here (not to
/// `Start`) per the XDG Desktop Portal spec to bypass the confirmation dialog.
///
/// Key options:
/// - `types=3`: window + viewport capture
/// - `multiple=true`: capture multiple windows
/// - `cursor_mode=1`: exclude cursor
/// - `persist_mode=2`: remember permission (wlr-specific)
async fn select_sources(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
    restore_token: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let unique = connection
        .unique_name()
        .ok_or("No unique D-Bus name")?
        .as_str()
        .trim_start_matches(':');
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
    )
    .await?;

    let mut signal = request.receive_signal("Response").await?;

    // Build options — restore_token must be on SelectSources, not Start,
    // to bypass the portal confirmation dialog.
    let mut options: HashMap<String, Variant<'_>> = HashMap::from([
        (
            "handle_token".to_string(),
            Variant::Str(handle_token.as_str().into()),
        ),
        ("types".to_string(), Variant::U32(3)), // 3 = window + viewport
        ("multiple".to_string(), Variant::Bool(true)),
        ("cursor_mode".to_string(), Variant::U32(1)),
        ("persist_mode".to_string(), Variant::U32(2)),
    ]);
    if let Some(token) = restore_token {
        options.insert("restore_token".to_string(), Variant::Str(token.into()));
    }

    desktop
        .call_method("SelectSources", &(session, options))
        .await?;

    // Wait for the response signal with a timeout to prevent the portal from
    // hanging indefinitely (e.g., when certain windows like pcmanfm-qt cause
    // the wlr portal to stall).
    let sig_opt = tokio::time::timeout(std::time::Duration::from_secs(10), signal.next())
        .await
        .map_err(|_| "SelectSources timed out waiting for Response signal")?;
    let sig = sig_opt.ok_or("SelectSources signal stream ended")?;
    let response: u32 = if let Ok((r,)) = sig.body().deserialize::<(u32,)>() {
        r
    } else if let Ok((r, _props)) = sig.body().deserialize::<(u32, HashMap<String, OwnedVariant>)>() {
        r
    } else {
        tracing::warn!("Failed to deserialize SelectSources response");
        return Err("Failed to parse SelectSources response".into());
    };
    if response != 0 {
        return Err(format!("SelectSources canceled (response={})", response).into());
    }
    Ok(())
}

/// Call `Start` on the XDG Desktop Portal to begin screen capture.
///
/// This is the third and final step in the ScreenCast portal flow. Returns
/// the list of captured streams (node ID + properties) and an optional
/// restore token for future sessions.
///
/// The portal spawns the chooser script (`chooser-all.sh`) during this call,
/// which writes the selected window's index to `/tmp/chooser-windows.txt`.
async fn start_capture(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
) -> Result<(Vec<(u32, HashMap<String, String>)>, Option<String>), Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let unique = connection
        .unique_name()
        .ok_or("No unique D-Bus name")?
        .as_str()
        .trim_start_matches(':');
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
    )
    .await?;

    let mut signal = request.receive_signal("Response").await?;

    let options: HashMap<String, Variant<'_>> = HashMap::from([(
        "handle_token".to_string(),
        Variant::Str(handle_token.as_str().into()),
    )]);

    desktop
        .call_method("Start", &(session, handle_token.as_str(), options))
        .await?;

    // Wait for the Start response signal with a timeout to prevent the portal
    // from hanging indefinitely (e.g., when certain windows like pcmanfm-qt
    // cause the wlr portal to stall).
    let sig_opt = tokio::time::timeout(std::time::Duration::from_secs(10), signal.next())
        .await
        .map_err(|_| "Start timed out waiting for Response signal")?;
    let sig = sig_opt.ok_or("Start signal stream ended")?;
    let (response, props): (u32, HashMap<String, OwnedVariant>) = sig.body().deserialize()?;

    if response != 0 {
        return Err(format!("Start failed: {}", response).into());
    }

    let mut streams = Vec::new();
    let mut restore_token = None;

    for (key, val) in &props {
        if key == "streams" {
            if let Variant::Array(arr) = &**val {
                for item in arr.inner() {
                    if let Variant::Structure(tup) = item {
                        let mut node_id = 0u32;
                        let mut stream_props = HashMap::new();
                        for (i, field) in tup.fields().into_iter().enumerate() {
                            match field {
                                Variant::U32(id) if i == 0 => {
                                    node_id = *id;
                                }
                                Variant::Dict(dict) if i == 1 => {
                                    for (k, v) in dict.iter() {
                                        if let Variant::Str(key_str) = k {
                                            stream_props
                                                .insert(key_str.to_string(), format!("{}", v));
                                        }
                                    }
                                }
                                Variant::Array(dict_arr) if i == 1 => {
                                    for entry in dict_arr.inner() {
                                        if let Variant::Structure(entry_tup) = entry {
                                            let mut entry_key = String::new();
                                            let mut entry_val = String::new();
                                            for (j, f) in entry_tup.fields().into_iter().enumerate()
                                            {
                                                match f {
                                                    Variant::Str(s) if j == 0 => {
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
            if let Variant::Str(s) = &**val {
                restore_token = Some(s.to_string());
            }
        }
    }

    Ok((streams, restore_token))
}

/// Open a PipeWire remote connection via the XDG Desktop Portal.
///
/// Calls `OpenPipeWireRemote` to get a file descriptor that can be used to
/// connect to the PipeWire core. The FD is passed to `spawn_stream_thread`
/// to create the actual capture stream.
async fn open_pipewire_remote(
    _connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    session: &OwnedObjectPath,
) -> Result<OwnedFd, Box<dyn std::error::Error>> {
    let handle_token = next_token();

    let options: HashMap<String, Variant<'_>> = HashMap::from([(
        "handle_token".to_string(),
        Variant::Str(handle_token.as_str().into()),
    )]);

    let msg = desktop
        .call_method("OpenPipeWireRemote", &(session, options))
        .await?;

    let body = msg.body();
    let fd_val: zbus::zvariant::Fd<'_> = body
        .deserialize()
        .map_err(|_e| "Failed to deserialize PipeWire FD")?;

    let owned_fd = match fd_val {
        zbus::zvariant::Fd::Borrowed(borrowed) => borrowed
            .try_clone_to_owned()
            .map_err(|_e| "Failed to clone file descriptor")?,
        zbus::zvariant::Fd::Owned(owned) => owned,
    };

    Ok(owned_fd)
}

/// Probe the portal to discover how many windows are available for capture.
///
/// Does a single portal session to trigger the chooser script, which writes
/// the window list to `/tmp/chooser-windows.txt`. The stream FD is discarded;
/// only the file is read to count the windows.
///
/// This prevents trying invalid indices (e.g., 0..20 when only 5 windows exist),
/// which would cause failed portal sessions and "wlroots: no output found" errors.
async fn probe_window_count(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    restore_token: Option<&str>,
) -> usize {
    // Do a single portal session to trigger the chooser, which writes the window
    // list to /tmp/chooser-windows.txt. We discard the stream FD and just read
    // the file to count the windows.
    let _ = std::fs::write("/tmp/chooser-index.txt", "0");
    let session = match create_session(connection, desktop).await {
        Ok(s) => s,
        Err(_) => return 0,
    };
    if select_sources(connection, desktop, &session, restore_token)
        .await
        .is_err()
    {
        // If portal session failed, try to read the existing chooser file
        if let Ok(content) = std::fs::read_to_string("/tmp/chooser-windows.txt") {
            return content.lines().filter(|l| !l.is_empty()).count();
        }
        return 0;
    }
    let _ = start_capture(connection, desktop, &session).await;
    // Small delay to let the chooser script finish writing the file
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Read the window count from the chooser file
    if let Ok(content) = std::fs::read_to_string("/tmp/chooser-windows.txt") {
        content.lines().filter(|l| !l.is_empty()).count()
    } else {
        0
    }
}

/// Capture a range of windows by their portal index.
///
/// For each window index in `[start_idx, end_idx)`:
/// 1. Writes the index to `/tmp/chooser-index.txt` for the chooser script
/// 2. Creates a portal session, selects sources, and starts capture
/// 3. Opens the PipeWire remote FD
/// 4. Reads the window title from `/tmp/chooser-windows.txt`
///
/// Individual failures are non-fatal (`continue`), so one failed window
/// doesn't kill the entire capture loop. Returns the list of captured
/// streams and the latest restore token.
async fn capture_windows_range(
    connection: &zbus::Connection,
    desktop: &Proxy<'_>,
    start_idx: usize,
    end_idx: usize,
    restore_token: Option<String>,
) -> Result<(Vec<(OwnedFd, u32, String)>, Option<String>), Box<dyn std::error::Error>> {
    let mut results: Vec<(OwnedFd, u32, String)> = Vec::new();
    let mut latest_restore_token: Option<String> = None;

    // Probe the actual window count to avoid trying indices that don't exist.
    // This prevents failed portal sessions and "wlroots: no output found" errors.
    let actual_count = probe_window_count(connection, desktop, restore_token.as_deref()).await;
    let end_idx = end_idx.min(actual_count);

    for idx in start_idx..end_idx {
        // Tell the chooser which window to select (0-based)
        std::fs::write("/tmp/chooser-index.txt", idx.to_string()).ok();

        let session = match create_session(connection, desktop).await {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Pass the restore token on the first iteration to bypass the portal dialog.
        // The token is passed to SelectSources per the XDG Desktop Portal spec.
        let token_for_this = if idx == start_idx {
            restore_token.as_deref()
        } else {
            None
        };
        if let Err(_) = select_sources(connection, desktop, &session, token_for_this).await {
            continue;
        }

        let (streams, new_restore_token) = match start_capture(connection, desktop, &session).await
        {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Save the latest restore token from the portal
        if new_restore_token.is_some() {
            latest_restore_token = new_restore_token.clone();
        }

        if streams.is_empty() {
            continue;
        }

        let fd = match open_pipewire_remote(connection, desktop, &session).await {
            Ok(f) => f,
            Err(_) => continue,
        };

        for (node_id, props) in &streams {
            // The PipeWire properties rarely contain the actual window title.
            // Use the chooser file which has the real titles written by chooser-all.sh.
            // The chooser file is updated after each portal session, so the last line
            // corresponds to the window we just captured (index idx).
            let chooser_title = std::fs::read_to_string("/tmp/chooser-windows.txt")
                .ok()
                .and_then(|content| {
                    let lines: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
                    // The chooser file lists all windows; the one we selected is at index `idx`
                    lines.get(idx).map(|s| s.trim().to_string())
                });

            let default_title = format!("Window {}", results.len() + 1);
            let title = chooser_title.unwrap_or_else(|| {
                props
                    .get("window.title")
                    .or_else(|| props.get("node.name"))
                    .or_else(|| props.get("application.name"))
                    .or_else(|| props.get("node.description"))
                    .unwrap_or(&default_title)
                    .clone()
            });
            match fd.try_clone() {
                Ok(cloned_fd) => results.push((cloned_fd, *node_id, title)),
                Err(e) => {
                    tracing::warn!("Failed to clone PipeWire FD for node {}: {}", node_id, e);
                }
            }
        }
    }

    // Save the restore token to disk if we got one
    if let Some(ref token) = latest_restore_token {
        save_restore_token(token);
    }

    Ok((results, latest_restore_token))
}

/// Spawn a dedicated thread for a single PipeWire capture stream.
///
/// Each thread runs its own `MainLoopRc` and continuously receives video
/// buffers from the PipeWire node. The thread:
/// 1. Creates a PipeWire context and connects via the provided FD
/// 2. Creates a stream with format negotiation (BGRx/BGRA/RGBA/RGB, up to 8K@60fps)
/// 3. Registers listeners for state changes, format negotiation, and buffer processing
/// 4. Enters an infinite loop iterating the main loop with 16ms timeout (~60fps)
///
/// The `process` callback copies each buffer into `stream_data.frame` for the
/// render thread to read. Format conversion happens on demand in `get_active_frame()`.
fn spawn_stream_thread(fd: OwnedFd, node_id: u32, idx: usize, stream_data: Arc<StreamFrameData>) {
    std::thread::spawn(move || {
        let mainloop = match pw::main_loop::MainLoopRc::new(None) {
            Ok(ml) => ml,
            Err(_) => {
                return;
            }
        };
        let context = match pw::context::ContextRc::new(&mainloop, None) {
            Ok(ctx) => ctx,
            Err(_) => {
                return;
            }
        };
        let loop_ = mainloop.loop_();

        let core = match context.connect_fd_rc(fd, None) {
            Ok(c) => c,
            Err(_) => {
                return;
            }
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
            Err(_) => {
                return;
            }
        };

        let frame_data_for_process = Arc::clone(&stream_data.frame);
        let format_info_for_process = Arc::clone(&stream_data.format_info);
        let format_info_for_param = Arc::clone(&stream_data.format_info);
        let ready_flag_for_param = Arc::clone(&stream_data.ready);
        let stream_idx_for_log = idx;
        let title_for_log = stream_data.title.clone();
        let mut fps_window_start = Instant::now();
        let mut frames_in_fps_window = 0u64;

        let _listener = stream
            .add_local_listener_with_user_data(())
            .state_changed(move |_stream, _user_data: &mut (), _old, _new| {})
            .param_changed(move |stream, _user_data: &mut (), id, param| {
                let Some(param) = param else {
                    return;
                };
                if id != pw::spa::param::ParamType::Format.as_raw() {
                    return;
                }

                let (media_type, media_subtype) =
                    match pw::spa::param::format_utils::parse_format(param) {
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
                    if let Some(mut guard) = format_info_for_param.try_lock() {
                        guard.replace((format.size().width, format.size().height, format.format()));
                    }
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
                        if datas.is_empty() {
                            return;
                        }

                        let data = &mut datas[0];
                        let chunk = data.chunk();
                        let size = chunk.size() as usize;

                        if let Some(samples) = data.data() {
                            if size > 0 {
                                // Use try_lock to avoid blocking the PipeWire main loop.
                                // If the draw thread holds the lock, we skip this frame
                                // rather than freezing the stream thread.
                                let (w, h, fmt) = {
                                    match format_info_for_process.try_lock() {
                                        Some(fi) => match fi.as_ref() {
                                            Some(f) => (f.0, f.1, f.2),
                                            None => return,
                                        },
                                        None => return,
                                    }
                                };
                                if w > 0 && h > 0 {
                                    let bytes = samples[0..size.min(samples.len())].to_vec();
                                    // If we can't get the frame lock, skip this frame
                                    let result =
                                        frame_data_for_process.try_lock().map(|mut guard| {
                                            guard.replace((w, h, fmt, bytes));
                                        });
                                    let _ = result;
                                    frames_in_fps_window += 1;
                                    let elapsed = fps_window_start.elapsed();
                                    if elapsed >= std::time::Duration::from_secs(2) {
                                        let fps = frames_in_fps_window as f64 / elapsed.as_secs_f64();
                                        eprintln!(
                                            "[PIPEWIRE] Stream {} (title={}) fps: {:.1}",
                                            stream_idx_for_log, title_for_log, fps
                                        );
                                        frames_in_fps_window = 0;
                                        fps_window_start = Instant::now();
                                    }
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
                pw::spa::utils::Rectangle {
                    width: 1920,
                    height: 1080
                },
                pw::spa::utils::Rectangle {
                    width: 1,
                    height: 1
                },
                pw::spa::utils::Rectangle {
                    width: 7680,
                    height: 4320
                }
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
        )
        .unwrap()
        .0
        .into_inner();

        let mut stream_params = vec![Pod::from_bytes(&values).unwrap()];
        if stream
            .connect(
                pw::spa::utils::Direction::Input,
                Some(node_id),
                pw::stream::StreamFlags::AUTOCONNECT
                    | pw::stream::StreamFlags::MAP_BUFFERS
                    | pw::stream::StreamFlags::RT_PROCESS,
                &mut stream_params,
            )
            .is_err()
        {
            return;
        }

        // Run the main loop persistently
        loop {
            let timeout = pw::loop_::Timeout::Finite(std::time::Duration::from_millis(16));
            loop_.iterate(timeout);
        }
    });
}

/// Start a background monitor task that incrementally manages PipeWire streams.
///
/// Reacts to `probe_signal` notifications (fired on `WindowEvent::Created`)
/// instead of polling, so it only runs when windows actually change.
///
/// Each monitor cycle has two phases:
///
/// **Phase 1 — Remove stale streams**: Compares current stream titles against
/// the portal's window list. Streams whose titles no longer exist are removed.
/// Uses count-based matching to handle duplicate titles (e.g., two "New tab" windows).
///
/// **Phase 2 — Add new streams**: For windows that don't have a stream yet,
/// captures them via `capture_windows_range` and binds them to Wayland IDs.
///
/// On startup, a 1-second timeout ensures the monitor runs at least once
/// even if no `WindowEvent::Created` fires (all windows already exist).
///
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
    let initial_count = service.capture_state.lock().count;

    let mut _last_count = initial_count;
    loop {
        // Wait for a probe signal, but with a timeout on the first iteration.
        // On startup, no WindowEvent::Created fires because all windows already
        // exist, so the monitor would block forever. The timeout ensures the
        // monitor runs at least once after update_window_ids has been called
        // by the draw loop, allowing stream-to-window binding to work.
        {
            let notified = probe_signal.notified();
            tokio::pin!(notified);
            let first_iter_delay = tokio::time::sleep(std::time::Duration::from_secs(1));
            tokio::pin!(first_iter_delay);
            tokio::select! {
                _ = &mut notified => {},
                _ = &mut first_iter_delay => {},
            }
        }

        // First, trigger a full portal session (SelectSources + Start) to refresh the chooser file
        // with all current window titles. The chooser script only runs during Start, so we must
        // complete the full cycle. We discard the resulting stream FD and node ID.
        // If the portal session fails or times out, we still try to read the existing chooser file below.
        // A timeout wraps the entire portal phase to prevent the monitor from freezing indefinitely
        // when certain windows (e.g., pcmanfm-qt) cause the wlr portal to stall.
        {
            let _ = std::fs::write("/tmp/chooser-index.txt", "0");
            let _portal_result = tokio::time::timeout(std::time::Duration::from_secs(15), async {
                let session = match create_session(&connection, &desktop).await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let portal_restore_token = {
                    let token = service.restore_token.lock();
                    token.clone()
                };
                if select_sources(
                    &connection,
                    &desktop,
                    &session,
                    portal_restore_token.as_deref(),
                )
                .await
                .is_ok()
                {
                    let _ = start_capture(&connection, &desktop, &session).await;
                }
            })
            .await;
            // If the timeout fired, the portal session was abandoned — the chooser file
            // may be stale, but the monitor continues instead of freezing.
        }

        // Small async delay to let the chooser script finish writing the file
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Read all window titles from the chooser file to detect new windows.
        // The chooser file is updated by chooser-all.sh on every portal call.
        // If the portal session failed, the file may be stale, but it's better than nothing.
        let chooser_titles: Vec<String> =
            if let Ok(content) = std::fs::read_to_string("/tmp/chooser-windows.txt") {
                content
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.trim().to_string())
                    .collect()
            } else {
                continue;
            };

        // Truncated titles for debug logging only — use char indices, not byte indices,
        // to avoid panics on multi-byte Unicode characters (e.g., apostrophes, emojis).
        let _chooser_titles_truncated: Vec<String> = chooser_titles
            .iter()
            .map(|t| {
                if t.chars().count() > 20 {
                    format!("{}...", t.chars().take(17).collect::<String>())
                } else {
                    t.clone()
                }
            })
            .collect();

        let new_count = chooser_titles.len();

        if new_count == 0 {
            continue;
        }

        // --- Phase 0: Update stream titles in-place ---
        // When a window changes its title (e.g., pcmanfm-qt navigating folders),
        // the stream is still valid — only the title string changed. Update titles
        // before Phase 1 so the count-based matching sees the current titles.
        // This prevents valid streams from being removed just because their title changed.
        //
        // Minimize lock hold time: extract only what we need, then release locks.
        {
            // Quick snapshot of stream titles and count
            let (stream_count, stream_titles): (usize, Vec<String>) = {
                let frames = service_frames.lock();
                let titles: Vec<String> = frames.iter().map(|s| s.title.clone()).collect();
                (frames.len(), titles)
            };

            if stream_count == chooser_titles.len() && stream_count > 0 {
                // Same count — likely just title changes.
                let mut portal_counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for ct in &chooser_titles {
                    let clean = PipewireCaptureService::strip_portal_hash(ct).to_string();
                    *portal_counts.entry(clean).or_insert(0) += 1;
                }

                let mut stream_title_counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                for t in &stream_titles {
                    let clean = PipewireCaptureService::strip_portal_hash(t).to_string();
                    *stream_title_counts.entry(clean).or_insert(0) += 1;
                }

                let counts_match = {
                    let pk: std::collections::HashSet<_> = portal_counts.iter().collect();
                    let sk: std::collections::HashSet<_> = stream_title_counts.iter().collect();
                    pk == sk
                };

                if counts_match {
                    let mut state = service_state.lock();
                    state.titles = chooser_titles.clone();
                    state.count = stream_count;
                }
            }
        }

        // --- Phase 1: Remove stale streams ---
        // A stream is stale if its bound window_id is no longer in the window_ids map
        // (window closed) OR if there are more streams with a title than portal windows
        // with that title (extra stream from a closed window with the same title).
        // Streams with valid window_id bindings are preserved even if their title changed.

        // Save the active row's window_id and stream count BEFORE retain() removes streams.
        // After removal, we'll remap active_row to the new index of this window.
        //
        // Minimize lock hold time: extract data quickly, release locks, then compute.
        let (active_window_id_before_removal, stream_count_before) = {
            let row = *service.active_row.lock();
            let frames = service_frames.lock();
            let wid = frames.get(row).and_then(|s| *s.window_id.lock());
            (wid, frames.len())
        };

        // Collect valid window IDs outside of stream_frames lock
        let valid_window_ids: HashSet<u32> = {
            let window_ids_map = service.window_ids.lock();
            window_ids_map.values().flatten().copied().collect()
        };

        // Count how many times each portal title appears (hash-stripped)
        let mut portal_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for ct in &chooser_titles {
            let clean = PipewireCaptureService::strip_portal_hash(ct).to_string();
            *portal_counts.entry(clean).or_insert(0) += 1;
        }

        // Phase 1: Remove stale streams
        // First, collect which indices to remove, then remove them in one pass.
        eprintln!(
            "[PIPEWIRE] Monitor Phase 1: checking {} streams for stale entries",
            { service_frames.lock().len() }
        );
        let indices_to_remove: Vec<usize> = {
            let frames = service_frames.lock();
            let mut to_remove = Vec::new();

            // First pass: mark streams whose window_id is no longer valid
            let mut bound_kept: std::collections::HashSet<usize> = std::collections::HashSet::new();
            for (i, stream) in frames.iter().enumerate() {
                if let Some(wid) = *stream.window_id.lock() {
                    if valid_window_ids.contains(&wid) {
                        bound_kept.insert(i);
                    } else {
                        to_remove.push(i);
                    }
                }
            }

            // Second pass: for unbound streams, use title count matching
            let mut kept_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for (i, stream) in frames.iter().enumerate() {
                if bound_kept.contains(&i) {
                    continue;
                }
                let clean_title =
                    PipewireCaptureService::strip_portal_hash(&stream.title).to_string();
                let max_keep = *portal_counts.get(&clean_title).unwrap_or(&0);
                let current_kept = *kept_counts.get(&clean_title).unwrap_or(&0);
                if current_kept < max_keep {
                    *kept_counts.entry(clean_title).or_insert(0) += 1;
                } else {
                    to_remove.push(i);
                }
            }
            to_remove
        };

        // Apply removals
        let removed = indices_to_remove.len();
        if removed > 0 {
            {
                let mut frames = service_frames.lock();
                // Remove in reverse order to preserve indices
                for idx in indices_to_remove.iter().rev() {
                    if *idx < frames.len() {
                        frames.remove(*idx);
                    }
                }
            }

            // Rebuild bindings from retained streams
            {
                let mut bindings = service.bindings.lock();
                let frames = service_frames.lock();
                bindings.clear();
                for (i, stream) in frames.iter().enumerate() {
                    if let Some(sid) = *stream.window_id.lock() {
                        bindings.insert(sid, i);
                    }
                }
            }
        }

        // Phase 1b: For retained streams without a window_id, try to bind them now.
        {
            let window_ids_map = service.window_ids.lock();
            let bindings = service.bindings.lock();
            let _bound_window_ids: HashSet<u32> = bindings.keys().copied().collect();
            drop(bindings);

            // Collect unbound streams info
            let unbound_streams: Vec<(usize, String)> = {
                let frames = service_frames.lock();
                frames
                    .iter()
                    .enumerate()
                    .filter_map(|(i, stream)| {
                        let current_id = *stream.window_id.lock();
                        if current_id.is_none() {
                            Some((i, stream.title.clone()))
                        } else {
                            None
                        }
                    })
                    .collect()
            };
            drop(window_ids_map);

            // Assign window IDs to unbound streams.
            // For same-title windows, use the chooser file order to determine which
            // window gets which stream, matching the portal's stream index order.
            for (i, title) in unbound_streams {
                let clean_title = PipewireCaptureService::strip_portal_hash(&title).to_string();
                let window_ids_map = service.window_ids.lock();
                let bindings = service.bindings.lock();
                let bound_window_ids: HashSet<u32> = bindings.keys().copied().collect();
                drop(bindings);
                if let Some(ids) = window_ids_map.get(&clean_title) {
                    let id_to_use = if ids.len() <= 1 {
                        ids.iter()
                            .find(|id| !bound_window_ids.contains(id))
                            .copied()
                    } else {
                        // Multiple windows with the same title — use chooser file order.
                        // Count how many chooser entries with this title appear before
                        // this stream index, then pick the corresponding unbound ID.
                        let mut same_title_count_before = 0;
                        for ci in 0..i {
                            if let Some(chooser_title) = chooser_titles.get(ci) {
                                let chooser_clean =
                                    PipewireCaptureService::strip_portal_hash(chooser_title)
                                        .to_string();
                                if chooser_clean == clean_title {
                                    same_title_count_before += 1;
                                }
                            }
                        }
                        ids.iter()
                            .filter(|id| !bound_window_ids.contains(id))
                            .nth(same_title_count_before)
                            .copied()
                    };
                    if let Some(id) = id_to_use {
                        let frames = service_frames.lock();
                        if let Some(stream) = frames.get(i) {
                            *stream.window_id.lock() = Some(id);
                            let t = stream.title.clone();
                            drop(frames);
                            service.bind_window_to_stream(id, i, &t);
                        }
                    }
                }
                drop(window_ids_map);
            }
        }

        // Remap active_row after Phase 1 removal: if the previously active window
        // still has a stream, point active_row to its new index.
        let stream_count_after = {
            let frames = service_frames.lock();
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
            let frames = service_frames.lock();
            let mut row = service.active_row.lock();
            if frames.is_empty() {
                *row = 0;
            } else if *row >= frames.len() {
                *row = frames.len() - 1;
            }
        }

        // Re-read existing titles after removal (collect before any .await)
        let existing_titles: Vec<String> = {
            let frames = service_frames.lock();
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
            let mut stream_title_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for t in &existing_titles {
                let clean = PipewireCaptureService::strip_portal_hash(t).to_string();
                *stream_title_counts.entry(clean).or_insert(0) += 1;
            }

            // Count portal windows per title (hash-stripped)
            let mut portal_title_counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for ct in &chooser_titles {
                let clean = PipewireCaptureService::strip_portal_hash(ct).to_string();
                *portal_title_counts.entry(clean).or_insert(0) += 1;
            }

            // Track how many streams we already have per title (for incremental matching)
            let mut matched_so_far: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();

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
                let mut state = service_state.lock();
                state.titles = chooser_titles.clone();
                state.count = current_stream_count;
            }
            _last_count = current_stream_count;
            continue;
        }

        // Capture each new window
        let mut added_count = 0;
        let current_restore_token = {
            let token = service.restore_token.lock();
            token.clone()
        };
        for (idx, portal_idx) in to_capture.into_iter().enumerate() {
            match capture_windows_range(
                &connection,
                &desktop,
                portal_idx,
                portal_idx + 1,
                current_restore_token.clone(),
            )
            .await
            {
                Ok((new_streams, _new_token)) => {
                    for (fd, node_id, captured_title) in new_streams {
                        let stream_idx = current_stream_count + idx + added_count;
                        // Look up the stable Wayland window ID for this title.
                        // Strip the portal hash suffix so "dock (hash)" matches Wayland title "dock".
                        // For same-title windows, use the chooser file order (portal_idx)
                        // to determine which window gets which stream.
                        let window_id = {
                            // Release window_ids before bindings to avoid lock ordering issues
                            let ids = {
                                let map = service.window_ids.lock();
                                let clean_title =
                                    PipewireCaptureService::strip_portal_hash(&captured_title);
                                map.get(clean_title).cloned()
                            };
                            ids.and_then(|v| {
                                let bindings = service.bindings.lock();
                                if v.len() <= 1 {
                                    v.iter().find(|id| !bindings.contains_key(id)).copied()
                                } else {
                                    // Multiple windows with the same title — use chooser file order.
                                    let clean_title_for_count =
                                        PipewireCaptureService::strip_portal_hash(&captured_title)
                                            .to_string();
                                    let mut same_title_count_before = 0;
                                    for ci in 0..portal_idx {
                                        if let Some(chooser_title) = chooser_titles.get(ci) {
                                            let chooser_clean =
                                                PipewireCaptureService::strip_portal_hash(
                                                    chooser_title,
                                                )
                                                .to_string();
                                            if chooser_clean == clean_title_for_count {
                                                same_title_count_before += 1;
                                            }
                                        }
                                    }
                                    v.iter()
                                        .filter(|id| !bindings.contains_key(id))
                                        .nth(same_title_count_before)
                                        .copied()
                                }
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
                            let mut frames = service_frames.lock();
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
            let mut state = service_state.lock();
            state.count = current_stream_count + added_count;
            state.titles = chooser_titles.clone();
        }
        _last_count = current_stream_count + added_count;
    }
}

/// Convert a raw video frame to RGBA with adaptive downscaling.
///
/// Supports BGRx, BGRA, RGBA, and RGB input formats. The `scale_factor`
/// parameter is computed by `get_active_frame()` based on the target display
/// dimensions: if the source frame is larger than the target, the scale is
/// computed to fit within the target; otherwise scale is 1.0 (no downscaling).
///
/// For downscaling, uses bilinear interpolation to produce smooth, anti-aliased
/// text and edges. Each output pixel is a weighted average of the 4 nearest
/// source pixels, eliminating the choppy aliasing artifacts of nearest-neighbor
/// sampling.
///
/// Returns `(width, height, rgba_bytes)` where `rgba_bytes` is a flat
/// vector of RGBA pixels (4 bytes per pixel, row-major order).
fn convert_to_rgba(
    w: u32,
    h: u32,
    fmt: pw::spa::param::video::VideoFormat,
    bytes: &[u8],
    scale_factor: f32,
) -> (u32, u32, Vec<u8>) {
    let out_w = if scale_factor < 1.0 {
        (w as f32 * scale_factor).max(1.0) as u32
    } else {
        w
    };
    let out_h = if scale_factor < 1.0 {
        (h as f32 * scale_factor).max(1.0) as u32
    } else {
        h
    };

    let mut rgba = vec![0u8; (out_w * out_h * 4) as usize];

    match fmt {
        pw::spa::param::video::VideoFormat::BGRx | pw::spa::param::video::VideoFormat::BGRA => {
            if scale_factor < 1.0 {
                // Bilinear interpolation for BGRx/BGRA → RGBA downscaling.
                // Each output pixel is a weighted average of 4 source pixels,
                // producing smooth text and edges instead of choppy aliasing.
                let src_w = w as usize;
                let src_h = h as usize;
                let dst_w = out_w as usize;
                let dst_h = out_h as usize;
                let scale_x = src_w as f64 / dst_w as f64;
                let scale_y = src_h as f64 / dst_h as f64;

                for y in 0..dst_h {
                    let y0 = (y as f64 * scale_y) as f64;
                    let x0_row = y0.floor() as usize * src_w * 4;
                    let y1_row = (y0.floor() as usize + 1).min(src_h - 1) * src_w * 4;
                    let dy = y0 - y0.floor();

                    for x in 0..dst_w {
                        let x0 = (x as f64 * scale_x) as f64;
                        let x0i = x0.floor() as usize;
                        let x1i = (x0i + 1).min(src_w - 1);
                        let dx = x0 - x0i as f64;

                        // Bilinear weights
                        let w00 = (1.0 - dx) * (1.0 - dy);
                        let w10 = dx * (1.0 - dy);
                        let w01 = (1.0 - dx) * dy;
                        let w11 = dx * dy;

                        // Source pixel offsets (BGRx/BGRA: B=0, G=1, R=2)
                        let s00 = x0_row + x0i * 4;
                        let s10 = x0_row + x1i * 4;
                        let s01 = y1_row + x0i * 4;
                        let s11 = y1_row + x1i * 4;
                        let dst = (y * dst_w + x) * 4;

                        if s11 + 3 < bytes.len() && dst + 3 < rgba.len() {
                            // R channel (src index 2)
                            rgba[dst] = ((bytes[s00 + 2] as f64 * w00
                                + bytes[s10 + 2] as f64 * w10
                                + bytes[s01 + 2] as f64 * w01
                                + bytes[s11 + 2] as f64 * w11)
                                + 0.5) as u8;
                            // G channel (src index 1)
                            rgba[dst + 1] = ((bytes[s00 + 1] as f64 * w00
                                + bytes[s10 + 1] as f64 * w10
                                + bytes[s01 + 1] as f64 * w01
                                + bytes[s11 + 1] as f64 * w11)
                                + 0.5) as u8;
                            // B channel (src index 0)
                            rgba[dst + 2] = ((bytes[s00] as f64 * w00
                                + bytes[s10] as f64 * w10
                                + bytes[s01] as f64 * w01
                                + bytes[s11] as f64 * w11)
                                + 0.5) as u8;
                            rgba[dst + 3] = 255;
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
                // Bilinear interpolation for RGBA → RGBA downscaling.
                let src_w = w as usize;
                let src_h = h as usize;
                let dst_w = out_w as usize;
                let dst_h = out_h as usize;
                let scale_x = src_w as f64 / dst_w as f64;
                let scale_y = src_h as f64 / dst_h as f64;

                for y in 0..dst_h {
                    let y0 = (y as f64 * scale_y) as f64;
                    let x0_row = y0.floor() as usize * src_w * 4;
                    let y1_row = (y0.floor() as usize + 1).min(src_h - 1) * src_w * 4;
                    let dy = y0 - y0.floor();

                    for x in 0..dst_w {
                        let x0 = (x as f64 * scale_x) as f64;
                        let x0i = x0.floor() as usize;
                        let x1i = (x0i + 1).min(src_w - 1);
                        let dx = x0 - x0i as f64;

                        let w00 = (1.0 - dx) * (1.0 - dy);
                        let w10 = dx * (1.0 - dy);
                        let w01 = (1.0 - dx) * dy;
                        let w11 = dx * dy;

                        let s00 = x0_row + x0i * 4;
                        let s10 = x0_row + x1i * 4;
                        let s01 = y1_row + x0i * 4;
                        let s11 = y1_row + x1i * 4;
                        let dst = (y * dst_w + x) * 4;

                        if s11 + 3 < bytes.len() && dst + 3 < rgba.len() {
                            // R channel
                            rgba[dst] = ((bytes[s00] as f64 * w00
                                + bytes[s10] as f64 * w10
                                + bytes[s01] as f64 * w01
                                + bytes[s11] as f64 * w11)
                                + 0.5) as u8;
                            // G channel
                            rgba[dst + 1] = ((bytes[s00 + 1] as f64 * w00
                                + bytes[s10 + 1] as f64 * w10
                                + bytes[s01 + 1] as f64 * w01
                                + bytes[s11 + 1] as f64 * w11)
                                + 0.5) as u8;
                            // B channel
                            rgba[dst + 2] = ((bytes[s00 + 2] as f64 * w00
                                + bytes[s10 + 2] as f64 * w10
                                + bytes[s01 + 2] as f64 * w01
                                + bytes[s11 + 2] as f64 * w11)
                                + 0.5) as u8;
                            // A channel
                            rgba[dst + 3] = ((bytes[s00 + 3] as f64 * w00
                                + bytes[s10 + 3] as f64 * w10
                                + bytes[s01 + 3] as f64 * w01
                                + bytes[s11 + 3] as f64 * w11)
                                + 0.5) as u8;
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
                // Bilinear interpolation for RGB → RGBA downscaling.
                let src_w = w as usize;
                let src_h = h as usize;
                let dst_w = out_w as usize;
                let dst_h = out_h as usize;
                let scale_x = src_w as f64 / dst_w as f64;
                let scale_y = src_h as f64 / dst_h as f64;

                for y in 0..dst_h {
                    let y0 = (y as f64 * scale_y) as f64;
                    let x0_row = y0.floor() as usize * src_w * 3;
                    let y1_row = (y0.floor() as usize + 1).min(src_h - 1) * src_w * 3;
                    let dy = y0 - y0.floor();

                    for x in 0..dst_w {
                        let x0 = (x as f64 * scale_x) as f64;
                        let x0i = x0.floor() as usize;
                        let x1i = (x0i + 1).min(src_w - 1);
                        let dx = x0 - x0i as f64;

                        let w00 = (1.0 - dx) * (1.0 - dy);
                        let w10 = dx * (1.0 - dy);
                        let w01 = (1.0 - dx) * dy;
                        let w11 = dx * dy;

                        let s00 = x0_row + x0i * 3;
                        let s10 = x0_row + x1i * 3;
                        let s01 = y1_row + x0i * 3;
                        let s11 = y1_row + x1i * 3;
                        let dst = (y * dst_w + x) * 4;

                        if s11 + 2 < bytes.len() && dst + 3 < rgba.len() {
                            // R channel
                            rgba[dst] = ((bytes[s00] as f64 * w00
                                + bytes[s10] as f64 * w10
                                + bytes[s01] as f64 * w01
                                + bytes[s11] as f64 * w11)
                                + 0.5) as u8;
                            // G channel
                            rgba[dst + 1] = ((bytes[s00 + 1] as f64 * w00
                                + bytes[s10 + 1] as f64 * w10
                                + bytes[s01 + 1] as f64 * w01
                                + bytes[s11 + 1] as f64 * w11)
                                + 0.5) as u8;
                            // B channel
                            rgba[dst + 2] = ((bytes[s00 + 2] as f64 * w00
                                + bytes[s10 + 2] as f64 * w10
                                + bytes[s01 + 2] as f64 * w01
                                + bytes[s11 + 2] as f64 * w11)
                                + 0.5) as u8;
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
            // Fallback: treat as BGRx with bilinear interpolation
            if scale_factor < 1.0 {
                let src_w = w as usize;
                let src_h = h as usize;
                let dst_w = out_w as usize;
                let dst_h = out_h as usize;
                let scale_x = src_w as f64 / dst_w as f64;
                let scale_y = src_h as f64 / dst_h as f64;

                for y in 0..dst_h {
                    let y0 = (y as f64 * scale_y) as f64;
                    let x0_row = y0.floor() as usize * src_w * 4;
                    let y1_row = (y0.floor() as usize + 1).min(src_h - 1) * src_w * 4;
                    let dy = y0 - y0.floor();

                    for x in 0..dst_w {
                        let x0 = (x as f64 * scale_x) as f64;
                        let x0i = x0.floor() as usize;
                        let x1i = (x0i + 1).min(src_w - 1);
                        let dx = x0 - x0i as f64;

                        let w00 = (1.0 - dx) * (1.0 - dy);
                        let w10 = dx * (1.0 - dy);
                        let w01 = (1.0 - dx) * dy;
                        let w11 = dx * dy;

                        let s00 = x0_row + x0i * 4;
                        let s10 = x0_row + x1i * 4;
                        let s01 = y1_row + x0i * 4;
                        let s11 = y1_row + x1i * 4;
                        let dst = (y * dst_w + x) * 4;

                        if s11 + 3 < bytes.len() && dst + 3 < rgba.len() {
                            rgba[dst] = ((bytes[s00 + 2] as f64 * w00
                                + bytes[s10 + 2] as f64 * w10
                                + bytes[s01 + 2] as f64 * w01
                                + bytes[s11 + 2] as f64 * w11)
                                + 0.5) as u8;
                            rgba[dst + 1] = ((bytes[s00 + 1] as f64 * w00
                                + bytes[s10 + 1] as f64 * w10
                                + bytes[s01 + 1] as f64 * w01
                                + bytes[s11 + 1] as f64 * w11)
                                + 0.5) as u8;
                            rgba[dst + 2] = ((bytes[s00] as f64 * w00
                                + bytes[s10] as f64 * w10
                                + bytes[s01] as f64 * w01
                                + bytes[s11] as f64 * w11)
                                + 0.5) as u8;
                            rgba[dst + 3] = 255;
                        }
                    }
                }
            }
        }
    }

    (out_w, out_h, rgba)
}
