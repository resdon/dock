# GPU Rendering Code Review — Findings and Validation Status

## Resource and Functional Issues

### 1. Per-draw GPU buffer allocation churn (renderer.rs:1037-1045, 1083-1091, 1149-1157)
**File:** `wgpu-api/src/renderer.rs`  
**Lines:** 1037-1045 (Rectangle), 1083-1091 (Image), 1149-1157 (Video)

Every draw call creates a short-lived `wgpu::Buffer` via `device.create_buffer()` and uploads vertices via `queue.write_buffer()`. The buffer handle is RAII-managed and is dropped after the draw function returns; wgpu retains submitted resources only as long as required by in-flight GPU work. This is **not an unbounded buffer leak**, though per-draw allocations may cause avoidable CPU/driver overhead.

```rust
// Line 1037 — called for EVERY rectangle, every frame
let buffer = backend.device().create_buffer(&wgpu::BufferDescriptor { ... });
backend.queue().write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));
```

**Impact:** Resource-creation churn, not proven linear VRAM growth.

**Status:** The redraw-loop bug was fixed in `src/app/draw.rs`, reducing unnecessary frame submissions. DMA-BUF `wl_buffer`s are destroyed after compositor release in `src/listeners/wayland/core.rs`. Consider batching/reusing vertex buffers if profiling shows this churn is still significant.

---

### 2. Coordinate space mismatch between dock.rs and renderer.rs (dock.rs:376 vs dashed_block.rs:239)
**File:** `src/app/dock.rs` line 376 vs `src/app/dashed_block.rs` line 239

- **dock.rs** calls `render_to_dmabuf(phys_width, phys_height, &draw_commands)` which internally calls `render_to_dmabuf_scaled(..., 1.0, ...)` — scale factor is **1.0**.
- **dashed_block.rs** calls `render_to_dmabuf_scaled(phys_width, phys_height, scale as f32, &draw_commands)` — scale factor is the **actual scale**.

In `draw_commands_to_gpu_impl`, coordinates are multiplied by `coordinate_scale` (line 1028-1031). This means:
- **dock.rs**: Draw commands use **physical pixel** coordinates (already scaled by dock.rs), but `coordinate_scale=1.0` means no additional scaling. This is correct.
- **dashed_block.rs**: Draw commands use **logical pixel** coordinates (unscaled), and `coordinate_scale=scale` applies scaling. This is also correct.

**However**, the inconsistency means dock.rs and dashed_block.rs use different coordinate conventions. If dock.rs ever switches to `render_to_dmabuf_scaled`, the coordinates will be double-scaled.

---

### 3. Rounded rectangle corners ignored (renderer.rs:1023)
**File:** `wgpu-api/src/renderer.rs` line 1023

```rust
radius: _radius,
stroke: _stroke,
stroke_width: _stroke_width,
```

The `radius`, `stroke`, and `stroke_width` fields of `DrawCommand::Rectangle` are **completely ignored** in the GPU path. The dock background has `border_radius` set, and the border has `stroke` and `stroke_width` — none of these render on GPU.

**Impact:** Dock background has sharp corners on GPU (not rounded pill shape). Border stroke is invisible. Running indicator dashes have `radius: 0.0` so they render as sharp rectangles, not rounded pills.

**Status:** Confirmed and not yet fixed.

---

### 4. Texture format mismatch — RGBA→BGRA conversion (renderer.rs:295-299)
**File:** `wgpu-api/src/renderer.rs` lines 295-299

```rust
let bgra_pixels: Vec<u8> = pixels
    .chunks_exact(4)
    .flat_map(|rgba| [rgba[2], rgba[1], rgba[0], rgba[3]])
    .collect();
```

The icon cache stores RGBA pixels. The texture is created with `Bgra8UnormSrgb` format. The conversion swaps R↔B. However, the DMA-BUF render target is also `Bgra8UnormSrgb`. The fragment shader samples the texture as `texture_2d<f32>` which returns the BGRA values. Since the render target is also BGRA, the colors should be correct.

**Status:** Verified. The cached upload path delegates to `upload_texture_from_pixels`, so both use the same RGBA→BGRA conversion. The video upload path uses that conversion before uploading to a BGRA texture.

---

### 5. `prepare_texture_bind_groups` called before render pass (renderer.rs:937)
**File:** `wgpu-api/src/renderer.rs` line 937

```rust
Self::prepare_texture_bind_groups(backend, commands);
```

This pre-creates all texture bind groups before the render pass. This is correct — bind groups must be created outside the render pass. However, `get_texture_bind_group` (line 683) is a `&self` method that only returns **already prepared** bind groups. If a texture was uploaded but `prepare_texture_bind_groups` didn't find it in the commands, the bind group won't exist.

**Edge case:** If `upload_texture_from_pixels_cached` is called but the texture isn't used in any draw command, the bind group is never prepared. This is fine — unused textures don't need bind groups.

---

### 6. `backend` borrow issue in `draw_commands_to_gpu_impl` (renderer.rs:1004)
**File:** `wgpu-api/src/renderer.rs` line 1004

```rust
fn draw_commands_to_gpu_impl(
    render_pass: &mut wgpu::RenderPass<'_>,
    backend: &GpuBackend,  // ← immutable borrow
```

The `backend` is `&GpuBackend` (immutable), but `get_texture_bind_group` is `&self`. This is fine. However, `device()` and `queue()` are also `&self` methods. The per-frame buffer allocation (issue #1) uses `backend.device().create_buffer()` which works with immutable borrow.

---

### 7. Dashed block uses logical pixel coordinates for GPU (dashed_block.rs:88-132)
**File:** `src/app/dashed_block.rs` lines 88-132

The dashed border rectangles use `block_width` and `block_height` (logical pixels) as coordinates. The `render_to_dmabuf_scaled` call passes `scale as f32` as the scale factor. The GPU renderer multiplies coordinates by scale. This means the dashed border is correctly scaled.

**However**, the icon positions (line 139-143) also use logical pixels:
```rust
let icons_start_x = (w - row_width) / 2;  // logical pixels
let row_start_x = icons_start_x + row_offset;
```

These are passed to `DrawCommand::Image` as `dest_rect` in logical pixels. The `render_to_dmabuf_scaled` with `scale` factor should scale them correctly.

---

### 8. Text rendering in dashed_block uses logical font size (dashed_block.rs:231-237)
**File:** `src/app/dashed_block.rs` lines 231-237

```rust
draw_commands.push(wgpu_api::DrawCommand::Text {
    position: wgpu_api::Point::new(kb_text_x, kb_text_y),
    font_size: kb_font_size,  // ← logical pixels
```

The `font_size` is in logical pixels. In `draw_commands_to_gpu_impl` (renderer.rs:1120), it's multiplied by `coordinate_scale`:
```rust
font_size: *font_size * coordinate_scale,
```

This is correct for `render_to_dmabuf_scaled` with scale factor.

---

### 9. `render_to_dmabuf` delegates to `render_to_dmabuf_scaled` with scale=1.0 (renderer.rs:912)
**File:** `wgpu-api/src/renderer.rs` line 912

```rust
pub fn render_to_dmabuf(...) -> Result<...> {
    self.render_to_dmabuf_scaled(width, height, 1.0, commands)
}
```

dock.rs calls `render_to_dmabuf` (not `_scaled`), which means `coordinate_scale=1.0`. Since dock.rs already scales all coordinates to physical pixels, this is correct. But it means the text rendering path in `draw_commands_to_gpu_impl` won't scale text font sizes, which is correct for dock.rs but would be wrong if dock.rs ever passed logical pixel coordinates.

---

### 10. Texture cache key collision risk (dock.rs)
**File:** `src/app/dock.rs` line 154

```rust
let cache_key = format!("dock-icon:{}", pin.app_id);
```

The cache key now includes the app ID and icon dimensions, preventing different-sized icons from sharing a cache entry. Replacing pixel data at the same dimensions still requires explicit cache invalidation.

**Status:** Fixed for dimension collisions.

---

### 11. Animation state uses `static` mutexes (dock.rs:123-125)
**File:** `src/app/dock.rs` lines 123-125

```rust
static GPU_BOUNCE_ANIMATIONS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, u32>>> = ...;
```

These static mutexes are shared across all dock instances and all render calls. If multiple docks exist, they share animation state. The mutex lock/unlock pattern on every frame for every pin is also a performance concern.

---

### 12. `upload_video_frame` doesn't upload to GPU (renderer.rs)
**File:** `wgpu-api/src/renderer.rs` lines 1264-1275

The previous implementation converted frames but did not register or upload the pixel data. The current implementation registers a BGRA GPU texture per stream, uploads converted frame pixels, reuses the texture while dimensions remain unchanged, and resolves the same stable texture ID when drawing video nodes.

**Status:** Fixed. A targeted test covers RGBA-to-BGRA conversion.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 1 | **Low** | Per-draw GPU buffer allocation churn | Avoidable CPU/driver overhead |
| 3 | **High** | Rounded corners, stroke ignored | Visual mismatch: sharp corners, no border |
| 12 | **High** | Video frame data never uploaded | Video renders as blank |
| 10 | **Medium** | Texture cache key collision | Wrong icon size for same app |
| 2 | **Medium** | Inconsistent coordinate conventions | Risk of double-scaling if API changes |
| 11 | **Low** | Static mutex animation state | Shared state across docks, perf concern |
| 4 | **Low** | RGBA→BGRA conversion | May be correct, needs verification |
| 5 | **Low** | Bind group preparation edge case | Unused textures don't need bind groups |
| 6 | **Info** | Immutable borrow pattern | Works correctly as-is |
| 7 | **Info** | Logical pixel coordinates | Correctly scaled by render_to_dmabuf_scaled |
| 8 | **Info** | Text font size scaling | Correctly scaled by coordinate_scale |
| 9 | **Info** | render_to_dmabuf delegates to _scaled | Correct for current usage |

## Validation update

The buffer-allocation finding was overstated as a leak; wgpu buffer handles are dropped normally. Video upload and icon-size cache collisions have been fixed. The RGBA-to-BGRA conversion is shared by the cached upload path and video path. Rounded rectangle corners and rectangle strokes remain unsupported in the GPU renderer.
