use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let home = env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"));

    // We store PathBufs directly to avoid lifetime/borrowing errors
    let dirs = vec![
        PathBuf::from("/usr/share/applications"),
        PathBuf::from("/usr/local/share/applications"),
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/appstream/flathub/x86_64/724b0f962e2504fe47cd53d7fce5085e518ca2fed6e66dd4444293d6b93f277d/icons"),
    ];

    let mut i = 0;

    for dir_path in dirs {
        if !dir_path.exists() {
            continue;
        }

        if let Ok(entries) = fs::read_dir(dir_path) {
            for entry in entries.flatten() {
                let p = entry.path();
                let file_name = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Process .desktop files
                if p.is_file() && file_name.ends_with(".desktop") {
                    let mut icon_val = "None".to_string();
                    if let Ok(content) = fs::read_to_string(&p) {
                        for line in content.lines() {
                            if let Some(val) = line.strip_prefix("Icon=") {
                                icon_val = val.to_string();
                                break;
                            }
                        }
                    }
                    println!(
                        " {:<4} | File: {:<30} | Icon Value: {}",
                        i, file_name, icon_val
                    );
                    i += 1;
                }
                // Process files in the Flatpak icons folder specifically
                else if p.is_file() && p.to_string_lossy().contains("flatpak") {
                    println!(" {:<4} | Icon File: {}", i, file_name);
                    i += 1;
                }
            }
        }
    }
}
