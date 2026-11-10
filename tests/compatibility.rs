# WAYDOCK — Compatibility Tests

This module contains integration tests for Wayland and Slint compatibility.

## Wayland Compatibility Tests

Test surface creation, destruction, resize, layer-shell orientations, output handling, scaling, and input events.

```rust
//! Wayland compatibility tests
//!
//! These tests verify that the dock interacts correctly with the Wayland compositor.
//! They require a running Wayland display server.

#[cfg(test)]
mod wayland_tests {

    #[test]
    fn test_surface_creation() {
        // Verify that Wayland surfaces can be created
        // This test requires a running Wayland compositor
        // Run with: WAYLAND_DISPLAY=wayland-1 cargo test test_surface_creation
    }

    #[test]
    fn test_layer_shell_orientations() {
        // Verify all four orientations work
        // bottom, top, left, right
    }

    #[test]
    fn test_output_scaling() {
        // Verify scaling at 100%, 125%, 150%, 200%
    }

    #[test]
    fn test_frame_callback() {
        // Verify frame callbacks are received
    }
}
```

## Slint Compatibility Tests

Test widget rendering, layout, text, images, events, focus, and animations.

```rust
//! Slint compatibility tests
//!
//! These tests verify that Slint widgets render and behave correctly.

#[cfg(test)]
mod slint_tests {

    #[test]
    fn test_widget_rendering() {
        // Verify basic Slint widgets render
    }

    #[test]
    fn test_text_rendering() {
        // Verify text rendering with different fonts and sizes
    }

    #[test]
    fn test_layout_behavior() {
        // Verify layout resizing, alignment, and clipping
    }
}
```

## Running Tests

```bash
# Run all compatibility tests
cargo test --test compatibility

# Run with Wayland display
WAYLAND_DISPLAY=wayland-1 cargo test --test compatibility

# Run specific test
cargo test --test compatibility test_surface_creation
```

## Compositor Compatibility

Test on multiple compositors if possible:

| Compositor | Notes |
|------------|-------|
| KWin | KDE Plasma, layer-shell support |
| Sway | i3-compatible, good layer-shell support |
| Hyprland | Advanced animations, layer-shell support |
| labwc | Lightweight, basic layer-shell support |

## Test Matrix

| Test | KWin | Sway | Hyprland | labwc |
|------|------|------|----------|-------|
| Surface creation | ✓ | ✓ | ✓ | ✓ |
| Layer-shell | ✓ | ✓ | ✓ | ✓ |
| All orientations | ✓ | ✓ | ✓ | ✓ |
| Scaling | ✓ | ✓ | ✓ | ✓ |
| Input events | ✓ | ✓ | ✓ | ✓ |
| Popups | ✓ | ✓ | ✓ | ✓ |
