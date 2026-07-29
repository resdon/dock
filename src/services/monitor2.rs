use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

// Base64 engine for terminal protocol payloads
use base64::{engine::general_purpose::STANDARD, Engine as _};

use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryHandler, RegistryState};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::{
    Event as HandleEvent, ZwlrForeignToplevelHandleV1,
};
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_manager_v1::{
    Event as ManagerEvent, ZwlrForeignToplevelManagerV1,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use linicon::lookup_icon;

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

fn extract_icon_name_from_desktop_file(app_id: &str, resolved_binary: &str) -> String {
    if app_id.is_empty() && resolved_binary.is_empty() { return "unknown-icon".to_string(); }
    
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return app_id.to_string(),
    };

    let mut potential_app_ids = Vec::new();

    // 1. If the app_id or resolved name is already a numeric Steam AppID
    if app_id.chars().all(|c| c.is_ascii_digit()) && !app_id.is_empty() {
        potential_app_ids.push(app_id.to_string());
    }
    let binary_lower = resolved_binary.to_lowercase();
    if resolved_binary.chars().all(|c| c.is_ascii_digit()) && !resolved_binary.is_empty() {
        potential_app_ids.push(resolved_binary.to_string());
    }

    // 2. Scan Steam library appmanifest files (.acf) to match executable names or app names globally
    let steam_roots = vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];

    for steam_root in &steam_roots {
        let steamapps_path = steam_root.join("steamapps");
        if steamapps_path.exists() {
            if let Ok(entries) = std::fs::read_dir(&steamapps_path) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                        if ext == "acf" {
                            // Parse basic key-values from appmanifest .acf files
                            if let Ok(file) = File::open(&path) {
                                let reader = BufReader::new(file);
                                let mut current_appid = String::new();
                                let mut current_name = String::new();
                                
                                for line in reader.lines().flatten() {
                                    let trimmed = line.trim();
                                    if trimmed.starts_with("\"appid\"") {
                                        current_appid = trimmed.split('"').nth(3).unwrap_or("").to_string();
                                    } else if trimmed.starts_with("\"name\"") {
                                        current_name = trimmed.split('"').nth(3).unwrap_or("").to_lowercase();
                                    }
                                }

                                if !current_appid.is_empty() {
                                    if current_name.contains(&binary_lower) || binary_lower.contains(&current_name) {
                                        potential_app_ids.push(current_appid.clone());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // 3. Check XDG desktop application directories for steam_app_*.desktop files
    let mut search_paths = vec![
        home.join(".local/share/applications"),
        PathBuf::from("/usr/share/applications"),
    ];
    let xdg_data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in xdg_data_dirs.split(':') {
        search_paths.push(Path::new(dir).join("applications"));
    }

    let mut found_icon_from_desktop = None;

    for path in &search_paths {
        if !path.exists() { continue; }
        // Look for any steam-generated desktop file matching app patterns
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                let p = entry.path();
                if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    let name_lower = name.to_lowercase();
                    if name_lower.starts_with("steam_app_") || name_lower.contains(&binary_lower) {
                        if let Ok(file) = File::open(&p) {
                            let reader = BufReader::new(file);
                            for line in reader.lines().flatten() {
                                if line.starts_with("Icon=") {
                                    let icon_val = line["Icon=".len()..].trim().to_string();
                                    // If we match the specific binary or appid in the filename, take it immediately
                                    if name_lower.contains(&binary_lower) {
                                        return icon_val;
                                    }
                                    found_icon_from_desktop = Some(icon_val);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // If we discovered a valid AppID via manifests, construct the standard Steam icon key
    if let Some(appid) = potential_app_ids.first() {
        return format!("steam_icon_{}", appid);
    }

    if let Some(icon) = found_icon_from_desktop {
        return icon;
    }

    app_id.to_string()
}
fn locate_actual_icon_path(icon_name: &str, app_id: &str) -> Option<PathBuf> {
    if let Some(icon) = lookup_icon(icon_name).from_theme("hicolor").next().and_then(|res| res.ok()) {
        return Some(icon.path);
    }
    if let Some(icon) = lookup_icon(app_id).from_theme("hicolor").next().and_then(|res| res.ok()) {
        return Some(icon.path);
    }

    let mut search_roots = vec![PathBuf::from("/usr/share/icons"), PathBuf::from("/usr/share/pixmaps")];
    if let Ok(home) = std::env::var("HOME") {
        search_roots.push(Path::new(&home).join(".icons"));
        search_roots.push(Path::new(&home).join(".local/share/icons"));
    }

    let target_lower = icon_name.to_lowercase();
    let app_lower = app_id.to_lowercase();

    for root in search_roots {
        if !root.exists() { continue; }
        for entry in WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    if ext == "png" || ext == "svg" {
                        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                            let name_lower = file_name.to_lowercase();
                            if name_lower.contains(&target_lower) || name_lower.contains(&app_lower) {
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

fn load_image_raw_rgba(path: &Path, target_size: u32) -> Option<(u32, u32, Vec<u8>)> {
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
        println!("{:?}", path.display());
        let scaled = img.resize_exact(target_size, target_size, image::imageops::FilterType::Lanczos3);
        let rgba = scaled.to_rgba8();
        Some((rgba.width(), rgba.height(), rgba.into_raw()))
    }
}

fn generate_terminal_image_string(icon_name: &str, app_id: &str, target_size: u32) -> String {
    let icon_path = match locate_actual_icon_path(icon_name, app_id) {
        Some(path) => path,
        None => return "📁 [No Icon Found]".to_string(),
    };

    if let Some((w, h, raw_bytes)) = load_image_raw_rgba(&icon_path, target_size) {
        let b64_data = STANDARD.encode(&raw_bytes);
        // Kitty Graphics Protocol escape sequence
        format!("\x1b_Ga=T,f=32,s={},v={};{}\x1b\\", w, h, b64_data)
    } else {
        "📁 [Rasterize Error]".to_string()
    }
}

impl ProvidesRegistryState for RealtimeTrackerApp {
    fn registry(&mut self) -> &mut RegistryState { &mut self.registry_state }
    smithay_client_toolkit::registry_handlers![RealtimeTrackerApp];
}

// CORRECTED: Remove &mut self and use the explicit data parameter
impl RegistryHandler<RealtimeTrackerApp> for RealtimeTrackerApp {
    fn new_global(
        _data: &mut RealtimeTrackerApp, 
        _conn: &Connection, 
        _qh: &QueueHandle<RealtimeTrackerApp>, 
        _name: u32, 
        _interface: &str, 
        _version: u32
    ) {}

    fn remove_global(
        _data: &mut RealtimeTrackerApp, 
        _conn: &Connection, 
        _qh: &QueueHandle<RealtimeTrackerApp>, 
        _name: u32, 
        _interface: &str
    ) {}
}
impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for RealtimeTrackerApp {
    fn event(_: &mut Self, _: &ZwlrForeignToplevelManagerV1, _: ManagerEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
    event_created_child!(RealtimeTrackerApp, ZwlrForeignToplevelManagerV1, [0 => (ZwlrForeignToplevelHandleV1, ())]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for RealtimeTrackerApp {
    fn event(state: &mut Self, proxy: &ZwlrForeignToplevelHandleV1, event: HandleEvent, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let id = proxy.id().protocol_id();
        let entry = state.window_cache.entry(id).or_default();

        match event {
			HandleEvent::AppId { app_id } => {
	            entry.app_name = if app_id.is_empty() { "taskman".to_string() } else { app_id };
	        }
            HandleEvent::Title { title } => entry.title = if title.is_empty() { "[Untitled]".to_string() } else { title },
			HandleEvent::Done => {
			    state.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
			
			    let target_app = entry.app_name.to_lowercase();

				// 1. Find the primary matching process from the Wayland app_id
				let matched_process = state.sys_scanner.processes().iter().find(|(_, p)| {
				    let proc_name = p.name().to_string_lossy().to_lowercase();
				    proc_name.contains(&target_app) || target_app.contains(&proc_name)
				});
				
				if let Some((pid, proc)) = matched_process {
				    let mut final_pid = *pid;
				    let mut resolved_name = proc.name().to_string_lossy().into_owned();
				    let initial_name_lower = resolved_name.to_lowercase();
				
				    let is_wrapper = initial_name_lower.contains("gamescope") 
				        || target_app.contains("gamescope")
				        || initial_name_lower.contains("wine")
				        || initial_name_lower.contains("bwrap");
				
				    // If it's a wrapper, we explicitly force finding a child payload instead of keeping the wrapper's PID
				    if is_wrapper {
				        let mut best_candidate: Option<(sysinfo::Pid, String, f32, u64)> = None;
				
				        for (c_pid, c_proc) in state.sys_scanner.processes() {
				            // Check if process is a descendant of the wrapper PID
				            let mut current = c_proc.parent();
				            let mut is_descendant = c_pid == pid;
				            
				            while let Some(p_id) = current {
				                if p_id == *pid {
				                    is_descendant = true;
				                    break;
				                }
				                if let Some(parent_proc) = state.sys_scanner.process(p_id) {
				                    current = parent_proc.parent();
				                } else {
				                    break;
				                }
				            }
				
				            if is_descendant {
				                let name = c_proc.name().to_string_lossy().into_owned();
				                let name_lower = name.to_lowercase();
				                
				                // Filter out wrappers, shells, and system utilities
				                let is_utility = name_lower.contains("gamescope")
				                    || name_lower.contains("wine")
				                    || name_lower.contains("wineserver")
				                    || name_lower.contains("bwrap")
				                    || name_lower.contains("steam")
				                    || name_lower.contains("updater")
				                    || name_lower.contains("reaper")
				                    || name_lower.contains("module-rt")
				                    || name_lower.contains("conhost")
				                    || name_lower.contains("helper")
				                    || name_lower.contains("sh")
				                    || name_lower.contains("bash")
				                    || name_lower.contains("flatpak");
				
				                if !is_utility {
				                    let cpu = c_proc.cpu_usage();
				                    let mem = c_proc.memory();
				                    
				                    if let Some((_, _, best_cpu, best_mem)) = &best_candidate {
				                        if cpu > *best_cpu || (cpu == *best_cpu && mem > *best_mem) {
				                            best_candidate = Some((*c_pid, name, cpu, mem));
				                        }
				                    } else {
				                        best_candidate = Some((*c_pid, name, cpu, mem));
				                    }
				                }
				            }
				        }
				
				        if let Some((cand_pid, cand_name, _, _)) = best_candidate {
				            final_pid = cand_pid;
				            resolved_name = cand_name;
				        }
				    }			
							
					entry.matched_pid = Some(final_pid.as_u32());
				    entry.app_name = resolved_name.clone();
				    
				    // Perform icon extraction and image rendering AFTER final process resolution
				    entry.icon_name = extract_icon_name_from_desktop_file(&entry.app_name, &resolved_name);
				    entry.terminal_icon_code = generate_terminal_image_string(&entry.icon_name, &resolved_name, 32);
			
			        println!("---------------------------------------------------------");
			        println!("[LINK ESTABLISHED]: Wayland Handle -> Linux OS Process");
			        println!("  Wayland Protocol ID: {}", id); 
			        println!("  Wayland Window ID  : {}", id); 
			        println!("  Resolved App Name  : {}", resolved_name);
			        println!("  System Icon Key    : {}", entry.icon_name);
			        println!("  Window Title Text  : {}", entry.title);
			        println!("  Linked Process PID : {}", final_pid.as_u32());
			        println!("  Application Icon   : {}", entry.terminal_icon_code); 
			        println!("---------------------------------------------------------");
			    }
			}
            HandleEvent::Closed => { state.window_cache.remove(&id); }
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
        sys_scanner: System::new_with_specifics(RefreshKind::nothing().with_processes(ProcessRefreshKind::everything())),
    };
    let qh = event_queue.handle();
    let _manager: ZwlrForeignToplevelManagerV1 = globals.bind(&qh, 1..=3, ()).expect("Manager bind failed");
    loop { event_queue.blocking_dispatch(&mut app).unwrap(); }
}
