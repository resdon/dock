# Memory Optimization Plan

**Version**: v1.5.5
**Date**: 2026-11-12
**Status**: In Progress

## Executive Summary

The memory audit identified **PipeWire capture** and **icon cache** as the largest consumers, accounting for ~70% of total memory usage. This plan outlines prioritized optimizations to reduce typical usage from ~120 MB to ~40 MB.

## Estimated Total Memory Footprint

| Component | Typical (MB) | Peak (MB) |
|-----------|--------------|-----------|
| PipeWire (5 streams) | 40 | 130 |
| Icons (50 apps, 3 copies) | 50 | 150 |
| wgpu Texture Cache | 10 | 64 |
| DMABUF Pool | 4 | 17 |
| Font rendering | 3 | 5 |
| sysinfo scanner | 5 | 20 |
| Rust heap collections | 5 | 15 |
| Wayland surfaces | 2 | 5 |
| **Total** | **~119 MB** | **~406 MB** |

---

## Prioritized Recommendations

### 1. Change `get_active_frame()` return type from `Vec<u8>` to `Arc<[u8]>`

**Priority**: High
**Impact**: Eliminates ~8 MB copy per frame read
**Status**: In Progress

**Problem**: Line 662 in `pipewire_capture.rs` calls `cached_rgba.to_vec()` on every cache hit, copying the full RGBA buffer even when reusing the cached conversion. This defeats much of the `Arc<[u8]>` optimization.

**Solution**: Change `get_active_frame()` to return `Arc<[u8]>` instead of `Vec<u8>`, eliminating the `.to_vec()` copy on cache hits. This requires updating the caller in `window_list.rs` to accept `Arc<[u8]>` for texture upload.

**Files**: `src/services/pipewire_capture.rs`, `src/app/window_list.rs`

---

### 2. Use `Arc<[u8]>` for icon cache

**Priority**: High
**Impact**: Eliminates 2 redundant copies per icon, saving ~100 MB with 50 pinned apps
**Status**: Not Started

**Problem**: Each icon exists in 3 independent copies:
1. CPU cache: `AppState.icon_cache: HashMap<String, (Vec<u8>, u32)>`
2. Slint Image: `SlintDock.icon_cache: HashMap<String, Image>`
3. GPU texture: `Renderer.textures` + backend `gpu_textures`

**Solution**: Use `Arc<[u8]>` for the icon cache payload so Slint and GPU paths share the same underlying data rather than copying.

**Files**: `src/state/mod.rs`, `src/render/slint_dock.rs`, `wgpu-api/src/renderer.rs`

---

### 3. Downscale icons to 256×256

**Priority**: Medium
**Impact**: Halves icon memory across all copies
**Status**: Not Started

**Problem**: 512×512 is overkill for dock icons (typically 64–128px display size).

**Solution**: Reduce icon resolution to 256×256. This halves icon memory across all copies.

**Files**: Icon resolver modules

---

### 4. Invalidate cached RGBA on new frame

**Priority**: Medium
**Impact**: Avoid holding both raw and converted frames simultaneously in PipeWire
**Status**: Not Started

**Problem**: The raw frame and cached RGBA both exist simultaneously in `StreamFrameData`.

**Solution**: Invalidate the cached RGBA when a new raw frame arrives, since the next `get_active_frame()` call will rebuild it.

**Files**: `src/services/pipewire_capture.rs`

---

### 5. Filter sysinfo to dock-relevant processes

**Priority**: Low
**Impact**: Reduces scanner memory from tracking all system processes
**Status**: Not Started

**Problem**: `sysinfo::System` in `AppState` tracks all system processes with CPU and memory data. On a system with 500+ processes, this can consume 5–20 MB.

**Solution**: Consider filtering to only track processes relevant to dock windows rather than all system processes.

**Files**: `src/state/mod.rs`, process monitoring modules

---

## Implementation Notes

- All changes should follow the existing coding conventions and annotation patterns (`**FIX (v1.5.5)**`).
- Each optimization should be committed separately with clear commit messages.
- Run `cargo check` after each change to verify compilation.
- The PipeWire and icon systems account for ~70% of total memory. Fixing these two would reduce typical usage from ~120 MB to ~40 MB.
