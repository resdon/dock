# Audio and side box update

- A persistent vinyl-style audio player now appears in a compact record-and-controls state beside the dock and expands while hovered. It matches the separate HTML preview's yellow surface, red transport accent, oversized silver-centered disc, metadata, seek track, transport row, and output-level display. With `playerctl` installed and an active MPRIS player, the transport controls work; without one, the card falls back to an active PipeWire stream and disables media transport.
- The four utility widgets occupy their own 76×72 rounded box to the right of the tray app box. Their existing popups remain anchored to their buttons.
- StatusNotifier tray icons are positioned in a separate rounded box immediately to the right of the app row and left of the utility widgets.
- New profiles receive AmneziaVPN, Steam, and qBittorrent dock pins. Their desktop IDs are `AmneziaVPN`, `steam`, and `org.qbittorrent.qBittorrent`; running windows reported under binary names are mapped back to those pins. A saved pin list remains authoritative, so users can remove any through the existing dock context menu.
- Running app context menus already provide Minimize and Quit Application actions.

The supplied reference depicts a media catalog/player. This dock shows the active local MPRIS session and PipeWire streams; it does not provide a broadcaster catalog or cover art feed, and it does not fabricate tracks.
