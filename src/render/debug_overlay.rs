//! Debug overlay renderer.
//!
//! Renders FPS counter and other debug information.

/// Live debug stats that print RAM, VRAM, and CPU usage to the terminal.
///
/// Enabled by setting the `DOCK_DEBUG` environment variable (e.g., `DOCK_DEBUG=1`).
/// Stats are printed to stderr once per second (throttled) to avoid spamming the terminal.
use sysinfo::System;

/// Collects system stats from the sysinfo scanner.
pub struct DebugStats {
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub ram_percent: f32,
    pub cpu_percent: f32,
    pub cpu_cores: usize,
    pub vram_total_mb: u64,
    pub vram_used_mb: u64,
    pub vram_percent: f32,
    pub process_count: usize,
}

impl DebugStats {
    /// Sample current stats from the sysinfo system scanner.
    pub fn sample(sys: &mut System) -> Self {
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let ram_total = sys.total_memory();
        let ram_used = ram_total.saturating_sub(sys.available_memory());
        let ram_percent = if ram_total > 0 {
            (ram_used as f64 / ram_total as f64) * 100.0
        } else {
            0.0
        };

        let cpu_cores = std::cmp::max(1, sys.cpus().len());
        let cpu_percent = sys.global_cpu_usage();

        // VRAM: read from /sys/class/drm/card0/device/memory_info_vram_* (AMD/NVIDIA)
        // or /sys/class/drm/card0/device/mem_info_vram_* as fallback.
        let (vram_total, vram_used, vram_percent) = read_vram();

        Self {
            ram_used_mb: ram_used / 1024 / 1024,
            ram_total_mb: ram_total / 1024 / 1024,
            ram_percent: ram_percent as f32,
            cpu_percent,
            cpu_cores,
            vram_total_mb: vram_total,
            vram_used_mb: vram_used,
            vram_percent,
            process_count: sys.processes().len(),
        }
    }
}

/// Read VRAM usage from sysfs. Returns (total_mb, used_mb, percent).
fn read_vram() -> (u64, u64, f32) {
    // Try common paths for DRM GPU memory info
    let paths = [
        "/sys/class/drm/card0/device/memory_info_vram_total",
        "/sys/class/drm/card1/device/memory_info_vram_total",
    ];

    for total_path in &paths {
        if let Ok(total_bytes) = std::fs::read_to_string(total_path) {
            if let Ok(total) = total_bytes.trim().parse::<u64>() {
                let dir = std::path::Path::new(total_path)
                    .parent()
                    .unwrap_or(std::path::Path::new(""));
                let used_path = dir.join("memory_info_vram_used");
                let used_bytes = std::fs::read_to_string(&used_path).unwrap_or_default();
                let used = used_bytes.trim().parse::<u64>().unwrap_or(0);
                let pct = if total > 0 {
                    (used as f64 / total as f64) * 100.0
                } else {
                    0.0
                };
                return (total / 1024 / 1024, used / 1024 / 1024, pct as f32);
            }
        }
    }
    (0, 0, 0.0)
}

/// Check whether the debug overlay is enabled.
pub fn is_enabled() -> bool {
    std::env::var_os("DOCK_DEBUG").is_some()
}

/// Print debug stats to stderr, throttled to once per second.
///
/// Outputs GPU rendering status, RAM, VRAM, CPU, and process count to the terminal.
pub fn print_debug_stats(sys: &mut sysinfo::System, gpu_active: bool) {
    if !is_enabled() {
        return;
    }

    // Throttle: print once per second using a static timestamp
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, SystemTime};

    static LAST_PRINT: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();

    let last = LAST_PRINT.load(Ordering::Relaxed);
    if now <= last {
        return;
    }
    LAST_PRINT.store(now, Ordering::Relaxed);

    let stats = DebugStats::sample(sys);

    // Format and print to stderr
    let _gpu_line = if gpu_active {
        "GPU: ON (wgpu Vulkan)"
    } else {
        "GPU: OFF (CPU fallback)"
    };
    let _ram_warn = if stats.ram_percent > 80.0 { "⚠ " } else { "" };
    let _vram_line = if stats.vram_total_mb > 0 {
        let vram_warn = if stats.vram_percent > 80.0 {
            "⚠ "
        } else {
            ""
        };
        format!(
            "VRAM: {}{}/{} MB ({:.0}%)",
            vram_warn, stats.vram_used_mb, stats.vram_total_mb, stats.vram_percent
        )
    } else {
        "VRAM: N/A".to_string()
    };
}
