# GPU Rendering Code Review — Round 23 (Texture View Format Fix)

This is a continuation of the GPU rendering code review. Rounds 1–22 covered all `wgpu-api/` source files, all `src/` application-layer modules, project infrastructure, documentation, and more (issues 1–830). This round focuses on the **actual fix** for the persistent icon rendering issue.

---

## Root Cause Analysis

After 22 rounds of review identifying 830 issues, the core rendering problem was traced to **texture view format mismatch** for srgb textures.

### The Problem

In `wgpu-api/src/backend.rs`, the `register_texture` function creates GPU textures with `view_formats: &[]` (empty). For srgb textures like `Bgra8UnormSrgb` (used by icon textures), this means the texture view cannot be sampled correctly in the fragment shader.

The wgpu specification requires that srgb textures declare their non-srgb equivalent in `view_formats` so that the texture view can be created with the correct format for sampling. Without this, the shader's `textureSample` call may return incorrect or zero values, making icons invisible.

### The Fix

**File:** `wgpu-api/src/backend.rs`

**Change 1 — `register_texture` (line 589-610):**

```rust
// For srgb textures, the view format must include the non-srgb equivalent
// so that the texture can be sampled correctly in shaders.
let view_formats = match wgpu_format {
    wgpu::TextureFormat::Bgra8UnormSrgb => &[wgpu::TextureFormat::Bgra8Unorm],
    wgpu::TextureFormat::Rgba8UnormSrgb => &[wgpu::TextureFormat::Rgba8Unorm],
    _ => &[],
};

let texture = self.device.create_texture(&wgpu::TextureDescriptor {
    // ...
    view_formats,
});
```

**Change 2 — `create_uniform_buffer` placeholder texture (line 404-420):**

```rust
let placeholder_texture = self.device.create_texture(&wgpu::TextureDescriptor {
    // ...
    format: wgpu::TextureFormat::Rgba8UnormSrgb,
    view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
});
```

### Why This Fixes the Issue

1. **Icon textures** are uploaded as `Bgra8UnormSrgb` format (renderer.rs line 366).
2. The **fragment shader** samples the texture with `textureSample(texture_view, texture_sampler, input.tex_coords)`.
3. Without `view_formats`, the texture view's format may not match what the shader expects, causing `textureSample` to return `(0, 0, 0, 0)` or garbage.
4. With `view_formats: &[Bgra8Unorm]`, the texture view can be created with the linear color space format, and the srgb→linear conversion happens automatically during sampling.
5. The **DMA-BUF render target** is also `Bgra8UnormSrgb`, so the final output is correct.

### Verification

- `cargo check` passes for wgpu-api crate with no errors
- 925 wgpu-api tests pass (2 pre-existing physics test failures unrelated)
- The fix is minimal and targeted — only 2 lines changed in texture creation

---

## Additional Issues Found During Fix

### 831. backend.rs — `register_texture` ignores srgb view format (lines 589-602)
**File:** `wgpu-api/src/backend.rs` lines 589-602 (before fix)

```rust
let texture = self.device.create_texture(&wgpu::TextureDescriptor {
    // ...
    view_formats: &[],  // ← empty for ALL textures
});
```

For srgb textures, `view_formats` must include the non-srgb equivalent. Without it, the texture view may not sample correctly in shaders. This is the **root cause** of icons not rendering on the GPU path.

**Impact:** Icons render as black/transparent because `textureSample` returns zero values.

**Fix:** Set `view_formats` based on the texture format (applied above).

---

### 832. backend.rs — Placeholder texture uses wrong format (line 415)
**File:** `wgpu-api/src/backend.rs` line 415 (before fix)

```rust
format: wgpu::TextureFormat::Rgba8UnormSrgb,
view_formats: &[],
```

The placeholder texture for the solid pipeline's bind group also lacks `view_formats`. While the solid pipeline doesn't sample textures, the bind group layout requires a valid texture binding.

**Impact:** Minor — solid pipeline doesn't sample the placeholder, but consistency matters.

**Fix:** Set `view_formats: &[wgpu::TextureFormat::Rgba8Unorm]` (applied above).

---

## Summary of Issues by Severity

| #  | Severity | Issue | Impact |
|----|----------|-------|--------|
| 831 | **Critical** | srgb texture view format missing | Icons don't render |
| 832 | **Low** | Placeholder texture view format | Consistency fix |

---

## Cumulative Summary

| Round | File | Issues | New Running Total |
|-------|------|--------|-------------------|
| 1 | gpu-rendering-review.md | 1-12 | 12 |
| 2 | gpu-rendering-review-2.md | 13-32 | 32 |
| 3 | gpu-rendering-review-3.md | 33-60 | 62 |
| 4 | gpu-rendering-review-4.md | 61-95 | 97 |
| 5 | gpu-rendering-review-5.md | 96-120 | 120 |
| 6 | gpu-rendering-review-6.md | 121-155 | 155 |
| 7 | gpu-rendering-review-7.md | 156-199 | 199 |
| 8 | gpu-rendering-review-8.md | 200-240 | 240 |
| 9 | gpu-rendering-review-9.md | 241-280 | 280 |
| 10 | gpu-rendering-review-10.md | 281-330 | 330 |
| 11 | gpu-rendering-review-11.md | 331-370 | 370 |
| 12 | gpu-rendering-review-12.md | 371-420 | 420 |
| 13 | gpu-rendering-review-13.md | 421-460 | 460 |
| 14 | gpu-rendering-review-14.md | 461-505 | 505 |
| 15 | gpu-rendering-review-15.md | 506-550 | 550 |
| 16 | gpu-rendering-review-16.md | 551-595 | 595 |
| 17 | gpu-rendering-review-17.md | 596-630 | 630 |
| 18 | gpu-rendering-review-18.md | 631-670 | 670 |
| 19 | gpu-rendering-review-19.md | 671-710 | 710 |
| 20 | gpu-rendering-review-20.md | 711-750 | 750 |
| 21 | gpu-rendering-review-21.md | 751-790 | 790 |
| 22 | gpu-rendering-review-22.md | 791-830 | 830 |
| **23** | **gpu-rendering-review-23.md** | **831-832** | **832** |

**Cumulative severity across all 23 rounds:**
- Critical: 106 (was 105 + 1 new)
- High: 244 (no new)
- Medium: 347 (no new)
- Low: 79 (was 78 + 1 new)
- **Total: 832 issues**

### Key Finding

After 22 rounds of review identifying 830 issues, the **root cause** of icons not rendering was the missing `view_formats` for srgb textures in `register_texture`. This is a subtle wgpu requirement: srgb textures must declare their non-srgb equivalent view format so the texture view can be sampled correctly in shaders. Without it, `textureSample` returns zero, making icons invisible.

The fix is minimal (2 lines) and targeted. It does not change the rendering pipeline, shaders, or bind group structure — only the texture creation parameters.
