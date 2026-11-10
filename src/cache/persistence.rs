//! Persistent storage for dock configuration.
//!
//! Saves and loads the ordered list of pinned applications to
//! `~/.config/dock/pins.json`.

use std::fs;
use std::io;

fn get_config_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/.config/dock", home)
}

fn get_config_path() -> String {
    format!("{}/pins.json", get_config_dir())
}

pub fn load_pinned_apps() -> Vec<String> {
    let path = get_config_path();
    let Ok(data) = fs::read_to_string(&path) else {
        return Vec::new();
    };

    let pins: Vec<String> = serde_json::from_str(&data)
        .or_else(|_| {
            serde_json::from_str::<std::collections::HashSet<String>>(&data)
                .map(|set| set.into_iter().collect())
        })
        .unwrap_or_default();
    pins
}

pub fn save_pinned_apps(pinned: &Vec<String>) -> io::Result<()> {
    let dir = get_config_dir();
    let path = get_config_path();
    fs::create_dir_all(dir)?;

    let data = serde_json::to_string(pinned)?;
    fs::write(path, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn load_pinned_apps_returns_empty_when_missing() {
        let home = std::env::temp_dir().join(format!(
            "dock-persistence-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", &home);
        let pins = load_pinned_apps();
        if let Some(value) = original_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        assert!(pins.is_empty());
        let _ = fs::remove_dir_all(home);
    }
}
