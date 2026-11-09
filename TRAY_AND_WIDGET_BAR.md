# Tray, side widgets, and lower status strip

The dock is arranged as three horizontal regions, with the main application row centered:

- **Left box:** all native StatusNotifierItem icons (Steam, qBittorrent, and other registered tray apps), centered in a 76×72px rounded panel. It matches the right widget box for up to four icons and grows vertically in two-column rows for additional items.
- **Center:** pinned and running application icons.
- **Right box:** Rclone, Processes, Audio, and Bluetooth widgets.

The left and right boxes are independent Wayland subsurfaces. They follow the main dock's live app-row geometry but do not contribute to its width. The main surface is sized only for the app icons, so changing the number of tray icons or status widgets cannot stretch or shrink the central dock. Both side boxes use matching 76×72px rounded geometry for the common one-to-four tray-item case.

Keyboard layout, network throughput, and date/time are drawn in a separate slim panel directly below the application row. The bottom status panel has an empty pointer input region and is click-through.

Geometry (logical pixels):

- App row: 60 px high.
- Left SNI box: 76 × 72 px for up to four icons; extra icons wrap into further rows, 8 px from the app row's left edge.
- Right widget box: 76 × 72 px, 8 px from the app row's right edge.
- Lower system status strip: 34 px high, at least 360 px wide, centered below the app row and 6 px below its edge.

The parent layer surface remains anchored to the bottom and uses exclusive zone 0, preserving overlay behavior. Auto-hide and the narrow reveal strip remain on the parent surface; hidden sidecars have empty pointer regions.


## Current three-region arrangement

```text
[ SNI tray icons ]   [ Main application dock ]   [ Rclone | Processes | Audio | Bluetooth ]
                     [ Keyboard | Network RX/TX | Date | Time ]
```

The main app surface uses only the width required by the pinned/running app row
(plus its compact horizontal margins). The left and right boxes are sibling
subsurfaces and use matching 76×72px footprints while there are up to four SNI
icons. Additional tray icons wrap into further rows instead of stretching the
box horizontally. The lower status strip is
centered beneath the app row and has a minimum width so its fields remain readable
with only one or two dock icons.
The lower strip has an empty input region and is therefore click-through.
