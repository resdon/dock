//! Handler utility functions.
//!
//! Shared helpers for event handlers.

use std::env;
use std::path::{Path, PathBuf};

/// Resolve the launcher script independently of the process working directory.
/// A dock started by a desktop entry commonly has $HOME as its CWD, so the old
/// ./scripts lookup silently selected an outdated /usr/share copy instead of
/// the script belonging to the source tree used for this build.
pub fn get_launcher_path() -> String {
    if let Some(override_path) = env::var_os("DOCK_LAUNCHER_PATH") {
        let path = PathBuf::from(override_path);
        if path.is_file() {
            return path.to_string_lossy().into_owned();
        }
    }

    // Cargo embeds the package root at compile time. During development this
    // consistently selects the script that was built and tested, regardless
    // of whether the dock was started from a terminal, desktop entry or WM.
    let manifest_script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/launcher.sh");
    if manifest_script.is_file() {
        return manifest_script.to_string_lossy().into_owned();
    }

    // Installed packages may keep a user-specific launcher in XDG_DATA_HOME.
    let data_home = env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
        let home = env::var("HOME").unwrap_or_default();
        format!("{}/.local/share", home)
    });
    let user_path = Path::new(&data_home).join("dock/launcher.sh");
    if user_path.is_file() {
        return user_path.to_string_lossy().into_owned();
    }

    // Arch package installation path (PKGBUILD installs launcher.sh here).
    let system_path = Path::new("/usr/share/dock/launcher.sh");
    if system_path.is_file() {
        return system_path.to_string_lossy().into_owned();
    }

    // Last-resort development lookup, retained for manually copied binaries.
    let cwd_script = Path::new("./scripts/launcher.sh");
    if cwd_script.is_file() {
        return cwd_script.to_string_lossy().into_owned();
    }

    system_path.to_string_lossy().into_owned()
}

pub use crate::events::utils::{parse_uri_list, parse_window_states, percent_decode};

#[cfg(test)]
mod launcher_path_tests {
    use super::*;

    #[test]
    fn source_build_uses_its_own_launcher_script() {
        let packaged_script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/launcher.sh");
        if packaged_script.is_file() && env::var_os("DOCK_LAUNCHER_PATH").is_none() {
            assert_eq!(PathBuf::from(get_launcher_path()), packaged_script);
        }
    }
}
