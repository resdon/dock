# System bar geometry compile fix

Fixed unresolved imports and calls introduced by the side-bar layout refactor.

## Changes

- Restored `system_bar_logical_width(dock_width)`, returning at least `SYSTEM_BAR_MIN_WIDTH`.
- Restored `system_bar_position(dock_width)`, centering the strip beneath the app row and placing it at `DOCK_HEIGHT + TRAY_GAP`.
- Removed the unused normalized `service_name` binding in tray watcher refresh.

The helper signatures match the call sites in `src/app/side_bars.rs` and `src/state/dock.rs`, and the existing unit test in `src/render/tray.rs` exercises the width/position behavior.

Cargo compilation was not run in this environment because the Rust toolchain is unavailable.
