use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

fn main() {
    println!("monitor3 service utility - testing universal dynamic Steam icon resolution...");
    
    let test_binary = "cs2";
    match find_steam_game_icon_universally(test_binary) {
        Some(path) => println!("SUCCESS! Found icon path: {}", path.display()),
        None => println!("Could not find a matching Steam icon for '{}'.", test_binary),
    }
}

pub fn find_steam_game_icon_universally(query: &str) -> Option<PathBuf> {
    let home_str = std::env::var("HOME").ok()?;
    let home = PathBuf::from(home_str);
    let query_lower = query.to_lowercase();

    let mut found_appid = None;

    // 1. Map known short names/aliases or query .acf manifests
    if query_lower == "cs2" || query_lower == "csgo" {
        found_appid = Some("730".to_string());
    }

    let steam_roots = vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];

    if found_appid.is_none() {
        for root in &steam_roots {
            let steamapps = root.join("steamapps");
            if !steamapps.exists() { continue; }

            if let Ok(entries) = std::fs::read_dir(&steamapps) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("acf") {
                        if let Ok(file) = File::open(&path) {
                            let reader = BufReader::new(file);
                            let mut appid = String::new();
                            let mut installdir = String::new();
                            let mut name = String::new();

                            for line in reader.lines().flatten() {
                                let trimmed = line.trim();
                                if trimmed.starts_with("\"appid\"") {
                                    appid = trimmed.split('"').nth(3).unwrap_or("").to_string();
                                } else if trimmed.starts_with("\"installdir\"") {
                                    installdir = trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                                } else if trimmed.starts_with("\"name\"") {
                                    name = trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                                }
                            }

                            if !appid.is_empty() {
                                if installdir.contains(&query_lower) || query_lower.contains(&installdir) 
                                    || name.contains(&query_lower) || query_lower.contains(&name) 
                                {
                                    found_appid = Some(appid);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if found_appid.is_some() { break; }
        }
    }

    let appid = found_appid?;
    println!("[+] Universally matched query '{}' to Steam AppID: {}", query, appid);

    // 2. Search Steam library cache directories
    let cache_dirs = vec![
        home.join(".local/share/Steam/appcache/librarycache"),
        home.join(".steam/steam/appcache/librarycache"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam/appcache/librarycache"),
    ];

    for dir in cache_dirs {
        if !dir.exists() { continue; }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                    let lower = file_name.to_lowercase();
                    // Match any cache file starting with the appid (e.g., 730_icon.jpg, 730_library_600x900.jpg, etc.)
                    if lower.starts_with(&appid) && (lower.contains("icon") || lower.contains("logo") || lower.contains("library")) {
                        return Some(p);
                    }
                }
            }
        }
    }

    // 3. Fallback: Search system-wide or user XDG hicolor icon themes for steam_icon_<AppID>
    let theme_dirs = vec![
        home.join(".local/share/icons/hicolor"),
        PathBuf::from("/usr/share/icons/hicolor"),
    ];

    let target_icon_key = format!("steam_icon_{}", appid);
    for theme_dir in theme_dirs {
        if !theme_dir.exists() { continue; }
        for entry in walkdir::WalkDir::new(&theme_dir).max_depth(6).into_iter().filter_map(|e| e.ok()) {
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
