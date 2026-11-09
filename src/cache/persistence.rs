//! Persistent storage for dock configuration.
//!
//! Saves and loads pinned apps list to ~/.config/dock/pins.json.

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
    if let Ok(data) = fs::read_to_string(&path) {
        // Try Vec first (new ordered format), fall back to HashSet (old format)
        let pins: Vec<String> = serde_json::from_str(&data)
            .or_else(|_| {
                serde_json::from_str::<std::collections::HashSet<String>>(&data)
                    .map(|set| set.into_iter().collect())
            })
            .unwrap_or_else(|_| Vec::new());
        pins
    } else {
        // These applications are requested as persistent dock entries. Once
        // the user changes the pin list, the saved file becomes authoritative.
        vec![
            "AmneziaVPN".into(),
            "steam".into(),
            "org.qbittorrent.qBittorrent".into(),
        ]
    }
}

pub fn save_pinned_apps(pinned: &Vec<String>) {
    let dir = get_config_dir();
    let path = get_config_path();
    let _ = fs::create_dir_all(dir);
    if let Ok(data) = serde_json::to_string(pinned) {
        let _ = fs::write(path, data);
    } else {
    }
}
