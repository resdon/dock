//! Icon caching system.
//!
//! Manages persistent icon cache on disk and in-memory lookup.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

const ICON_CACHE_MAGIC: u32 = 0x444F434B;
const ICON_CACHE_VERSION: u32 = 5;

pub mod icon_indexer;
pub mod persistence;

/// Refreshes the on-disk cache without destroying valid entries.
///
/// A full wipe on each launch defeats the point of a persistent icon cache and
/// forces all icons to be re-resolved on startup. We keep valid cached assets,
/// and only prune entries that are obviously corrupt or from an incompatible
/// cache version.
pub fn refresh_cache_on_launch() {
    refresh_cache_on_launch_in_dir(&get_cache_dir());
}

fn refresh_cache_on_launch_in_dir(cache_dir: &std::path::Path) {
    let _ = fs::create_dir_all(cache_dir);

    let Ok(entries) = fs::read_dir(cache_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && is_cache_file_stale(&path) {
            let _ = fs::remove_file(path);
        }
    }
}

fn is_cache_file_stale(path: &std::path::Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return true;
    };

    let mut magic = [0u8; 4];
    let mut version = [0u8; 4];
    if file.read_exact(&mut magic).is_err() || file.read_exact(&mut version).is_err() {
        return true;
    }

    (u32::from_ne_bytes(magic) != ICON_CACHE_MAGIC)
        || (u32::from_ne_bytes(version) != ICON_CACHE_VERSION)
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

    let mut file = match File::create(&path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Failed to create icon cache file '{}': {e}", path.display());
            return;
        }
    };

    // Versioned metadata headers. Version 5 stores canonical RGBA sources
    // generated at 48px (matching dock icon size) for Slint rendering;
    // entries from older cache layouts are rejected.
    if let Err(e) = file.write_all(&ICON_CACHE_MAGIC.to_ne_bytes())
        .and_then(|_| file.write_all(&ICON_CACHE_VERSION.to_ne_bytes()))
        .and_then(|_| file.write_all(&width.to_ne_bytes()))
        .and_then(|_| file.write_all(&height.to_ne_bytes()))
        .and_then(|_| file.write_all(rgba))
    {
        eprintln!("Failed to write icon cache file '{}': {e}", path.display());
    }
}

/// Loads raw RGBA icon data from the default directory if it exists
pub fn load_cached_icon(app_id: &str) -> Option<(Vec<u8>, u32)> {
    let mut path = get_cache_dir();
    path.push(format!("{}.raw", app_id));

    let mut file = File::open(path).ok()?;
    let mut magic_bytes = [0u8; 4];
    let mut version_bytes = [0u8; 4];
    let mut w_bytes = [0u8; 4];
    let mut h_bytes = [0u8; 4];

    file.read_exact(&mut magic_bytes).ok()?;
    file.read_exact(&mut version_bytes).ok()?;
    file.read_exact(&mut w_bytes).ok()?;
    file.read_exact(&mut h_bytes).ok()?;

    if u32::from_ne_bytes(magic_bytes) != ICON_CACHE_MAGIC
        || u32::from_ne_bytes(version_bytes) != ICON_CACHE_VERSION
    {
        return None;
    }

    let width = u32::from_ne_bytes(w_bytes);
    let height = u32::from_ne_bytes(h_bytes);
    if width == 0 || height == 0 {
        return None;
    }
    let expected_len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;

    let mut rgba = Vec::new();
    file.read_to_end(&mut rgba).ok()?;
    if rgba.len() != expected_len {
        return None;
    }

    Some((rgba, width))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn refresh_cache_on_launch_keeps_valid_icons() {
        let root = std::env::temp_dir().join(format!(
            "dock-cache-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let valid_path = root.join("keep.raw");
        let mut valid = File::create(&valid_path).unwrap();
        valid.write_all(&ICON_CACHE_MAGIC.to_ne_bytes()).unwrap();
        valid.write_all(&ICON_CACHE_VERSION.to_ne_bytes()).unwrap();
        valid.write_all(&(2u32).to_ne_bytes()).unwrap();
        valid.write_all(&(2u32).to_ne_bytes()).unwrap();
        valid.write_all(&vec![0u8; 16]).unwrap();

        refresh_cache_on_launch_in_dir(&root);

        assert!(valid_path.exists());
        let _ = fs::remove_dir_all(&root);
    }
}
