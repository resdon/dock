// src/resolvers/icon.rs

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub use super::desktop::find_desktop_file_by_exec;
use super::steam::resolve_steam_game_details;

/// Resolves default path for generated icon_list.txt
pub fn get_icon_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(format!("{}/.cache/dock", home));
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join("icon_list.txt")
}

pub fn find_icon_by_name(search_name: &str) -> Option<String> {
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
                        let mut current_section = String::new();
                        let mut current_name = String::new();
                        let mut current_icon = String::new();

                        for line in reader.lines().map_while(Result::ok) {
                            let trimmed = line.trim();
                            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                                current_section = trimmed[1..trimmed.len() - 1].to_string();
                                continue;
                            }

                            if current_section == "Desktop Entry" {
                                if let Some(rest) = trimmed.strip_prefix("Name=") {
                                    current_name = rest.trim().to_string();
                                } else if let Some(rest) = trimmed.strip_prefix("Icon=") {
                                    current_icon = rest.trim().to_string();
                                }

                                if current_name.eq_ignore_ascii_case(search_name)
                                    && !current_icon.is_empty()
                                {
                                    return Some(current_icon);
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

pub fn get_icon_from_desktop(desktop_id: &str) -> Option<String> {
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut search_paths: Vec<PathBuf> = xdg_data_dirs
        .split(':')
        .map(|s| Path::new(s).join("applications"))
        .collect();
    if let Ok(home) = std::env::var("HOME") {
        search_paths.insert(0, Path::new(&home).join(".local/share/applications"));
    }

    for path in search_paths {
        let desktop_path = path.join(format!("{}.desktop", desktop_id));
        if desktop_path.exists() {
            if let Ok(file) = File::open(desktop_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().map_while(Result::ok) {
                    if let Some(rest) = line.strip_prefix("Icon=") {
                        return Some(rest.trim().to_string());
                    }
                }
            }
        }
    }
    None
}

pub fn extract_icon_name(app_id: &str) -> String {
    if let Some(icon) = get_icon_from_desktop(app_id) {
        return icon;
    }

    let candidates = [
        format!("{}.desktop", app_id.to_lowercase()),
        format!("org.gnome.{}.desktop", app_id),
        format!("org.kde.{}.desktop", app_id),
        format!("com.{}.desktop", app_id),
    ];

    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut search_paths: Vec<PathBuf> = xdg_data_dirs
        .split(':')
        .map(|s| Path::new(s).join("applications"))
        .collect();
    if let Ok(home) = std::env::var("HOME") {
        search_paths.insert(0, Path::new(&home).join(".local/share/applications"));
    }

    for path in search_paths {
        for candidate in &candidates {
            let desktop_path = path.join(candidate);
            if desktop_path.exists() {
                if let Ok(file) = File::open(desktop_path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().map_while(Result::ok) {
                        if let Some(rest) = line.strip_prefix("Icon=") {
                            return rest.trim().to_string();
                        }
                    }
                }
            }
        }
    }

    if let Some(resolved_id) = find_desktop_file_by_exec(app_id) {
        if let Some(icon) = get_icon_from_desktop(&resolved_id) {
            return icon;
        }
    }

    app_id.to_string()
}

pub fn find_icon_path(root_dir: &str, target_name: &str) -> Option<PathBuf> {
    for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
            if stem.eq_ignore_ascii_case(target_name) {
                return Some(path.to_path_buf());
            }
        }
    }
    None
}

pub fn search_icon_list_file(query: &str) -> Option<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();

    let candidate_paths = [
        get_icon_list_path(),
        PathBuf::from("./icon_list.txt"),
        PathBuf::from(format!("{}/.local/share/dock/icon_list.txt", home)),
        PathBuf::from("/usr/share/dock/icon_list.txt"),
    ];

    let list_path = candidate_paths.into_iter().find(|p| p.exists())?;
    let file = File::open(list_path).ok()?;
    let reader = BufReader::new(file);

    let query_lower = query.to_lowercase();
    let mut best_fallback: Option<PathBuf> = None;

    for line in reader.lines().map_while(Result::ok) {
        let line_lower = line.trim().to_lowercase();

        if line_lower.contains(&query_lower) {
            let path = PathBuf::from(&line);

            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.to_lowercase() == query_lower {
                    if line_lower.ends_with(".png") {
                        return Some(path);
                    }
                    best_fallback = Some(path.clone());
                }
            }

            if best_fallback.is_none() {
                best_fallback = Some(path);
            }
        }
    }

    best_fallback
}

pub fn get_icon_path(app_id: &str) -> Option<PathBuf> {
    if app_id.starts_with("steam_icon_") {
        let sys_scanner = sysinfo::System::new();
        if let Some((_, _, icon_path)) = resolve_steam_game_details(app_id, "", &sys_scanner, None)
        {
            return Some(icon_path);
        }
    }

    let name = extract_icon_name(app_id);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());

    let search_paths = [
        format!("{}/.local/share/icons/hicolor/scalable/apps", home),
        format!("{}/.local/share/icons/hicolor/256x256/apps", home),
        format!("{}/.local/share/icons/hicolor/128x128/apps", home),
        format!("{}/.local/share/icons/hicolor/48x48/apps", home),
        "/usr/share/icons/hicolor/scalable/apps".to_string(),
        "/usr/share/icons/hicolor/256x256/apps".to_string(),
        "/usr/share/icons/hicolor/128x128/apps".to_string(),
        "/usr/share/pixmaps".to_string(),
    ];

    for path in &search_paths {
        if let Some(found) = find_icon_path(path, &name) {
            return Some(found);
        }
    }

    search_icon_list_file(&name)
}
