use linicon::{lookup_icon};
use std::path::PathBuf;

/// Searches for an icon path using the Freedesktop specification.
/// 'theme_name' is typically retrieved from your system settings (e.g., "Adwaita").
pub fn get_icon_path(icon_name: &str, size: u32, theme_name: &str) -> Option<PathBuf> {
    // 1. Strict match
    if let Some(res) = lookup_icon(icon_name)
        .from_theme(theme_name)
        .with_size(size as u16)
        .next()
        .and_then(|result| result.ok())
    {
        return Some(res.path);
    }
    
    // 2. Fallback to hicolor without size constraints
    if let Some(res) = lookup_icon(icon_name)
        .from_theme("hicolor")
        .next()
        .and_then(|result| result.ok())
    {
        return Some(res.path);
    }

    // 3. Fallback to any theme without size constraints
    if let Some(res) = lookup_icon(icon_name)
        .next()
        .and_then(|result| result.ok())
    {
        return Some(res.path);
    }

    // 4. Hardcoded pixmaps check
    let pixmap = PathBuf::from(format!("/usr/share/pixmaps/{}.png", icon_name));
    if pixmap.exists() {
        return Some(pixmap);
    }

    None
}

pub fn main() {
    let icon_name = "firefox"; // Example: icon name from a .desktop file
    let theme = "Adwaita";     // You can read this from ~/.config/gtk-3.0/settings.ini
    let size = 48;

    match get_icon_path(icon_name, size, theme) {
        Some(path) => println!("Found icon at: {:?}", path),
        None => println!("Icon '{}' not found in theme '{}'", icon_name, theme),
    }
}
