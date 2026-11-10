# WAYDOCK — GPU Validation Guide

This document describes how to validate GPU rendering correctness, detect Vulkan API misuse, and profile GPU performance.

## wgpu Error Handling

### Development Mode

Enable wgpu validation during development. Set these environment variables:

```bash
# Enable wgpu validation
export WGPU_VALIDATION=1
# Enable Vulkan validation layers
export VK_LAYER_PATH=/usr/share/vulkan/explicit_layer.d
export VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation
```

### Code-Level Error Scopes

Use `wgpu::Device::push_error_scope()` around resource creation to catch GPU errors:

```rust
// Example: Wrap texture creation in error scope
device.push_error_scope(wgpu::ErrorFilter::Validation);
let texture = device.create_texture(&wgpu::TextureDescriptor { /* ... */ });
// Poll for errors
pollster::block_on(async {
    if let Some(error) = device.pop_error_scope().await {
        eprintln!("GPU error: {:?}", error);
    }
});
```

### Uncaptured Error Handler

Set a global handler for uncaught GPU errors:

```rust
wgpu::Instance::on_uncaptured_error(|error| {
    eprintln!("Uncaptured GPU error: {:?}", error);
});
```

### Device Lost Callback

Handle GPU device loss gracefully:

```rust
device.on_lost_callback(|reason, message| {
    eprintln!("GPU device lost: {} — {}", reason, message);
    // Trigger recovery or graceful shutdown
});
```

## Vulkan Validation Layers

### Installation

```bash
# Ubuntu/Debian
sudo apt install vulkan-validationlayers

# Arch Linux
sudo pacman -S vulkan-validation-layers

# Verify installation
vulkaninfo | grep "VK_LAYER_KHRONOS_validation"
```

### Configuration

Create `~/.local/share/vulkan/implicit_layer.d/vulkan-validation-layers.json`:

```json
{
    "file_format_version": "1.0.0",
    "layer": {
        "name": "VK_LAYER_KHRONOS_validation",
        "enable": true,
        "disable": false
    }
}
```

### What Validation Layers Detect

- Invalid resource lifetimes (use-after-free textures/buffers)
- Synchronization errors (missing barriers)
- Invalid pipeline state
- Shader validation errors
- Memory binding issues
- Command buffer misuse

**Never disable validation during development** just to make error messages disappear.

## RenderDoc

### Installation

Download from [renderdoc.org](https://renderdoc.org/)

```bash
# Extract and run
tar -xzf renderdoc-*.tar.gz
./renderdoc-*/renderdoc
```

### Capturing Frames

1. Launch RenderDoc
2. Click "Capture" or set `RENDERDOC_CAPTURE=1`
3. Run the dock: `cargo run`
4. Interact to trigger the rendering you want to inspect
5. Stop capture in RenderDoc

### What to Inspect

| Category | What to Check |
|----------|---------------|
| Draw Calls | No redundant draws, proper instancing |
| Shaders | Correct WGSL compilation, no warnings |
| Textures | Proper format, no unnecessary mipmaps |
| Render Passes | Minimal passes, no redundant clears |
| Blending | Correct alpha compositing |
| Buffers | Proper alignment, no wasted space |
| Pipeline State | Cached pipelines, no recreation |

## MangoHud

### Installation

```bash
# Ubuntu/Debian
sudo apt install mangohud

# Arch Linux
sudo pacman -S mangohud
```

### Usage

```bash
# Run dock with MangoHud overlay
MANGOHUD=1 cargo run

# Configure what to show
MANGOHUD=1 MANGOHUD_OVERLAP=1 MANGOHUD_OUTPUT_TO_FILE=0 cargo run
```

### Metrics Displayed

- FPS (frames per second)
- Frame time (ms)
- GPU load (%)
- CPU load (%)
- GPU memory usage
- CPU memory usage
- GPU temperature (if available)

## GPU Performance Targets

| Metric | Target | Measurement Method |
|--------|--------|-------------------|
| Idle GPU usage | Near 0% | MangoHud |
| Animation FPS | ≥60 | MangoHud |
| Frame time | ≤16.67ms | MangoHud, Tracy |
| VRAM usage | Bounded | `nvtop` or `radeontop` |
| Render passes | Minimal | RenderDoc |
| Texture uploads | Zero per-frame | RenderDoc |
| GPU readback | None for UI state | RenderDoc |

## GPU Resource Lifetime Tests

Test these scenarios to verify resource management:

1. **Texture creation and removal** — Create 100 textures, remove 50, verify no leak
2. **Pipeline cache** — Create pipelines, verify they're cached on reuse
3. **Buffer reuse** — Verify dynamic buffers are reused, not reallocated per-frame
4. **Surface resize** — Resize surface 10 times, verify old resources released
5. **Device loss simulation** — If feasible, simulate device loss and verify recovery

## Shader Validation

All WGSL shaders must compile without warnings:

```bash
# Check shader compilation at runtime
# The wgpu-api backend compiles shaders on init; check logs for warnings
cargo run 2>&1 | grep -i "shader"
```

## Tools Summary

| Tool | Purpose | When to Use |
|------|---------|-------------|
| wgpu error scopes | Catch GPU errors at creation time | During development |
| Vulkan validation layers | Detect API misuse | Always during development |
| RenderDoc | Frame capture and inspection | Debugging rendering issues |
| MangoHud | Real-time FPS and GPU metrics | Performance testing |
| Tracy | CPU/GPU timeline correlation | Frame timing analysis |
| `nvtop`/`radeontop` | GPU memory monitoring | VRAM budget verification |
