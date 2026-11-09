//! Performance telemetry.
//!
//! Collects CPU, GPU, memory, network, and frame timing metrics.

use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct CpuTelemetry {
    pub usage_percent: f32,
    pub per_core_percent: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct MemoryTelemetry {
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct GpuTelemetry {
    pub usage_percent: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct NetworkTelemetry {
    pub connected: bool,
    pub download_bytes_per_sec: u64,
    pub upload_bytes_per_sec: u64,
    pub download_total: u64,
    pub upload_total: u64,
    pub download_active: bool,
    pub upload_active: bool,
    pub download_trigger: bool,
    pub upload_trigger: bool,
}

#[derive(Clone, Debug)]
pub struct FrameTelemetry {
    pub fps: f32,
    pub frame_time: Duration,
    pub rendered_frames: u64,
    pub dropped_frames: u64,
    pub last_frame: Option<Instant>,
}

impl Default for FrameTelemetry {
    fn default() -> Self {
        Self {
            fps: 0.0,
            frame_time: Duration::ZERO,
            rendered_frames: 0,
            dropped_frames: 0,
            last_frame: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TelemetryState {
    pub cpu: CpuTelemetry,
    pub memory: MemoryTelemetry,
    pub gpu: GpuTelemetry,
    pub network: NetworkTelemetry,
    pub frame: FrameTelemetry,
    pub last_system_sample: Option<Instant>,
    pub last_network_sample: Option<Instant>,
    pub last_gpu_sample: Option<Instant>,
}
