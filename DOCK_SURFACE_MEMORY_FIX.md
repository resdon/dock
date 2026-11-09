# Dock surface sizing and SHM growth fix

## Root cause

The layer surface was created with a 540 px logical width, but the render pass recalculated a content width (for example 624 px) and called `LayerSurface::set_size()` on every redraw. Layer-shell size changes are configure-driven. Changing size during rendering caused configure/redraw churn while each pass allocated and attached another SHM buffer. This could leave the surface without a stable configured/buffer size and steadily grow the SHM pool.

## Changes

- Request one stable 800 x 103 logical layer surface during initialization.
- Keep the 800 px width (or the compositor-negotiated width) for the entire surface lifetime.
- Never call `LayerSurface::set_size()` from `AppState::draw()`.
- Allocate physical buffers from the configured logical width/height and scale.
- Add stage diagnostics for SHM allocation, main icon rendering, status/tray rendering, and buffer attach/commit.

## Expected startup diagnostics

```text
[DOCK] Layer configure received: ..., size=800x103
[DOCK] Surface configured: 800x103 logical
[DOCK] Rendering first full frame: 800x103 logical
[DOCK] SHM buffer allocated: 800x103 physical, ... bytes
[DOCK] Main icon/background render complete
[DOCK] Tray/status render complete
[DOCK] Attaching frame: buffer=800x103, configured_surface=800x103
[DOCK] First framebuffer committed: 800x103 physical, 800x103 logical
```

If the compositor clamps the width, the configure dimensions are authoritative; the renderer now uses those dimensions without issuing another size request.
