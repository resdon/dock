# WAYDOCK — Mandatory AI Agent Testing & Verification Protocol

## 1. Mission

You are responsible for verifying the correctness, stability, performance and compatibility of the entire Waydock application.

The application uses:

- Rust
- Wayland
- Vulkan through wgpu
- Custom wgpu renderer
- Slint UI and widgets
- Custom animation and effects systems

Your responsibility is to identify bugs, regressions, memory leaks, rendering problems, performance issues and architectural violations.

**Never assume that code is correct because it compiles.**

**Never claim a test passed unless it was actually executed and its result verified.**

**Do not modify source code during an audit unless explicitly instructed to fix identified problems.**

## 2. Mandatory optimization priorities

All performance testing must follow this order:

1. Lowest possible CPU usage.
2. Lowest possible system RAM usage.
3. Highest possible responsiveness.
4. Graphics, animations, interactivity and Slint compatibility.
5. Lowest possible VRAM usage.

Performance improvements must not silently break functionality.

## 3. Phase 1 — Repository inspection

Before testing:

```bash
git status
git branch --show-current
git log -5 --oneline
```

Inspect:

- Cargo workspace.
- Cargo.toml files.
- Cargo.lock.
- All crates.
- Renderer architecture.
- Wayland integration.
- Slint integration.
- Shader files.
- Existing tests.
- Existing CI configuration.

Verify that:

- wgpu is the only rendering implementation.
- Vulkan is the intended graphics backend.
- Femto rendering has been removed.
- CPU/software rendering backends have been removed.
- Slint remains integrated.
- No obsolete rendering dependencies remain.

Do not delete or overwrite user changes.

## 4. Phase 2 — Formatting and static analysis

Run:

```bash
cargo fmt --all -- --check

cargo check --workspace --all-targets

cargo clippy --workspace --all-targets -- -D warnings
```

Check for:

- Compilation failures.
- Warnings.
- Unused code.
- Incorrect lifetimes.
- Unnecessary cloning.
- Unnecessary allocations.
- Potential panics.
- Incorrect error handling.
- Suspicious unsafe blocks.

Do not suppress warnings merely to obtain a passing result.

## 5. Phase 3 — Dependency and security checks

Run:

```bash
cargo audit
cargo deny check
cargo machete
```

Inspect:

- Vulnerable dependencies.
- Unmaintained dependencies.
- Unnecessary dependencies.
- Duplicate dependencies.
- License compatibility.
- Unexpected rendering backends.

Do not automatically upgrade dependencies without checking compatibility.

Preserve Cargo.lock.

## 6. Phase 4 — Automated tests

Run:

```bash
cargo test --workspace
```

If cargo-nextest is installed:

```bash
cargo nextest run --workspace
```

Test every crate independently where useful.

### Required coverage

**wgpu-api crate:**

- Physics engine (springs, rigid bodies, fields, constraints, visual effects)
- Scene graph operations
- Geometry calculations
- Coordinate transformations
- Animation state (easing, keyframes, particles)
- Text rendering pipeline (cosmic-text shaping, glyph atlas)
- Batch processing and sorting
- Error handling across all subsystems

**dock crate:**

- Application startup and shutdown
- Configuration loading
- Error handling
- State management
- Icon resolution
- Process monitoring
- PipeWire capture initialization

## 7. Phase 5 — Shader validation

Inspect every WGSL shader.

Verify:

- Shader syntax.
- Binding layouts.
- Uniform alignment.
- Buffer layouts.
- Texture formats.
- Sampler configuration.
- Vertex attributes.
- Fragment output formats.
- Pipeline compatibility.

Test shader compilation through wgpu.

Do not rely exclusively on a text-based shader inspection.

Check that all shaders used by the application are actually compiled and validated.

## 8. Phase 6 — GPU and Vulkan testing

Run the application using the Vulkan backend.

Verify:

- Vulkan adapter selection.
- Device creation.
- Queue creation.
- Surface configuration.
- Presentation.
- Buffer creation.
- Texture uploads.
- Pipeline creation.
- Resource destruction.
- Surface resize.
- Device-loss handling.

Enable development validation.

Use Vulkan validation layers where supported.

Use wgpu uncaptured error reporting and error scopes.

Test:

- Surface lost.
- Surface outdated.
- Surface timeout.
- Zero-sized surface.
- GPU out-of-memory handling.
- Device loss.
- Unsupported surface formats.

Never allow rendering failures to cause a busy loop.

## 9. Phase 7 — Wayland integration tests

Test on real Wayland sessions.

Where available, test:

- KWin.
- Sway.
- Hyprland.
- labwc.

Verify:

- Application startup.
- Layer-shell surface creation.
- Dock positioning.
- Exclusive zone behavior.
- Keyboard interactivity.
- Pointer events.
- Frame callbacks.
- Surface destruction.
- Monitor addition and removal.
- Monitor resolution changes.
- Monitor scaling changes.
- Four dock orientations (bottom, top, left, right).

Do not claim compositor compatibility based only on compilation.

## 10. Phase 8 — Slint compatibility tests

Test actual Slint widgets.

Verify:

- Buttons.
- Text inputs.
- Labels.
- Images.
- Checkboxes.
- Sliders.
- Scrollable content.
- Custom widgets.
- Layouts.
- Property bindings.
- Signals and callbacks.
- Focus.
- Keyboard navigation.
- Pointer interaction.
- Text rendering.
- Clipping.
- Transparency.
- Themes.
- Animations.

Test both ordinary UI rendering and complex widget combinations.

Verify that no CPU full-frame rasterization or unnecessary GPU readback has been introduced.

Document any Slint API limitations.

## 11. Phase 9 — Visual regression testing

Capture reference screenshots for:

1. Default dock.
2. Bottom orientation.
3. Top orientation.
4. Left orientation.
5. Right orientation.
6. Hover state.
7. Magnification.
8. Open popup.
9. Transparent background.
10. Text and icons.
11. Shadows and glow.
12. Blur and bloom.
13. Different scaling factors.

Compare screenshots against approved references.

Use tolerance-based comparisons to account for font rasterization and GPU driver differences.

Report:

- Pixel difference.
- Missing elements.
- Incorrect positioning.
- Clipping.
- Color differences.
- Alpha compositing errors.

## 12. Phase 10 — CPU testing

Use Linux perf and other profiling tools.

### Idle scenario

Leave the dock untouched for five minutes.

Measure:

- Average CPU usage.
- CPU wakeups.
- Thread activity.
- Event-loop activity.
- Unnecessary redraws.

### Active interaction scenario

Move the pointer rapidly over icons.

Measure:

- CPU consumption.
- Event processing time.
- Scene update time.
- Allocation count.
- Frame scheduling.

### Animation scenario

Run multiple simultaneous animations.

Measure:

- CPU consumption.
- Frame preparation time.
- Animation update time.
- Unnecessary layout recalculations.

**Priority: eliminate unnecessary CPU work, particularly while idle.**

## 13. Phase 11 — RAM testing

Use heaptrack, Massif or equivalent tools.

Test:

- Startup.
- Idle.
- Hover.
- Multiple animations.
- Image loading.
- Popup opening and closing.
- Theme changes.
- Monitor changes.
- Repeated resource creation and destruction.

Identify:

- Memory leaks.
- Unbounded caches.
- Excessive allocations.
- Duplicate resources.
- Memory retained after effects finish.
- Memory growth during repeated interaction.

Run a longer stress test to identify gradual memory growth.

## 14. Phase 12 — VRAM testing

Measure GPU memory where driver instrumentation permits.

Inspect:

- Texture allocations.
- Buffer allocations.
- Render targets.
- Glyph atlas.
- Icon atlas.
- Intermediate effect textures.
- Resource destruction.

Verify:

- Optional effects allocate resources only when needed.
- Disabled effects do not consume unnecessary VRAM.
- Textures are reused.
- Buffers are reused.
- No unnecessary full-resolution intermediate textures exist.

Report limitations if precise VRAM attribution is unavailable.

## 15. Phase 13 — Responsiveness and frame timing

Target at least 60 FPS during ordinary animations when supported by the hardware and compositor.

Test at different refresh rates where available.

Measure:

- Frame intervals.
- Missed frames.
- Input-to-presentation latency.
- CPU preparation time.
- GPU execution time.
- Presentation scheduling delay.

**Do not use average FPS alone.**

Report frame-time percentiles, particularly p50, p95 and p99.

Test:

- Rapid pointer movement.
- Rapid hover transitions.
- Animation interruption.
- Animation reversal.
- Simultaneous animations.
- Popup transitions.

Verify that completed animations stop requesting frames.

## 16. Phase 14 — Stress and stability tests

Perform:

- 30-minute normal-use test.
- 1-hour extended test.
- Repeated startup and shutdown.
- Rapid monitor changes.
- Repeated resizing.
- Repeated popup operations.
- Large icon counts.
- Repeated image loading.
- Theme switching.
- GPU resource recreation.

Watch for:

- Crashes.
- Deadlocks.
- Memory growth.
- Rendering corruption.
- Lost input.
- Broken surfaces.
- GPU errors.
- Unexpected CPU usage.

## 17. Phase 15 — Fuzzing and safety

Where applicable, use:

```bash
cargo fuzz list
cargo miri test
```

Fuzz:

- Configuration parsing.
- Geometry.
- Animation parameters.
- Scene updates.
- Input sequences.
- Resource lifecycle state machines.

Use Miri for applicable Rust-only components.

Do not interpret successful Miri tests as proof that Vulkan or Wayland integration is correct.

## 18. Phase 16 — Performance regression comparison

Compare the tested implementation against the last known working Git checkpoint.

Record:

| Metric | Previous | Current |
|--------|----------|---------|
| Idle CPU | Measure | Measure |
| Active CPU | Measure | Measure |
| Idle RAM | Measure | Measure |
| Peak RAM | Measure | Measure |
| VRAM | Measure | Measure |
| Frame time p50 | Measure | Measure |
| Frame time p95 | Measure | Measure |
| Frame time p99 | Measure | Measure |
| Input latency | Measure | Measure |

Identify regressions.

Do not claim improvements without comparable measurements using the same hardware and test conditions.

## 19. Phase 17 — Git safety

Before testing:

```bash
git status
git log -5 --oneline
```

Do not overwrite existing user changes.

Do not automatically discard failed test modifications.

After testing, report the changed files.

Do not commit test fixes without authorization unless explicitly assigned to implement fixes.

Preserve all test logs and performance reports as appropriate.

## 20. Mandatory testing report

Create:

**TEST_REPORT.md**

Include:

### Environment

- OS.
- Kernel.
- Wayland compositor.
- GPU.
- Vulkan driver.
- Rust version.
- wgpu version.
- Slint version.
- Display refresh rate.

### Build

- Compilation result.
- Clippy result.
- Formatting result.

### Tests

- Tests executed.
- Tests passed.
- Tests failed.
- Tests skipped.
- Tests unavailable.

### Rendering

- Vulkan initialization.
- Shader validation.
- Presentation.
- Surface lifecycle.

### Compatibility

- Wayland.
- Slint widgets.
- Orientations.
- Monitor scaling.

### Performance

- Idle CPU.
- Active CPU.
- Idle RAM.
- Peak RAM.
- VRAM.
- Frame-time percentiles.
- Input latency.

### Problems

- Critical issues.
- High-priority issues.
- Medium-priority issues.
- Low-priority issues.

### Conclusion

Choose exactly one:

- **PASS**: All mandatory checks passed.
- **PASS WITH WARNINGS**: No blocking failures, but documented limitations remain.
- **FAIL**: One or more mandatory checks failed.
- **INCOMPLETE**: Required testing could not be performed.

## 21. Absolute rules

- **Never fabricate test results.**
- **Never report unexecuted tests as passed.**
- **Never hide warnings.**
- **Never silently disable failing tests.**
- **Never suppress Vulkan validation errors without investigation.**
- **Never accept a performance improvement that breaks UI functionality.**
- **Never declare production readiness without real Wayland and Vulkan testing.**
- **Never modify unrelated user changes.**
- **Never remove failing tests merely to obtain a green test result.**

## 22. Final objective

Prove that Waydock is:

- Correct.
- Stable.
- Memory efficient.
- CPU efficient.
- Responsive.
- GPU efficient.
- Visually correct.
- Slint compatible.
- Wayland compatible.
- Recoverable through Git.

Testing is not complete until results are documented and independently reproducible.

## Three Testing Gates

These are mandatory before accepting any release.

### Gate 1: Build correctness

Formatting, compilation, Clippy, unit tests and dependency checks.

### Gate 2: Runtime correctness

Real Wayland session, Vulkan rendering, Slint widgets, visual regression and stress testing.

### Gate 3: Performance acceptance

CPU, RAM, VRAM, frame times and input responsiveness measured against the previous Git checkpoint.

## Recommended workflow

1. Implementation agent completes migration.
2. Automated test suite runs.
3. Profiling agent measures CPU, RAM and VRAM.
4. Independent AI auditor reviews the source.
5. Fix all critical and high-priority findings.
6. Run the complete test suite again.
7. Benchmark against the original application.
8. Only then accept the migration.

This approach will give you much more confidence that you have a genuinely lightweight renderer, rather than simply a renderer that happens to compile.
