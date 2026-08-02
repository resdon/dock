// src/resolvers/desktop.rs

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

// DesktopAction struct to hold name and exec commands
#[derive(Clone, Debug, PartialEq)]
pub struct DesktopAction {
    pub name: String,
    pub exec: String,
}

/// Strips field codes (e.g. %u, %F, %i) from Exec lines
pub fn clean_exec_field(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|arg| !arg.starts_with('%'))
        .collect::<Vec<&str>>()
        .join(" ")
}

/// Parses [Desktop Entry] Actions= and [Desktop Action <ID>] sections
pub fn parse_desktop_actions(desktop_path: &Path) -> Vec<DesktopAction> {
    let file = match File::open(desktop_path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    let reader = BufReader::new(file);
    let mut current_section = String::new();
    let mut action_keys: Vec<String> = Vec::new();
    let mut action_map: HashMap<String, (String, String)> = HashMap::new();

    for line in reader.lines().flatten() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            current_section = trimmed[1..trimmed.len() - 1].to_string();
            continue;
        }

        if current_section == "Desktop Entry" {
            if trimmed.starts_with("Actions=") {
                let actions_str = &trimmed["Actions=".len()..];
                action_keys = actions_str
                    .split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        } else if current_section.starts_with("Desktop Action ") {
            let action_id = current_section["Desktop Action ".len()..].trim().to_string();
            if let Some((key, value)) = trimmed.split_once('=') {
                let key = key.trim();
                let value = value.trim();
                let entry = action_map.entry(action_id).or_insert_with(|| (String::new(), String::new()));
                if key == "Name" {
                    entry.0 = value.to_string();
                } else if key == "Exec" {
                    entry.1 = clean_exec_field(value);
                }
            }
        }
    }

    let mut actions = Vec::new();
    for action_id in action_keys {
        if let Some((name, exec)) = action_map.get(&action_id) {
            if !name.is_empty() && !exec.is_empty() {
                actions.push(DesktopAction {
                    name: name.clone(),
                    exec: exec.clone(),
                });
            }
        }
    }

    actions
}

/// Finds the .desktop file for app_id and returns all desktop actions
pub fn get_desktop_actions(app_id: &str) -> Vec<DesktopAction> {
    let mut search_id = app_id.to_string();
    if !search_id.starts_with("steam_icon_") {
        if let Some(idx) = search_id.rfind('_') {
            if search_id[idx+1..].chars().all(|c| c.is_numeric()) {
                search_id = search_id[..idx].to_string();
            }
        }
    }
    if search_id.to_lowercase().contains("transmission") {
        search_id = "transmission-gtk".to_string();
    }

    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut search_paths: Vec<PathBuf> = xdg_data_dirs.split(':').map(|s| Path::new(s).join("applications")).collect();
    if let Ok(home) = std::env::var("HOME") {
        search_paths.insert(0, Path::new(&home).join(".local/share/applications"));
    }

    let candidates = [
        format!("{}.desktop", search_id),
        format!("{}.desktop", search_id.to_lowercase()),
        format!("org.gnome.{}.desktop", search_id),
        format!("org.kde.{}.desktop", search_id),
        format!("com.{}.desktop", search_id),
    ];

    for path in &search_paths {
        for candidate in &candidates {
            let desktop_path = path.join(candidate);
            if desktop_path.exists() {
                let actions = parse_desktop_actions(&desktop_path);
                if !actions.is_empty() {
                    return actions;
                }
            }
        }
    }

    if let Some(resolved_id) = find_desktop_file_by_exec(&search_id) {
        for path in &search_paths {
            let desktop_path = path.join(format!("{}.desktop", resolved_id));
            if desktop_path.exists() {
                let actions = parse_desktop_actions(&desktop_path);
                if !actions.is_empty() {
                    return actions;
                }
            }
        }
    }

    Vec::new()
}

pub fn find_desktop_file_by_name(search_name: &str) -> Option<String> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }

    for dir in dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Ok(file) = File::open(&path) {
                        let reader = BufReader::new(file);
                        for line in reader.lines().flatten() {
                            if line.starts_with("Name=") {
                                let name = line["Name=".len()..].trim();
                                let title_lower = search_name.to_lowercase();
                                let name_lower = name.to_lowercase();
                                
                                // 1. Substring match
                                if title_lower.contains(&name_lower) || name_lower.contains(&title_lower) {
                                    return path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
                                }

                                // 2. First word prefix match (e.g. "Task Manager" vs "Taskman")
                                let title_first = title_lower.split_whitespace().next().unwrap_or("");
                                let name_first = name_lower.split_whitespace().next().unwrap_or("");
                                if !title_first.is_empty() && (title_first.starts_with(name_first) || name_first.starts_with(title_first)) {
                                    return path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn find_desktop_file_by_exec(app_id: &str) -> Option<String> {
    let mut dirs = vec![PathBuf::from("/usr/share/applications")];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(PathBuf::from(home).join(".local/share/applications"));
    }
    
    // Clean up app_id: many compositors append PIDs or random strings (e.g. app_1234)
    let app_id_clean = app_id.split('_').next().unwrap_or(app_id).to_lowercase();

    for dir in dirs {
        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Ok(file) = File::open(&path) {
                        let reader = BufReader::new(file);
                        for line in reader.lines().flatten() {
                            if line.starts_with("Exec=") {
                                let exec_line = line["Exec=".len()..].trim().to_lowercase();
                                if let Some(binary_path) = exec_line.split_whitespace().next() {
                                    let binary_name = Path::new(binary_path).file_name()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or(binary_path);
                                    
                                    if binary_name.contains(&app_id_clean) || app_id_clean.contains(binary_name) {
                                        return path.file_stem().and_then(|s| s.to_str()).map(|s| s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}