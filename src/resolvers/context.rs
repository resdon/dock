//! Resolution context.
//!
//! Provides shared context for all resolver operations.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use sysinfo::System;

use crate::models::system::{AppId, OutputId, ProcessId, WindowId};

#[derive(Clone, Debug, Default)]
pub struct WindowHints {
    pub app_id: Option<AppId>,
    pub title: String,
    pub activated: bool,
    pub minimized: bool,
    pub outputs: Vec<OutputId>,
    pub dirty: bool,
}

#[derive(Clone, Debug)]
pub struct ProcessSnapshot {
    pub pid: ProcessId,
    pub ppid: Option<ProcessId>,
    pub name: String,
    pub executable: Option<PathBuf>,
    pub cwd: Option<PathBuf>,
    pub command_line: Vec<String>,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
}

impl ProcessSnapshot {
    pub fn executable_name(&self) -> Option<String> {
        self.executable
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ResolverSnapshot {
    pub processes: Vec<ProcessSnapshot>,
}

impl ResolverSnapshot {
    pub fn from_system(system: &System) -> Self {
        let processes = system
            .processes()
            .iter()
            .map(|(pid, process)| ProcessSnapshot {
                pid: ProcessId(pid.as_u32()),
                ppid: process.parent().map(|p| ProcessId(p.as_u32())),
                name: process.name().to_string_lossy().into_owned(),
                executable: process.exe().map(Path::to_path_buf),
                cwd: process.cwd().map(Path::to_path_buf),
                command_line: process
                    .cmd()
                    .iter()
                    .map(|v| v.to_string_lossy().into_owned())
                    .collect(),
                cpu_usage: process.cpu_usage(),
                memory_bytes: process.memory(),
            })
            .collect();
        Self { processes }
    }
}

#[derive(Clone, Debug)]
pub struct ResolutionRequest {
    pub window_id: WindowId,
    pub hints: WindowHints,
    pub snapshot: ResolverSnapshot,
}

pub fn read_proc_environment(pid: ProcessId) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let Ok(data) = fs::read(format!("/proc/{}/environ", pid.0)) else {
        return result;
    };
    for item in data.split(|byte| *byte == 0) {
        let Ok(item) = std::str::from_utf8(item) else {
            continue;
        };
        if let Some((key, value)) = item.split_once('=') {
            result.insert(key.to_string(), value.to_string());
        }
    }
    result
}

/// Normalize a token for comparison: lowercase, ASCII alphanumeric only.
///
/// Single-pass implementation avoiding the intermediate String allocation
/// from `to_lowercase()`. Pre-allocates capacity to avoid reallocations.
pub fn normalize_token(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for c in value.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            output.push(c);
        }
    }
    output
}
