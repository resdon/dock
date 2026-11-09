# Tray discovery, widget layout, and persistent bottom status

## Tray discovery

- Watcher registry is polled every five seconds so Qt/Ayatana tray registrations do not wait for a long interval.
- If the watcher registry omits a known StatusNotifierItem service, the listener checks filtered D-Bus names (`StatusNotifierItem`, `NotificationItem`, qBittorrent, Steam) and only accepts candidates that actually expose an SNI interface.
- KDE-style service names probe `org.kde.StatusNotifierItem` before the freedesktop spelling.
- Object-path discovery checks introspection interfaces, not only object-path naming conventions, and includes standard Qt, KDE, Ayatana, and root-path candidates.
- qBittorrent receives an icon-theme fallback; if no icon can be resolved, the tray renderer still draws a text fallback so the item remains visible.
- Enable detailed diagnostics with `DOCK_TRAY_DEBUG=1`.

## Right utility box

The four buttons are arranged in a 2×2 box:

- Top row: Rclone, Processes.
- Bottom row: Audio, Bluetooth.

The box is 76×72 logical pixels and remains a sibling subsurface to the right of the app row. It fades using the same auto-hide alpha as the main dock. Popup panels anchor above the whole box to avoid obscuring the top row when opening a bottom-row widget.

## Bottom status strip

Keyboard layout, network throughput, and date/time remain fully opaque and do not participate in the dock fade. The strip stays visible while the dock auto-hides and remains click-through. Status-only updates redraw sidecar surfaces without repainting the main application row unless another UI event already requires a full frame.

## Local checks

```bash
DOCK_TRAY_DEBUG=1 cargo run
cargo test widget_bar
cargo test --lib listeners::dbus_tray
cargo check
```

When testing qBittorrent, start it after the dock and inspect the `Watcher currently reports` and `Recovered item missing from watcher registry` log lines.
