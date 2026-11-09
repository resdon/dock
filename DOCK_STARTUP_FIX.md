# Dock startup visibility fix

This update removes the startup race between the first draw and the initial
`zwlr_layer_surface_v1.configure` event.

After creating and committing each layer surface, initialization now performs
a Wayland roundtrip and applies the queued `LayerConfigured` events before it
returns to the main loop. This ensures the first normal render sees a
configured surface instead of silently skipping it and losing its initial
redraw request.

The layer configure handler also preserves the requested dimensions when the
compositor reports zero for a dimension (meaning the client-requested size is
accepted). Startup diagnostics now report:

- detected output count;
- layer surface creation per output;
- configure event and negotiated dimensions;
- configured surface count after the initialization roundtrip;
- first committed framebuffer dimensions.

Expected startup progression:

```text
[DOCK] Wayland reports 1 output(s)
[DOCK] Creating layer surface for output ...
[DOCK] Layer configure received: ...
[DOCK] Surface configured: ...
[DOCK] Initial layer configure complete: 1/1 surface(s) configured
[DOCK] First framebuffer committed: ...
```

If the output count is zero, no layer surface can be created. If the configure
count remains zero after the roundtrip, the compositor did not configure the
new layer surface and the diagnostic output identifies that stage.


## First-frame rendering correction

The status-bar build initialized closed popup fades at alpha 1.0, then the first
interaction update hid them. That manufactured a popup-only fade-out before any
popup existed. Because popup-only frames intentionally skip the expensive main
dock render, the first dock buffer could be skipped and the final invalidation
consumed. Closed hover/context-menu/window-list fades now start at alpha 0.0
through `FadeAnimation::new_hidden()`. The render loop also forces a full dock
frame for any configured surface that has no committed buffer yet, independent
of popup animation state.

Added unit tests verifying hidden fades start idle and can be shown normally.
