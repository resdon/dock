//! CPU/RAM resource monitor.
//!
//! Background thread that prints CPU and RAM usage every 2 seconds.

use std::thread;
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

/// Spawn a background thread that periodically prints resource usage stats
/// for the dock process (CPU, RAM). VRAM tracking removed due to NVML conflicts.
pub fn start_resource_monitor() {
    thread::spawn(|| {
        let mut sys = System::new_all();

        let pid = match sysinfo::get_current_pid() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("[RESOURCE] Failed to get current PID: {e}");
                return;
            }
        };

        // Wait 1 second before first reading so CPU usage has time to accumulate
        thread::sleep(Duration::from_secs(1));

        loop {
            sys.refresh_processes_specifics(
                ProcessesToUpdate::Some(&[pid]),
                true,
                ProcessRefreshKind::nothing().with_cpu().with_memory(),
            );

            if let Some(process) = sys.process(pid) {
                let memory_mb = process.memory() / 1024 / 1024;
                let cpu_usage = process.cpu_usage();
                eprintln!(
                    "[RESOURCE] CPU: {:.1}% | RAM: {} MB",
                    cpu_usage, memory_mb
                );
            } else {
                break;
            }

            thread::sleep(Duration::from_secs(2));
        }
    });
}
