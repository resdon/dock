# Tray mini-dock implementation

This build adds a StatusNotifierItem (SNI) tray mini-dock above the main dock.

## Included

- StatusNotifierHost registration on the session D-Bus.
- Uses an existing `org.freedesktop.StatusNotifierWatcher` or `org.kde.StatusNotifierWatcher` when available.
- Provides a minimal `org.freedesktop.StatusNotifierWatcher` service when no watcher exists, which is useful on bare wlroots-style sessions.
- Tracks SNI registrations from both freedesktop and KDE-compatible watchers.
- Loads tray icons from `IconName`/theme paths and falls back to `IconPixmap`, attention icons, overlay icons, and the item Id.
- Tray and status widgets share the existing main dock layer surface; no second tray layer-shell lifecycle is created.
- Supports clients that expose `org.freedesktop.StatusNotifierItem` or `org.kde.StatusNotifierItem`.
- Handles path-only registrations such as Steam's `/org/ayatana/NotificationItem/steam` by combining the registered object path with the D-Bus sender.
- Compact tray layout with hover state and attention indicator.
- Left click / middle click activation through SNI methods.
- Right-click opens a dock-native tray context menu with Activate, Secondary Activate, and the SNI ContextMenu action.
- Mouse wheel forwarding to the SNI `Scroll` method.
- Tray-specific input hit testing and pointer-coordinate mapping.
- Independent rendering/cache path so moving the tray does not force icon rerasterization.
- Popup/window-list/context-menu layering remains above the tray.

## Compatibility notes

Some applications, including Steam, register an object path instead of a fully qualified service name and expose their item through `org.kde.StatusNotifierItem`. The listener accepts both forms and probes both item interfaces. The StatusNotifierItem specification defines `IconName` and `IconPixmap` as the primary icon mechanisms.

## Current scope

This is the SNI layer. Full `com.canonical.dbusmenu` tree rendering for applications that expose a separate menu object is not yet implemented. The dock-native tray context menu provides common SNI actions, including an explicit action that sends the item's `ContextMenu` method.

XEmbed-only legacy tray applications are not included in this implementation because they use a different protocol from StatusNotifierItem.


## Current rendering model

The tray is rendered into the existing main dock surface. The surface reserves a 38px tray band plus a 5px gap above the normal 60px dock region. This keeps tray and dock in one Wayland coordinate space, avoids a second layer-shell configure lifecycle, and prevents tray-layer churn from interfering with the main dock.

## Main-dock integration

The tray panel and system status strip are rendered into the top band of the main dock layer surface. The surface reserves `38px` for the top strip plus a `5px` gap and the normal `60px` application dock. The left status cluster places tray icons to the left of the dock center, then keyboard layout and download/upload rates; date and time are independently right-aligned, with the date immediately left of the clock.

Running window IDs are canonicalized against pinned desktop IDs; Steam helper windows such as `steamwebhelper` are associated with the pinned `steam` item rather than creating a second dock icon.

## Tray context menu and system status strip

- Right-clicking a tray icon opens the dock's shared context-menu popup, anchored to that icon.
- The menu offers `Activate`, `Secondary Activate`, and `Context Menu` (or `Open Application Menu` for menu-style items). The final action sends the SNI `ContextMenu` method; full `com.canonical.dbusmenu` tree rendering remains future work.
- The status strip displays keyboard layout, RX/download and TX/upload rates, date, and time. Network counters are sampled off the Wayland thread; keyboard layout discovery is refreshed less frequently. The renderer receives only a cached `SystemBarSnapshot`.
- Main application icons are loaded into 512px canonical-RGBA cache sources when a theme/SVG source is available, then downsampled with bilinear filtering for the dock and drag preview. Cache format version 4 invalidates older low-resolution entries.
