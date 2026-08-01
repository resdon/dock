// src/lib.rs
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader};
use walkdir::WalkDir;
use base64::{engine::general_purpose::STANDARD, Engine as _};
// Add this to the top of src/models.rs

use wayland_client::protocol::wl_output::WlOutput;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

pub use self::models::WindowDiagnostics;
pub use self::models::LastState;
pub use self::icon_utils::extract_icon_name;
pub use self::terminal_graphics::load_image_raw_rgba;



pub mod models {
	use super::ZwlrForeignToplevelHandleV1;
    use super::*;

	#[derive(PartialEq, Clone, Copy, Debug)]
	pub enum LastState {
	    None,
	    ReceivedFocus,
	    ReportedInactive,
	}

	#[derive(Clone, Debug)] // Removed Default
	pub struct WindowDiagnostics {
		pub id: u64,
	    pub app_name: String,
	    pub title: String,
	    pub matched_pid: Option<u32>,
	    pub icon_name: String,
	    pub terminal_icon_code: String,
	    pub app_id: String,       
	    pub is_activated: bool,
	    pub is_minimized: bool,
	    pub icon_rgba: Option<Vec<u8>>,
	    pub icon_size: u32,
	    pub handle: ZwlrForeignToplevelHandleV1,
	    pub last_state: LastState,
	    pub is_pending: bool,
	    pub icon_resolved: bool,
        pub outputs: Vec<WlOutput>, // Track active outputs for multi-monitor scoping
	}

	impl WindowDiagnostics {
	    pub fn new(id: u64, handle: ZwlrForeignToplevelHandleV1) -> Self {
	        Self {
	        	id,
	            app_name: "Unknown".to_string(),
	            title: "Unknown".to_string(),
	            matched_pid: None,
	            icon_name: "".to_string(),
	            terminal_icon_code: "".to_string(),
	            app_id: "".to_string(),
	            is_activated: false,
	            is_minimized: false,
	            icon_rgba: None,
	            icon_size: 48,
	            handle, // Initialize with the passed handle
	            last_state: LastState::None,
	            is_pending: false,
	            icon_resolved: false,
                outputs: Vec::new(),
	        }
	    }
	}
}


pub mod icon_utils {
    use super::*;

use std::fs;
use std::path::PathBuf;

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

	pub fn find_icon_by_name(search_name: &str) -> Option<String> {
        let mut dirs = vec![PathBuf::from("/usr/share/applications")];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(PathBuf::from(home).join(".local/share/applications"));
        }

	    for dir in dirs {
	        if let Ok(entries) = fs::read_dir(dir) {
	            for entry in entries.flatten() {
	                let path = entry.path();
	                // Only process .desktop files
	                if path.extension().and_then(|s| s.to_str()) == Some("desktop") {
	                    if let Ok(file) = File::open(&path) {
	                        let reader = BufReader::new(file);
	                        let mut current_name = String::new();
	                        let mut current_icon = String::new();

	                        for line in reader.lines().flatten() {
	                            if line.starts_with("Name=") {
	                                current_name = line["Name=".len()..].trim().to_string();
	                            }
	                            if line.starts_with("Icon=") {
	                                current_icon = line["Icon=".len()..].trim().to_string();
	                            }
	                            // If we found the right app, return the icon immediately
	                            if current_name.eq_ignore_ascii_case(search_name) && !current_icon.is_empty() {
	                                return Some(current_icon);
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
	    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
	    let mut search_paths: Vec<PathBuf> = xdg_data_dirs.split(':').map(|s| Path::new(s).join("applications")).collect();
	    if let Ok(home) = std::env::var("HOME") { search_paths.insert(0, Path::new(&home).join(".local/share/applications")); }

	    for path in search_paths {
	        let desktop_path = path.join(format!("{}.desktop", desktop_id));
	        if desktop_path.exists() {
	            if let Ok(file) = File::open(desktop_path) {
	                let reader = BufReader::new(file);
	                for line in reader.lines().flatten() {
	                    if line.starts_with("Icon=") {
	                        return Some(line["Icon=".len()..].trim().to_string());
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

        // Try common variations
        let candidates = [
            format!("{}.desktop", app_id.to_lowercase()),
            format!("org.gnome.{}.desktop", app_id),
            format!("org.kde.{}.desktop", app_id),
            format!("com.{}.desktop", app_id),
        ];

        let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
        let mut search_paths: Vec<PathBuf> = xdg_data_dirs.split(':').map(|s| Path::new(s).join("applications")).collect();
        if let Ok(home) = std::env::var("HOME") { search_paths.insert(0, Path::new(&home).join(".local/share/applications")); }

        for path in search_paths {
            for candidate in &candidates {
                let desktop_path = path.join(candidate);
                if desktop_path.exists() {
                    if let Ok(file) = File::open(desktop_path) {
                        let reader = BufReader::new(file);
                        for line in reader.lines().flatten() {
                            if line.starts_with("Icon=") {
                                return line["Icon=".len()..].trim().to_string();
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback: search by Exec
        if let Some(resolved_id) = find_desktop_file_by_exec(app_id) {
            if let Some(icon) = get_icon_from_desktop(&resolved_id) {
                return icon;
            }
        }

        app_id.to_string()
    }

	// This function performs a recursive depth-first search of a directory tree to locate a specific file by its exact name
	pub fn find_icon_path(root_dir: &str, target_name: &str) -> Option<PathBuf> {
	    for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
	        let path = entry.path();
	        
	        // Check if the current file stem matches the target (case-insensitive)
	        if let Some(stem) = path.file_stem().and_then(|n| n.to_str()) {
	            if stem.eq_ignore_ascii_case(target_name) {
	                return Some(path.to_path_buf());
	            }
	        }
	    }
	    None
	}

	/// Parses icon_list.txt generated by your bash script
	pub fn search_in_icon_list(icon_name: &str) -> Option<PathBuf> {
	    // Debug: Check where the program is looking
	    let _current_dir = std::env::current_dir().ok()?;
	    //eprintln!("DEBUG: Current working directory is: {:?}", current_dir);
	    
	    let file_path = "icon_list.txt";
	    let file = match File::open(file_path) {
	        Ok(f) => {
	            //eprintln!("DEBUG: Successfully opened {}", file_path);
	            f
	        }
	        Err(_e) => {
	            //eprintln!("DEBUG: Failed to open {}: {}", file_path, e);
	            return None;
	        }
	    };

	    let reader = BufReader::new(file);

	    for (_i, line_result) in reader.lines().enumerate() {
	        let line = match line_result {
	            Ok(l) => l,
	            Err(_) => continue,
	        };

	        // Debug: See what each raw line looks like
	        //eprintln!("DEBUG: Reading line {}: '{}'", i, line);

	        let parts: Vec<&str> = line.split('|').collect();
	        
	        // Debug: See if the split worked as expected
	        //eprintln!("DEBUG: Line split into parts: {:?}", parts);

	        if let Some(path_str) = parts.get(1) {
	            let path = Path::new(path_str.trim());
	            
	            if let Some(file_name) = path.file_stem().and_then(|n| n.to_str()) {
	                // Debug: Check what name we are comparing against
	                //eprintln!("DEBUG: Comparing '{}' with target '{}'", file_name, icon_name);
	                
	                if file_name.eq_ignore_ascii_case(icon_name) {
	                    eprintln!("DEBUG: Match found for '{}'! Returning path: {:?}", icon_name, path);
	                    return Some(path.to_path_buf());
	                }
	            }
	        }
	    }
	    
	    eprintln!("DEBUG: Finished reading file, no match found.");
	    None
	}



}

pub fn resolve_steam_game_details(
    target_app: &str,
    window_title: &str,
    sys_scanner: &sysinfo::System,
    entry_pid: Option<sysinfo::Pid>,
) -> Option<(String, String, PathBuf)> {
    let mut steam_appid = None;

    if let Some(entry_pid) = entry_pid {
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
                if steam_appid.is_none() {
                    if let Ok(env_data) = std::fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
                        for env_pair in env_data.split('\0') {
                            if let Some(val) = env_pair.strip_prefix("SteamAppId=") {
                                steam_appid = Some(val.to_string());
                            } else if let Some(val) = env_pair.strip_prefix("STEAM_COMPAT_APP_ID=") {
                                steam_appid = Some(val.to_string());
                            }
                        }
                    }
                    if steam_appid.is_none() {
                        if let Ok(cmd_data) = std::fs::read_to_string(format!("/proc/{}/cmdline", c_pid.as_u32())) {
                            let parts: Vec<&str> = cmd_data.split('\0').collect();
                            for (i, part) in parts.iter().enumerate() {
                                if *part == "-applaunch" || *part == "--steam-app-id" {
                                    if let Some(id) = parts.get(i + 1) {
                                        if !id.is_empty() {
                                            steam_appid = Some(id.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let is_gamescope = target_app.to_lowercase().contains("gamescope") || window_title.to_lowercase().contains("gamescope");

    if steam_appid.is_none() && is_gamescope {
        for (c_pid, _) in sys_scanner.processes() {
            if let Ok(env_data) = std::fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
                for env_pair in env_data.split('\0') {
                    if let Some(val) = env_pair.strip_prefix("SteamAppId=") {
                        if !val.is_empty() && val != "0" {
                            steam_appid = Some(val.to_string());
                            break;
                        }
                    } else if let Some(val) = env_pair.strip_prefix("STEAM_COMPAT_APP_ID=") {
                        if !val.is_empty() && val != "0" {
                            steam_appid = Some(val.to_string());
                            break;
                        }
                    }
                }
            }
            if steam_appid.is_some() {
                break;
            }
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let home_path = PathBuf::from(home);
    let steam_roots = vec![
        home_path.join(".local/share/Steam"),
        home_path.join(".steam/steam"),
        home_path.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];

    let target_lower = target_app.to_lowercase();
    let title_lower = window_title.to_lowercase();

    if steam_appid.is_none() {
        for root in &steam_roots {
            let steamapps = root.join("steamapps");
            if !steamapps.exists() {
                continue;
            }

            if let Ok(entries) = std::fs::read_dir(&steamapps) {
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
                                    steam_appid = Some(appid_val);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            if steam_appid.is_some() {
                break;
            }
        }
    }

    let appid = steam_appid?;

    let mut game_name = None;
    for root in &steam_roots {
        let manifest_path = root.join(format!("steamapps/appmanifest_{}.acf", appid));
        if manifest_path.exists() {
            if let Ok(file) = File::open(&manifest_path) {
                let reader = BufReader::new(file);
                for line in reader.lines().flatten() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("\"name\"") {
                        game_name = Some(trimmed.split('"').nth(3).unwrap_or("").to_string());
                        break;
                    }
                }
            }
        }
        if game_name.is_some() {
            break;
        }
    }

    let final_name = game_name.unwrap_or_else(|| window_title.to_string());

    let mut icon_path = None;
    for root in &steam_roots {
        let cache_dir = root.join("appcache/librarycache");
        if cache_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                        let lower = file_name.to_lowercase();
                        if lower.starts_with(&appid) && (lower.contains("icon") || lower.contains("logo") || lower.contains("library")) {
                            icon_path = Some(p);
                            break;
                        }
                    }
                }
            }
        }
        if icon_path.is_some() {
            break;
        }
    }

    if icon_path.is_none() {
        let theme_dirs = vec![
            home_path.join(".local/share/icons/hicolor"),
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
                            icon_path = Some(path.to_path_buf());
                            break;
                        }
                    }
                }
            }
            if icon_path.is_some() {
                break;
            }
        }
    }

    Some((appid, final_name, icon_path?))
}

pub fn get_icon_path(app_id: &str) -> Option<PathBuf> {
    if app_id.starts_with("steam_icon_") {
        let sys_scanner = sysinfo::System::new();
        if let Some((_, _, icon_path)) = resolve_steam_game_details(app_id, "", &sys_scanner, None) {
            return Some(icon_path);
        }
    }
    let name = icon_utils::extract_icon_name(app_id);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    
    let search_paths = [
        // User local hicolor only (exact theme, not a flat scan of all themes)
        format!("{}/.local/share/icons/hicolor/scalable/apps", home),
        format!("{}/.local/share/icons/hicolor/256x256/apps", home),
        format!("{}/.local/share/icons/hicolor/128x128/apps", home),
        format!("{}/.local/share/icons/hicolor/48x48/apps", home),
        // System hicolor
        "/usr/share/icons/hicolor/scalable/apps".to_string(),
        "/usr/share/icons/hicolor/256x256/apps".to_string(),
        "/usr/share/icons/hicolor/128x128/apps".to_string(),
        "/usr/share/icons/hicolor/64x64/apps".to_string(),
        "/usr/share/icons/hicolor/48x48/apps".to_string(),
        // Pixmaps
        "/usr/share/pixmaps".to_string(),
    ];

    for path in &search_paths {
        if let Some(found) = icon_utils::find_icon_path(path, &name) {
            return Some(found);
        }
    }
    
    // Final fallback: exhaustive search in icon_list.txt
    icon_utils::search_in_icon_list(&name)
}

pub mod terminal_graphics {
    use super::*;

    pub fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
        let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

        if extension == "svg" {
            let svg_data = std::fs::read(path).ok()?;
            let tree = resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default()).ok()?;
            let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;
            
            let transform = resvg::tiny_skia::Transform::from_scale(
                target_size as f32 / tree.size().width(),
                target_size as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());
            
            Some((target_size, target_size, pixmap.data().to_vec()))
        } else {
            let img = image::open(path).ok()?;
            let scaled = img.resize_exact(target_size, target_size, image::imageops::FilterType::Lanczos3);
            let rgba = scaled.to_rgba8();
            Some((rgba.width(), rgba.height(), rgba.into_raw()))
        }
    }

    pub fn generate_terminal_image_string(app_id: &str, target_size: u32) -> String {
        let icon_path = match get_icon_path(app_id) {
            Some(path) => path,
            None => return "📁 [No Icon Found]".to_string(),
        };

        if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, target_size) {
            let b64_data = STANDARD.encode(&raw_bytes);
            format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data)
        } else {
            "📁 [Rasterize Error]".to_string()
        }
    }
}
