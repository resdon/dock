use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Instant;

pub fn get_icon_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(format!("{}/.cache/dock", home));
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join("icon_list.txt")
}

pub fn spawn_startup_indexer() {
    thread::spawn(|| {
        let list_path = get_icon_list_path();
        let start = Instant::now();
        println!("[RAM-OPTIMIZED INDEXER] Starting low-memory parallel scan...");

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

        let num_workers = 4.min(top_level_dirs.len().max(1));
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
                if let Ok(file) = File::create(&shard_path_clone) {
                    let mut writer = BufWriter::new(file);
                    for dir in chunk {
                        scan_directory_to_writer(&dir, &mut writer);
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

        // 4. Merge shards into the final file and clean up temp shards
        if let Ok(file) = File::create(&list_path) {
            let mut final_writer = BufWriter::new(file);
            for shard in shard_paths {
                if let Ok(mut shard_file) = File::open(&shard) {
                    let _ = std::io::copy(&mut shard_file, &mut final_writer);
                }
                let _ = std::fs::remove_file(&shard);
            }
            let _ = final_writer.flush();
        }

        println!(
            "[RAM-OPTIMIZED INDEXER] Completed and memory fully dropped in {:0.2?}",
            start.elapsed()
        );
    }); // <-- The thread stack, local vectors, and buffers go out of scope here.
        // The OS immediately reclaims all RAM allocated to this thread.
}

/// Recursively scans directory and streams paths directly into the disk buffer
fn scan_directory_to_writer(dir: &Path, writer: &mut BufWriter<File>) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan_directory_to_writer(&path, writer);
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
