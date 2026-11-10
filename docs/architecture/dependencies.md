# Library Versions and Compatibility

## Dependency Matrix

### Core — GPU Rendering & Graphics

| Library | Version | Crate | Purpose |
|---------|---------|-------|---------|
| **wgpu** | `30` | dock, wgpu-api | GPU rendering backend, compute & render pipelines |
| **wgpu-dmabuf** | `0.1` | dock | DMA-BUF sharing for zero-copy Wayland surfaces |

**Note**: wgpu-api has a **direct wgpu 30 dependency** for the custom GPU backend implementation.

### Core — Wayland & Windowing

| Library | Version | Purpose |
|---------|---------|---------|
| **wayland-client** | `0.31.15` | Wayland protocol client (sockets, events, roundtrips) |
| **wayland-protocols** | `0.32.13` | Wayland protocol definitions (client + staging) |
| **wayland-protocols-wlr** | `0.3` | Sway/WLR extensions (layer shell, foreign toplevels) |
| **smithay-client-toolkit** | `0.20.0` | Client toolkit (event loop, globals, data device) |
| **raw-window-handle** | `0.6` | Abstract window handle for wgpu surface creation |

### Core — PipeWire (Screen/Window Capture)

| Library | Version | Purpose |
|---------|---------|---------|
| **pipewire** | `0.10.1` | PipeWire Rust bindings (stream management, nodes) |
| **pipewire-sys** | `0.10.1` | Low-level PipeWire FFI |
| **libspa-sys** | `0.10.1` | SPA (Simple Plugin API) FFI for audio/video processing |

### Core — UI & Widget System

| Library | Version | Purpose |
|---------|---------|---------|
| **slint** | `1.18.1` | Dock icon/widget rendering via Slint UI framework |
| **slint-build** | `1.18.1` | Build-time Slint compiler (build dependency) |

### Physics Engine

| Library | Version | Purpose |
|---------|---------|---------|
| **glam** | `0.29` | Math library: vectors, matrices, quaternions (wgpu-api) |
| **bytemuck** | `1` | Zero-cost byte-level type casting (GPU buffer safety) |

### Image & SVG Processing

| Library | Version | Purpose |
|---------|---------|---------|
| **image** | `0.25.10` | Raster image loading/saving (PNG, JPEG, etc.) |
| **resvg** | `0.47.0` | SVG-to-pixel conversion (Resvg) |
| **usvg** | `0.48.1` | SVG parsing/layout (used by resvg) |
| **tiny-skia** | `0.12.0` | 2D Canvas API (Skia subset) for raster operations |
| **cairo-rs** | `0.19` | Cairo 2D graphics bindings |
| **fast_image_resize** | `5.0` | SIMD-optimized image resizing |
| **imgref** | `1.10` | Immutable image reference type |
| **rgb** | `0.8` | RGB pixel type |
| **gif** | `0.13` | GIF decoding for animated images (wgpu-api) |
| **linicon** | `2.3` | Linux icon theme lookup |
| **base64** | `0.22` | Base64 encoding for image bytes |
| **sixel-rs** | `0.5.0` | Sixel image format encoding |
| **icy_sixel** | `0.5.0` | Sixel terminal output |

### Text & Fonts

| Library | Version | Purpose |
|---------|---------|---------|
| **cosmic-text** | `0.18` | Text shaping and layout (wgpu-api) |
| **fontdb** | `0.16` | Font discovery and database (wgpu-api) |
| **swash** | `0.1.18` | Font outline extraction and glyph rasterization (wgpu-api) |
| **fontdue** | `0.9` | Simple font glyph rasterization |

### Async & Concurrency

| Library | Version | Purpose |
|---------|---------|---------|
| **tokio** | `1.54` | Async runtime (D-Bus, PipeWire, general async) |
| **futures-util** | `0.3` | Future combinators and async utilities |
| **parking_lot** | `0.12` | Timeout-based Mutex (prevents PipeWire threads from freezing) |
| **dashmap** | `6.1` | Concurrent hash map for caching |
| **once_cell** | `1.21` | Lazy static initialization |
| **crossbeam** | `0.8` | Lock-free structures, channels, and thread coordination |

### D-Bus & System Integration

| Library | Version | Purpose |
|---------|---------|---------|
| **zbus** | `6` (tokio) | D-Bus client/server (system service communication) |
| **sysinfo** | `0.40` | System monitoring (CPU, memory, processes) |
| **libc** | `0.2` | Low-level C FFI bindings |

**Note**: `zvariant` is no longer a separate dependency — it's merged into `zbus` 6.0.

### Serialization & Data

| Library | Version | Purpose |
|---------|---------|---------|
| **serde** | `1.0.229` (derive) | Serialization/deserialization framework |
| **serde_json** | `1.0` | JSON serialization |
| **lru** | `0.12` | Bounded LRU cache |

### System & Utilities

| Library | Version | Purpose |
|---------|---------|---------|
| **memmap2** | `0.9` | Memory-mapped file I/O |
| **walkdir** | `2.5.0` | Recursive directory traversal |
| **dirs** | `6.0.0` | Platform-specific directories (config, data, cache) |
| **urlencoding** | `2.1.3` | URL encoding/decoding |
| **os_pipe** | `1.2.3` | OS pipe (FIFO) creation |
| **log** | `0.4` | Logging facade (wgpu-api) |

### Observability & Performance (New)

| Library | Version | Purpose |
|---------|---------|---------|
| **tracing** | `0.1` | Structured logging, performance tracing, async task diagnostics |
| **tracing-subscriber** | `0.3` | Tracing backend with env-filter support |
| **criterion** | `0.5` | Benchmarking framework (dev-dependency) |
| **mimalloc** | `0.1` | Fast memory allocator (optional, benchmark before enabling) |

## Compatibility Notes

### Critical Compatibility Pairs

| Pair | Constraint |
|------|-----------|
| **wgpu + wgpu-dmabuf** | wgpu-dmabuf 0.1 depends on wgpu 30.0.1 |
| **rapier2d + nalgebra** | **Removed** per ADR 005 — analytical springs are sufficient |
| **resvg + usvg** | resvg 0.47.0 and usvg 0.48.1 — do not upgrade independently |
| **slint + slint-build** | Must be on the same version (1.18.1) |
| **pipewire + pipewire-sys + libspa-sys** | Must be on the same version (0.10.1) |
| **zbus** | Version 6.0 — `zvariant` is merged; no separate dependency needed |

### Upgrade History

| Date | Change | Reason |
|------|--------|--------|
| 2026-11-10 | wgpu-api: added `wgpu = "30"` | Custom GPU backend implementation |
| 2026-11-10 | femtovg **removed** | Replaced by wgpu-api custom renderer |
| 2026-11-10 | rapier2d `0.32` → `0.36.1` | Upgraded, then removed per ADR 005 |
| 2026-11-10 | nalgebra `0.34` → `0.35` | Upgraded, then removed per ADR 005 |
| 2026-11-10 | rapier2d, nalgebra **removed** | ADR 005: analytical springs over general-purpose physics |
| 2026-11-10 | glam `0.28` → `0.29` | Latest compatible version |
| 2026-11-10 | cosmic-text `0.12` → `0.18` | Recommended upgrade |
| 2026-11-10 | zbus `4.0` → `6` | Major upgrade; `zvariant` merged into zbus |
| 2026-11-10 | sysinfo `0.33` → `0.40` | Recommended upgrade; constructors now return `Result` |
| 2026-11-10 | tokio `1.0` → `1.54` | Latest stable version |
| 2026-11-10 | serde `1.0` → `1.0.229` | Latest stable version |
| 2026-11-10 | Removed `lazy_static` | Consolidated with `once_cell` |
| 2026-11-10 | Added `tracing`, `criterion`, `mimalloc`, `crossbeam` | Recommended observability and performance libraries |

### Known Version Issues

1. **zbus 6.0 deprecation warnings**: `zbus::zvariant::*` types are deprecated in favor of `zbus::*` directly. ~15 warnings remain until migration is complete.

2. **resvg/usvg coupling**: resvg 0.47.0 and usvg 0.48.1 are tightly coupled. When upgrading, always check resvg's declared usvg dependency version first.

3. **glam version**: Latest is `0.29.3` — version `0.34.3` does not exist. Check `crates.io` before upgrading.

## Dependency Counts

| Crate | Dependencies | Dev Dependencies |
|-------|-------------|-----------------|
| **dock** (root) | ~42 | 1 (slint-build) |
| **wgpu-api** | ~14 | 2 (tokio, criterion) |
| **Total unique** | ~50 | — |

## Adding New Dependencies

### Guidelines

1. **Prefer existing dependencies** before adding new ones.
2. **Check version compatibility** with existing dependencies before adding.
3. **Add to the correct crate**: wgpu-api for library code, root for application code.
4. **Update this document** with the new dependency and any compatibility notes.
5. **Run `cargo check`** to verify the dependency resolves correctly.
6. **Run `cargo test`** to verify no existing tests break.

### wgpu-api Dependency Rules

- **No wgpu types** in the public API.
- **No Wayland dependencies** in wgpu-api.
- **No PipeWire dependencies** in wgpu-api.
- Physics modules must remain **pure Rust** (no wgpu, Wayland, or PipeWire).

## Reference

- **Reference library**: `docs/reference/` (detailed docs for each library category)
- **Known issues**: `docs/architecture/known_issues.md`
- **Coding rules**: `docs/architecture/coding_rules.md`
