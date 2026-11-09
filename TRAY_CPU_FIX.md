# Tray idle-CPU fix

The tray listener now avoids repeatedly probing both StatusNotifierItem interfaces when an item has already been identified.

Changes:
- Refresh interval increased from 1.2 s to 10 s.
- Successful snapshots are sent to the Wayland/main thread only when their actual contents change.
- Previously identified KDE/freedesktop item interfaces are tried first, avoiding the known failed interface query on every refresh.
- Transient D-Bus read failures preserve the last known tray item instead of removing it.
- No repeated successful-item diagnostics unless `DOCK_TRAY_DEBUG` is set.

This preserves tray functionality while removing the periodic D-Bus/property/icon churn visible in the runtime log.
