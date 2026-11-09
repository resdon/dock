# Dock width and tray/status alignment

- The layer-shell width is now driven by the pinned application row unless the compact status strip genuinely needs more room. Ten pinned apps request 576 logical pixels instead of 656.
- SNI icons stay in a compact panel at the left of the top status band.
- Keyboard layout and network rates are grouped on the right, immediately before the right-aligned date/time group.
- The status strip no longer reserves half the dock for a left-of-centre cluster.
- The startup width remains fixed after layer-shell creation; the render loop does not trigger configure/SHM allocation churn.
- Set `DOCK_TRAY_DEBUG=1` to log the exact item references returned by the StatusNotifierWatcher. This distinguishes a client that never registered (for example, qBittorrent's tray integration is disabled) from one that registered but could not be loaded.
- Tray action logs now say `acknowledged` only after the SNI method reply arrives. The previous `call_noreply` path only confirmed queueing a message, not that the item accepted the action.

The current shared context menu still exposes SNI `Activate`, `SecondaryActivate`, and `ContextMenu` method calls. Full `com.canonical.dbusmenu` tree parsing and menu-item activation is not implemented yet, so applications that publish their actual actions only through that interface need that integration before their native menu actions can work.


## Overlay behavior (no work-area reservation)

- The dock remains on `Layer::Top` and is anchored to the bottom edge.
- Its layer-shell exclusive zone is always `0`, including while visible.
- Auto-hide only changes opacity and the input region; it never changes the
  exclusive zone. Normal windows therefore keep their original work area and
  can extend behind the dock.
