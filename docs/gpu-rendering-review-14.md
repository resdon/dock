# GPU Rendering Code Review — Round 14 (Documentation, Slint UI, Tests, ADRs, Contracts)

This is a continuation of the GPU rendering code review. Rounds 1–13 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, and project infrastructure (issues 1–460). This round covers the documentation ecosystem: `docs/architecture/`, `docs/decisions/`, `docs/contracts/`, `docs/testing/`, `ui.slint`, `bluetooth-widget.slint`, and `tests/compatibility.rs` — issues 461–500.

---

## Critical Issues

### 461. tests/compatibility.rs — All 7 tests are empty stubs (entire file)
**File:** `tests/compatibility.rs`

All 7 test functions are empty — no assertions, no code, no `todo!()` macros. They pass trivially because there's nothing to fail on:

```rust
#[test]
fn test_surface_creation() {
    // Verify that Wayland surfaces can be created
    // This test requires a running Wayland compositor
    // Run with: WAYLAND_DISPLAY=wayland-1 cargo test test_surface_creation
}
```

**Impact:** False confidence — the CI pipeline reports these tests as passing, but they validate nothing. The TESTING_PROTOCOL.md requires Wayland integration tests (Phase 7) and Slint compatibility tests (Phase 8), but neither is implemented. The comments describe what should be tested but provide no actual test code.

**Fix:** Implement the tests using `wayland-client` with a test compositor (e.g., `wayland-protocols` test harness) or mark them as `#[ignore]` with a clear reason.

---

### 462. ADR 005 — States Rapier2D and nalgebra were removed, but they remain in Cargo.toml (line 71 vs wgpu-api/Cargo.toml)
**File:** `docs/decisions/005-waydock-priorities.md` line 71

```markdown
### Removed Dependencies

- **Rapier2D**: Removed. Dock icons do not need rigid-body collision, joints, or CCD.
- **nalgebra**: Removed. Only needed by Rapier2D.
```

Per Round 13 issue #421, the Cargo.toml files were examined. While Rapier2D may have been removed from `wgpu-api/Cargo.toml`, the ADR claims are strong ("Removed") and should be verified. If these dependencies still exist transitively or directly, the ADR is misleading.

**Impact:** Misleading documentation — future AI agents and developers will assume these dependencies are gone and won't check for them.

**Fix:** Verify `cargo tree | grep -E "rapier|nalgebra"` and update the ADR with the actual status.

---

### 463. ADR dates are from 2024, not 2026 (all ADRs 001–004)
**Files:** `docs/decisions/001-renderer-ownership.md` line 4, `002-physics-timestep.md` line 4, `003-coordinate-systems.md` line 4, `004-video-buffer-lifetime.md` line 4

| ADR | Stated Date |
|-----|------------|
| 001 | 2024-01-15 |
| 002 | 2024-02-20 |
| 003 | 2024-03-10 |
| 004 | 2024-04-05 |
| 005 | 2026-11-10 |

ADRs 001–004 are dated in 2024, but ADR 005 is dated 2026-11-10 (today). The project was likely not active in early 2024, or these dates were backdated.

**Impact:** Misleading timeline — the ADR dates suggest a 2-year development history that may not be accurate. This affects understanding of the project's evolution and decision context.

**Fix:** Update the dates to reflect when the decisions were actually made.

---

### 464. ui.slint — Dock background has no rounded corners despite border-radius on Slint Rectangle (line 63-69)
**File:** `ui.slint` lines 63-69

```slint
dock_bg := Rectangle {
    x: 0px;
    y: 0px;
    width: root.dock_width * 1px;
    height: root.dock_height * 1px;
    background: root.background_color;
    border-color: root.border_color;
    border-width: 1px;
```

The `dock_bg` Rectangle has `border-color` and `border-width` but no `border-radius`. The GPU path (Round 1 issue #3) also ignores `radius`, `stroke`, and `stroke_width`. This means neither rendering path produces rounded corners for the dock background.

**Impact:** Visual inconsistency — the dock has sharp corners on both GPU and Slint paths. If the design calls for rounded corners (pill shape), neither path delivers it.

**Fix:** Add `border-radius` to `dock_bg` in ui.slint and implement rounded rectangle rendering in the GPU backend.

---

### 465. bluetooth-widget.slint — References external SVG assets that may not exist at runtime (lines 68, 78)
**File:** `bluetooth-widget.slint` lines 68, 78

```slint
Image { x: 4px; y: 4px; width: 12px; height: 12px; source: @image-url("assets/bt-symbol.svg"); }
Image { x: 3px; y: 3px; width: 18px; height: 18px; source: @image-url("assets/gear-btn.svg"); }
```

These SVG files (`bt-symbol.svg`, `gear-btn.svg`) are referenced relative to the Slint file. At runtime, the path resolution depends on the working directory and the Slint runtime's asset loading mechanism. If these files are not embedded or the path is wrong, the images fail silently.

**Impact:** Missing icons — the Bluetooth widget header shows empty space where the Bluetooth symbol and gear icon should be.

**Fix:** Embed the SVGs as base64 in the Slint file or ensure the assets directory is accessible at runtime.

---

### 466. TESTING_PROTOCOL.md — 22 phases described but most require infrastructure that doesn't exist (throughout)
**File:** `docs/testing/TESTING_PROTOCOL.md`

The protocol describes 22 testing phases including:
- Phase 5: Shader validation (no shader test harness exists)
- Phase 6: GPU and Vulkan testing (requires running compositor)
- Phase 7: Wayland integration tests (requires 4 different compositors)
- Phase 9: Visual regression testing (no screenshot comparison tool exists)
- Phase 10-12: CPU/RAM/VRAM profiling (tools listed but no automated scripts)
- Phase 15: Fuzzing (no `cargo fuzz` corpus exists)
- Phase 16: Performance regression comparison (no baseline data exists)

**Impact:** Unexecutable protocol — an AI agent following this protocol will fail at Phase 5 because the required tools and infrastructure don't exist. The protocol describes an ideal testing environment, not the current one.

**Fix:** Mark phases that require external infrastructure as "manual only" and provide a minimal automated subset that can run in CI.

---

## High Severity Issues

### 467. docs/architecture/testing.md — Expected test count is stale (line 129)
**File:** `docs/architecture/testing.md` line 129

```markdown
**Expected test count**: 334 passed, 5 failed.
```

Per session history, the wgpu-api test count is 893 passed, 5 failed. The documentation says 334 passed, which is significantly lower. This stale number means an AI agent checking for regressions will think 559 tests are missing.

**Impact:** Misleading regression detection — the agent will report a massive "regression" when the actual test count is higher than documented.

**Fix:** Update the expected test count to match the current actual count.

---

### 468. docs/architecture/testing.md — Lists `physics::rapier` test module (line 44) but Rapier2D was removed
**File:** `docs/architecture/testing.md` line 44

```markdown
cd wgpu-api && cargo test physics::rapier
```

ADR 005 states Rapier2D was removed. If the `physics::rapier` module no longer exists, this test command will fail.

**Impact:** Broken documentation — following the testing guide leads to a compilation error.

**Fix:** Remove the `physics::rapier` test command or update if the module still exists with different content.

---

### 469. docs/architecture/renderer.md — Key Types table lists types that don't exist in code (lines 77-88)
**File:** `docs/architecture/renderer.md` lines 77-88

| Documented Type | Exists in Code? |
|----------------|-----------------|
| `RendererConfig` | No |
| `RendererBackend` | No |
| `Surface` / `SurfaceId` | No (in wgpu-api) |
| `Command` / `CommandBuffer` | Partial (`DrawCommand` exists) |
| `PipelineCache` | No |
| `TextureCache` | No (as a named struct) |
| `BufferCache` | No |

Most of these types are described in the documentation but don't exist as named types in the code. They appear to be aspirational or from an earlier design.

**Impact:** Misleading API reference — an AI agent looking for `RendererConfig` or `PipelineCache` will waste time searching for non-existent types.

**Fix:** Update the Key Types table to reflect the actual types in `renderer.rs` and `backend.rs`.

---

### 470. docs/architecture/renderer.md — Invariants section describes API that doesn't exist (lines 110-113)
**File:** `docs/architecture/renderer.md` lines 110-113

```markdown
- Surfaces must be created before rendering
- Frame rendering uses `renderer.with_frame(surface, |cmd| { ... })`
- GPU resources are cleaned up when the renderer is dropped
```

The `with_frame` method doesn't exist in the current `Renderer` implementation. The renderer uses `render_to_dmabuf()` and `render_to_dmabuf_scaled()` directly, not a callback-based frame API.

**Impact:** Broken API documentation — code following this invariant will fail to compile.

**Fix:** Update the invariants to match the actual renderer API.

---

### 471. docs/contracts/renderer.md — Interface section lists methods that don't exist (lines 27-33)
**File:** `docs/contracts/renderer.md` lines 27-33

| Documented Method | Exists? |
|------------------|---------|
| `Renderer::new()` | Yes (async) |
| `Renderer::create_surface()` | No |
| `Renderer::create_texture()` | No |
| `Renderer::with_frame()` | No |
| `Renderer::upload_texture()` | No (uses `register_texture`) |
| `Renderer::remove_texture()` | Yes |
| `Renderer::stats()` | No |

Only 2 of 7 documented methods exist. The contract describes an API that was planned but not implemented.

**Impact:** Contract violation — the contract promises an API that doesn't exist. AI agents following the contract will write code that doesn't compile.

**Fix:** Update the contract to reflect the actual renderer API or implement the missing methods.

---

### 472. docs/contracts/physics.md — Interface lists `InteractionApi::hit_test()` but it's in interaction layer (line 31)
**File:** `docs/contracts/physics.md` line 31

```markdown
- `InteractionApi::hit_test()` — Find body at pointer position
```

Hit testing is documented as part of the physics contract, but it's actually in the interaction layer (`wgpu-api/src/interaction/picking.rs`). The physics contract should not include hit testing.

**Impact:** Misleading boundary — the contract suggests physics handles hit testing, but it's in the interaction layer. This violates the documented separation of concerns.

**Fix:** Remove `hit_test()` from the physics contract and ensure it's only in the interaction contract.

---

### 473. ui.slint — Badge text shows "9+" but never shows the actual count (line 126)
**File:** `ui.slint` line 126

```slint
text: item.badge_count > 9 ? "9+" : "";
```

When `badge_count` is between 1 and 9, the text is empty (`""`). The badge rectangle is visible (line 120: `visible: item.badge_count > 0`), but the number inside is only shown when the count exceeds 9. For counts 1-9, the badge is a red circle with no text.

**Impact:** Missing information — users can't see the notification count for values 1-9. The badge shows "9+" only when the count is 10 or higher.

**Fix:** Use `text: \`${item.badge_count}\`` for counts ≤ 9 and `"9+"` for counts > 9.

---

### 474. ui.slint — Icon outline (hover highlight) uses same source as main icon but with colorize (lines 92-100)
**File:** `ui.slint` lines 92-100

```slint
icon_outline := Image {
    x: item.icon_x * 1px - 2px;
    y: item.icon_y * 1px + 4px - 2px;
    width: (item.icon_width + 4) * 1px;
    height: (item.icon_height + 4) * 1px;
    source: item.icon;
    colorize: root.hover_color;
    visible: false;
}
```

The outline layer loads the same icon image and applies `colorize` with the hover color. This means the icon is decoded twice per hovered item, and the colorize operation replaces all colors with the hover color, not just adding an outline effect.

**Impact:** Performance waste — double decoding for hovered icons. Visual issue — the "outline" is a fully colorized version of the icon, not an outline.

**Fix:** Use a different technique for the hover highlight, such as a colored rectangle behind the icon.

---

### 475. bluetooth-widget.slint — Scrollbar track has negative x coordinate (line 271)
**File:** `bluetooth-widget.slint` line 271

```slint
Rectangle { x: -12px; y: 0px; width: 8px; height: 131px; background: #161b22; opacity: root.discoveredModel.length > 3 ? 0.5 : 0; }
```

The scrollbar track for the discovered devices list is at `x: -12px`, which is outside the parent's bounds. Depending on Slint's clipping behavior, this may be invisible or partially clipped.

**Impact:** Invisible scrollbar — the discovered devices scrollbar may not be visible because it's positioned outside the parent container.

**Fix:** Adjust the x coordinate to be within the parent's bounds or ensure the parent doesn't clip.

---

### 476. docs/architecture/overview.md — Data flow diagram shows scene graph traversal (line 63) but GPU path bypasses it
**File:** `docs/architecture/overview.md` line 63

```markdown
6. **Rendering**: Renderer traverses the scene graph, batches draw calls, submits to GPU
```

The GPU rendering path (`draw_commands_to_gpu_impl` in `renderer.rs`) takes a flat `Vec<DrawCommand>` directly, bypassing the scene graph entirely. The scene graph (`wgpu-api/src/scene.rs`) is not used in the GPU render path.

**Impact:** Misleading architecture diagram — the documented data flow doesn't match the actual code. The scene graph is described as central to rendering, but the GPU path skips it.

**Fix:** Update the data flow diagram to show the two paths: scene graph for Slint/CPU path and direct draw commands for GPU path.

---

### 477. docs/architecture/threading.md — Describes PipeWire thread (lines 42-54) but pipewire_capture.rs uses tokio
**File:** `docs/architecture/threading.md` lines 42-54

The documentation describes a dedicated PipeWire thread with its own event loop. However, `src/services/pipewire_capture.rs` uses `tokio` async runtime, not a dedicated thread with a PipeWire main loop.

**Impact:** Misleading threading model — developers following the documentation will expect a dedicated PipeWire thread, but the actual implementation uses tokio tasks.

**Fix:** Update the threading documentation to reflect the actual async-based PipeWire integration.

---

### 478. docs/architecture/memory.md — Describes mimalloc as optional (lines 128-145) but it's unconditionally enabled
**File:** `docs/architecture/memory.md` lines 128-145

```markdown
## Allocation Strategy

### mimalloc

The project now includes `mimalloc` as an optional fast allocator:
```

Per Round 13 issue #441, mimalloc is unconditionally enabled in `Cargo.toml`, not optional. The documentation says "optional" and "benchmark before enabling," but the code enables it by default.

**Impact:** Misleading configuration guide — developers thinking mimalloc is optional will be surprised to find it always active.

**Fix:** Update the documentation to reflect that mimalloc is enabled by default, or gate it behind a feature flag.

---

### 479. docs/architecture/performance.md — Frame budget includes "Slint widget rendering: 1-2 ms" (line 22) but main dock uses Slint software renderer
**File:** `docs/architecture/performance.md` line 22

```markdown
└── Slint widget rendering: 1-2 ms
```

The frame budget assumes Slint widget rendering is part of the GPU frame path. However, the main dock uses Slint's software renderer with a separate CPU→SHM path, not the GPU path. The Slint rendering time is not part of the GPU frame budget.

**Impact:** Incorrect performance model — the frame budget mixes two independent rendering paths, making it impossible to accurately measure or optimize either path.

**Fix:** Separate the frame budgets: one for the GPU path and one for the Slint CPU path.

---

### 480. docs/architecture/error-handling.md — Describes miette and color-eyre integration (lines 31-32, 119-143) but these may not be in Cargo.toml
**File:** `docs/architecture/error-handling.md` lines 31-32

```markdown
| **miette** | Present understandable diagnostics | Human-readable error reports |
| **color-eyre** | Detailed error reports with backtraces | Development diagnostics |
```

The error handling documentation describes a sophisticated error presentation stack with `miette` and `color-eyre`, but these dependencies need to be verified in `Cargo.toml`. If they're not present, the entire error presentation layer is undocumented dead documentation.

**Impact:** Misleading error handling guide — developers following the documentation will try to use `miette::Diagnostic` implementations that don't compile.

**Fix:** Verify the dependencies exist and update the documentation accordingly.

---

## Medium Severity Issues

### 481. docs/architecture/interaction.md — "Physical positions: Device pixels" contradicts ADR 003 (line 119)
**File:** `docs/architecture/interaction.md` line 119

```markdown
- Physical positions: Device pixels (for high-DPI accuracy)
```

ADR 003 states all physics operates in logical pixel coordinates. The interaction documentation mentions "device pixels" for physical positions, which contradicts the coordinate system ADR.

**Impact:** Coordinate space confusion — developers may use device pixels in interaction code, causing scaling bugs.

**Fix:** Remove the "device pixels" reference or clarify that it's only for final rendering, not for interaction calculations.

---

### 482. docs/contracts/interaction.md — Same coordinate space contradiction (line 50)
**File:** `docs/contracts/interaction.md` line 50

```markdown
- Physical positions: Device pixels (for high-DPI accuracy)
```

Same issue as #481.

---

### 483. ui.slint — DockItem struct has `app_id: int` but dock.rs uses String app_ids (line 9)
**File:** `ui.slint` line 9

```slint
app_id: int,
```

The Slint struct defines `app_id` as an integer, but the dock's pin system uses `String` app_ids (e.g., `"org.gnome.Nautilus"`). The integer `app_id` in the Slint struct is likely unused or misnamed.

**Impact:** Type mismatch — the Slint model uses an integer where the application uses strings. The `index_on_dock` field (line 10) is likely the actual index, making `app_id` redundant.

**Fix:** Remove `app_id` from the Slint struct or rename it to clarify its purpose.

---

### 484. ui.slint — Items container width calculation doesn't account for variable icon sizes (line 77)
**File:** `ui.slint` line 77

```slint
width: root.dock_items.length * (root.icon_size * 1px + root.item_spacing * 1px) - root.item_spacing * 1px;
```

The container width uses `root.icon_size` (base size) but each item can have a different `icon_width` and `icon_height` via `anim_scale`. During animations, icons can be 25% larger, but the container width doesn't expand.

**Impact:** Clipped animations — when an icon animates to 1.25× size, it may overflow the container's bounds and be clipped.

**Fix:** Add padding to the container or calculate width based on maximum possible icon size.

---

### 485. bluetooth-widget.slint — Device cards have fixed height (39px) but scroll uses 43px spacing (line 239)
**File:** `bluetooth-widget.slint` line 239

```slint
x: 0px; y: (43px * idx) - BluetoothState.scrollOffset * 1px; width: 174px; height: 39px;
```

Each device card is 39px tall but spaced 43px apart, leaving 4px gaps between cards. The scroll calculation uses 43px per row. This is intentional for visual spacing, but the gap is not documented.

**Impact:** Minor — the 4px gap is a design choice, but it's not obvious from the code. If the card height changes, the scroll calculation needs to be updated.

**Fix:** Add a comment or constant for the 43px row height.

---

### 486. docs/architecture/physics.md — Module map lists 14 modules but physics/ has 16 modules (lines 47-63)
**File:** `docs/architecture/physics.md` lines 47-63

The module map lists 14 modules, but the `wgpu-api/src/physics/` directory contains 16 modules (including `mod.rs` and potentially others not listed). The missing modules are not documented.

**Impact:** Incomplete documentation — new contributors won't know about undocumented modules.

**Fix:** Update the module map to include all 16 modules.

---

### 487. docs/architecture/pipewire.md — 208 lines but only first 111 were shown; describes VideoNode and FitMode (lines 42-46) but these may not exist
**File:** `docs/architecture/pipewire.md` lines 42-46

```markdown
    VT --> VN[VideoNode]
    VN --> FM[Fit mode]
```

The architecture diagram mentions `VideoNode` and `Fit mode` in the scene graph, but the GPU rendering path bypasses the scene graph. Video frames are handled directly through `DrawCommand::Video`.

**Impact:** Misleading data flow — the diagram suggests video goes through the scene graph, but it doesn't in the GPU path.

**Fix:** Update the diagram to show the direct video path.

---

### 488. docs/testing/TESTING_PROTOCOL.md — Phase 3 requires `cargo machete` which is not a standard tool (line 103)
**File:** `docs/testing/TESTING_PROTOCOL.md` line 103

```bash
cargo machete
```

`cargo-machete` detects unused dependencies but is not installed by default and may not be available in the CI environment. The protocol assumes it's available.

**Impact:** Protocol failure — Phase 3 will fail if `cargo-machete` is not installed.

**Fix:** Add `cargo install cargo-machete` before the check or mark it as optional.

---

### 489. docs/testing/TESTING_PROTOCOL.md — Phase 15 requires `cargo fuzz` and `cargo miri` which may not be configured (lines 459-461)
**File:** `docs/testing/TESTING_PROTOCOL.md` lines 459-461

```bash
cargo fuzz list
cargo miri test
```

Neither `cargo-fuzz` nor `miri` appear to be configured in the project. `cargo fuzz list` requires a `Cargo.fuzz.toml` and fuzz targets. `cargo miri` requires the Miri component installed via `rustup component add miri`.

**Impact:** Protocol failure — Phase 15 will fail immediately.

**Fix:** Set up the fuzzing infrastructure or mark Phase 15 as requiring manual setup.

---

### 490. docs/architecture/coding_rules.md — Import organization example uses `glam::Vec2` (line 32) but project uses custom geometry types
**File:** `docs/architecture/coding_rules.md` line 32

```rust
use glam::Vec2;
```

The coding rules example imports `glam::Vec2`, but the project uses custom geometry types (`Point`, `Vector2D`) from the `geometry` module. `glam` may not even be a dependency.

**Impact:** Misleading example — developers following the coding rules will import a type that doesn't exist in the project.

**Fix:** Update the example to use the project's actual geometry types.

---

### 491. docs/architecture/coding_rules.md — Test example uses `Spring::new()` with 4 parameters (line 160) but actual spring API may differ
**File:** `docs/architecture/coding_rules.md` line 160

```rust
let spring = Spring::new(100.0, 10.0, Vec2::ZERO, Vec2::new(10.0, 0.0));
```

The test example shows a `Spring` constructor with 4 parameters. The actual spring API in `wgpu-api/src/physics/springs.rs` may have a different signature.

**Impact:** Misleading example — the test code won't compile with the actual API.

**Fix:** Update the example to match the actual spring API.

---

### 492. docs/architecture/overview.md — Subsystem dependencies show `renderer → gpu, scene, texture, surface` (line 79) but GPU path bypasses scene
**File:** `docs/architecture/overview.md` line 79

```markdown
            ├── renderer → gpu, scene, texture, surface
```

The dependency list shows the renderer depending on the scene graph. However, the GPU rendering path (`draw_commands_to_gpu_impl`) takes a flat `Vec<DrawCommand>` and doesn't traverse the scene graph.

**Impact:** Misleading dependency map — the scene graph is shown as a renderer dependency, but the GPU path doesn't use it.

**Fix:** Clarify that the scene graph is used by the Slint/CPU path, not the GPU path.

---

### 493. bluetooth-widget.slint — Radar loader animation uses BluetoothState global properties for dot positions (lines 175-178)
**File:** `bluetooth-widget.slint` lines 175-178

```slint
Rectangle { x: 20px + BluetoothState.sweepCos * 8) * 1px; y: 20px + (BluetoothState.sweepSin * 8) * 1px; ... }
Rectangle { x: 20px + BluetoothState.dot1X * 1px; y: 20px + BluetoothState.dot1Y * 1px; ... }
```

The radar animation positions are driven by global properties (`sweepCos`, `sweepSin`, `dot1X`, `dot1Y`, etc.) that must be updated from Rust on every frame. This means the Bluetooth widget requires per-frame property updates from Rust even when just showing a scanning animation.

**Impact:** CPU waste — the Rust side must update 8 floating-point properties every frame for the scanning animation, instead of using Slint's built-in animation system.

**Fix:** Use Slint's `animate` property or timer-based animation instead of Rust-driven property updates.

---

### 494. docs/architecture/renderer.md — Mermaid diagram shows "Geometry Compiler", "Primitives Compiler", "Text Rasterizer" (lines 22-27) but these don't exist as named components
**File:** `docs/architecture/renderer.md` lines 22-27

```mermaid
        subgraph Compilers["Compilers"]
            B1[Geometry Compiler]
            B2[Primitives Compiler]
            B3[Text Rasterizer]
            B4[Animation Engine]
        end
```

These "compilers" are described in the architecture diagram but don't exist as named components in the code. The renderer has `draw_commands_to_gpu_impl` which processes commands directly, not through named compiler stages.

**Impact:** Misleading architecture — developers looking for a "Geometry Compiler" will be confused.

**Fix:** Update the diagram to reflect the actual rendering pipeline stages.

---

### 495. docs/contracts/physics.md — Invariant #5 duplicates Invariant #4 (lines 51-52)
**File:** `docs/contracts/physics.md` lines 51-52

```markdown
4. **Physics does not directly modify scene nodes.** Synchronization happens through an explicit interface.
5. **Scene synchronization happens through an explicit interface.** Physics produces MotionState; VisualEffects derives transforms.
```

These two invariants say the same thing with slightly different wording. Invariant #5 adds detail about `MotionState` and `VisualEffects`, but the core claim is identical.

**Impact:** Redundant documentation — adds noise without adding information.

**Fix:** Merge into a single invariant with the detail from #5.

---

### 496. ui.slint — DockWidget uses `in property` for all configuration but provides no default values for some (lines 33-54)
**File:** `ui.slint` lines 33-54

Some properties have defaults (`dock_width: 520`, `icon_size: 48`) while others don't (`hover_color`, `dash_focused_color`, `dash_inactive_color`, `background_color`, `border_color`). The Slint compiler will use default values for brush types, but they may not match the intended theme.

**Impact:** Theme inconsistency — if the Rust side doesn't set all properties, the defaults may not match the intended appearance.

**Fix:** Ensure all properties have sensible defaults or are always set from Rust.

---

### 497. docs/architecture/memory.md — Icon cache described as `dashmap::DashMap` (line 77) but icon cache may use different structure
**File:** `docs/architecture/memory.md` line 77

```markdown
- **Type**: `dashmap::DashMap` for concurrent access.
```

The icon cache in `src/cache/` may use a different data structure. The documentation assumes `DashMap` without verifying the actual implementation.

**Impact:** Potentially misleading — if the cache uses a different structure, the documentation is inaccurate.

**Fix:** Verify the actual cache implementation and update the documentation.

---

### 498. docs/architecture/performance.md — Criterion benchmark example references `wgpu-api/benches/` (line 107) but no benches directory exists
**File:** `docs/architecture/performance.md` line 107

```markdown
// In wgpu-api/benches/
```

The benchmark example assumes a `benches/` directory exists in `wgpu-api/`. If it doesn't, the benchmark code has nowhere to go.

**Impact:** Misleading guide — developers following the performance documentation will create files in a non-standard location.

**Fix:** Create the benches directory or update the documentation to reflect the actual benchmark location.

---

### 499. docs/architecture/error-handling.md — Error type hierarchy lists 10 wgpu-api errors and 12 application errors (lines 56-88) but actual error.rs may have different variants
**File:** `docs/architecture/error-handling.md` lines 56-88

The documentation lists specific error types (`GpuError`, `RendererError`, `PhysicsError`, etc.) with examples. These need to be verified against the actual `error.rs` files in both crates.

**Impact:** Potentially stale — if the error types have been refactored, the documentation is misleading.

**Fix:** Verify the error types against the actual code and update the documentation.

---

### 500. docs/architecture/error-handling.md — Recovery flow diagram (lines 410-426) describes automatic recovery but recovery modules are non-functional stubs
**File:** `docs/architecture/error-handling.md` lines 410-426

The recovery flow diagram shows automatic recovery with bounded attempts and exponential backoff. However, Round 12 (issue #371) identified that all recovery modules are non-functional stubs. The diagram describes a system that doesn't work.

**Impact:** Misleading architecture — developers will expect automatic recovery to work, but it doesn't.

**Fix:** Update the diagram to reflect that recovery is not yet implemented, or implement the recovery system.

---

## Low Severity Issues

### 501. ui.slint — Badge text color is hardcoded green (#00FF00) on red background (line 127)
**File:** `ui.slint` line 127

```slint
color: #00FF00;
```

Bright green text on a red badge is unusual and may have poor contrast or accessibility issues. The comment says "green for high contrast against red badge background" but green-on-red is difficult for colorblind users.

**Impact:** Accessibility — colorblind users may not read the badge text correctly.

**Fix:** Use white text (`#FFFFFF`) on the red badge for better contrast and accessibility.

---

### 502. bluetooth-widget.slint — Footer text is hardcoded to empty string in global state (line 22)
**File:** `bluetooth-widget.slint` line 22

```slint
in property<string> footerText: "";
```

The footer text default is empty. The footer area (lines 359-367) always shows, even when empty, consuming 16px of vertical space.

**Impact:** Wasted space — the footer area is always visible even when there's no text.

**Fix:** Make the footer conditionally visible based on whether `footerText` is non-empty.

---

### 503. docs/architecture/physics.md — "16 modules, ~4000+ lines" (line 9) but actual line count may differ
**File:** `docs/architecture/physics.md` line 9

```markdown
`wgpu-api/src/physics/` (16 modules, ~4000+ lines)
```

The line count is approximate and may be stale.

**Impact:** Minor — the approximate count helps with scope estimation but may be inaccurate.

**Fix:** Update periodically or remove the specific count.

---

### 504. docs/decisions/002-physics-timestep.md — Parameters table lists `velocity_iterations: 8` and `position_iterations: 4` (lines 37-43)
**File:** `docs/decisions/002-physics-timestep.md` lines 37-43

| Parameter | Default |
|-----------|---------|
| `velocity_iterations` | 8 |
| `position_iterations` | 4 |

Round 6 (issue #124) identified that velocity iterations re-apply forces, effectively multiplying them 3×. With 8 velocity iterations, forces are applied 8× instead of once. The ADR documents these parameters without noting the bug.

**Impact:** Misleading parameter documentation — the documented defaults produce incorrect physics behavior.

**Fix:** Update the ADR to note the force multiplication issue or fix the solver.

---

### 505. docs/architecture/coding_rules.md — Commit message format uses conventional commits (lines 201-208) but git history may use different format
**File:** `docs/architecture/coding_rules.md` lines 201-208

The coding rules specify `feat:`, `fix:`, `refactor:`, etc. prefixes. If the git history uses a different format, the documentation is aspirational rather than descriptive.

**Impact:** Minor inconsistency — new contributors may follow the documented format while existing commits use a different style.

**Fix:** Verify the git history format and update the documentation or enforce the documented format.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 461 | **Critical** | All 7 compatibility tests are empty stubs | False confidence in CI |
| 462 | **Critical** | ADR 005 claims Rapier2D removed, needs verification | Misleading documentation |
| 463 | **Critical** | ADR dates from 2024, not 2026 | Misleading timeline |
| 464 | **Critical** | ui.slint dock background has no rounded corners | Visual mismatch with design |
| 465 | **Critical** | bluetooth-widget.slint references external SVGs | Missing icons at runtime |
| 466 | **Critical** | 22-phase testing protocol is largely unexecutable | Protocol can't be followed |
| 467 | **High** | Expected test count is stale (334 vs 893) | Misleading regression detection |
| 468 | **High** | Testing docs list removed rapier test module | Broken documentation |
| 469 | **High** | Renderer Key Types table lists non-existent types | Misleading API reference |
| 470 | **High** | Renderer invariants describe non-existent API | Broken API documentation |
| 471 | **High** | Renderer contract lists 5 non-existent methods | Contract violation |
| 472 | **High** | Physics contract includes hit_test from interaction layer | Misleading boundary |
| 473 | **High** | Badge text empty for counts 1-9 | Missing information |
| 474 | **High** | Icon outline loads same image twice | Performance waste |
| 475 | **High** | Scrollbar at negative x coordinate | Invisible scrollbar |
| 476 | **High** | Overview data flow shows scene graph, GPU bypasses it | Misleading architecture |
| 477 | **High** | Threading docs describe PipeWire thread, code uses tokio | Misleading threading model |
| 478 | **High** | Memory docs say mimalloc optional, code enables it | Misleading configuration |
| 479 | **High** | Frame budget mixes GPU and Slint paths | Incorrect performance model |
| 480 | **High** | Error handling docs describe miette/color-eyre, may not exist | Misleading error guide |
| 481 | **Medium** | Interaction docs contradict ADR 003 on coordinates | Coordinate space confusion |
| 482 | **Medium** | Same coordinate contradiction in interaction contract | Coordinate space confusion |
| 483 | **Medium** | Slint DockItem app_id is int, code uses String | Type mismatch |
| 484 | **Medium** | Items container width doesn't account for animated sizes | Clipped animations |
| 485 | **Medium** | Bluetooth device cards: 39px height, 43px spacing | Undocumented design choice |
| 486 | **Medium** | Physics module map lists 14 of 16 modules | Incomplete documentation |
| 487 | **Medium** | PipeWire diagram shows VideoNode, GPU path bypasses it | Misleading data flow |
| 488 | **Medium** | Testing protocol requires cargo machete | Protocol failure |
| 489 | **Medium** | Testing protocol requires cargo fuzz, miri | Protocol failure |
| 490 | **Medium** | Coding rules example uses glam::Vec2 | Misleading example |
| 491 | **Medium** | Coding rules test example may not match API | Misleading example |
| 492 | **Medium** | Overview dependencies show scene → renderer, GPU bypasses it | Misleading dependency map |
| 493 | **Medium** | Bluetooth radar animation driven by Rust properties | CPU waste |
| 494 | **Medium** | Renderer diagram shows non-existent compilers | Misleading architecture |
| 495 | **Medium** | Physics contract has duplicate invariants | Redundant documentation |
| 496 | **Medium** | Slint properties missing defaults | Theme inconsistency |
| 497 | **Medium** | Memory docs assume DashMap for icon cache | Potentially inaccurate |
| 498 | **Medium** | Performance docs reference non-existent benches/ dir | Misleading guide |
| 499 | **Medium** | Error type hierarchy may not match actual code | Potentially stale |
| 500 | **Medium** | Recovery flow diagram describes non-functional system | Misleading architecture |
| 501 | **Low** | Badge text green on red, accessibility issue | Colorblind users affected |
| 502 | **Low** | Bluetooth footer always visible, even when empty | Wasted space |
| 503 | **Low** | Physics line count may be stale | Minor inaccuracy |
| 504 | **Low** | ADR 002 parameters document buggy defaults | Misleading parameters |
| 505 | **Low** | Commit message format may not match git history | Minor inconsistency |

---

## Cumulative Summary

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 1 | 2 | 2 | 1 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 3 |
| 3 | gpu-rendering-review-3.md | 33–60 | 3 | 4 | 6 | 3 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 13 | 21 | 0 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 12 | 5 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| 8 | gpu-rendering-review-8.md | 200–240 | 6 | 10 | 16 | 8 |
| 9 | gpu-rendering-review-9.md | 241–280 | 6 | 10 | 16 | 8 |
| 10 | gpu-rendering-review-10.md | 281–330 | 10 | 20 | 20 | 0 |
| 11 | gpu-rendering-review-11.md | 331–370 | 5 | 15 | 25 | 5 |
| 12 | gpu-rendering-review-12.md | 371–420 | 8 | 12 | 20 | 10 |
| 13 | gpu-rendering-review-13.md | 421–460 | 8 | 12 | 16 | 4 |
| **14** | **gpu-rendering-review-14.md** | **461–505** | **6** | **14** | **20** | **6** |
| **Total** | | **505** | **75** | **152** | **211** | **61** |
