//! Process monitor service.
//!
//! Samples top CPU and memory processes for the process panel popup.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, RefreshKind, System};

use crate::models::{ProcessEntry, ProcessSnapshot};

/// Samples process CPU/RAM only while the process tray panel is visible.
/// CPU percentages come from sysinfo's delta between consecutive refreshes.
pub fn spawn_process_monitor(snapshot_tx: Sender<ProcessSnapshot>, enabled_rx: Receiver<bool>) {
    thread::Builder::new()
        .name("dock-process-monitor".into())
        .spawn(move || {
            // Only fetch fields used by this panel. `everything()` also refreshes
            // executable paths, command lines, environment and other metadata;
            // that adds avoidable /proc work when many processes are running.
            let process_refresh = ProcessRefreshKind::nothing().with_cpu().with_memory();
            let mut system =
                System::new_with_specifics(RefreshKind::nothing().with_processes(process_refresh))
                    .expect("Failed to create sysinfo System");
            loop {
                match enabled_rx.recv() {
                    Ok(true) => {}
                    Ok(false) => continue,
                    Err(_) => return,
                }

                let mut enabled = true;
                while enabled {
                    system.refresh_processes_specifics(
                        sysinfo::ProcessesToUpdate::All,
                        true,
                        process_refresh,
                    );
                    let mut processes: Vec<ProcessEntry> = system
                        .processes()
                        .iter()
                        .map(|(pid, process)| {
                            let name = process.name().to_string_lossy().trim().to_string();
                            let name = if name.is_empty() {
                                "(unnamed)".to_string()
                            } else {
                                name
                            };
                            ProcessEntry {
                                pid: pid.as_u32(),
                                name,
                                cpu_percent: process.cpu_usage().max(0.0),
                                memory_bytes: process.memory(),
                            }
                        })
                        .collect();
                    processes.sort_by(|a, b| {
                        b.cpu_percent
                            .total_cmp(&a.cpu_percent)
                            .then_with(|| b.memory_bytes.cmp(&a.memory_bytes))
                            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    });
                    let snapshot = ProcessSnapshot {
                        total_processes: processes.len(),
                        processes,
                    };
                    if snapshot_tx.send(snapshot).is_err() {
                        return;
                    }

                    match enabled_rx.recv_timeout(Duration::from_millis(1200)) {
                        Ok(value) => enabled = value,
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        })
        .expect("failed to start process monitor");
}
