use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

pub mod icon_indexer;
pub mod persistence;

/// Clears the cache directory on startup to ensure fresh data
pub fn refresh_cache_on_launch() {
    let cache_dir = get_cache_dir();
    
    if cache_dir.exists() {
        if let Err(e) = fs::remove_dir_all(&cache_dir) {
            eprintln!("Failed to clear cache: {}", e);
        }
    }
    let _ = fs::create_dir_all(&cache_dir);
    println!("[CACHE RELOADED]")
}

/// Resolves the default storage directory for cached app assets
fn get_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("dock");
    path.push("cached_icons");
    let _ = fs::create_dir_all(&path);
    path
}

/// Permanently saves raw RGBA icon data to the default directory
pub fn save_cached_icon(app_id: &str, width: u32, height: u32, rgba: &[u8]) {
    let mut path = get_cache_dir();
    path.push(format!("{}.raw", app_id));
    
    if let Ok(mut file) = File::create(path) {
        // Encode metadata headers (width, height) followed by raw byte stream
        let _ = file.write_all(&width.to_ne_bytes());
        let _ = file.write_all(&height.to_ne_bytes());
        let _ = file.write_all(rgba);
    }
}

/// Loads raw RGBA icon data from the default directory if it exists
pub fn load_cached_icon(app_id: &str) -> Option<(Vec<u8>, u32)> {
    let mut path = get_cache_dir();
    path.push(format!("{}.raw", app_id));
    
    let mut file = File::open(path).ok()?;
    let mut w_bytes = [0u8; 4];
    let mut h_bytes = [0u8; 4];
    
    file.read_exact(&mut w_bytes).ok()?;
    file.read_exact(&mut h_bytes).ok()?;
    
    let width = u32::from_ne_bytes(w_bytes);
    let mut rgba = Vec::new();
    file.read_to_end(&mut rgba).ok()?;
    
    Some((rgba, width))
}
