// src/resolvers/steam.rs

use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use sysinfo::{Pid, System};
use walkdir::WalkDir;

/// Resolves Steam AppID, Game Name, and Icon Path for a given process/window.
pub fn resolve_steam_game_details(
    target_app: &str,
    window_title: &str,
    sys_scanner: &System,
    entry_pid: Option<Pid>,
) -> Option<(String, String, PathBuf)> {
    let steam_roots = get_steam_roots();

    // 1. Try resolving AppID from process /proc environment or gamescope
    let mut steam_appid = resolve_appid_from_process(sys_scanner, entry_pid)
        .or_else(|| resolve_appid_from_gamescope(sys_scanner, target_app, window_title));

    // 2. Fallback: Try matching AppID from .acf manifests
    if steam_appid.is_none() {
        steam_appid = find_appid_by_manifest(&steam_roots, target_app, window_title);
    }

    let appid = steam_appid?;

    // 3. Resolve Game Name from appmanifest_<appid>.acf
    let game_name = get_game_name_from_manifest(&steam_roots, &appid)
        .unwrap_or_else(|| window_title.to_string());

    // 4. Resolve Icon Path
    let icon_path = find_steam_icon(&steam_roots, &appid)?;

    Some((appid, game_name, icon_path))
}

// --- Internal Helpers ---

fn get_steam_roots() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let home_path = PathBuf::from(home);
    vec![
        home_path.join(".local/share/Steam"),
        home_path.join(".steam/steam"),
        home_path.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ]
}

fn resolve_appid_from_process(sys_scanner: &System, entry_pid: Option<Pid>) -> Option<String> {
    let entry_pid = entry_pid?;

    for (c_pid, c_proc) in sys_scanner.processes() {
        let mut current = c_proc.parent();
        let mut is_descendant = c_pid == &entry_pid;

        while let Some(p_id) = current {
            if p_id == entry_pid {
                is_descendant = true;
                break;
            }
            if let Some(parent_proc) = sys_scanner.process(p_id) {
                current = parent_proc.parent();
            } else {
                break;
            }
        }

        if is_descendant || entry_pid.as_u32() == c_pid.as_u32() {
            // Check /proc/<pid>/environ
            if let Ok(env_data) = fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
                for env_pair in env_data.split('\0') {
                    if let Some(val) = env_pair.strip_prefix("SteamAppId=") {
                        return Some(val.to_string());
                    } else if let Some(val) = env_pair.strip_prefix("STEAM_COMPAT_APP_ID=") {
                        return Some(val.to_string());
                    }
                }
            }

            // Check /proc/<pid>/cmdline
            if let Ok(cmd_data) = fs::read_to_string(format!("/proc/{}/cmdline", c_pid.as_u32())) {
                let parts: Vec<&str> = cmd_data.split('\0').collect();
                for (i, part) in parts.iter().enumerate() {
                    if *part == "-applaunch" || *part == "--steam-app-id" {
                        if let Some(id) = parts.get(i + 1) {
                            if !id.is_empty() {
                                return Some(id.to_string());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn resolve_appid_from_gamescope(sys_scanner: &System, target_app: &str, window_title: &str) -> Option<String> {
    let is_gamescope = target_app.to_lowercase().contains("gamescope") 
        || window_title.to_lowercase().contains("gamescope");

    if !is_gamescope {
        return None;
    }

    for (c_pid, _) in sys_scanner.processes() {
        if let Ok(env_data) = fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
            for env_pair in env_data.split('\0') {
                if let Some(val) = env_pair.strip_prefix("SteamAppId=") {
                    if !val.is_empty() && val != "0" {
                        return Some(val.to_string());
                    }
                } else if let Some(val) = env_pair.strip_prefix("STEAM_COMPAT_APP_ID=") {
                    if !val.is_empty() && val != "0" {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    None
}

fn find_appid_by_manifest(steam_roots: &[PathBuf], target_app: &str, window_title: &str) -> Option<String> {
    let target_lower = target_app.to_lowercase();
    let title_lower = window_title.to_lowercase();

    for root in steam_roots {
        let steamapps = root.join("steamapps");
        if !steamapps.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(&steamapps) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("acf") {
                    if let Ok(file) = File::open(&path) {
                        let reader = BufReader::new(file);
                        let mut appid_val = String::new();
                        let mut installdir_val = String::new();
                        let mut name_val = String::new();

                        for line in reader.lines().flatten() {
                            let trimmed = line.trim();
                            if trimmed.starts_with("\"appid\"") {
                                appid_val = trimmed.split('"').nth(3).unwrap_or("").to_string();
                            } else if trimmed.starts_with("\"installdir\"") {
                                installdir_val = trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                            } else if trimmed.starts_with("\"name\"") {
                                name_val = trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                            }
                        }

                        if !appid_val.is_empty() {
                            if (!installdir_val.is_empty() && (target_lower.contains(&installdir_val) || installdir_val.contains(&target_lower)))
                                || (!name_val.is_empty() && (target_lower.contains(&name_val) || name_val.contains(&target_lower) || title_lower.contains(&name_val)))
                            {
                                return Some(appid_val);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn get_game_name_from_manifest(steam_roots: &[PathBuf], appid: &str) -> Option<String> {
    for root in steam_roots {
        let manifest_path = root.join(format!("steamapps/appmanifest_{}.acf", appid));
        if manifest_path.exists() {
            if let Ok(file) = File::open(&manifest_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("\"name\"") {
                        return Some(trimmed.split('"').nth(3).unwrap_or("").to_string());
                    }
                }
            }
        }
    }
    None
}

fn find_steam_icon(steam_roots: &[PathBuf], appid: &str) -> Option<PathBuf> {
    // Check library cache first
    for root in steam_roots {
        let cache_dir = root.join("appcache/librarycache");
        if cache_dir.exists() {
            if let Ok(entries) = fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                        let lower = file_name.to_lowercase();
                        if lower.starts_with(appid) && (lower.contains("icon") || lower.contains("logo") || lower.contains("library")) {
                            return Some(p);
                        }
                    }
                }
            }
        }
    }

    // Fallback: hicolor system theme
    let home = std::env::var("HOME").unwrap_or_default();
    let theme_dirs = vec![
        PathBuf::from(home).join(".local/share/icons/hicolor"),
        PathBuf::from("/usr/share/icons/hicolor"),
    ];
    let target_icon_key = format!("steam_icon_{}", appid);

    for theme_dir in theme_dirs {
        if !theme_dir.exists() {
            continue;
        }
        for entry in WalkDir::new(&theme_dir).max_depth(6).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.to_lowercase().contains(&target_icon_key) {
                        return Some(path.to_path_buf());
                    }
                }
            }
        }
    }

    None
}