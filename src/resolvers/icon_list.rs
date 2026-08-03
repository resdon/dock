use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;

/// Returns the standard path for the generated icon list cache file.
pub fn get_icon_list_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    let cache_dir = PathBuf::from(format!("{}/.cache/dockman", home));
    let _ = std::fs::create_dir_all(&cache_dir);
    cache_dir.join("icon_list.txt")
}

/// Spawns a background thread to index icon directories in parallel using a sharded streaming approach,
/// dropping RAM immediately after completion.
pub fn spawn_startup_indexer() {
    thread::spawn(|| {
        let list_path = get_icon_list_path();
        let start = Instant::now();
        println!("[ICON INDEXER] Starting low-memory parallel scan...");

        let home = std::env::var("HOME").unwrap_or_default();
        let search_dirs = vec![
            PathBuf::from("/usr/share/icons"),
            PathBuf::from("/usr/share/pixmaps"),
            PathBuf::from(format!("{}/.local/share/icons", home)),
            PathBuf::from(format!("{}/.icons", home)),
        ];

        // 1. Gather top-level directories to distribute work evenly
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
        let chunk_size = (top_level_dirs.len() + num_workers - 1) / num_workers;
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

        println!("[ICON INDEXER] Completed and memory fully dropped in {:0.2?}", start.elapsed());
    });
}

/// Recursively scans a directory and streams valid icon paths directly into the writer buffer
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

/// Searches candidate icon list files in parallel across multiple worker threads using byte-offset
/// chunking, supporting early-exit cancellation and fallback scoring.
pub fn search_icon_list_file(query: &str) -> Option<PathBuf> {
    let search_start = Instant::now(); // <-- Start timer here
    let home = std::env::var("HOME").unwrap_or_default();
    
    let candidate_paths = [
        get_icon_list_path(),
        PathBuf::from("./icon_list.txt"),
        PathBuf::from(format!("{}/.local/share/dock/icon_list.txt", home)),
        PathBuf::from("/usr/share/dock/icon_list.txt"),
    ];

    let list_path = candidate_paths.into_iter().find(|p| p.exists())?;
    let file = File::open(&list_path).ok()?;
    let metadata = file.metadata().ok()?;
    let file_size = metadata.len();

    if file_size == 0 {
        return None;
    }

    let query_lower = query.to_lowercase();

    // For smaller files (< 32 KB), use a fast sequential scan to avoid thread-spawn overhead
    if file_size < 32 * 1024 {
        let reader = BufReader::new(file);
        let mut best_fallback: Option<PathBuf> = None;
        for line in reader.lines().flatten() {
            let line_lower = line.trim().to_lowercase();

            if line_lower.contains(&query_lower) {
                let path = PathBuf::from(&line);

                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if stem.to_lowercase() == query_lower {
                        if line_lower.ends_with(".png") {
                            return Some(path);
                        }
                        best_fallback = Some(path.clone());
                    }
                }

                if best_fallback.is_none() {
                    best_fallback = Some(path);
                }
            }
        }
        return best_fallback;
    }

    // Parallel chunked search across multiple workers
    let num_workers = 4.min((file_size / (16 * 1024)).max(1) as usize);
    let chunk_size = file_size / num_workers as u64;

    let found_exact = Arc::new(AtomicBool::new(false));
    // Stores (exact_png_match, best_fallback)
    let best_result: Arc<Mutex<(Option<PathBuf>, Option<PathBuf>)>> = Arc::new(Mutex::new((None, None)));
    let mut handles = vec![];

    for i in 0..num_workers {
        let start_pos = i as u64 * chunk_size;
        let end_pos = if i == num_workers - 1 { file_size } else { (i + 1) as u64 * chunk_size };

        let list_path_clone = list_path.clone();
        let query_lower_clone = query_lower.clone();
        let found_exact_clone = Arc::clone(&found_exact);
        let best_result_clone = Arc::clone(&best_result);

        let handle = thread::spawn(move || {
            let file = match File::open(&list_path_clone) {
                Ok(f) => f,
                Err(_) => return,
            };
            let mut reader = BufReader::new(file);

            // Seek to chunk start position and align to the next clean newline boundary
            if start_pos > 0 {
                if reader.seek(SeekFrom::Start(start_pos)).is_err() {
                    return;
                }
                let mut buf = Vec::new();
                if reader.read_until(b'\n', &mut buf).is_err() {
                    return;
                }
            } else {
                let _ = reader.seek(SeekFrom::Start(0));
            }

            let mut current_pos = reader.stream_position().unwrap_or(start_pos);
            let mut line_buf = String::new();
            let mut local_exact: Option<PathBuf> = None;
            let mut local_fallback: Option<PathBuf> = None;

            while current_pos < end_pos {
                // Short-circuit if another worker already found an exact .png match
                if found_exact_clone.load(Ordering::Relaxed) {
                    break;
                }

                line_buf.clear();
                match reader.read_line(&mut line_buf) {
                    Ok(0) => break,
                    Ok(bytes_read) => {
                        current_pos += bytes_read as u64;
                        let trimmed = line_buf.trim();
                        if trimmed.is_empty() {
                            continue;
                        }

                        let line_lower = trimmed.to_lowercase();
                        if line_lower.contains(&query_lower_clone) {
                            let path = PathBuf::from(trimmed);

                            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                                if stem.to_lowercase() == query_lower_clone {
                                    if line_lower.ends_with(".png") {
                                        local_exact = Some(path);
                                        found_exact_clone.store(true, Ordering::Relaxed);
                                        break;
                                    }
                                    if local_fallback.is_none() {
                                        local_fallback = Some(path.clone());
                                    }
                                }
                            }

                            if local_fallback.is_none() {
                                local_fallback = Some(path);
                            }
                        }
                    }
                    Err(_) => break,
                }
            }

            // Push local findings into the shared thread-safe container
            if local_exact.is_some() || local_fallback.is_some() {
                let mut res = best_result_clone.lock().unwrap();
                if let Some(ref exact) = local_exact {
                    res.0 = Some(exact.clone());
                }
                if res.1.is_none() {
                    res.1 = local_fallback;
                }
            }
        });

        handles.push(handle);
    }

    for handle in handles {
        let _ = handle.join();
    }

    let res = best_result.lock().unwrap();
    let elapsed = search_start.elapsed(); // <-- Stop timer here
    if let Some(exact) = res.0.clone() {
        println!("[icon_list.txt SEARCH] Returning parallel exact match: {:?} in {:0.2?}", exact, elapsed);
        return Some(exact);
    }
    println!("[icon_list.txt SEARCH] Returning parallel fallback match: {:?} in {:0.2?}", res.1, elapsed);
    res.1.clone()
}