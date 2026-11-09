# Tray/display fixes

- Icon loader now keeps canonical RGBA pixels; the previous red/blue swap caused main-dock icons to render with swapped channel colors.
- Icon cache format is versioned so legacy channel-swapped caches are ignored and refreshed.
- WezTerm aliases such as `wezterm-gui` are canonicalized to the pinned `org.wezfurlong.wezterm` item.
- The dock layer surface reserves the tray band from the initial configure (103 logical pixels: 38 tray + 5 gap + 60 dock), avoiding a late height change that can leave the tray clipped.
- SNI pixmaps with zero alpha are rejected so the loader falls back to icon-name/ID resolution.
- Tray item tiles receive a subtle background to keep dark/monochrome status icons visible.
