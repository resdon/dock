# System status strip

The top 38 logical pixels of the main dock surface contain the tray and compact system indicators, followed by a 5px gap and the normal 60px application dock.

Layout from left to right:

1. Registered SNI tray icons at the left edge.
2. Audio and Bluetooth launchers for the PipeWire mixer and BlueZ device panel.
3. Keyboard layout badge and network rates (download/RX and upload/TX).
4. Date immediately left of the time; the date/time pair is right-aligned to the dock surface.

`src/app/system_bar.rs` samples `/proc/net/dev` on a background thread every two seconds. It excludes loopback and sends a new snapshot only when displayed text changes. Keyboard-layout discovery is refreshed every eight seconds using available compositor/session tools (Hyprland, Sway, or `setxkbmap`) with static system configuration as a fallback. No process or filesystem I/O occurs in the renderer.

The left cluster reserves a fixed network-label width so changing throughput numbers do not resize the Wayland surface. Date and clock width are also reserved at startup to avoid a surface resize when the first snapshot arrives.


## Surface sizing and first presentation

The layer-shell width is calculated once at startup from the current application
row and a reserve for three tray items. For ten pinned apps this chooses 656
logical pixels instead of always reserving the full 800px maximum. The main
application row remains centered. The tray/keyboard/network group ends just left
of center, and date/time stays independently right-aligned.

The main dock framebuffer is committed first. A per-surface
`status_strip_rendered` flag then forces the tray/status frame on the next draw,
even if a popup-only animation pass would otherwise skip the main surface. Any
change to the tray snapshot or system-bar snapshot invalidates that flag so the
status band is repainted. Tray items with missing/transparent icon data receive
a small identifier tile rather than silently disappearing.
