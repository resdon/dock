use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;

pub fn get_icon_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(format!("{}/.cache/dockman", home));
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join("icon_list.txt")
}

/// Scans system icon paths asynchronously on startup and updates icon_list.txt
pub fn spawn_startup_indexer() {
    thread::spawn(|| {
        let list_path = get_icon_list_path();
        let file = match File::create(&list_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[WARN] Failed to create icon_list.txt: {}", e);
                return;
            }
        };

        let mut writer = BufWriter::new(file);
        let home = std::env::var("HOME").unwrap_or_default();

        let search_dirs = [
            "/usr/share/icons",
            "/usr/share/pixmaps",
            &format!("{}/.local/share/icons", home),
            &format!("{}/.icons", home),
        ];

        for dir in search_dirs {
            let path = Path::new(dir);
            if path.exists() {
                scan_directory_recursive(path, &mut writer);
            }
        }
    });
}

fn scan_directory_recursive(dir: &Path, writer: &mut BufWriter<File>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory_recursive(&path, writer);
            } else if path.is_file() {
                if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "png" || ext_lower == "svg" || ext_lower == "xpm" {
                        let _ = writeln!(writer, "{}", path.display());
                    }
                }
            }
        }
    }
}