use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Base64 engine for terminal protocol payloads
use base64::{engine::general_purpose::STANDARD, Engine as _};

use linicon::lookup_icon;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryHandler, RegistryState};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use wayland_client::globals::registry_queue_init;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event as HandleEvent, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    Event as ManagerEvent, ZwlrForeignToplevelManagerV1,
};

struct RealtimeTrackerApp {
    registry_state: RegistryState,
    window_cache: HashMap<u32, WindowDiagnostics>,
    sys_scanner: System,
}

#[derive(Default, Clone, Debug)]
struct WindowDiagnostics {
    app_name: String,
    title: String,
    matched_pid: Option<u32>,
    icon_name: String,
    terminal_icon_code: String,
}

// Purely dynamic .desktop file inspection: finds name and icon by matching app_id or StartupWMClass
fn query_desktop_file_metadata(app_id: &str) -> (Option<String>, Option<String>) {
    if app_id.is_empty() {
        return (None, None);
    }

    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    let mut search_paths: Vec<PathBuf> = xdg_data_dirs
        .split(':')
        .map(|s| Path::new(s).join("applications"))
        .collect();
    if let Ok(home) = std::env::var("HOME") {
        search_paths.insert(0, Path::new(&home).join(".local/share/applications"));
    }

    let app_lower = app_id.to_lowercase();
    let target_files = vec![
        format!("{}.desktop", app_id),
        format!("{}.desktop", app_lower),
    ];

    let mut found_name = None;
    let mut found_icon = None;

    // 1. Direct filename match
    for path in &search_paths {
        for filename in &target_files {
            let desktop_path = path.join(filename);
            if desktop_path.exists() {
                if let Ok(file) = File::open(&desktop_path) {
                    let reader = BufReader::new(file);
                    for line in reader.lines().map_while(Result::ok) {
                        let trimmed = line.trim();
                        if trimmed.starts_with("Name=") && found_name.is_none() {
                            found_name = Some(trimmed["Name=".len()..].trim().to_string());
                        } else if trimmed.starts_with("Icon=") && found_icon.is_none() {
                            found_icon = Some(trimmed["Icon=".len()..].trim().to_string());
                        }
                    }
                }
            }
        }
        if found_name.is_some() && found_icon.is_some() {
            break;
        }
    }

    // 2. Scan all desktop files for StartupWMClass or matching executable/content if not found
    if found_name.is_none() || found_icon.is_none() {
        for path in search_paths {
            if !path.exists() {
                continue;
            }
            for entry in WalkDir::new(path)
                .max_depth(2)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let p = entry.path();
                if p.extension().and_then(|s| s.to_str()) == Some("desktop") {
                    if let Ok(file) = File::open(p) {
                        let reader = BufReader::new(file);
                        let mut current_name = None;
                        let mut current_icon = None;
                        let mut matches = false;

                        for line in reader.lines().map_while(Result::ok) {
                            let trimmed = line.trim();
                            if trimmed.starts_with("Name=") && current_name.is_none() {
                                current_name = Some(trimmed["Name=".len()..].trim().to_string());
                            } else if trimmed.starts_with("Icon=") && current_icon.is_none() {
                                current_icon = Some(trimmed["Icon=".len()..].trim().to_string());
                            } else if let Some(rest) = trimmed.strip_prefix("StartupWMClass=") {
                                let wm_class = rest.trim().to_lowercase();
                                if wm_class == app_lower {
                                    matches = true;
                                }
                            }
                        }

                        if let Some(file_name) = p.file_name().and_then(|n| n.to_str()) {
                            if file_name.to_lowercase().contains(&app_lower) {
                                matches = true;
                            }
                        }

                        if matches {
                            if found_name.is_none() {
                                found_name = current_name;
                            }
                            if found_icon.is_none() {
                                found_icon = current_icon;
                            }
                            if found_name.is_some() && found_icon.is_some() {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    (found_name, found_icon)
}

// Purely dynamic icon path location via linicon and filesystem traversal
fn locate_actual_icon_path(icon_name: &str, app_id: &str) -> Option<PathBuf> {
    if !icon_name.is_empty() {
        if let Some(icon) = lookup_icon(icon_name)
            .from_theme("hicolor")
            .next()
            .and_then(|res| res.ok())
        {
            return Some(icon.path);
        }
    }
    if !app_id.is_empty() {
        if let Some(icon) = lookup_icon(app_id)
            .from_theme("hicolor")
            .next()
            .and_then(|res| res.ok())
        {
            return Some(icon.path);
        }
    }

    let mut search_roots = vec![
        PathBuf::from("/usr/share/icons"),
        PathBuf::from("/usr/share/pixmaps"),
    ];
    if let Ok(home) = std::env::var("HOME") {
        let home_path = PathBuf::from(home);
        search_roots.push(home_path.join(".icons"));
        search_roots.push(home_path.join(".local/share/icons"));
        search_roots.push(home_path.join(".local/share/Steam"));
        search_roots.push(home_path.join(".steam/steam"));
    }

    let target_lower = icon_name.to_lowercase();
    let app_lower = app_id.to_lowercase();

    for root in search_roots {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .max_depth(6)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "png" || ext == "svg" || ext == "xpm" {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            let name_lower = file_name.to_lowercase();
                            if (!target_lower.is_empty() && name_lower.contains(&target_lower))
                                || (!app_lower.is_empty() && name_lower.contains(&app_lower))
                            {
                                return Some(path.to_path_buf());
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

// Purely dynamic Steam metadata discovery via environment/cmdline and .acf manifests
fn resolve_steam_game_details(
    target_app: &str,
    window_title: &str,
    sys_scanner: &System,
    entry_pid: sysinfo::Pid,
) -> Option<(String, String, PathBuf)> {
    let mut steam_appid = None;

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

        if is_descendant && steam_appid.is_none() {
            if let Ok(env_data) =
                std::fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32()))
            {
                for env_pair in env_data.split('\0') {
                    if let Some(val) = env_pair.strip_prefix("SteamAppId=") {
                        steam_appid = Some(val.to_string());
                    } else if let Some(val) = env_pair.strip_prefix("STEAM_COMPAT_APP_ID=") {
                        steam_appid = Some(val.to_string());
                    }
                }
            }
            if steam_appid.is_none() {
                if let Ok(cmd_data) =
                    std::fs::read_to_string(format!("/proc/{}/cmdline", c_pid.as_u32()))
                {
                    let parts: Vec<&str> = cmd_data.split('\0').collect();
                    for (i, part) in parts.iter().enumerate() {
                        if *part == "-applaunch" {
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

                            for line in reader.lines().map_while(Result::ok) {
                                let trimmed = line.trim();
                                if trimmed.starts_with("\"appid\"") {
                                    appid_val = trimmed.split('"').nth(3).unwrap_or("").to_string();
                                } else if trimmed.starts_with("\"installdir\"") {
                                    installdir_val =
                                        trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                                } else if trimmed.starts_with("\"name\"") {
                                    name_val =
                                        trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                                }
                            }

                            if !appid_val.is_empty()
                                && ((!installdir_val.is_empty()
                                    && (target_lower.contains(&installdir_val)
                                        || installdir_val.contains(&target_lower)))
                                    || (!name_val.is_empty()
                                        && (target_lower.contains(&name_val)
                                            || name_val.contains(&target_lower)
                                            || title_lower.contains(&name_val))))
                            {
                                steam_appid = Some(appid_val);
                                break;
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
                for line in reader.lines().map_while(Result::ok) {
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
                        if lower.starts_with(&appid)
                            && (lower.contains("icon")
                                || lower.contains("logo")
                                || lower.contains("library"))
                        {
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
            for entry in WalkDir::new(&theme_dir)
                .max_depth(6)
                .into_iter()
                .filter_map(|e| e.ok())
            {
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

fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
    let extension = path.extension().and_then(|s| s.to_str())?.to_lowercase();

    if extension == "svg" {
        let svg_data = std::fs::read(path).ok()?;
        let tree =
            resvg::usvg::Tree::from_data(&svg_data, &resvg::usvg::Options::default()).ok()?;
        let mut pixmap = resvg::tiny_skia::Pixmap::new(target_size, target_size)?;

        let transform = resvg::tiny_skia::Transform::from_scale(
            target_size as f32 / tree.size().width(),
            target_size as f32 / tree.size().height(),
        );
        resvg::render(&tree, transform, &mut pixmap.as_mut());

        Some((target_size, target_size, pixmap.data().to_vec()))
    } else {
        let img = image::open(path).ok()?;
        let scaled = img.resize_exact(
            target_size,
            target_size,
            image::imageops::FilterType::Lanczos3,
        );
        let rgba = scaled.to_rgba8();
        Some((rgba.width(), rgba.height(), rgba.into_raw()))
    }
}

impl ProvidesRegistryState for RealtimeTrackerApp {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    smithay_client_toolkit::registry_handlers![RealtimeTrackerApp];
}

impl RegistryHandler<RealtimeTrackerApp> for RealtimeTrackerApp {
    fn new_global(
        _data: &mut RealtimeTrackerApp,
        _conn: &Connection,
        _qh: &QueueHandle<RealtimeTrackerApp>,
        _name: u32,
        _interface: &str,
        _version: u32,
    ) {
    }

    fn remove_global(
        _data: &mut RealtimeTrackerApp,
        _conn: &Connection,
        _qh: &QueueHandle<RealtimeTrackerApp>,
        _name: u32,
        _interface: &str,
    ) {
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for RealtimeTrackerApp {
    fn event(
        _: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        _: ManagerEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
    event_created_child!(RealtimeTrackerApp, ZwlrForeignToplevelManagerV1, [0 => (ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for RealtimeTrackerApp {
    fn event(
        state: &mut Self,
        proxy: &ZwlrForeignToplevelHandleV1,
        event: HandleEvent,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = proxy.id().protocol_id();
        let entry = state.window_cache.entry(id).or_default();

        match event {
            HandleEvent::AppId { app_id } => {
                entry.app_name = if app_id.is_empty() {
                    "taskman".to_string()
                } else {
                    app_id.clone()
                };

                // Dynamically query desktop file metadata for user-facing name and icon
                let (desktop_name, desktop_icon) = query_desktop_file_metadata(&app_id);
                if let Some(name) = desktop_name {
                    entry.app_name = name;
                }
                entry.icon_name = desktop_icon.unwrap_or_else(|| app_id.clone());

                if let Some(icon_path) = locate_actual_icon_path(&entry.icon_name, &app_id) {
                    if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, 32) {
                        let b64_data = STANDARD.encode(&raw_bytes);
                        entry.terminal_icon_code =
                            format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data);
                    } else {
                        entry.terminal_icon_code = "📁 [Rasterize Error]".to_string();
                    }
                } else {
                    entry.terminal_icon_code = "📁 [No Icon Found]".to_string();
                }
            }
            HandleEvent::Title { title } => {
                entry.title = if title.is_empty() {
                    "[Untitled]".to_string()
                } else {
                    title
                }
            }
            HandleEvent::Done => {
                state
                    .sys_scanner
                    .refresh_processes(sysinfo::ProcessesToUpdate::All, true);

                if let Some((pid, _proc)) = state.sys_scanner.processes().iter().find(|(_, p)| {
                    let proc_name = p.name().to_string_lossy().to_lowercase();
                    let target_app = entry.app_name.to_lowercase();
                    proc_name.contains(&target_app) || target_app.contains(&proc_name)
                }) {
                    entry.matched_pid = Some(pid.as_u32());

                    // Dynamic Steam lookup if applicable
                    if let Some((appid, steam_name, steam_icon_path)) = resolve_steam_game_details(
                        &entry.app_name,
                        &entry.title,
                        &state.sys_scanner,
                        *pid,
                    ) {
                        entry.app_name = steam_name;
                        entry.icon_name = format!("steam_icon_{}", appid);
                        if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&steam_icon_path, 32) {
                            let b64_data = STANDARD.encode(&raw_bytes);
                            entry.terminal_icon_code =
                                format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data);
                        } else {
                            entry.terminal_icon_code = "📁 [Rasterize Error]".to_string();
                        }
                    }

                    println!("---------------------------------------------------------");
                    println!("[LINK ESTABLISHED]: Wayland Handle -> Linux OS Process");
                    println!("  Wayland Protocol ID: {}", id);
                    println!("  Wayland Window ID  : {}", id);
                    println!("  Resolved App Name  : {}", entry.app_name);
                    println!("  System Icon Key    : {}", entry.icon_name);
                    println!("  Window Title Text  : {}", entry.title);
                    println!("  Linked Process PID : {}", pid.as_u32());
                    println!("  Application Icon   : {}", entry.terminal_icon_code);
                    println!("---------------------------------------------------------");
                }
            }
            HandleEvent::Closed => {
                state.window_cache.remove(&id);
            }
            _ => {}
        }
    }
}

smithay_client_toolkit::delegate_registry!(RealtimeTrackerApp);

fn main() {
    let conn = Connection::connect_to_env().expect("Wayland connection failed");
    let (globals, mut event_queue) = registry_queue_init(&conn).unwrap();
    let mut app = RealtimeTrackerApp {
        registry_state: RegistryState::new(&globals),
        window_cache: HashMap::new(),
        sys_scanner: System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        ),
    };
    let qh = event_queue.handle();
    let _manager: ZwlrForeignToplevelManagerV1 =
        globals.bind(&qh, 1..=3, ()).expect("Manager bind failed");
    loop {
        event_queue.blocking_dispatch(&mut app).unwrap();
    }
}
