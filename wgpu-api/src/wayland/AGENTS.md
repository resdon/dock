# Wayland Subsystem — Local Rules

## What This Subsystem Does

Provides a high-level abstraction over Wayland client protocols for creating windows, popups, and handling input events. Works with `labwc` (and other Wayland compositors) without implementing a compositor itself.

## Files

| File | Purpose |
|------|---------|
| `wayland.rs` | WaylandWindow, WaylandPopup, input events, WaylandConnection |

## Rules

1. **Dock does not implement compositor functionality.** Window management is delegated to the compositor.
2. **Wayland surfaces are managed by the compositor.** Dock creates and configures surfaces; the compositor positions and stacks them.
3. **Input events are delivered by the compositor.** Dock receives and processes them; it does not route them.
4. **Popup surfaces have parent relationships.** A popup is dismissed when its parent is closed.
5. **Distinguish visual popups from Wayland popups.** Visual popups are drawn inside a window; Wayland popups are separate surfaces.

## Responsibilities

| Responsibility | Owner |
|----------------|-------|
| Window positioning, stacking, decorations | **Compositor (labwc)** |
| Input routing | **Compositor (labwc)** |
| Rendering pixels inside windows | **Dock (wgpu-api)** |
| Scene graph management | **Dock (wgpu-api)** |

## Coordinate Spaces

- **Wayland surface coordinates**: Surface-relative, may differ from logical pixels due to scale factor
- **Logical pixels**: UI coordinates used by the scene graph
- **Conversion**: Viewport handles scale factor transformation

## Testing

Run under labwc. Check window creation, popup behavior, and input delivery.

## Safe to Add

- New window types
- New popup configurations
- New input event types
- New output info fields
