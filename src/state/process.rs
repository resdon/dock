//! Process health state.
//!
//! Tracks process health metrics and telemetry.

use std::path::PathBuf;
use std::time::Instant;

use crate::models::system::{
    AppId, BinaryContext, DistributionContext, PackagingContext, PlatformContext, ProcessId,
    ResolutionConfidence, ResolutionTrace, RuntimeContext,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessHealthState {
    Running,
    Sleeping,
    Stopped,
    Zombie,
    Unresponsive,
    Unknown,
}

impl Default for ProcessHealthState {
    fn default() -> Self {
        Self::Unknown
    }
}

#[derive(Clone, Debug, Default)]
pub struct ProcessTelemetry {
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub gpu_percent: Option<f32>,
    pub gpu_memory_bytes: Option<u64>,
    pub threads: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct ProcessState {
    pub pid: ProcessId,
    pub app_id: Option<AppId>,
    pub parent_pid: Option<ProcessId>,
    pub binary: BinaryContext,
    pub runtime: RuntimeContext,
    pub packaging: PackagingContext,
    pub distribution: DistributionContext,
    pub platform: PlatformContext,
    pub health: ProcessHealthState,
    pub responding: bool,
    pub telemetry: ProcessTelemetry,
    pub environment: Option<String>,
    pub permissions: Vec<String>,
    pub dependencies: Vec<String>,
    pub started_at: Option<Instant>,
    pub executable_path: Option<PathBuf>,
    pub resolution_confidence: ResolutionConfidence,
    pub resolution_trace: ResolutionTrace,
}

impl ProcessState {
    pub fn new(pid: ProcessId) -> Self {
        Self {
            pid,
            app_id: None,
            parent_pid: None,
            binary: BinaryContext::default(),
            runtime: RuntimeContext::default(),
            packaging: PackagingContext::default(),
            distribution: DistributionContext::default(),
            platform: PlatformContext::default(),
            health: ProcessHealthState::Unknown,
            responding: true,
            telemetry: ProcessTelemetry::default(),
            environment: None,
            permissions: Vec::new(),
            dependencies: Vec::new(),
            started_at: None,
            executable_path: None,
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
        }
    }
}
