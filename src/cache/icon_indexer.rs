//! Icon cache indexer.
//!
//! Background startup task that scans icon themes and builds the cache index.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

pub fn get_icon_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(format!("{}/.cache/dock", home));
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join("icon_list.txt")
}

pub fn spawn_startup_indexer() {
    thread::spawn(|| {
        // Keep the indexer below the UI's scheduling priority. It is cache
        // maintenance, never foreground work.
        #[cfg(target_os = "linux")]
        {
            let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) };
            if rc != 0 {
                // Best-effort: failure to lower priority should not stop indexing.
            }
        }

        let list_path = get_icon_list_path();

        // The index is persistent cache data. Rebuilding it on every launch
        // needlessly competes with the UI for CPU and disk bandwidth. Refresh
        // only when missing or older than 24 hours.
        if let Ok(metadata) = std::fs::metadata(&list_path) {
            if let Ok(modified) = metadata.modified() {
                if modified.elapsed().unwrap_or(Duration::MAX) < Duration::from_secs(24 * 60 * 60) {
                    return;
                }
            }
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let search_dirs = vec![
            PathBuf::from("/usr/share/icons"),
            PathBuf::from("/usr/share/pixmaps"),
            PathBuf::from(format!("{}/.local/share/icons", home)),
            PathBuf::from(format!("{}/.icons", home)),
        ];

        // 1. Gather top-level directories to chunk work evenly
        let mut top_level_dirs = Vec::new();
        for root in search_dirs {
            if let Ok(entries) = std::fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        top_level_dirs.push(path);
                    }
                }
            }
        }

        let num_workers = 2.min(top_level_dirs.len().max(1));
        let chunk_size = top_level_dirs.len().div_ceil(num_workers);
        let chunks: Vec<Vec<PathBuf>> = top_level_dirs
            .chunks(chunk_size.max(1))
            .map(|c| c.to_vec())
            .collect();

        let mut worker_handles = vec![];
        let mut shard_paths = vec![];

        // 2. Spawn workers that stream directly to disk shards (zero massive RAM accumulation)
        for (i, chunk) in chunks.into_iter().enumerate() {
            let shard_path = list_path.with_extension(format!("shard.{}", i));
            let shard_path_clone = shard_path.clone();
            shard_paths.push(shard_path);

            let handle = thread::spawn(move || {
                // Icon indexing is deliberately lower priority than the UI.
                // This prevents directory traversal from starving the Wayland
                // event/render thread during startup.
                #[cfg(target_os = "linux")]
                {
                    let rc = unsafe { libc::setpriority(libc::PRIO_PROCESS, 0, 10) };
                    if rc != 0 {
                        // Best-effort: failure to lower priority should not stop indexing.
                    }
                }

                if let Ok(file) = File::create(&shard_path_clone) {
                    let mut writer = BufWriter::new(file);
                    for dir in chunk {
                        scan_directory_to_writer(&dir, &mut writer, 0);
                    }
                    let _ = writer.flush();
                }
            });
            worker_handles.push(handle);
        }

        // 3. Wait for all workers to complete and drop their local buffers
        for handle in worker_handles {
            let _ = handle.join();
        }

        // 4. Merge into a temporary file and atomically replace the old cache.
        // Icon lookups may happen while indexing is in progress, so never
        // truncate the live cache in-place. Readers see either the old complete
        // index or the new complete index.
        let temp_path = list_path.with_extension("icon_list.tmp");
        if let Ok(file) = File::create(&temp_path) {
            let mut final_writer = BufWriter::new(file);
            for shard in &shard_paths {
                if let Ok(mut shard_file) = File::open(shard) {
                    let _ = std::io::copy(&mut shard_file, &mut final_writer);
                }
            }
            let _ = final_writer.flush();
            drop(final_writer);
            let _ = std::fs::rename(&temp_path, &list_path);
        }

        for shard in shard_paths {
            let _ = std::fs::remove_file(shard);
        }
        let _ = std::fs::remove_file(temp_path);

        // Indexer complete; thread stack and buffers go out of scope here.
        // The OS immediately reclaims all RAM allocated to this thread.
    });
}

/// Recursively scans directory and streams paths directly into the disk buffer.
fn scan_directory_to_writer(dir: &Path, writer: &mut BufWriter<File>, depth: usize) {
    const MAX_SCAN_DEPTH: usize = 32;
    if depth > MAX_SCAN_DEPTH {
        return;
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && !path.is_symlink() {
                scan_directory_to_writer(&path, writer, depth + 1);
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
