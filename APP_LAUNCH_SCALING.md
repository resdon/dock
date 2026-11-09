# Application launch scaling

The launcher path is resolved from `CARGO_MANIFEST_DIR/scripts/launcher.sh`
for source builds, rather than relying on the current working directory. This
prevents a desktop-launched dock (whose CWD is commonly `$HOME`) from silently
using an older `/usr/share/dock/launcher.sh`. `DOCK_LAUNCHER_PATH` can override
the selected script; the Arch package fallback remains `/usr/share/dock/launcher.sh`.

Scale overrides are removed at both the Rust child-process boundary and inside
`launcher.sh`: GTK/GDK, Qt, Enlightenment, Winit and FLTK variables are cleared
unless `DOCK_PRESERVE_APP_SCALE=1` is set. The dock also detects desktop entries
with `DBusActivatable=true` and an `Exec` key. For those apps, it launches a
temporary copy with only `DBusActivatable=false`, so GTK executes the desktop
entry's `Exec` directly instead of handing the request to a user D-Bus/systemd
activation environment that may still contain the old scale overrides.

Enable `DOCK_LAUNCH_DEBUG=1` to log the exact launcher script path and any
inherited scale variables removed at launch time.

The left tray box and right widget box use the same auto-hide alpha as the main
dock. The bottom keyboard/network/clock strip remains fully opaque.
