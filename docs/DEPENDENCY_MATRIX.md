# Dependency Matrix and Verification

This document provides automated commands for verifying all library versions, detecting compatibility issues, and checking the health of the dependency tree.

## Quick Verification Commands

Run these commands regularly to verify the dependency state:

```bash
# 1. Show all installed dependency versions
cargo tree

# 2. Detect duplicate dependency versions (critical for wgpu, nalgebra, etc.)
cargo tree -d

# 3. Check outdated dependencies
cargo install cargo-outdated
cargo outdated

# 4. Check dependency security advisories
cargo install cargo-audit
cargo audit

# 5. Check unused dependencies
cargo install cargo-machete
cargo machete

# 6. Check compilation across all targets
cargo check --workspace --all-targets

# 7. Check formatting
cargo fmt --all -- --check

# 8. Check for implementation issues
cargo clippy --workspace --all-targets
```

## Priority Dependency Checks

These commands verify the most critical dependency relationships in this project:

### wgpu Version Unity

```bash
# Verify only one wgpu version exists in the dependency tree
cargo tree -i wgpu
```

**Expected**: Single wgpu 30.0.1 entry. No duplicate versions.

wgpu-api has a direct wgpu 30 dependency for the custom GPU backend. The root crate uses wgpu 30.0.1, which wgpu-dmabuf 0.1 also depends on.

### SVG Compatibility (resvg ↔ usvg)

```bash
# Verify resvg and usvg version compatibility
cargo tree -i usvg
cargo tree -i resvg
```

**Expected**: resvg 0.47.x depends on usvg 0.48.1. These are tightly coupled — never upgrade one without checking the other.

### Physics Math Stack (rapier2d ↔ nalgebra)

```bash
# Verify Rapier2D and nalgebra compatibility
cargo tree -i nalgebra
cargo tree -i rapier2d
```

**Expected**: rapier2d 0.36.1 depends on nalgebra 0.35.x. glam 0.29 is independent (used by wgpu-api for non-Rapier math).

### Wayland Ecosystem Consistency

```bash
# Verify Wayland client ecosystem
cargo tree -i wayland-client
cargo tree -i wayland-protocols
cargo tree -i smithay-client-toolkit
```

**Expected**: wayland-client 0.31.x, wayland-protocols 0.32.x, smithay-client-toolkit 0.20.x — all compatible within the same ecosystem.

### PipeWire Stack

```bash
# Verify PipeWire bindings consistency
cargo tree -i pipewire
cargo tree -i pipewire-sys
cargo tree -i libspa-sys
```

**Expected**: All three at 0.10.x, aligned with the same PipeWire release.

### D-Bus Stack

```bash
# Verify zbus and zvariant compatibility
cargo tree -i zbus
```

**Expected**: zbus 6.0 (zvariant is merged into zbus, no separate dependency needed).

## Current Dependency Versions

### Core — GPU Rendering & Graphics

| Library | Version | Crate | Notes |
|---------|---------|-------|-------|
| wgpu | 30.0.1 | dock, wgpu-api | Vulkan backend, unified version |
| wgpu-dmabuf | 0.1.x | dock | Depends on wgpu 30 |

### Core — Wayland & Windowing

| Library | Version | Notes |
|---------|---------|-------|
| wayland-client | 0.31.15 | Wayland protocol client |
| wayland-protocols | 0.32.13 | Protocol definitions |
| wayland-protocols-wlr | 0.3.x | Sway/WLR extensions |
| smithay-client-toolkit | 0.20.0 | Client toolkit |
| raw-window-handle | 0.6 | Window handle for wgpu |

### Core — PipeWire

| Library | Version | Notes |
|---------|---------|-------|
| pipewire | 0.10.1 | Screen capture |
| pipewire-sys | 0.10.x | Low-level FFI |
| libspa-sys | 0.10.x | SPA FFI |

### Core — UI

| Library | Version | Notes |
|---------|---------|-------|
| slint | 1.18.1 | Software renderer |
| slint-build | 1.18.1 | Build-time compiler |

### Physics

| Library | Version | Notes |
|---------|---------|-------|
| rapier2d | 0.36.1 | Depends on nalgebra 0.35 |
| nalgebra | 0.35 | Linear algebra for Rapier |
| glam | 0.29 | Math library (independent of Rapier) |
| bytemuck | 1.x | Zero-cost byte casting |

### Image & SVG

| Library | Version | Notes |
|---------|---------|-------|
| image | 0.25.10 | Raster images |
| resvg | 0.47.x | SVG rendering (coupled with usvg) |
| usvg | 0.48.1 | SVG parsing (coupled with resvg) |
| tiny-skia | 0.12.x | 2D canvas |
| cairo-rs | 0.19.x | Cairo bindings |
| fast_image_resize | 5.x | SIMD resize |

### Text & Fonts

| Library | Version | Notes |
|---------|---------|-------|
| cosmic-text | 0.18.x | Text shaping |
| fontdb | 0.16.x | Font discovery |
| swash | 0.1.x | Glyph outlines |
| fontdue | 0.9.x | Glyph rasterization |

### Async & Concurrency

| Library | Version | Notes |
|---------|---------|-------|
| tokio | 1.54 | Async runtime |
| futures-util | 0.3 | Future combinators |
| parking_lot | 0.12 | Timeout mutex |
| dashmap | 6.1 | Concurrent hash map |
| crossbeam | 0.8 | Lock-free structures |
| once_cell | 1.21 | Lazy static (unified) |

### D-Bus & System

| Library | Version | Notes |
|---------|---------|-------|
| zbus | 6.0 | D-Bus client/server |
| sysinfo | 0.40 | System monitoring |
| libc | 0.2.x | C FFI |

### Serialization & Utils

| Library | Version | Notes |
|---------|---------|-------|
| serde | 1.0.229 | Serialization |
| serde_json | 1.0.x | JSON |
| lru | 0.12 | LRU cache |
| memmap2 | 0.9 | Memory-mapped files |
| walkdir | 2.5.x | Directory traversal |
| dirs | 6.x | Platform directories |

## Coupled Dependencies

These dependencies must be upgraded together:

| Pair | Relationship |
|------|-------------|
| **resvg ↔ usvg** | resvg depends on specific usvg version |
| **rapier2d ↔ nalgebra** | rapier2d 0.36 depends on nalgebra 0.35 |
| **slint ↔ slint-build** | Same major.minor version |
| **pipewire ↔ pipewire-sys ↔ libspa-sys** | Same major.minor version |
| **zbus ↔ zvariant** | zvariant merged into zbus 6.0 |
| **wgpu-api ↔ wgpu** | wgpu-api depends on wgpu 30 |
| **wgpu-dmabuf ↔ wgpu** | wgpu-dmabuf 0.1 depends on wgpu 30 |

## Upgrade History

| Date | Library | Old → New | Reason |
|------|---------|-----------|--------|
| 2026-11 | wayland-client | 0.31.14 → 0.31.15 | Compatible release |
| 2026-11 | wayland-protocols | 0.32.9 → 0.32.13 | Compatible release |
| 2026-11 | slint | 1.18 → 1.18.1 | Patch release |
| 2026-11 | pipewire | 0.10 → 0.10.1 | Patch release |
| 2026-11 | rapier2d | 0.32 → 0.36.1 | Major upgrade, API changes |
| 2026-11 | nalgebra | 0.34 → 0.35 | Required by rapier2d 0.36 |
| 2026-11 | glam | 0.28 → 0.29 | Compatible release |
| 2026-11 | cosmic-text | 0.12 → 0.18 | Major upgrade |
| 2026-11 | tokio | 1.0 → 1.54 | Latest release |
| 2026-11 | sysinfo | 0.33 → 0.40 | Major upgrade, API changes |
| 2026-11 | zbus | 4.0 → 6.0 | Major upgrade, API changes |
| 2026-11 | serde | 1.0 → 1.0.229 | Latest release |
| 2026-11 | image | 0.25 → 0.25.10 | Patch release |
| 2026-11 | once_cell | 1.19 → 1.21 | Unified version |

## Removed Dependencies

| Dependency | Reason |
|------------|--------|
| wgpu 0.20 (wgpu-api) | Dead weight — wgpu-api never imported wgpu types |
| lazy_static | Consolidated with once_cell |
| zvariant (separate) | Merged into zbus 6.0 |

## Dependency Health Checklist

- [ ] `cargo tree -d` shows no duplicate versions
- [ ] `cargo audit` shows no security advisories
- [ ] `cargo machete` shows no unused dependencies
- [ ] `cargo check --workspace --all-targets` passes
- [ ] `cargo clippy --workspace --all-targets` passes
- [ ] Coupled dependencies are version-aligned
- [ ] No wgpu type leakage across crate boundaries

## Reference

- **Full dependency docs**: `docs/reference/` (11 files)
- **Architecture deps**: `docs/architecture/dependencies.md`
- **AI instructions**: `docs/AI_INSTRUCTIONS.md`
