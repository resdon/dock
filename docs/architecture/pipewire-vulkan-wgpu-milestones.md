# PipeWire + Vulkan + wgpu Unified Frame Pipeline — Milestones

**Status:** Tracking document (active roadmap)  
**Related:** `docs/architecture/pipewire.md`, `docs/architecture/renderer.md`, `docs/architecture/WAYDOCK_WGPU_BACKEND.md`, `docs/contracts/renderer.md`, ADRs in `docs/decisions/`  
**Scope:** Dock window/screen/camera capture rendering via PipeWire, normalized frame ingestion, Vulkan-backed wgpu rendering/composition, DMA-BUF/SHM import paths.

## Purpose

Define an incremental, verifiable implementation plan for a unified video frame pipeline:

- **One normalized frame representation** for all PipeWire sources (screen, window, camera, other nodes), regardless of transport (DMA-BUF/SHM) or pixel layout (RGB/YUV, single/multi-plane).
- **Vulkan-only, wgpu-centered rendering** where renderer/app code consumes normalized frames/textures, not PipeWire/SPA buffer internals.
- **Explicit buffer lifetime and synchronization** (frame leases, bounded in-flight frames, GPU-completion-aware release for DMA-BUF).
- **Safe fallbacks** (SHM upload path always available for unsupported/failed zero-copy imports).

This document tracks implementation progress from current Dock behavior toward the target architecture.

## Invariants (must hold throughout)

- **Renderer owns GPU resources.** Import/conversion/upload paths create/update textures via renderer/backend-owned APIs only.
- **No raw PipeWire buffers outside capture ownership.** App/renderer never receive `pw_buffer`, raw SPA data pointers, or borrowed SHM slices with unmanaged lifetimes.
- **Prompt buffer release.** SHM buffers returned after copy; DMA-BUF leases held only until GPU read/completion as defined by sync policy.
- **No render-thread blocking.** No subprocesses, no blocking waits on GPU/semaphores in the render loop; bounded async completion/release queues.
- **No steady-state hot-path allocations/churn.** Reuse textures, staging buffers, bind groups, and pipelines; bounded caches and in-flight frames.
- **Event-driven rendering.** Render/composite on new frame or composition change; latest-frame policy per source (drop stale frames).
- **Vulkan-only backend for this pipeline.** Fail clearly on missing Vulkan capabilities instead of silently changing backend semantics.
- **Explicit fallbacks.** Unsupported format/modifier/import/sync path => negotiated SHM or supported conversion fallback with diagnostics, never crash.

## Current baseline (codebase)

| Area | Current state | Notes |
|---|---|---|
| Capture service | `src/services/pipewire_capture.rs` | Portal ScreenCast + per-stream PipeWire threads, format negotiation, CPU conversion/downscale, window binding/monitoring |
| Video API/types | `wgpu-api/src/video.rs` | Stream/frame abstractions, format/cache concepts, CPU/DMA-BUF buffer fields (`FrameBufferSource`) |
| GPU backend | `wgpu-api/src/backend.rs`, `wgpu-api/src/renderer.rs`, `wgpu-api/src/texture.rs` | wgpu/Vulkan resource ownership, texture registration/upload/render integration |
| DMA-BUF export | `src/render/dmabuf_export.rs` | Output/export-related path (not PipeWire frame import) |
| Consumers | `src/app/window_list.rs`, `src/render/*` | Popup/thumbnail rendering consumes capture results |

**Known gaps vs target:** unified `CaptureManager/FrameBroker/FrameLease`, isolated Vulkan external-memory importer, explicit GPU-completion-based DMA-BUF release, multi-plane YUV import+GPU conversion pipeline, fully normalized descriptor-driven negotiation/import/render boundary.

## Target component mapping

| Target component | Responsibility | Suggested location | Replaces/evolves |
|---|---|---|---|
| `CaptureManager` / source registry | Source lifecycle, stable IDs, session ownership | `src/services/capture/*` | parts of `pipewire_capture.rs` |
| `PipeWireBackend` | Stream connect, negotiation, callbacks, buffer dequeue | `src/services/capture/pipewire/*` | stream/thread/callback logic in `pipewire_capture.rs` |
| `FrameBroker` + `FrameLease` | Latest-frame queue, lease tracking, release dispatch | `wgpu-api/src/video/*` and/or capture-side broker | implicit copy/release behavior |
| `VulkanImporter` | DMA-BUF/external memory import, capability checks, unsafe boundary | `wgpu-api/src/gpu/import/*` (or backend import module) | new (currently CPU upload path dominant) |
| `TexturePool` | Reusable imported/normalized/upload textures and bind resources | `wgpu-api/src/gpu/texture_pool.rs` / `wgpu-api/src/texture.rs` | existing cache/reuse patterns |
| `WgpuRenderer` integration | Compose normalized textures only | `wgpu-api/src/renderer.rs`, app render consumers | format-specific handling in consumers |
| Conversion shaders/pipelines | YUV/multi-plane -> RGBA normalization | `wgpu-api/src/shaders/*`, pipeline cache | new |

## Milestone tracker

| ID | Milestone | Status | Dependencies | Exit verification |
|---|---|---|---|---|
| M0 | Architecture/contracts/scaffold alignment | [ ] | — | Docs/contracts/ADR drafted, baseline audited, `cargo check` |
| M1 | Normalized frame + lease API scaffolding | [ ] | M0 | Types/traits compile, no behavior regression, tests for metadata/lease state |
| M2 | Capture backend refactor (loop ownership, bounded queues) | [ ] | M1 | Modular capture, bounded latest-frame queues, correct release routing, manual stress pass |
| M3 | Canonical SHM fallback path + texture pooling | [~] legacy exists | M1,M2 | Correct RGB upload/conversion, texture reuse, no steady-state churn |
| M4 | Single-plane DMA-BUF import (Vulkan interop) | [ ] | M2,M3 | Supported modifiers/formats imported, FD/lifetime safe, deterministic fallback |
| M5 | Multi-plane/YUV formats + GPU conversion | [ ] | M3,(M4 opt.) | NV12/P010/I420/YUY2/UYVY support or safe fallback, correct color handling, cached pipelines |
| M6 | Explicit synchronization + async completion-based release | [ ] | M1,M4 | Bounded in-flight leases, no premature reuse/corruption, implicit/explicit sync policy enforced |
| M7 | Integration, recovery, performance hardening | [ ] | M3–M6 | Hotplug/renegotiation/device-loss recovery, perf/memory budgets met, diagnostics |

Status legend: `[ ]` not started, `[~]` partial/legacy, `[x]` complete.

## Milestone details

### M0 — Architecture/contracts/scaffold alignment
- [ ] Audit current buffer acquire/convert/release flow in `src/services/pipewire_capture.rs` and `wgpu-api/src/video.rs`
- [ ] Document normalized `FrameDescriptor`/`FrameMemory`/`PlaneLayout`/lease semantics and threading/ownership contract
- [ ] Add ADR for unified capture-render import boundary and fallback policy (Vulkan-only, SHM fallback required)
- [ ] Define capability/negotiation matrix (formats, modifiers, color metadata, sources)
- [ ] Verify: `cargo check`, docs cross-links updated

### M1 — Normalized frame + lease API scaffolding
- [ ] Introduce/align types: `SourceId`, `PixelFormat` mapping, `PlaneLayout`, `FrameDescriptor`, `DmaBufPlane`, `FrameMemory`, `VideoFrame` (normalized), `BufferToken`, `GpuCompletion`, `FrameLease`
- [ ] Add `ExternalFrameImporter`/`ImportPath`/`ImportSupport` scaffold interfaces (no runtime behavior change required initially)
- [ ] Add lease/broker abstraction hooks for latest-frame and release dispatch
- [ ] Adapt existing capture->video boundary behind adapters (preserve current behavior)
- [ ] Verify: unit tests for format/descriptor/lease transitions; existing capture path still works; `cargo check`

### M2 — Capture backend refactor (PipeWire loop ownership, bounded queues)
- [ ] Split `pipewire_capture.rs` into focused modules (portal/session, stream backend, negotiation, callbacks, bindings, broker integration)
- [ ] Ensure callbacks are lightweight; move work/copies/releases to appropriate threads/queues
- [ ] Enforce bounded per-source latest-frame queue and stale-frame dropping
- [ ] Centralize buffer return/release on PipeWire-owning context
- [ ] Handle source add/remove, resize/format renegotiation (`generation`), portal/session teardown
- [ ] Verify: manual multi-window stress, no buffer starvation/leaks, bounded memory, `cargo check`

### M3 — Canonical SHM fallback path + texture pooling
- [ ] Normalize supported SHM formats (BGRx/BGRA/RGBA/RGB baseline) to renderer-consumable textures
- [ ] Reuse persistent textures/staging buffers by `(source,size,format,generation)`; avoid per-frame churn
- [ ] Keep conversion/upload off hot render path where possible; bounded upload queues
- [ ] Preserve downscale/thumbnail behavior for window list if required
- [ ] Verify: visual correctness, resize/renegotiation, steady-state allocation/VRAM behavior, `cargo check`

### M4 — Single-plane DMA-BUF import (Vulkan interop)
- [ ] Probe Vulkan external-memory/modifier capabilities and build validated modifier/format support table
- [ ] Implement isolated unsafe importer (single-plane): FD duplication, modifier/layout validation, import/cache, safe retirement
- [ ] Negotiate DMA-BUF params (mandatory + don't-fixate + SHM fallback) correctly
- [ ] Integrate with wgpu-compatible texture usage/lifetime; copy-to-owned texture if required by interop constraints
- [ ] Fallback to SHM on unsupported modifier/import failure/device constraints
- [ ] Verify: supported single-plane streams render, import failures fallback cleanly, no FD/resource leaks

### M5 — Multi-plane/YUV formats + GPU conversion
- [ ] Extend negotiation/parsing for NV12, P010, I420/YV12, YUY2/UYVY (capability-driven)
- [ ] Implement GPU conversion path to normalized RGBA (WGSL shader + cached pipelines/bind groups) as baseline
- [ ] (Optional/advanced) Vulkan multi-plane import + YCbCr path behind strict boundary
- [ ] Plumb and apply color metadata (range/matrix/transfer/primaries) with SDR-safe defaults
- [ ] Verify: format correctness (fixtures/visual), pipeline/bind-group reuse, fallback behavior

### M6 — Explicit synchronization + async completion-based release
- [ ] Implement lease registry with in-flight bounds and completion notifications
- [ ] Tie DMA-BUF buffer release to GPU read completion (no CPU blocking in render loop)
- [ ] Support implicit sync baseline; negotiate/use explicit sync timeline metadata when supported by producer/driver/path
- [ ] Document and enforce queue/ownership/layout transition rules at Vulkan-wgpu boundary
- [ ] Verify: soak tests for corruption/reuse hazards, bounded leases, correct release ordering

### M7 — Integration, recovery, performance hardening
- [ ] Integrate unified pipeline into all video consumers (window list/popups/camera/screen as applicable)
- [ ] Device-lost/reset recovery: rebuild caches/importers/pipelines and rebind streams safely
- [ ] Enforce budgets (in-flight frames, textures per source, staging memory, cache eviction)
- [ ] Add diagnostics: import path, fallback reasons, drops, renegotiations, latency/age, FD/lease counts
- [ ] Verify: end-to-end manual matrix (multi-source, hotplug, close/reopen, format changes), perf/memory targets, `cargo check` + relevant tests

## Verification checklist (per milestone where applicable)

- [ ] `cargo check` passes (workspace)
- [ ] Relevant unit/integration tests added or updated and passing
- [ ] No raw PipeWire buffer/FD lifetime escapes capture boundary
- [ ] SHM buffers released immediately after copy; DMA-BUF leases released only after safe GPU consumption point (M6+)
- [ ] No per-frame texture/pipeline/bind-group/staging allocations in steady state
- [ ] Bounded in-flight frames and bounded caches enforced
- [ ] Unsupported format/modifier/import path falls back with actionable diagnostics
- [ ] Wayland + PipeWire manual validation performed for affected capture/render scenarios

## Progress log

- **2026-11-10:** Created milestone tracking document (`M0` tracking started). Initial baseline mapped to `src/services/pipewire_capture.rs`, `wgpu-api/src/video.rs`, `wgpu-api/src/backend.rs`, and render consumers.
