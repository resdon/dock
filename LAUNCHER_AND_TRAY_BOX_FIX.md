# Launcher scale and compact SNI box fix

## Follow-up: scale boundary and first-frame fallback

- Set neutral `GDK_SCALE=1`, `GDK_DPI_SCALE=1`, and `QT_SCALE_FACTOR=1` (with Qt automatic scaling disabled) on the dock-owned launcher process after removing inherited overrides. The launcher script repeats this before `gtk-launch` or a direct executable.
- A previously running process cannot have its toolkit scale changed by the dock. A normal click on an existing window only asks the compositor to activate that window; D-Bus activation may also route a request to an existing process. Such processes keep the scale selected at their original startup. Restarting the application after correcting its launch environment is required for those cases.
- When the icon cache has no image yet, the dock now keeps its neutral icon tile empty instead of drawing a 24px app-ID initial. The asynchronous icon loader replaces the tile as soon as a real icon is available.

The explicit 1x toolkit defaults are intended for systems where the dock itself was started under an accidental scale override. `DOCK_PRESERVE_APP_SCALE=1` remains available for users who deliberately want inherited toolkit settings.

## Launcher resolution and scale isolation

- Resolve the development launcher from the compile-time Cargo manifest path, not the dock process's working directory.
- Keep an explicit `DOCK_LAUNCHER_PATH` override and packaged `/usr/share/dock/launcher.sh` fallback.
- Invoke the launcher with `bash` because the script uses Bash syntax.
- Remove GTK/GDK, Qt, Enlightenment, Winit and FLTK scaling overrides at the spawned launcher boundary and again inside the script. `DOCK_PRESERVE_APP_SCALE=1` opts out.
- For `DBusActivatable=true` desktop entries that also provide `Exec=`, create a temporary desktop-entry copy with only `DBusActivatable=false`. `gtk-launch` then executes the desktop entry directly using the sanitized child environment instead of requesting activation from a user D-Bus/systemd environment that may retain old scale variables.
- Use `DOCK_LAUNCH_DEBUG=1` to log which launcher script was selected, which scale variables were removed, and whether D-Bus activation was bypassed.

## Left tray box

- Match the right utility box at 76×72 logical pixels for one to four SNI icons.
- Arrange icons in a centered two-column grid; a lone icon is centered.
- More than four items wrap into additional rows and increase height without stretching the box horizontally.
- Extra rows expand upward, keeping the panel's bottom edge aligned with the right utility panel and clear of the always-visible lower status strip.
- The left tray hit-test uses the same calculated icon positions as rendering.

## Verification performed

- `bash -n scripts/launcher.sh` passed.
- A synthetic DBus-activatable desktop-entry test confirmed that the temporary entry is rewritten to `DBusActivatable=false`, toolkit scale variables are absent from the `gtk-launch` process, and URI arguments are preserved.
- Cargo compilation and Rust tests were not run because Cargo is unavailable in the build environment.
