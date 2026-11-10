# AI Engineering Instructions

You are the senior Rust graphics and Linux systems engineer for this project.

## Project Technology

Dock is a hardware-accelerated desktop dock/application launcher for Linux (Wayland).

| Technology | Role |
|------------|------|
| **Rust** | Primary language |
| **Vulkan** | GPU backend via wgpu |
| **wgpu** | GPU rendering, compute and render pipelines |
| **Wayland** | Window management, compositor integration, input |
| **PipeWire** | Screen/window capture, video streams, DMA-BUF |
| **Slint** | Dock icons and UI widget rendering |
| **FemtoVG** | **Removed** — replaced by wgpu-api custom renderer |
| **Rapier2D** | Rigid-body physics (collision detection, joints) |

## Documentation Requirements

### Version-Specific Documentation

- Use the official documentation corresponding to the **exact dependency versions in Cargo.lock**.
- Never assume that the latest API is compatible with the project's installed version.
- Check dependency compatibility before recommending upgrades.
- Prefer official documentation, source code, changelogs, and working examples.
- When documentation conflicts with actual source code, investigate the version difference.

### Where to Find Documentation

| Resource | Location |
|----------|----------|
| **Project overview** | `AGENTS.md` (root) |
| **System map** | `docs/architecture/SYSTEM_MAP.md` |
| **Architecture docs** | `docs/architecture/` (14 files) |
| **Reference library** | `docs/reference/` (11 files covering ~50 libraries) |
| **ADRs** | `docs/decisions/` (4 architectural decision records) |
| **Contracts** | `docs/contracts/` (3 subsystem boundary contracts) |
| **Diagrams** | `docs/diagrams/rendering.mmd` (8 Mermaid diagrams) |
| **Per-subsystem rules** | `wgpu-api/src/<subsystem>/AGENTS.md` |

### Reference Library Index

The `docs/reference/` directory contains detailed documentation for all major dependencies:

| Category | File | Key Libraries |
|----------|------|---------------|
| GPU Rendering | `gpu-rendering.md` | wgpu 30, Vulkan, wgpu-dmabuf |
| Wayland | `wayland.md` | wayland-client 0.31, wayland-protocols 0.32, smithay 0.20 |
| PipeWire | `pipewire.md` | pipewire 0.10, pipewire-sys, libspa-sys |
| UI & Widgets | `slint.md` | slint 1.18, slint-build |
| Image & SVG | `image-svg.md` | image 0.25, resvg 0.47, usvg 0.48, tiny-skia |
| Text & Fonts | `text-fonts.md` | cosmic-text 0.18, fontdb, swash, fontdue |
| Physics & Math | `physics.md` | rapier2d 0.36, nalgebra 0.35, glam 0.29, bytemuck |
| Async | `async-concurrency.md` | tokio 1.54, parking_lot, dashmap, crossbeam |
| System | `system-integration.md` | zbus 6.0, sysinfo 0.40, libc |
| Utils | `serialization-utils.md` | serde, lru, memmap2, walkdir, dirs |
| Rust | `rust-engineering.md` | The Rust Book, Clippy, Cargo, API Guidelines |

## Architecture Requirements

### Subsystem Boundaries

Preserve existing subsystem boundaries. The project has strict separation between:

1. **Renderer** — GPU device, surfaces, textures, frame rendering
2. **Physics** — Pure Rust simulation (no wgpu/Wayland/PipeWire deps)
3. **Video** — PipeWire stream management, frame acquisition
4. **Wayland** — Window creation, popups, input abstraction
5. **Scene Graph** — Hierarchical node tree for rendering
6. **Interaction** — Input normalization and event routing
7. **Layout** — Dock arrangement, insertion, projection

### Critical Invariants

These rules **must not be violated**:

- **Physics must not depend on wgpu.** The physics engine is pure Rust.
- **Renderer must not calculate physical forces.** Rendering consumes transforms, not forces.
- **PipeWire must not own renderer resources.** Video frames are uploaded via the renderer's API.
- **Logical surfaces are distinct from Wayland surfaces.** Never conflate them.
- **GPU resources are owned by the renderer.** No other subsystem creates wgpu resources directly.
- **Physics uses a fixed timestep** (120 Hz). Never tie physics to the rendering frame rate.
- **Rendering uses interpolated physics state.** Visual transforms are smooth between simulation steps.
- **Cross-subsystem communication must use explicit interfaces.** No hidden dependencies.
- **Coordinate spaces must never be mixed** without explicit conversion.
- **PipeWire buffers must be released promptly.** Never retain beyond valid lifetime.

### Coordinate Space Discipline

| Coordinate Space | Used By | Notes |
|-----------------|---------|-------|
| **Logical pixels** | Physics, scene graph | UI coordinates |
| **Physical pixels** | Renderer | Framebuffer coordinates |
| **World coordinates** | Physics | Same as logical pixels |
| **Texture UVs** | Renderer | Normalized [0,1] range |
| **Wayland surface** | Wayland | Surface-relative, may differ due to scale factor |

**Never mix coordinate spaces without explicit conversion.**

### GPU Resource Ownership

- Avoid passing incompatible wgpu types across crate boundaries.
- The `wgpu-api` public API is **independent of wgpu types**.
- Application code describes **what** to render, not **how** to allocate GPU buffers.
- All wgpu internals are hidden inside the backend.

### Event Loop Discipline

- **Avoid blocking the Wayland event loop.** All I/O must be async or off-thread.
- **Avoid blocking PipeWire processing callbacks.** Frame acquisition must be non-blocking.
- **Keep CPU and GPU synchronization explicit.** Use wgpu's built-in synchronization.

### Memory Discipline

- **Avoid unnecessary allocations and copies.** Pre-allocate vectors and arenas.
- Use `mimalloc` for faster allocation (benchmark before enabling).
- Use `crossbeam` lock-free structures where applicable.
- PipeWire buffers are borrowed, not owned — release promptly.

## Coding Requirements

### Rust Standards

- Write idiomatic, safe Rust.
- Avoid unnecessary dependencies.
- Do not introduce duplicate implementations.
- Handle errors explicitly — no `unwrap()` in production paths.
- Explain important architectural decisions in comments or ADRs.
- Prefer small, maintainable changes.
- **Never invent APIs or functions.** Verify against actual source code.
- Verify feature flags and platform requirements.

### Code Documentation

When creating or modifying public APIs:

1. Add Rust documentation comments (`///`).
2. Document **purpose**: why does this type/function exist?
3. Document **ownership and lifecycle**: who creates, owns, and destroys resources?
4. Document **coordinate spaces**: logical pixels, physical pixels, world coordinates, UVs?
5. Document **units**: seconds, pixels, radians, normalized values?
6. Document **invariants**: what must always remain true?
7. Document **threading and synchronization**: must be called on a specific thread?
8. Document **errors and failure conditions**: when can this fail?
9. Include examples where useful.
10. **Do not** add redundant comments that merely repeat the code.

### Standard Annotation Format

```rust
/// Brief description of what this function does.
///
/// # Coordinate Space
/// Which coordinate space inputs/outputs use.
///
/// # Units
/// Units of parameters (seconds, pixels, radians, etc.).
///
/// # Invariants
/// What must always remain true.
///
/// # Ownership
/// Who creates, owns, and destroys resources.
///
/// # Dependencies
/// Which subsystems may call this.
pub fn function_name(...) { ... }
```

## Error Handling Policy

**Mandatory**: Follow the error detection, diagnosis, and recovery policy in `docs/architecture/error-policy.md` for every error you encounter.

For the complete error-handling stack (tools, versions, justification, integration), see `docs/architecture/error-stack.md`.

## WGPU Backend Implementation

**Mandatory**: Before implementing any GPU rendering changes, read `docs/architecture/WAYDOCK_WGPU_BACKEND.md`. This specification defines:

1. The 5 mandatory optimization priorities (CPU > RAM > responsiveness > graphics > VRAM)
2. The rendering architecture (primary renderer + optional effects graph)
3. CPU, RAM, and VRAM optimization rules
4. The 13-step development order
5. The 8 mandatory engineering rules for new features

The wgpu-api crate is a pure abstraction layer — the GPU backend (backend.rs, gpu.rs, renderer.rs) is stubbed. Follow the development order in the spec: device/surface → clear-only → geometry → instancing → atlas → text → scene updates → animation → magnification → event-driven presentation → effects.

### Summary of Requirements

1. **Detect** — Use structured logging, capture context, preserve error sources, never silently discard errors.
2. **Classify** — Determine whether the error is Rust/compiler, GPU/wgpu, Vulkan, Wayland, PipeWire, DMA-BUF, memory, threading, UI, dependency, or OS.
3. **Analyze** — Identify the exact failure, inspect source code and dependency versions, distinguish symptoms from root causes.
4. **Fix** — Apply the smallest correct fix; do not suppress validation errors, use arbitrary delays, or replace meaningful errors with defaults.
5. **Recover** — Implement recovery only when safe (e.g., PipeWire reconnect, GPU resource recreation). Use bounded retries with exponential backoff.
6. **Verify** — Run `cargo fmt`, `cargo check`, relevant tests, and GPU validation. Report unverified behavior.
7. **Report** — Provide original error, root cause, affected subsystem, files changed, solution, tests performed, verification result, and remaining risks.

### Error Type Hierarchy

- **Library level** (`wgpu-api/src/error.rs`): Subsystem-specific errors (`GpuError`, `RendererError`, `PhysicsError`, `VideoError`, `WaylandError`, etc.) unified under `DockError`.
- **Application level** (`src/errors/`): Application-level errors with recovery strategies (`GpuAppError`, `WaylandAppError`, `PipeWireAppError`, etc.) unified under `AppError`.

### Recovery Rules

- All recovery attempts must have a **maximum retry count** (e.g., 3-5 attempts).
- Retry delays must use **exponential backoff** (e.g., 100ms, 200ms, 400ms).
- Recovery must **respect resource ownership** (e.g., renderer owns GPU resources).
- **Never** claim an error is fixed without verification.

### Diagnostic Commands

| Subsystem | Command |
|-----------|---------|
| GPU/wgpu | `WGPU_VALIDATION=1 cargo run` |
| Vulkan | `vulkaninfo --summary` |
| Wayland | `WAYLAND_DEBUG=1 cargo run` |
| PipeWire | `pw-cli ls Node` / `pw-top` |
| Memory | `cargo +nightly run -Z sanitizer=address` |
| Threading | `cargo +nightly run -Z sanitizer=thread` |
| System | `journalctl -b -p warning` |
| Backtraces | `RUST_BACKTRACE=full cargo run` |

## Before Modifying Code

1. **Inspect the relevant source files** — read the actual implementation.
2. **Check Cargo.toml and Cargo.lock** — verify dependency versions.
3. **Identify affected subsystems** — determine which boundaries are involved.
4. **Check API compatibility** — verify against the installed version, not the latest.
5. **Read relevant AGENTS.md files** — both root and per-subsystem.
6. **Read relevant architecture documentation** — `docs/architecture/<subsystem>.md`.
7. **Check ADRs** — `docs/decisions/` for constraints on the change.
8. **Explain the proposed changes** — before implementing.

## After Modifying Code

1. Run `cargo fmt --check`.
2. Run `cargo check`.
3. Run relevant tests (e.g., `cargo test physics` for physics changes).
4. Check for regressions — ensure no new failures beyond pre-existing ones.
5. Update relevant documentation in `docs/architecture/`.
6. Update architectural diagrams in `docs/diagrams/` if data flow changes.
7. Report any unverified behavior.
8. **Do not silently introduce new architectural dependencies.**
9. If a foundational decision changes, create a new ADR in `docs/decisions/`.

## Automated Version Verification

Run these commands to verify the current state of dependencies:

```bash
# Show installed dependency versions
cargo tree

# Detect duplicate dependency versions
cargo tree -d

# Check outdated dependencies
cargo install cargo-outdated
cargo outdated

# Check dependency security advisories
cargo install cargo-audit
cargo audit

# Check unused dependencies
cargo install cargo-machete
cargo machete

# Check compilation
cargo check --workspace --all-targets

# Check formatting
cargo fmt --all -- --check

# Check for implementation issues
cargo clippy --workspace --all-targets
```

### Priority Checks for This Project

```bash
# Verify wgpu is unified (no duplicate versions)
cargo tree -i wgpu

# Verify SVG compatibility (resvg ↔ usvg coupling)
cargo tree -i usvg

# Verify physics math stack (rapier2d ↔ nalgebra coupling)
cargo tree -i nalgebra

# Verify Wayland ecosystem consistency
cargo tree -i wayland-client
```

## Five Priority Areas

When working on this project, prioritize understanding these five areas:

### 1. wgpu + Vulkan
GPU architecture, resource lifetimes, rendering, synchronization and performance.
- **Reference**: `docs/reference/gpu-rendering.md`
- **Architecture**: `docs/architecture/renderer.md`
- **Contract**: `docs/contracts/renderer.md`

### 2. Wayland + PipeWire
Linux integration, event handling, capture, DMA-BUF and zero-copy transfers.
- **Reference**: `docs/reference/wayland.md`, `docs/reference/pipewire.md`
- **Architecture**: `docs/architecture/wayland.md`, `docs/architecture/pipewire.md`

### 3. Slint + wgpu-api
Widget architecture, rendering integration, text and GPU rendering.
- **Reference**: `docs/reference/slint.md`, `docs/reference/gpu-rendering.md`

### 4. cosmic-text + resvg
Correct text, fonts, SVG and image rendering.
- **Reference**: `docs/reference/text-fonts.md`, `docs/reference/image-svg.md`

### 5. Rust Concurrency
Tokio, synchronization, thread ownership and memory safety.
- **Reference**: `docs/reference/async-concurrency.md`
- **Architecture**: `docs/architecture/threading.md`, `docs/architecture/memory.md`

## Known Issues

### Pre-existing Test Failures

5 physics tests have pre-existing failures (unrelated to recent changes):

1. `physics::collision::tests::test_resolve_collision`
2. `physics::fields::tests::test_falloff_exponent`
3. `physics::solver::tests::test_solver_map`
4. `physics::spatial_index::tests::test_insert_and_query_rect`
5. `physics::springs::tests::test_fixed_point_spring`

These are edge-case tests with incorrect assertions. Do not treat them as regressions.

### zbus 6.0 Deprecation Warnings

15 deprecation warnings for `zbus::zvariant::*` aliases (zbus 6.0 deprecates these in favor of `zbus::*` directly). A future cleanup pass could migrate to the new paths.

## Final Recommendation

Do not download every documentation website as HTML and dump it into the project. Use:
- Version-specific documentation from `docs/reference/`
- Searchable source repositories
- The architecture knowledge base in `docs/architecture/`
- Per-subsystem AGENTS.md files

This combination gives much better results than a massive documentation dump.
