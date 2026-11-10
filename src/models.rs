//! Data model re-exports.
//!
//! Re-exports all model types from the system submodule.

// src/models.rs

use crate::geometry::Rect;
use wayland_client::backend::ObjectId;

// --- Status Notifier / System Tray Models ---

#[derive(Debug, Clone)]
pub struct TrayIcon {
    /// Icon pixels in RGBA order. The renderer converts this to wl_shm's
    /// little-endian ARGB8888 layout when it writes the destination buffer.
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrayStatus {
    Passive,
    Active,
    NeedsAttention,
    Unknown,
}

impl TrayStatus {
    pub fn from_dbus(status: &str) -> Self {
        match status {
            "Passive" => Self::Passive,
            "Active" => Self::Active,
            "NeedsAttention" => Self::NeedsAttention,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrayItem {
    /// Stable identifier used by the dock-side state registry.
    pub key: String,
    /// D-Bus destination/service name advertised to StatusNotifierWatcher.
    pub service_name: String,
    pub object_path: String,
    /// Interface implemented by this item. Most clients use the freedesktop
    /// spelling, while Steam and several Ayatana/KDE-era clients use the KDE
    /// spelling.
    pub interface_name: String,
    pub id: String,
    pub title: String,
    pub category: String,
    pub status: TrayStatus,
    pub icon_name: String,
    pub icon_theme_path: Option<String>,
    pub icon: Option<TrayIcon>,
    pub tooltip: String,
    pub item_is_menu: bool,
    pub menu_path: Option<String>,
}

impl TrayItem {
    pub fn display_name(&self) -> &str {
        if !self.title.is_empty() {
            &self.title
        } else if !self.id.is_empty() {
            &self.id
        } else {
            &self.service_name
        }
    }
}

#[derive(Debug, Clone)]
pub struct TraySnapshot {
    pub items: Vec<TrayItem>,
}

#[derive(Debug, Clone)]
pub enum TrayActionKind {
    Activate,
    SecondaryActivate,
    ContextMenu,
    Scroll {
        delta: i32,
        orientation: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct TrayAction {
    pub key: String,
    pub service_name: String,
    pub object_path: String,
    /// D-Bus interface used by the item (freedesktop or KDE spelling).
    pub interface_name: String,
    /// Optional DBusMenu object path advertised by the SNI item.
    pub menu_path: Option<String>,
    pub kind: TrayActionKind,
    /// Screen-position hint in dock-local logical coordinates. Tray clients are
    /// permitted to treat these coordinates as a hint; many ignore them.
    pub x: i32,
    pub y: i32,
}

/// Compact status-bar data sampled outside the Wayland render loop. Values are
/// display-ready strings so the renderer does no process or filesystem I/O.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemBarSnapshot {
    pub keyboard_layout: String,
    pub download_rate: String,
    pub upload_rate: String,
    pub date: String,
    pub time: String,
}

/// Audio device (sink or source) from PipeWire/WirePlumber.
///
/// IDs are PipeWire node IDs, not PulseAudio indexes, so they can be passed
/// directly to `wpctl` commands for volume/mute control.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
    pub is_default: bool,
}

/// Active audio stream from PipeWire (e.g., a playing application).
///
/// Each stream has its own PipeWire node ID that can be controlled independently.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub id: u32,
    pub name: String,
    pub volume: f32,
    pub muted: bool,
}

/// Complete snapshot of the audio state from PipeWire/WirePlumber.
///
/// Contains output/input devices, active streams, default device info,
/// and MPRIS media player metadata. Sent from the audio monitor thread
/// to the main loop only when the state changes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AudioSnapshot {
    pub available: bool,
    pub error: Option<String>,
    pub outputs: Vec<AudioDevice>,
    pub inputs: Vec<AudioDevice>,
    pub streams: Vec<AudioStream>,
    pub default_output: Option<u32>,
    pub default_input: Option<u32>,
    pub output_volume: f32,
    pub output_muted: bool,
    pub input_volume: f32,
    pub input_muted: bool,
    pub media_artist: Option<String>,
    pub media_title: Option<String>,
    pub media_status: Option<String>,
    pub media_position_seconds: Option<f64>,
    pub media_duration_seconds: Option<f64>,
}

/// Media player transport controls sent to `playerctl`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaControl {
    Previous,
    PlayPause,
    Next,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioSliderTarget {
    Output,
    Input,
    Stream(u32),
}

/// User audio action sent from the UI to the audio monitor thread.
///
/// The monitor thread coalesces rapid actions (e.g., slider drag) at ~40Hz
/// and executes them via `wpctl` or `playerctl` subprocesses.
#[derive(Debug, Clone)]
pub enum AudioAction {
    SetOutputVolume { node_id: u32, volume: f32 },
    SetInputVolume { node_id: u32, volume: f32 },
    SetStreamVolume { node_id: u32, volume: f32 },
    SetOutputMute { node_id: u32, muted: bool },
    SetInputMute { node_id: u32, muted: bool },
    SetStreamMute { node_id: u32, muted: bool },
    SetDefaultOutput { node_id: u32 },
    SetDefaultInput { node_id: u32 },
    LaunchMixer,
    MediaControl(MediaControl),
}

/// One BlueZ remote device. `path` is the object path returned by BlueZ's
/// ObjectManager and is used for Device1 actions; the address is for display
/// and diagnostics only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BluetoothDevice {
    pub path: String,
    pub address: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
}

/// Snapshot of the local BlueZ adapter and discovered/known remote devices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BluetoothSnapshot {
    pub available: bool,
    pub error: Option<String>,
    pub adapter_path: Option<String>,
    pub adapter_name: String,
    pub powered: bool,
    pub discovering: bool,
    /// Whether this dock instance owns a BlueZ discovery session. BlueZ shares
    /// the radio scan across clients, but each client must stop only its own
    /// session; an external bluetoothctl/Blueman scan must not be stopped here.
    pub discovery_owned: bool,
    pub devices: Vec<BluetoothDevice>,
    /// Short-lived result text shown in the popup after an action.
    pub status_message: Option<String>,
}

#[derive(Debug, Clone)]
pub enum BluetoothAction {
    SetPowered(bool),
    SetDiscovering(bool),
    Connect(String),
    Disconnect(String),
    Pair(String),
}

/// Capacity and identity for one mounted rclone FUSE filesystem.
/// A zero `total_bytes` means that the backend did not expose capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RcloneMount {
    pub mount_point: String,
    pub source: String,
    pub fs_type: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RcloneSnapshot {
    pub mounts: Vec<RcloneMount>,
    pub error: Option<String>,
}

/// Per-process data sampled from `/proc` through sysinfo on a worker thread.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcessSnapshot {
    pub processes: Vec<ProcessEntry>,
    pub total_processes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ProcessSort {
    #[default]
    Cpu,
    Memory,
}

// --- D-Bus / Dock Badge Models ---

#[derive(Debug, Clone, Default)]
pub struct BadgeUpdate {
    pub desktop_id: String,
    pub count: i64,
    pub count_visible: bool,
    pub progress: f64,
    pub progress_visible: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct DockItem {
    pub name: String,
    pub desktop_id: String, // e.g., "discord", "thunderbird"
    pub badge_count: i64,
    pub show_badge: bool,
    pub progress: f64,
    pub show_progress: bool,
    pub is_urgent: bool,
}

impl DockItem {
    pub fn apply_badge_update(&mut self, update: &BadgeUpdate) {
        self.badge_count = update.count;
        self.show_badge = update.count_visible && update.count > 0;
        self.progress = update.progress.clamp(0.0, 1.0);
        self.show_progress = update.progress_visible;
        self.is_urgent = update.urgent;
    }
}

// --- Media Window Tracking Models ---

#[derive(Clone, Debug, PartialEq)]
pub enum MediaSource {
    Unknown,
    WaylandToplevel,
    XWaylandWindow,
    PipeWire,
    Mpris,
    Browser,
    GameCapture,
    ScreenCast,
    FileUrl,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaWindowLink {
    /// Underlying compositor/window handle if known.
    pub handle: Option<ObjectId>,
    /// Media URI / source link (file, stream, page URL, etc.).
    pub uri: Option<String>,
    /// The outer window bounds in logical coordinates.
    pub window_rect: Option<Rect>,
    /// The actual content/video bounds if they differ from the outer window.
    pub content_rect: Option<Rect>,
    /// Known video region to crop later when extracting or mirroring video.
    pub video_rect: Option<Rect>,
    /// Human readable label (browser tab, player title, game capture, etc.).
    pub title: Option<String>,
    /// Playback state is independent from whether a media link exists.
    pub is_playing: bool,
    pub muted: bool,
    pub volume: Option<f32>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub source: MediaSource,
}

impl Default for MediaWindowLink {
    fn default() -> Self {
        Self {
            handle: None,
            uri: None,
            window_rect: None,
            content_rect: None,
            video_rect: None,
            title: None,
            is_playing: false,
            muted: false,
            volume: None,
            position_ms: None,
            duration_ms: None,
            source: MediaSource::Unknown,
        }
    }
}

#[path = "models/system.rs"]
pub mod system;

pub use system::*;
