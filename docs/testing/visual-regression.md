# WAYDOCK — Visual Regression Testing Guide

This document describes how to set up and run visual regression tests to catch rendering regressions after backend changes.

## Overview

After replacing the rendering backend, visual regression testing is essential to verify that all UI elements render correctly. GPU drivers and font rasterization can produce slight differences, so tests use tolerance-based comparison rather than exact pixel equality.

## Reference Screenshots

Create reference screenshots for these scenarios:

| Scenario | Description |
|----------|-------------|
| Default dock | Dock at rest, no hover, no popups |
| Hovered icon | Pointer hovering over a dock icon |
| Magnified dock | Dock with icon magnification active |
| Open popup | Context menu or window list popup open |
| Transparent background | Dock with transparency effects |
| Text rendering | Dock with text labels visible |
| Gradients | Dock with gradient backgrounds |
| Shadows | Dock with shadow effects |
| Blur | Dock with blur effects (if enabled) |
| Bottom orientation | Dock at the bottom of the screen |
| Top orientation | Dock at the top of the|
| Left orientation | Dock on the left side |
| Right orientation | Dock on the right side |

## Screenshot Capture

### Manual Capture

```bash
# Run the dock
cargo run &

# Capture screenshot using grim (Wayland screenshot tool)
grim dock_screenshot.png

# Or capture a specific region
grim -g "800,700,800,100" dock_region.png
```

### Automated Capture

Use a script to capture all reference scenarios:

```bash
#!/usr/bin/env bash
# scripts/capture_references.sh

SCENE_DIR="tests/visual/references"
mkdir -p "$SCENE_DIR"

# Start dock with known configuration
cargo run -- --config tests/visual/test_config.json &
DOCK_PID=$!

sleep 2  # Wait for dock to initialize

# Capture default dock
grim "$SCENE_DIR/default_dock.png"

# Simulate hover (requires Wayland input simulation)
# grim "$SCENE_DIR/hovered_icon.png"

# Capture each orientation
# ... (modify config and restart)

kill $DOCK_PID
```

## Screenshot Comparison

### Using ImageMagick

```bash
# Install: sudo apt install imagemagick

# Compare two images with tolerance
compare -fuzz 5% reference.png actual.png difference.png

# Get numeric difference
diff_result=$(compare -metric AE -fuzz 5% reference.png actual.png null: 2>&1)
echo "Pixel differences: $diff_result"

# If differences < threshold, test passes
if [ "$diff_result" -lt 100 ]; then
    echo "PASS: Visual regression test passed"
else
    echo "FAIL: Too many pixel differences"
    exit 1
fi
```

### Using Rust (Custom Comparison)

```rust
use image::{GenericImageView, ImageBuffer, Rgba};

fn compare_images(
    reference: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    actual: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    tolerance: u8,
) -> (u32, u32) {
    let (ref_width, ref_height) = reference.dimensions();
    let (act_width, act_height) = actual.dimensions();

    if ref_width != act_width || ref_height != act_height {
        return (0, ref_width * ref_height); // Size mismatch
    }

    let mut diff_count = 0;
    for pixel in actual.pixels() {
        let (x, y) = pixel.position();
        let ref_pixel = reference.get_pixel(x, y);

        let r_diff = (pixel[0] as i32 - ref_pixel[0] as i32).unsigned_abs();
        let g_diff = (pixel[1] as i32 - ref_pixel[1] as i32).unsigned_abs();
        let b_diff = (pixel[2] as i32 - ref_pixel[2] as i32).unsigned_abs();
        let a_diff = (pixel[3] as i32 - ref_pixel[3] as i32).unsigned_abs();

        if r_diff > tolerance || g_diff > tolerance || b_diff > tolerance || a_diff > tolerance {
            diff_count += 1;
        }
    }

    (diff_count, ref_width * ref_height)
}
```

## Tolerance Settings

Use different tolerances for different scenarios:

| Scenario | Tolerance | Reason |
|----------|-----------|--------|
| Geometry (rectangles, shapes) | 2% | GPU rendering should be exact |
| Text rendering | 5% | Font rasterization varies by driver |
| Gradients | 3% | Color interpolation may vary slightly |
| Shadows/blur | 8% | Effect algorithms may differ |
| Icons | 3% | Image decoding should be consistent |

## Test Workflow

1. **Create reference screenshots** — Capture all scenarios with the current rendering
2. **Store references** — Commit to `tests/visual/references/`
3. **Run comparison** — After changes, capture new screenshots and compare
4. **Review differences** — Inspect `difference.png` for actual regressions
5. **Update references** — If changes are intentional, update reference screenshots

## Limitations

- **GPU driver differences**: Different GPUs may produce slightly different results
- **Font rasterization**: FreeType version differences affect text rendering
- **Anti-aliasing**: MSAA settings affect edge rendering
- **Color profiles**: Display color profiles may affect captured screenshots

## Tools Summary

| Tool | Purpose | Installation |
|------|---------|-------------|
| `grim` | Wayland screenshot capture | `sudo apt install grim` |
| `ImageMagick` | Image comparison | `sudo apt install imagemagick` |
| `cargo-fuzz` | Fuzzing for edge cases | `cargo install cargo-fuzz` |
| Custom Rust comparison | Integration with test suite | Built into project |
