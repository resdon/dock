# WAYDOCK — WGPU Backend Engineering Specification

## 1. Project objective

Build a custom, modular, memory-efficient GPU rendering backend for Waydock.

Technology:

- Rust
- wgpu
- Vulkan backend
- WGSL shaders
- Wayland presentation
- Custom rendering pipeline
- Slint compatibility through an independent adapter

The renderer must support sophisticated graphics, animations, interaction and physics while minimizing CPU usage, system RAM and VRAM.

## 2. Mandatory optimization priorities

Priority order:

1. **Lowest possible CPU usage.**
2. **Lowest possible system RAM usage.**
3. **Highest possible responsiveness.**
4. **Graphics, animations, interactivity and Slint integration.**
5. **Lowest possible VRAM usage.**

Every architectural decision must respect this order.

Do not sacrifice the first three priorities unnecessarily to improve visual effects.

## 3. Core rendering philosophy

Use a hybrid CPU/GPU architecture.

### CPU responsibilities:

- Receive and process Wayland events.
- Maintain widget and application state.
- Perform necessary layout calculations.
- Update compact GPU parameters.
- Schedule animations and rendering.
- Manage GPU resource lifetimes.

### GPU responsibilities:

- Render geometry.
- Calculate visual transformations.
- Perform icon magnification.
- Calculate opacity and color transitions.
- Render rounded rectangles and gradients.
- Apply shadows, glow and reflections.
- Perform optional post-processing.
- Execute GPU-assisted particle effects when justified.

Avoid CPU-side per-frame geometry generation whenever possible.

Do not transfer data between CPU and GPU unless necessary.

## 4. Rendering architecture

Implement a modular renderer with a fast primary rendering path and optional effects graph.

### Primary renderer

Must support:

- Instanced rectangles.
- Rounded rectangles using signed distance fields.
- Icons and images.
- Texture atlases.
- Text and glyph atlases.
- Gradients.
- Borders.
- Basic shadows.
- Clipping.
- GPU-assisted transformations.
- Opacity and color transitions.

The primary renderer must operate without allocating unnecessary intermediate textures.

### Optional effects graph

Support independently configurable effects:

- Gaussian blur.
- Bloom.
- Glow.
- Reflections.
- Distortion.
- Particles.
- Advanced compositing.
- Glass-like effects.

Effects must use lazy resource allocation.

If an effect is disabled, it must not consume persistent GPU memory or perform unnecessary rendering work.

Use reduced-resolution intermediate textures whenever visual quality permits.

Avoid full-resolution intermediate render targets unless justified.

## 5. CPU optimization

### Mandatory rules:

- No continuous idle rendering.
- No unnecessary polling.
- No per-frame heap allocations in steady state.
- No per-icon draw calls when instancing is appropriate.
- No unnecessary per-frame GPU buffer uploads.
- No repeated shader compilation.
- No blocking GPU waits.
- No GPU readback for ordinary UI state.
- No redundant layout calculations.
- No unnecessary render graph traversal.

### Use:

- Persistent buffers.
- Compact instance data.
- Reusable command and resource structures.
- Incremental scene updates.
- Cached pipelines.
- Event-driven rendering.
- GPU-side transformations.

Do not assume that moving every calculation to the GPU improves efficiency. Profile the CPU/GPU tradeoff.

## 6. RAM optimization

The backend must minimize persistent host memory.

Requirements:

- Avoid unnecessary resource duplication.
- Use compact CPU-side scene structures.
- Bound all caches.
- Reuse vectors and temporary storage.
- Avoid excessive worker threads.
- Avoid unnecessary task queues.
- Decode images outside the rendering-critical path.
- Avoid retaining unused image data.
- Release obsolete resources.
- Keep GPU metadata and CPU metadata compact.

Do not maintain multiple complete copies of the scene unless synchronization requires them.

## 7. VRAM optimization

Requirements:

- Share icon textures through atlases.
- Share glyph textures through a glyph atlas.
- Reuse buffers.
- Avoid individual textures for every icon.
- Avoid unnecessary MSAA.
- Avoid unnecessary depth buffers.
- Allocate intermediate render targets lazily.
- Use reduced-resolution effect targets.
- Bound texture atlas growth.
- Release obsolete textures.
- Avoid unnecessary mipmaps for small UI assets.

Use an explicit VRAM budget.

The renderer must be able to operate with advanced effects disabled and a minimal persistent resource set.

## 8. Animation and interaction

Animations must remain smooth and responsive without excessive CPU usage.

Use GPU-assisted calculations for:

- Icon magnification.
- Neighbor displacement.
- Hover scaling.
- Opacity.
- Color interpolation.
- Simple visual spring behavior.
- Transformations.

Use analytical spring equations for ordinary animations instead of maintaining a general-purpose physics simulation.

GPU particle simulation is optional and must be bounded.

The pointer position must be updated immediately.

Do not unnecessarily smooth or delay raw pointer input.

Use a monotonic animation clock.

Animation calculations must be independent of frame rate.

Target:

- At least 60 FPS during ordinary animations.
- Higher refresh rates when available.
- No continuous rendering when animations finish.
- No unnecessary updates when the scene is unchanged.

## 9. Presentation and frame scheduling

Use the Wayland compositor's frame scheduling mechanisms.

Use:

- Wayland frame callbacks.
- Presentation feedback when available.
- Event-driven rendering.
- Monotonic timestamps.
- Minimal frames in flight.

Initial configuration:

- FIFO presentation as a reliable baseline.
- Two frames in flight as the initial balance.
- Benchmark one frame in flight for latency improvements.
- No forced rendering at a fixed FPS.
- Adapt to the display refresh rate.

Do not render while idle.

Do not confuse frame callbacks with confirmation that a frame was physically displayed.

Measure presentation timing where possible.

## 10. Vulkan and wgpu

Use wgpu with Vulkan as the selected backend.

Initial backend configuration:

- Vulkan only.
- WGSL shaders.
- High-performance adapter preference.
- No forced software fallback.
- Surface-compatible adapter selection.
- Minimal required device features.
- Minimal required device limits.

Avoid unnecessary Vulkan-specific code outside the wgpu integration layer.

Do not enable additional graphics backends unless explicitly required.

Keep wgpu version-specific APIs isolated.

Pin compatible dependencies and commit Cargo.lock.

## 11. Resource manager

Create a centralized GPU resource manager.

Responsibilities:

- Device ownership.
- Queue ownership.
- Surface management.
- Pipeline cache.
- Shader cache.
- Texture atlas.
- Glyph atlas.
- Static geometry buffers.
- Dynamic instance buffers.
- Uniform buffers.
- Optional effect textures.
- Resource lifetime management.

Resource allocation must be predictable.

Avoid allocating GPU resources in response to ordinary pointer movement or hover transitions.

## 12. Modular crate architecture

The `wgpu-api` crate already provides the foundation. The following modules exist and need GPU backend implementation:

```
wgpu-api/src/
├── backend.rs          ← GPU device, queue, surface initialization
├── renderer.rs         ← Frame rendering, scene traversal, command encoding
├── gpu.rs              ← Pipeline types, caches, uniform buffers, presentation
├── batching.rs         ← Draw command batching and sorting
├── scene.rs            ← Scene graph (Node, NodeContent, NodeStyle)
├── geometry.rs         ← Vector2D, Rect, Color, Transform, NodeId
├── color.rs            ← Color types and conversions
├── text/               ← Text shaping, glyph atlas, font database
├── physics/            ← Physics engine (springs, fields, constraints)
├── animation/          ← Easing, keyframes, particles
├── layout/             ← Dock layout, arrangement, insertion, projection
├── interaction/        ← Input normalization and event routing
├── wayland/            ← Wayland client abstraction
├── video/              ← PipeWire video integration
└── error.rs            ← Subsystem-specific error types
```

### Implementation status

| Module | Status | Notes |
|--------|--------|-------|
| `backend.rs` | **Stub** | Types defined, no wgpu device/queue |
| `renderer.rs` | **Stub** | Scene traversal counts frames, no GPU submission |
| `gpu.rs` | **Stub** | Pipeline types defined, no wgpu pipelines |
| `batching.rs` | **Ready** | Batching logic complete, needs GPU submission |
| `scene.rs` | **Ready** | Full scene graph with all content types |
| `geometry.rs` | **Ready** | All geometry types complete |
| `physics/` | **Ready** | Fully implemented and tested (320 tests) |
| `text/` | **Ready** | cosmic-text shaping, glyph atlas |
| `animation/` | **Ready** | Easing, keyframes, particles |
| `layout/` | **Ready** | Dock layout, arrangement, insertion |
| `interaction/` | **Ready** | Input normalization, event routing |
| `error.rs` | **Ready** | 11 subsystem error types |

### What needs implementation

1. **Add wgpu dependency** to `wgpu-api/Cargo.toml`
2. **Implement `backend.rs`**: wgpu device, queue, surface creation
3. **Implement `gpu.rs`**: Actual wgpu render pipelines, shaders, buffers
4. **Implement `renderer.rs`**: GPU command encoding and submission
5. **Wire DMA-BUF presentation**: Connect to existing `dmabuf_export.rs` patterns
6. **Implement texture atlas**: Replace per-icon textures with shared atlas
7. **Implement glyph atlas**: Shared text rendering
8. **Implement uniform buffers**: Animation and transform uniforms
9. **Implement WGSL shaders**: Textured quad, solid color, text, effects

## 13. Public interfaces

The renderer should expose a small stable API.

Conceptual interface:

```rust
pub trait Renderer {
    type Error;

    fn resize(
        &mut self,
        width: u32,
        height: u32,
    ) -> Result<(), Self::Error>;

    fn update_scene(
        &mut self,
        scene: &Scene,
    ) -> Result<(), Self::Error>;

    fn update_animation(
        &mut self,
        animation: &AnimationState,
    );

    fn render(&mut self) -> Result<(), Self::Error>;
}
```

The implementation may add asynchronous resource preparation, texture upload and effect configuration interfaces.

Avoid exposing wgpu internal resource types to the application layer unnecessarily.

## 14. Error handling and recovery

Handle:

- Surface lost.
- Surface outdated.
- Surface timeout.
- Zero-size surfaces.
- Device loss.
- Out-of-memory conditions.
- Unsupported surface formats.
- Unsupported presentation modes.

Do not spin or busy-loop on rendering failures.

Reconfigure surfaces when appropriate.

Recover from device loss where possible, but ensure graceful shutdown when recovery is impossible.

## 15. Performance profiling

Profile before optimizing.

Measure separately:

### CPU

- Idle CPU consumption.
- Event processing time.
- Scene update time.
- Command encoding time.
- Allocation count.
- Worker thread activity.

### RAM

- Resident memory.
- Peak memory.
- Allocation count.
- Cache sizes.
- Image decoding memory.

### GPU

- VRAM usage.
- Texture allocation.
- Buffer allocation.
- Render-pass count.
- GPU execution time.
- Intermediate render target sizes.

### Responsiveness

- Input processing latency.
- Frame scheduling delay.
- Presentation timing.
- Animation frame consistency.

Use Tracy, perf, heaptrack, wgpu tracing and RenderDoc where appropriate.

## 16. Performance acceptance criteria

Initial engineering targets:

- Near-zero CPU usage while idle.
- Zero unnecessary idle rendering.
- Zero steady-state frame allocations.
- At least 60 FPS during ordinary animations on suitable hardware.
- Higher refresh support when available.
- No unnecessary texture uploads.
- No blocking GPU readback.
- Bounded CPU and GPU caches.
- Minimal persistent VRAM allocation.
- No unnecessary full-resolution intermediate targets.

Do not claim these targets are achieved without measurement.

## 17. Development order

Implement in this order:

1. **Vulkan device and surface** — wgpu initialization in `backend.rs`
2. **Clear-only rendering** — First GPU frame, verify surface presentation
3. **Basic geometry pipeline** — Solid color rectangles via wgpu render pipeline
4. **Instanced rendering** — Batch multiple rectangles in one draw call
5. **Texture atlas** — Shared icon textures, replace per-icon allocations
6. **Text rendering** — cosmic-text shaping + glyph atlas + GPU text pipeline
7. **Incremental scene updates** — Only upload changed instance data
8. **Animation uniforms** — Spring parameters, transform matrices on GPU
9. **GPU-assisted magnification** — Icon magnification calculated in vertex shader
10. **Event-driven presentation** — Wayland frame callbacks, no idle rendering
11. **Optional effects graph** — Lazy-allocated blur, glow, reflections
12. **Advanced effects** — Glass, distortion, particles
13. **Resource profiling and optimization** — Measure and optimize all metrics

Keep the project compiling after every milestone.

## 18. Mandatory engineering rules

Before introducing a new rendering feature, answer:

1. How much CPU does it require?
2. How much persistent RAM does it require?
3. How much VRAM does it require?
4. Does it require an additional render pass?
5. Can it be implemented in an existing shader?
6. Can it be allocated only when needed?
7. Can its resources be reused?
8. Does it affect input-to-presentation latency?

Prefer the simplest implementation that meets visual requirements.

Never introduce a complex GPU subsystem without demonstrating its performance or visual benefit.

The final objective is a sophisticated, customizable dock renderer that remains lightweight in idle and active operation.

---

## Appendix A: Current wgpu-api Module Map

| File | Lines | Public Types | GPU Backend Status |
|------|-------|-------------|-------------------|
| `backend.rs` | 590+ | `GpuBackend`, `GpuConfig`, `GpuStats` | **Implemented** — wgpu device/queue, pipelines, shaders |
| `renderer.rs` | 1043+ | `Renderer`, `RenderConfig`, `RenderStats` | **Implemented** — scene traversal, draw command encoding, GPU submission |
| `gpu.rs` | 1805+ | `PipelineType`, `TextureCache`, `BufferCache`, `UniformBuffer`, `GpuResources`, `SurfacePresenter` | **Implemented** — actual wgpu pipelines |
| `batching.rs` | 993 | `Batch`, `DrawCommand`, `BatchSorter` | **Ready** — logic complete |
| `scene.rs` | 1001 | `Scene`, `Node`, `NodeContent`, `NodeStyle` | **Ready** — full scene graph |
| `geometry.rs` | — | `Vector2D`, `Rect`, `Transform`, `NodeId` | **Ready** |
| `color.rs` | — | `Color`, `Brush` | **Ready** |
| `error.rs` | 720 | `DockError`, `GpuError`, `RendererError`, etc. | **Ready** |
| `slint.rs` | — | `SlintWgpuAdapter` | **Implemented** — Slint wgpu-30 integration |

## Appendix B: Current Dock Rendering Pipeline

| Component | Current Method | Target Method |
|-----------|---------------|---------------|
| Main dock | Slint → CPU SHM buffer (headless Wayland) | wgpu-api → DMA-BUF (blocked by Slint architecture) |
| Popups (context menu, window list) | CPU SHM buffer (FemtoVG removed) | wgpu-api → DMA-BUF |
| Side bars | CPU SHM buffer | wgpu-api → DMA-BUF |
| Icons | CPU pixel blend | GPU texture sample from atlas |
| Text | CPU raster | GPU text from glyph atlas |
| Background | No-op (CPU pixel rendering removed) | GPU rounded rect primitive |
| Physics animation | None | 120 Hz analytical springs |

## Appendix C: wgpu Dependency Plan

```toml
# wgpu-api/Cargo.toml additions
[dependencies]
wgpu = { version = "30", features = ["vulkan"] }
wgpu-dmabuf = "0.1"
```

The `wgpu-api` crate now depends on `wgpu 30` directly. `wgpu-dmabuf 0.1` also depends on `wgpu 30.0.1`, so no version conflict.

## Appendix D: Shader Plan

| Shader | Purpose | Inputs | Outputs |
|--------|---------|--------|---------|
| `dock.wgsl` | Main vertex + fragment | Instance buffer, texture atlas | Color target |
| `shapes.wgsl` | Rounded rects, SDF primitives | Instance buffer, shape params | Color target |
| `icons.wgsl` | Textured quads with magnification | Instance buffer, icon atlas | Color target |
| `text.wgsl` | Glyph rendering | Instance buffer, glyph atlas | Color target |
| `effects.wgsl` | Blur, glow, bloom, reflections | Source texture, effect params | Effect target |

## Appendix E: Integration with Existing Code

### FemtoVG Replacement — Complete

FemtoVG has been removed from the codebase. The `src/render/femtovg_wgpu.rs` file (1277 lines) has been deleted. All popup rendering now uses CPU SHM buffers as a fallback until wgpu-api's Renderer is fully integrated into the app layer.

The wgpu-api crate provides the replacement:
- `GpuBackend` replaces `GpuContext` with wgpu device/queue
- `Renderer` + `Scene` replace FemtoVG canvas operations
- `TextureCache` replaces `FemtoVGIconCache`

### DMA-BUF Export

Current `src/render/dmabuf_export.rs` (~222 lines) is well-implemented:

1. `wgpu_dmabuf::export()` creates DMA-BUF textures
2. wgpu-api renderer renders to the texture
3. GPU command buffer submitted via queue
4. `WlBuffer` attached to Wayland surface

The wgpu-api renderer follows this same pattern.

### Physics Integration

The physics engine (`wgpu-api/src/physics/`) is fully implemented:

- `PhysicsWorld` with springs, fields, constraints
- `InteractionApi` for semantic operations (drag, push, pull, insert)
- `VisualEffects` for squash, stretch, glow from physics state
- `FixedTimestepSimulator` at 120 Hz

Integration:

1. Physics runs on CPU at fixed timestep
2. Physics produces `MotionState` for each body
3. `VisualEffects` derives `VisualTransform` from `MotionState`
4. `VisualTransform` updates scene node positions/scales
5. Renderer uploads instance data to GPU

## Appendix F: ADR Cross-References

| ADR | Relevance |
|-----|-----------|
| [ADR 001](001-renderer-ownership.md) | Renderer owns wgpu device/queue — enforced by this spec |
| [ADR 002](002-physics-timestep.md) | Fixed 120 Hz timestep — physics section references this |
| [ADR 003](003-coordinate-systems.md) | Coordinate space discipline — all shaders must respect this |
| [ADR 004](004-video-buffer-lifetime.md) | PipeWire buffer lifetime — video section references this |
| [ADR 005](005-waydock-priorities.md) | Core priorities — this spec is the GPU implementation of ADR 005 |
