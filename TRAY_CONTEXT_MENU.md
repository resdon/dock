# Tray item context menu

Right-clicking an SNI icon opens the dock's shared context-menu popup, anchored at the selected tray icon. The menu offers:

- **Activate** — sends the item's SNI `Activate` method.
- **Secondary Activate** — sends `SecondaryActivate`.
- **Context Menu / Open Application Menu** — sends `ContextMenu` with the latest pointer position.

The popup uses the existing context-menu subsurface, fade, geometry, and input handling rather than adding another layer-shell surface. The target item is stored by its stable tray key, so menu actions do not depend on the item's current vector index.

Full `com.canonical.dbusmenu` layout/tree support is not included yet; the menu action requests the application's native SNI context menu where supported.
