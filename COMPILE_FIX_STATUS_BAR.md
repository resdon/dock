# Status bar compile fixes

This patch fixes the compiler diagnostics reported for the side-tray/bottom-status-bar layout:

- Renamed `draw_main_dock`'s `_render_status` parameter to `render_status`, because it is used by the first-status-frame condition.
- Replaced `String::as_deref()` in the missing-tray-icon label fallback with an `is_empty()` check and `&String`/`&str` coercion.
- Removed the unused `WaylandSurface` import from `state/mod.rs`.
- Renamed the intentionally unused tray renderer Bluetooth snapshot parameter to `_bluetooth`.

The archive was checked statically and passed ZIP integrity validation. Cargo compilation could not be run in the editing environment because Cargo is unavailable.
