# PipeWire audio tray widget

The dock's speaker button opens an in-dock quick mixer backed by the current
user's PipeWire session and WirePlumber policy manager.

## Controls

- Output: master volume, mute, and up to three detected playback devices.
- Input: capture volume, mute, and up to three detected recording devices.
- Application volume: draggable volume and mute controls for up to three active
  audio stream nodes in the compact panel.
- Full mixer: optionally opens `pavucontrol` when installed. Its PulseAudio
  compatibility connection requires `pipewire-pulse`; the in-dock controls do
  not.

Selecting an output or input row makes it the WirePlumber default for new
streams. Existing streams keep their current routing unless their application
reconnects or the user changes routing in the full mixer.

## Runtime requirements

- PipeWire running for the logged-in user.
- WirePlumber running as the PipeWire session manager.
- `wpctl` available in `PATH` (provided by WirePlumber).
- `pavucontrol` is optional; the in-dock controls do not depend on it.

No additional Rust crate or PulseAudio server is required. The dock invokes
`wpctl` from a dedicated worker thread, never from the Wayland render thread.
The worker publishes immutable snapshots to the UI and polls at a 1.5 second
interval. Slider updates are coalesced off the render thread, while mute and
device-selection commands are sent asynchronously.

## Quick checks

Run these from the same desktop session as the dock:

```sh
wpctl status -n
wpctl list audio sinks
wpctl list audio sources
wpctl get-volume @DEFAULT_SINK@
wpctl get-volume @DEFAULT_SOURCE@
```

If the widget says audio is unavailable, verify that `wpctl` is on the dock's
`PATH` and that the user session can access the PipeWire socket. Action errors
are logged with the `[AUDIO]` prefix.
