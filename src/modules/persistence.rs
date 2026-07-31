use std::fs;

fn get_config_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/.config/dock", home)
}

fn get_config_path() -> String {
    format!("{}/pins.json", get_config_dir())
}

pub fn load_pinned_apps() -> Vec<String> {
    let path = get_config_path();
    println!("[PERSISTENCE] Loading pins from: {}", path);
    if let Ok(data) = fs::read_to_string(&path) {
        // Try Vec first (new ordered format), fall back to HashSet (old format)
        let pins: Vec<String> = serde_json::from_str(&data)
            .or_else(|_| {
                serde_json::from_str::<std::collections::HashSet<String>>(&data)
                    .map(|set| set.into_iter().collect())
            })
            .unwrap_or_else(|_| Vec::new());
        println!("[PERSISTENCE] Loaded: {:?}", pins);
        pins
    } else {
        println!("[PERSISTENCE] No config file found, starting empty.");
        Vec::new()
    }
}

pub fn save_pinned_apps(pinned: &Vec<String>) {
    let dir = get_config_dir();
    let path = get_config_path();
    println!("[PERSISTENCE] Saving pins to: {} Data: {:?}", path, pinned);
    let _ = fs::create_dir_all(dir);
    if let Ok(data) = serde_json::to_string(pinned) {
        let _ = fs::write(path, data);
        println!("[PERSISTENCE] Save successful.");
    } else {
        println!("[PERSISTENCE] Save failed.");
    }
}
