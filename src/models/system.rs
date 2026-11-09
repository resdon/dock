//! System data models.
//!
//! Defines snapshot structs for audio, Bluetooth, rclone, processes,
//! system bar, tray items, and badge updates.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::models::MediaWindowLink;
use wayland_client::backend::ObjectId;

/// Stable identity for an application in the dock/system registry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppId(pub String);

impl AppId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for AppId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for AppId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Stable identity for a Wayland window/toplevel object.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WindowId(pub ObjectId);

impl PartialOrd for WindowId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WindowId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.protocol_id().cmp(&other.0.protocol_id())
    }
}

impl From<ObjectId> for WindowId {
    fn from(value: ObjectId) -> Self {
        Self(value)
    }
}

/// Stable identity for a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProcessId(pub u32);

impl From<u32> for ProcessId {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

/// Stable identity for an output/monitor. Uses the Wayland object identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OutputId(pub ObjectId);

impl From<ObjectId> for OutputId {
    fn from(value: ObjectId) -> Self {
        Self(value)
    }
}

/// Stable identifier for a Wayland surface used by the event/state boundary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SurfaceId(pub ObjectId);

impl From<ObjectId> for SurfaceId {
    fn from(value: ObjectId) -> Self {
        Self(value)
    }
}

/// Stable identifier for a Wayland data offer. Protocol objects remain owned by
/// the Wayland listener; this ID is what crosses into domain events.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OfferId(pub ObjectId);

impl From<ObjectId> for OfferId {
    fn from(value: ObjectId) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatingSystem {
    Linux,
    Windows,
    MacOS,
    FreeBsd,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisplayServer {
    Wayland,
    X11,
    Win32,
    Cocoa,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformContext {
    pub os: OperatingSystem,
    pub display_server: DisplayServer,
    pub architecture: Option<String>,
}

impl Default for PlatformContext {
    fn default() -> Self {
        Self {
            os: OperatingSystem::Unknown,
            display_server: DisplayServer::Unknown,
            architecture: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeKind {
    Native,
    Flatpak,
    Snap,
    AppImage,
    Wine,
    Proton,
    SteamLinuxRuntime,
    PythonVenv,
    DotNet,
    Java,
    Container,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeContext {
    pub kind: RuntimeKind,
    pub version: Option<String>,
    pub root: Option<PathBuf>,
}

impl Default for RuntimeContext {
    fn default() -> Self {
        Self {
            kind: RuntimeKind::Unknown,
            version: None,
            root: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackagingKind {
    SystemPackage,
    Flatpak,
    Snap,
    AppImage,
    Portable,
    Source,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackagingContext {
    pub kind: PackagingKind,
    pub package_id: Option<String>,
    pub source: Option<String>,
}

impl Default for PackagingContext {
    fn default() -> Self {
        Self {
            kind: PackagingKind::Unknown,
            package_id: None,
            source: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributionProvider {
    System,
    Steam,
    GOG,
    Epic,
    Lutris,
    Heroic,
    Bottles,
    FlatHub,
    Custom(String),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionContext {
    pub provider: DistributionProvider,
    pub channel: Option<String>,
    pub store_id: Option<String>,
}

impl Default for DistributionContext {
    fn default() -> Self {
        Self {
            provider: DistributionProvider::Unknown,
            channel: None,
            store_id: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BinaryContext {
    pub path: Option<PathBuf>,
    pub executable: Option<String>,
    pub working_directory: Option<PathBuf>,
    pub command_line: Option<String>,
    pub launcher: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ResolutionConfidence {
    #[default]
    Unknown,
    Heuristic,
    Probable,
    Strong,
    Exact,
}

/// Evidence accumulated while resolving an object. Multiple independent
/// signals can contribute without forcing a record to be rebuilt from scratch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolutionEvidence {
    ExactAppId,
    DesktopEntry,
    ExecutablePath,
    ExecutableName,
    ProcessId,
    ProcessEnvironment,
    RuntimeMarker,
    PackagingMarker,
    DistributionMarker,
    StoreId,
    IconMetadata,
    TitleHeuristic,
    OutputAssociation,
    WaylandState,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ResolutionTrace {
    pub confidence: ResolutionConfidence,
    pub evidence: Vec<ResolutionEvidence>,
}

impl ResolutionTrace {
    pub fn new(
        confidence: ResolutionConfidence,
        evidence: impl IntoIterator<Item = ResolutionEvidence>,
    ) -> Self {
        let mut trace = Self {
            confidence,
            evidence: Vec::new(),
        };
        for item in evidence {
            trace.add(item);
        }
        trace
    }

    pub fn add(&mut self, evidence: ResolutionEvidence) {
        if !self.evidence.contains(&evidence) {
            self.evidence.push(evidence);
        }
    }

    pub fn strengthen(
        &mut self,
        confidence: ResolutionConfidence,
        evidence: impl IntoIterator<Item = ResolutionEvidence>,
    ) {
        self.confidence = self.confidence.max(confidence);
        for item in evidence {
            self.add(item);
        }
    }

    pub fn merge(&mut self, other: &ResolutionTrace) {
        self.confidence = self.confidence.max(other.confidence);
        self.merge_evidence(other);
    }

    pub fn merge_evidence(&mut self, other: &ResolutionTrace) {
        for item in &other.evidence {
            self.add(*item);
        }
    }

    pub fn has(&self, evidence: ResolutionEvidence) -> bool {
        self.evidence.contains(&evidence)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationIdentity {
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub developer: Option<String>,
    pub publisher: Option<String>,
    pub homepage: Option<String>,
    pub description: Option<String>,
    pub icon: Option<PathBuf>,
    pub category: Option<String>,
}

impl Default for ApplicationIdentity {
    fn default() -> Self {
        Self {
            identifier: String::new(),
            name: String::new(),
            version: None,
            developer: None,
            publisher: None,
            homepage: None,
            description: None,
            icon: None,
            category: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchContext {
    pub launch_interface: Option<String>,
    pub command: Option<String>,
    pub uri: Option<String>,
    pub dbus_name: Option<String>,
    pub arguments: Vec<String>,
    pub working_directory: Option<PathBuf>,
}

impl Default for LaunchContext {
    fn default() -> Self {
        Self {
            launch_interface: None,
            command: None,
            uri: None,
            dbus_name: None,
            arguments: Vec::new(),
            working_directory: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallationContext {
    pub install_root: Option<PathBuf>,
    pub desktop_file: Option<PathBuf>,
    pub desktop_actions: Vec<String>,
}

impl Default for InstallationContext {
    fn default() -> Self {
        Self {
            install_root: None,
            desktop_file: None,
            desktop_actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplicationCapabilities {
    pub can_launch: bool,
    pub can_focus: bool,
    pub can_close: bool,
    pub can_pin: bool,
    pub can_show_jump_list: bool,
    pub can_show_window_list: bool,
    pub can_show_telemetry: bool,
    pub can_open_location: bool,
    pub can_open_settings: bool,
    pub can_uninstall: bool,
}

impl Default for ApplicationCapabilities {
    fn default() -> Self {
        Self {
            can_launch: true,
            can_focus: true,
            can_close: true,
            can_pin: true,
            can_show_jump_list: true,
            can_show_window_list: true,
            can_show_telemetry: false,
            can_open_location: false,
            can_open_settings: false,
            can_uninstall: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ApplicationPriority {
    Background,
    Normal,
    Important,
    Foreground,
    System,
}

impl Default for ApplicationPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ApplicationStatus {
    pub running: bool,
    pub foreground: bool,
    pub background: bool,
    pub hidden: bool,
    pub locked: bool,
    pub suspended: bool,
    pub paused: bool,
    pub stuck: bool,
    pub attention: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProcessPriority {
    Background,
    Normal,
    Important,
    Foreground,
    System,
}

impl Default for ProcessPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProcessStatus {
    pub running: bool,
    pub foreground: bool,
    pub background: bool,
    pub sandboxed: bool,
    pub locked: bool,
    pub suspended: bool,
    pub paused: bool,
    pub stuck: bool,
    pub responding: bool,
    pub exited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WindowPriority {
    Background,
    Normal,
    Important,
    Foreground,
    AlwaysOnTop,
    SystemTray,
}

impl Default for WindowPriority {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WindowLayer {
    Background,
    Normal,
    AlwaysOnTop,
    Overlay,
    SystemTray,
}

impl Default for WindowLayer {
    fn default() -> Self {
        Self::Normal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WindowStatus {
    Active,
    Minimized,
    Background,
    Hidden,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowCapabilities {
    pub can_activate: bool,
    pub can_minimize: bool,
    pub can_close: bool,
    pub can_move: bool,
    pub can_raise: bool,
    pub can_pin: bool,
    pub can_move_to_output: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutputGeometry {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OutputStatus {
    pub active: bool,
    pub primary: bool,
    pub enabled: bool,
    pub locked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CpuStats {
    pub usage_percent: f32,
    pub load_one: Option<f32>,
    pub load_five: Option<f32>,
    pub load_fifteen: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MemoryStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GraphicsStats {
    pub vram_total_bytes: u64,
    pub vram_used_bytes: u64,
    pub gpu_usage_percent: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NetworkStats {
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub rx_total_bytes: u64,
    pub tx_total_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FrameStats {
    pub fps: f32,
    pub frame_time_ms: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SystemTelemetry {
    pub cpu: CpuStats,
    pub memory: MemoryStats,
    pub graphics: GraphicsStats,
    pub network: NetworkStats,
    pub frame: FrameStats,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessTelemetry {
    pub cpu: CpuStats,
    pub memory_bytes: u64,
    pub vram_bytes: u64,
    pub io_bytes_per_sec: f64,
}

#[derive(Debug, Clone)]
pub struct ProcessRecord {
    pub pid: ProcessId,
    pub app_id: Option<AppId>,
    pub ppid: Option<ProcessId>,
    pub binary: BinaryContext,
    pub path: Option<PathBuf>,
    pub working_directory: Option<PathBuf>,
    pub execution_owner: Option<String>,
    pub target_architecture: Option<String>,
    pub digital_signature: Option<String>,
    pub env_vars: HashMap<String, String>,
    pub permissions: Vec<String>,
    pub dependencies: Vec<String>,
    pub platform: PlatformContext,
    pub runtime: RuntimeContext,
    pub packaging: PackagingContext,
    pub distribution: DistributionContext,
    pub status: ProcessStatus,
    pub priority: ProcessPriority,
    pub telemetry: ProcessTelemetry,
    pub resolution_confidence: ResolutionConfidence,
    pub resolution_trace: ResolutionTrace,
}

impl ProcessRecord {
    pub fn new(pid: ProcessId) -> Self {
        Self {
            pid,
            app_id: None,
            ppid: None,
            binary: BinaryContext::default(),
            path: None,
            working_directory: None,
            execution_owner: None,
            target_architecture: None,
            digital_signature: None,
            env_vars: HashMap::new(),
            permissions: Vec::new(),
            dependencies: Vec::new(),
            platform: PlatformContext::default(),
            runtime: RuntimeContext::default(),
            packaging: PackagingContext::default(),
            distribution: DistributionContext::default(),
            status: ProcessStatus {
                running: true,
                responding: true,
                ..Default::default()
            },
            priority: ProcessPriority::default(),
            telemetry: ProcessTelemetry::default(),
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WindowRecord {
    pub id: WindowId,
    pub app_id: AppId,
    pub process_id: Option<ProcessId>,
    pub title: String,
    pub status: WindowStatus,
    pub priority: WindowPriority,
    pub order_index: u64,
    pub geometry: WindowGeometry,
    pub capabilities: WindowCapabilities,
    pub layer: WindowLayer,
    pub outputs: Vec<OutputId>,
    pub is_activated: bool,
    pub is_minimized: bool,
    pub is_foreground: bool,
    pub is_background: bool,
    pub is_locked: bool,
    pub raw_app_name: Option<String>,
    pub media: Option<MediaWindowLink>,
    pub resolution_confidence: ResolutionConfidence,
    pub resolution_trace: ResolutionTrace,
}

impl WindowRecord {
    pub fn new(id: WindowId, app_id: AppId) -> Self {
        Self {
            id,
            app_id,
            process_id: None,
            title: String::new(),
            status: WindowStatus::Unknown,
            priority: WindowPriority::default(),
            order_index: 0,
            geometry: WindowGeometry::default(),
            capabilities: WindowCapabilities::default(),
            layer: WindowLayer::Normal,
            outputs: Vec::new(),
            is_activated: false,
            is_minimized: false,
            is_foreground: false,
            is_background: true,
            is_locked: false,
            raw_app_name: None,
            media: None,
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ApplicationRecord {
    pub id: AppId,
    pub identity: ApplicationIdentity,
    pub binary: BinaryContext,
    pub platform: PlatformContext,
    pub runtime: RuntimeContext,
    pub packaging: PackagingContext,
    pub distribution: DistributionContext,
    pub installation: InstallationContext,
    pub launch: LaunchContext,
    pub status: ApplicationStatus,
    pub priority: ApplicationPriority,
    pub processes: HashSet<ProcessId>,
    pub windows: Vec<WindowId>,
    pub capabilities: ApplicationCapabilities,
    pub resolution_confidence: ResolutionConfidence,
    pub resolution_trace: ResolutionTrace,
}

impl ApplicationRecord {
    pub fn new(id: AppId) -> Self {
        Self {
            identity: ApplicationIdentity {
                identifier: id.as_str().to_string(),
                name: id.as_str().to_string(),
                ..Default::default()
            },
            binary: BinaryContext::default(),
            platform: PlatformContext::default(),
            runtime: RuntimeContext::default(),
            packaging: PackagingContext::default(),
            distribution: DistributionContext::default(),
            installation: InstallationContext::default(),
            launch: LaunchContext::default(),
            status: ApplicationStatus::default(),
            priority: ApplicationPriority::default(),
            processes: HashSet::new(),
            windows: Vec::new(),
            capabilities: ApplicationCapabilities::default(),
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
            id,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputRecord {
    pub id: OutputId,
    pub name: Option<String>,
    pub description: Option<String>,
    pub make_model: Option<String>,
    pub geometry: OutputGeometry,
    pub status: OutputStatus,
    pub refresh_rate_hz: Option<f32>,
    pub scale_factor: f64,
    pub windows: Vec<WindowId>,
}

impl OutputRecord {
    pub fn new(id: OutputId) -> Self {
        Self {
            id,
            name: None,
            description: None,
            make_model: None,
            geometry: OutputGeometry::default(),
            status: OutputStatus {
                enabled: true,
                ..Default::default()
            },
            refresh_rate_hz: None,
            scale_factor: 1.0,
            windows: Vec::new(),
        }
    }
}

#[cfg(test)]
mod resolution_tests {
    use super::*;

    #[test]
    fn resolution_trace_merges_evidence_without_duplicates() {
        let mut trace = ResolutionTrace::new(
            ResolutionConfidence::Probable,
            [ResolutionEvidence::TitleHeuristic],
        );
        trace.strengthen(
            ResolutionConfidence::Strong,
            [
                ResolutionEvidence::DesktopEntry,
                ResolutionEvidence::TitleHeuristic,
            ],
        );
        assert_eq!(trace.confidence, ResolutionConfidence::Strong);
        assert_eq!(trace.evidence.len(), 2);
        assert!(trace.has(ResolutionEvidence::DesktopEntry));
        assert!(trace.has(ResolutionEvidence::TitleHeuristic));
    }
}
