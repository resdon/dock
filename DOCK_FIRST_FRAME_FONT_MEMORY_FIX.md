# First-frame and font-fallback memory fix

## Symptoms addressed

The first full dock draw reached the status-strip renderer and stopped after Fontconfig selected a fallback face for the download arrow glyph. Since the dock buffer is committed only after status-strip rendering, a slow or memory-heavy fallback operation could prevent the main dock from ever being presented.

## Changes

- The first configured frame now commits the main dock (background, pinned/running icons, indicators) without optional status-strip drawing.
- The draw loop schedules one follow-up frame to paint the tray/status strip after the base dock is already visible.
- Network direction arrows are rasterized as tiny vector shapes instead of Unicode font glyphs. The download/upload labels are plain ASCII, so the status strip does not trigger Fontconfig fallback for arrows.
- Fallback glyph extraction no longer caches up to eight parsed faces per font collection. It parses one face at a time, rasterizes only the requested glyph, and releases the face before trying another collection index.
- Optional `DOCK_RENDER_DEBUG=1` diagnostics identify tray panel, keyboard, network, date/time, and icon render stages.

## Expected first-frame sequence

```text
[DOCK] Rendering first full frame: ...
[DOCK] SHM buffer allocated: ...
[DOCK] Main icon/background render complete
[DOCK] Deferring tray/status strip until base dock is committed
[DOCK] Attaching frame: ...
[DOCK] First framebuffer committed: ...
```

The follow-up frame then paints the tray and status strip. Set `DOCK_RENDER_DEBUG=1` to see per-stage status-strip diagnostics.
