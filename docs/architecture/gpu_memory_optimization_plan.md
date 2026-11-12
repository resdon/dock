# GPU Memory Optimization Plan

**Version**: v1.5.5
**Date**: 2026-11-12
**Status**: In progress

## Overview

This plan addresses GPU memory overhead in Dock's wgpu renderer. The goal is to reduce both CPU RAM (RSS) and GPU VRAM by eliminating unnecessary GPU texture storage and configuring wgpu/Vulkan for memory-efficient operation.

## Review Summary

A comprehensive review identified 10 optimization areas. Out of these:
- **7 are already implemented** (texture reuse, downscaling, buffer reuse, single device, icon sizing, texture usages, staging buffers)
- **2 are actionable one-line changes** (MemoryHints, surface latency)
- **1 is partially done** (cache accounting — good enough for now)

## Optimization Status

| # | Optimization | Priority | Status | Expected Impact |
|---|-------------|----------|--------|----------------|
| 1 | `MemoryHints::MemoryUsage` | High | ✅ Done | Modest on NVIDIA, meaningful on AMD/Intel |
| 2 | One GPU texture per stream | Critical | ✓ Already done | No action needed |
| 3 | Downscaled window previews | Critical | ✓ Already done | No action needed |
| 4 | Reusable texture allocations | Critical | ✓ Already done (DMABUF pool) | No action needed |
| 5 | Reusable vertex/uniform buffers | High | ✓ Already done (BufferCache) | No action needed |
| 6 | Single shared wgpu Device | High | ✓ Already done | No action needed |
| 7 | Surface latency = 1 | Medium | ✅ Done | ~8 MB per surface (1920×1080) |
| 8 | Smaller icon textures | High | ✓ Already done (48px default) | No action needed |
| 9 | Minimal texture usages | Medium | ✓ Already done | No action needed |
| 10 | Reusable staging allocation | Medium | ✓ Already done | No action needed |

## Implemented Changes

### 1. MemoryHints::MemoryUsage

**File**: `wgpu-api/src/backend.rs`
**Change**: `wgpu::MemoryHints::default()` → `wgpu::MemoryHints::MemoryUsage`
**Why**: The default `Performance` strategy favors performance over memory usage. `MemoryUsage` asks the Vulkan backend to favor lower memory consumption with smaller allocation blocks and more aggressive cleanup.
**Risk**: Low. This is a hint, not a guarantee. Some backends may ignore it.

### 2. Surface latency = 1

**File**: `wgpu-api/src/backend.rs`
**Change**: `desired_maximum_frame_latency: 2` → `desired_maximum_frame_latency: 1`
**Why**: The default latency of 2 results in ~3 swapchain images on Vulkan. For a dock that renders infrequently (content hash caching skips ~95% of frames), 3 swapchain images is overkill. Reducing to 1 saves one frame buffer per surface (~8 MB for 1920×1080).
**Risk**: Low. The dock doesn't need GPU-CPU parallelism for rendering. FIFO present mode + latency 1 is appropriate.

## Already Implemented (No Action Needed)

### One GPU texture per active capture stream
The `replace_cached_texture_from_bgfx()` method in `renderer.rs` updates an existing cached texture by key rather than creating a new one. Same-dimension updates reuse the texture via `queue.write_texture`.

### Downscaled window previews
`window_list.rs` calculates `target_frame_w` and `target_frame_h` based on popup row dimensions. A 1920×1080 capture displayed at ~320×180 is downscaled before GPU upload.

### Reusable texture allocations
The DMABUF pool (`dmabuf_export.rs`) reuses textures per dimension with 2 slots and a 32 MB global budget. The `TextureCache` in `gpu.rs` has LRU eviction.

### Reusable vertex/uniform buffers
The `BufferCache` in `gpu.rs` reuses vertex and instance buffers that grow as needed. No `create_buffer_init` calls in the render loop.

### Single shared wgpu Device
One `Renderer::new()` call creates one `GpuBackend` with one wgpu device. All popup rendering paths share this single renderer.

### Smaller icon textures
`config.icon_size` defaults to 48px with a max of 128px. Icons are resized during loading, not stored at source resolution.

### Minimal texture usages
Textures use `Bgra8UnormSrgb` (4 bytes/pixel). Glyph atlas uses `Grayscale` mode (1 byte/pixel). Mip levels are 1 by default. MSAA is disabled. No unnecessary texture usage flags.

### Reusable staging allocation
The DMABUF pool reuses presentation textures. No large staging belt for PipeWire frames — correctly avoided as it would be another memory consumer.

## Memory Measurement Procedure

For future verification, use these 5 steps:

1. **Before wgpu initialization**: Measure Dock RSS and PSS
2. **After device creation**: Measure the incremental CPU and GPU memory footprint
3. **After surface creation**: Identify swapchain allocation costs
4. **After loading all icons and textures**: Measure cache memory
5. **After five PipeWire streams**: Measure steady-state and peak memory

On Linux:
```bash
cat /proc/$(pidof dock)/smaps_rollup
```

Pay attention to:
- `Rss`: resident memory
- `Pss`: proportionally shared resident memory
- `Private_Dirty`: private resident allocations

Use `heaptrack` for CPU heap analysis, but don't interpret it as a complete measurement of Vulkan VRAM allocations. Use `nvtop` for real-time VRAM monitoring on NVIDIA.

## Architecture Target

```
PipeWire
   |
   v
Single retained CPU frame (Arc<[u8]>)
   |
   v
Downscale to preview resolution (GPU linear sampling)
   |
   v
Reusable GPU texture (DMABUF pool, 32 MB budget)
   |
   v
wgpu shader (MemoryUsage hints, latency 1)
   |
   v
Wayland surface
```

```
One wgpu Device
    |
    +-- Shared pipeline cache
    +-- Shared texture cache (64 MB budget)
    +-- Reusable geometry buffers
    +-- Reusable upload allocations
    +-- Single glyph atlas
    +-- Minimal surface buffering (latency 1)
    +-- MemoryUsage allocation hints
```

## References

- [wgpu MemoryHints documentation](https://docs.rs/wgpu/latest/wgpu/enum.MemoryHints.html)
- [wgpu SurfaceConfiguration documentation](https://docs.rs/wgpu/latest/wgpu/struct.SurfaceConfiguration.html)
- [Vulkan memory allocation](https://docs.vulkan.org/)
- Review source: Comprehensive GPU memory optimization analysis (2026-11-12)
