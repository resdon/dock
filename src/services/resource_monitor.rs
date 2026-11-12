//! CPU, RAM, and VRAM resource monitor.
//!
//! Background thread that tracks the dock process's resource consumption
//! and prints debug stats to stderr every 2 seconds.
//!
//! # Metrics
//! - **CPU**: Per-process CPU usage percentage (can exceed 100% on multi-core).
//! - **RAM**: Resident Set Size (RSS) in MB — actual physical memory used.
//! - **VRAM**: GPU dedicated memory usage in MB, queried via `nvidia-smi`
//!   for NVIDIA GPUs. Falls back to "N/A" if not available.

use std::process::Command;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Rolling window for instantaneous FPS calculation (2 seconds).
const FPS_WINDOW_SECS: f64 = 2.0;

/// Global FPS tracker shared between the event loop and resource monitor.
///
/// The main loop records each frame's timestamp via `increment_frame_counter()`.
/// The resource monitor calculates instantaneous FPS over the last 2 seconds.
static FRAME_TIMESTAMPS: Mutex<Vec<Instant>> = Mutex::new(Vec::new());

/// Increment the global render-frame counter with the current timestamp.
/// Call this once after each call to `AppState::draw`.
pub fn increment_frame_counter() {
    let mut timestamps = FRAME_TIMESTAMPS.lock().unwrap_or_else(|e| e.into_inner());
    // Prune old entries outside the rolling window to keep memory bounded.
    let cutoff = Instant::now().checked_sub(Duration::from_secs_f64(FPS_WINDOW_SECS + 1.0)).unwrap_or(Instant::now());
    timestamps.retain(|t| *t > cutoff);
    timestamps.push(Instant::now());
}

/// Calculate instantaneous FPS from recent frame timestamps.
/// Returns the number of frames rendered in the last 2 seconds.
pub fn get_fps() -> f64 {
    let mut timestamps = FRAME_TIMESTAMPS.lock().unwrap_or_else(|e| e.into_inner());
    let now = Instant::now();
    let cutoff = now.checked_sub(Duration::from_secs_f64(FPS_WINDOW_SECS)).unwrap_or(now);
    // Count frames within the rolling window
    let count = timestamps.iter().filter(|t| **t > cutoff).count() as f64;
    // Prune old entries
    timestamps.retain(|t| *t > cutoff);
    count / FPS_WINDOW_SECS
}

/// Query VRAM usage from `nvidia-smi` for the current process.
///
/// Parses the output of `nvidia-smi pmon -c 1 -d MEM -q -p <pid>` to extract
/// the dedicated GPU memory usage in MB. Returns `None` if `nvidia-smi` is
/// not available, fails, or the process is not found in the output.
fn query_vram_mb(pid: u32) -> Option<u64> {
    let output = Command::new("nvidia-smi")
        .args(&["pmon", "-c", "1", "-d", "MEM", "-q", "-p", &pid.to_string()])
        .output()
        .ok()?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    // nvidia-smi pmon output has a table with "used-mem" column (in MB).
    // Look for lines containing the PID and extract the used memory value.
    for line in stdout.lines() {
        let trimmed = line.trim();
        // Skip header lines
        if trimmed.is_empty()
            || trimmed.starts_with('-')
            || trimmed.starts_with('[')
            || trimmed.starts_with("Computing")
            || trimmed.starts_with("GPU")
            || trimmed.starts_with("Timestamp")
            || trimmed.starts_with("PID")
        {
            continue;
        }

        // Parse the used memory value — typically appears as a number followed by "MiB" or "MB"
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        for part in &parts {
            if let Ok(val) = part
                .trim_end_matches('M')
                .trim_end_matches("iB")
                .parse::<u64>()
            {
                if val > 0 {
                    return Some(val);
                }
            }
        }
    }

    // Removed fallback to `nvidia-smi --query-gpu=memory.used` which reports
    // system-wide GPU memory usage, not per-process usage. Returning that value
    // as dock's VRAM is misleading. Display "N/A" when per-process VRAM cannot
    // be determined via `nvidia-smi pmon`.
    None
}

/// Spawn a background thread that periodically prints resource usage stats
/// for the dock process (CPU, RAM, VRAM).
///
/// The thread sleeps 1 second before the first reading to let CPU usage
/// accumulate, then prints stats every 2 seconds. VRAM is queried via
/// `nvidia-smi` and shows "N/A" if the command is unavailable.
pub fn start_resource_monitor() {
    thread::spawn(|| {
        // FIX (v1.5.5): Use minimal specifics instead of new_all() to avoid loading
        // all process metadata at startup. Only the dock process is tracked.
        let mut sys = System::new_with_specifics(
            sysinfo::RefreshKind::nothing()
                .with_processes(ProcessRefreshKind::nothing().with_cpu().with_memory()),
        )
        .expect("Failed to create sysinfo System");

        let pid = match sysinfo::get_current_pid() {
            Ok(p) => p,
            Err(_e) => return,
        };

        // Wait 1 second before first reading so CPU usage has time to accumulate
        thread::sleep(Duration::from_secs(1));

        loop {
            // Refresh process info for CPU and RAM
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );

            if let Some(process) = sys.process(pid) {
                let memory_mb = process.memory() / 1024 / 1024;
                let cpu_usage = process.cpu_usage();

                // Query VRAM from nvidia-smi
                let vram_str = query_vram_mb(pid.as_u32())
                    .map(|mb| format!("{} MB", mb))
                    .unwrap_or_else(|| "N/A".to_string());

                eprintln!(
                    "[RESOURCE] CPU: {:.1}% | RAM: {} MB | VRAM: {} | FPS: {:.1}",
                    cpu_usage,
                    memory_mb,
                    vram_str,
                    get_fps()
                );
            } else {
                break;
            }

            thread::sleep(Duration::from_secs(2));
        }
    });
}
