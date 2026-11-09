# Bluetooth tray widget

The Bluetooth glyph in the status strip opens a compact, dock-native device
panel. The panel is 220 x 240 logical pixels and has a minimize control;
minimizing it only hides the panel and does not change adapter or device state.

## Controls

- **Power**: turn the selected BlueZ adapter on or off.
- **Scan devices / Stop scan**: start or release the dock's own discovery
  session. If another program (for example, `bluetoothctl`) is already scanning,
  the panel shows **Scan also** rather than claiming ownership of that scan.
- **Pair**: pair a newly discovered device.
- **Connect / Disconnect**: connect or disconnect a paired device.
- Connected devices are sorted first, followed by paired and then newly
  discovered devices. Up to four devices are shown in the compact panel.

## Runtime requirements

- BlueZ (`bluez`) and its system D-Bus service (`bluetooth.service`) must be
  running. On Arch Linux, `bluetoothctl` comes from `bluez-utils`.
- Pairing requires an active BlueZ agent to handle confirmation or passkey
  prompts. A running interactive `bluetoothctl` session after `agent on`, or a
  desktop Bluetooth agent such as Blueman, can provide that agent. The dock
  invokes BlueZ's `Device1.Pair` API but does not implement a separate PIN/passkey
  dialog.
- No additional Rust dependency is needed; this uses the project's existing
  `zbus` dependency and the BlueZ system bus API.

## Troubleshooting

Check the adapter and service with:

```sh
systemctl status bluetooth.service
bluetoothctl show
bluetoothctl devices
bluetoothctl devices Connected
```

If the panel says BlueZ is unavailable, inspect the dock log for `[BLUETOOTH]`
messages and confirm that the user session can access the system D-Bus. If pair
operations fail with an agent-related error, start or enable a desktop BlueZ
agent before pairing.
