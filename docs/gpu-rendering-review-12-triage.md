# GPU Rendering Review Round 12 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-12.md` (issues #371–#420)  
**Scope reviewed:** `src/geometry/primitives.rs`, `src/graphics/{mod,hide,fade,animation}.rs`, `src/handlers/{mod,utils}.rs`, `src/models/system.rs`, `src/pointer/{coordinates,handler,motion,mod,proximity}.rs`, `src/recovery/{mod,capture_recovery,gpu_recovery,resource_recovery,service_recovery}.rs`, `src/render/{mod,audio,audio_player,badges,bluetooth,font,text,theme,tray,utils,widget_bar,popup,dashed_block,date_time_block,debug_overlay,processes,rclone,color_check}.rs`, `src/resolvers/{capabilities,distribution,environment,icon_list,icon_stage,process,steam,window}.rs`, `src/services/{resource_monitor,test}.rs`, `src/state/*`, `src/diagnostics/{crash_report,health_monitor,logger,mod}.rs`, `tests/compatibility.rs`, `build.rs`, `src/error.rs`  
**Totals:** 50 issues — 8 Critical, 12 High, 20 Medium, 10 Low  
**Cumulative (Rounds 1–12):** 432 issues — 56 Critical, 107 High, 166 Medium, 103 Low

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** to remove false safety nets and hot-path regressions first.
- Items marked **[cluster]** should be addressed together to avoid partial/inconsistent behavior.
- Preserve project invariants (`AGENTS.md`): event-driven behavior, no steady-state heap allocations, no render-thread blocking, bounded caches, shared state across CPU/GPU paths where applicable, and avoid introducing new architectural dependencies.
- For each fix, run relevant checks: `cargo check`, targeted `cargo test`, and manual verification on Wayland for pointer/popup/rendering behavior.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Stub/non-functional infrastructure | 371, 377, 378, 383, 384 | False fault tolerance, dead monitoring/diagnostics, misleading green tests, disabled feature |
| Pointer/hit-test duplication & drift | 372, 373, 379, 390, 404 | Wrong popup/hover behavior, config drift, per-event overhead, CPU/GPU mismatch |
| CPU render hot-path waste | 374, 380, 381, 382, 405, 406, 416 | Excess per-frame CPU/allocations and slow pixel loops in popup/audio/text rendering |
| Font/text resource & blocking issues | 375, 376, 385, 386 | Unbounded cache growth, render-thread stalls via subprocess, mmap safety risk, confused API |
| Hardcoded config/theme/capabilities | 388, 389, 398 | Incorrect UI/actions and visual inconsistency across paths/themes |
| Maintainability/build/error quality | 387, 392, 393, 394, 395, 399, 400, 401, 402, 403, 407, 409, 410 | Large monoliths, dead code, fragile build, weak abstractions, lossy errors |
| Low-level correctness/edge cases | 391, 396, 397, 408, 411, 412, 413, 414, 415, 417, 418, 419, 420 | Precision/ordering/display/initialization issues and minor inefficiencies |

## Recommended fix order

1. **#371, #383, #384** — make recovery/health/crash-reporting real or explicitly remove/disable claims; stop false operational signals. [cluster]
2. **#372, #373, #379, #390** — unify popup/surface mapping and config-driven layout/proximity across pointer & render paths. [cluster]
3. **#375, #376, #385** — fix font cache bounds, eliminate render-thread `fc-match` spawning, and harden font file access. [cluster]
4. **#374, #380, #381, #382, #406, #416** — remove per-frame/per-pixel hot-path waste in audio/bluetooth/text rendering. [cluster]
5. **#377, #378** — address misleading empty tests and disabled hide animation (implement, gate properly, or remove). [cluster]
6. **#388, #389, #398** — drive capabilities/colors/theme from real sources shared across CPU/GPU paths. [cluster]
7. **#386, #399, #401, #402, #403** — reduce duplication/monoliths and improve typed error/logging boundaries.
8. **Remaining Medium/Low** — hardening, precision, ordering, validation, build/startup ergonomics.

---

## Issues (sorted by severity, then subsystem)

### Critical

#### [recovery] #371 — All recovery modules are non-functional stubs
- **Severity:** Critical | **Files:** `src/recovery/capture_recovery.rs:89-93`, `src/recovery/resource_recovery.rs:83-87`, `src/recovery/service_recovery.rs:103-107` (+ related recovery impls)
- **Problem:** Recovery methods log/increment counters and return `Retryable`/`Recovered` without performing reconnect/recreate/cleanup actions.
- **Impact:** Illusion of fault tolerance; failures (PipeWire disconnect, GPU/resource exhaustion, D-Bus issues) persist silently while system reports recovery.
- **Fix direction:** Implement real recovery flows (delays/backoff, reconnect, stream/texture/listener recreation, state transitions) or remove stub APIs and handle errors at call sites with honest status.
- **Notes:** Audit all callers for reliance on `RecoveryResult`.

#### [pointer] #372 — 9 identical surface-check blocks in `map_coordinates`
- **Severity:** Critical | **File:** `src/pointer/coordinates.rs:22-77`
- **Problem:** Repeated per-popup `if surface == ... { return offset }` blocks for each popup type.
- **Impact:** High maintenance/error risk; new popups can be missed causing coordinate mapping bugs.
- **Fix direction:** Centralize popup surface→offset mapping (iterate a unified popup collection, enum/trait accessor, or `HashMap<WlSurface,(i32,i32)>` maintained on popup lifecycle changes).
- **Notes:** [cluster with #379].

#### [pointer] #373 — Hardcoded layout constants ignore config
- **Severity:** Critical | **File:** `src/pointer/handler.rs:45-46`
- **Problem:** `dock_height=60`, `box_size=48`, `spacing=8` hardcoded in pointer hit logic.
- **Impact:** Hit-test drift vs renderer/layout when config differs.
- **Fix direction:** Source layout metrics from dock/config/runtime state and pass into pointer handling.
- **Notes:** [cluster with #372, #379].

#### [render/audio_player] #374 — O(r²) per-pixel circle rendering
- **Severity:** Critical | **File:** `src/render/audio_player.rs:556-563`
- **Problem:** Filled circles drawn by scanning full bounding box with distance test per pixel; called repeatedly per frame.
- **Impact:** Large CPU waste in render path (~hundreds of thousands of pixel ops/frame).
- **Fix direction:** Use midpoint/scanline circle fill, pre-render/cache disc layers, or move effect to GPU/shared primitive path.
- **Notes:** Prioritize largest radii/hot call sites.

#### [render/font] #375 — Unbounded glyph cache
- **Severity:** Critical | **File:** `src/render/font.rs:22-23,289`
- **Problem:** `HashMap<(char,u32), CachedGlyph>` never evicts; `clear_cache` manual only.
- **Impact:** Unbounded memory growth over session (worse with large/CJK glyph bitmaps).
- **Fix direction:** Replace with bounded LRU/size-budget cache (by count/bytes) with eviction policy; consider per-size buckets.
- **Notes:** [cluster with #376, #385].

#### [render/font] #376 — Spawns `fc-match` subprocess per missing ASCII character on render thread
- **Severity:** Critical | **File:** `src/render/font.rs:113-117`
- **Problem:** On missing glyph, forks/execs `fc-match` synchronously during rendering.
- **Impact:** Render-thread blocking and visible stutter (compounded by multiple misses).
- **Fix direction:** Resolve/cache fallback fonts at startup or per-script/language on demand off render path; never spawn per character during frame rendering.
- **Notes:** [cluster with #375].

#### [tests] #377 — All 7 compatibility tests are empty stubs
- **Severity:** Critical | **File:** `tests/compatibility.rs:11-50`
- **Problem:** Tests contain comments only and pass without assertions.
- **Impact:** False confidence/green CI for unverified behavior.
- **Fix direction:** Implement meaningful assertions (with compositor/test harness guards) or remove/ignore with tracked TODOs so test counts reflect reality.
- **Notes:** Avoid flaky environment-dependent tests in default CI unless conditioned.

#### [graphics] #378 — Entire hide animation disabled
- **Severity:** Critical | **File:** `src/graphics/hide.rs:76-93`
- **Problem:** `tick`, visibility, and alpha application forced to always-visible/no-op.
- **Impact:** Advertised auto-hide behavior non-functional; dead constants/logic paths.
- **Fix direction:** Re-enable and fix animation state machine, or remove feature/UI/config surface to match behavior.
- **Notes:** [cluster with #407].

### High

#### [pointer] #379 — O(docks × popups) popup-type scans per pointer event
- **Severity:** High | **File:** `src/pointer/motion.rs:20-54`
- **Problem:** Iterates all docks repeatedly (once per popup type check) on each event.
- **Impact:** Avoidable CPU overhead on pointer motion; scales poorly with popups/docks.
- **Fix direction:** Maintain O(1) lookup (`WlSurface -> popup kind/dock`) updated on open/close.
- **Notes:** [cluster with #372].

#### [render/bluetooth] #380 — Filters/maps Bluetooth devices into new Vecs every render
- **Severity:** High | **File:** `src/render/bluetooth.rs:138-178`
- **Problem:** Recomputes discovered/paired `Vec<DeviceEntry>` and `SharedString`s each frame.
- **Impact:** Per-frame heap allocations/churn.
- **Fix direction:** Rebuild view models only when snapshot/state changes; cache render-ready lists.

#### [render/bluetooth] #381 — Clears Slint buffer with per-pixel loop
- **Severity:** High | **File:** `src/render/bluetooth.rs:187-194`
- **Problem:** Manual struct assignment per pixel for full buffer clear.
- **Impact:** Unnecessary CPU cost.
- **Fix direction:** Use `fill`, bulk memset-equivalent, or typed slice fill.

#### [render/bluetooth] #382 — RGBA→BGRA conversion in per-pixel loop
- **Severity:** High | **File:** `src/render/bluetooth.rs:208-217`
- **Problem:** Scalar per-pixel channel swap with per-iteration bounds checks.
- **Impact:** Slow conversion path.
- **Fix direction:** Batch conversion (unchecked inner loop after bounds validation, SIMD, or existing fast conversion utility).

#### [diagnostics] #383 — `HealthMonitor` never actually checks health
- **Severity:** High | **File:** `src/diagnostics/health_monitor.rs:117-152`
- **Problem:** Report/check APIs exist but no periodic/task-driven health evaluation uses interval/threshold; statuses remain effectively unknown.
- **Impact:** Dead monitoring; degradations not detected.
- **Fix direction:** Wire periodic subsystem checks and updates, or remove unused monitor infrastructure.

#### [diagnostics] #384 — Crash reports stored in memory only
- **Severity:** High | **File:** `src/diagnostics/crash_report.rs:68,85`
- **Problem:** Reports appended to in-memory `Vec` with no persistence/rotation/limits.
- **Impact:** Diagnostics lost on crash/restart.
- **Fix direction:** Persist to disk (e.g. cache dir) with rotation/size cap, or integrate with journald/logging backend.

#### [render/font] #385 — Unsafe `Mmap` without sufficient safety handling
- **Severity:** High | **File:** `src/render/font.rs:28,155`
- **Problem:** Memory-mapped font files can become invalid if truncated/replaced while mapped.
- **Impact:** Potential UB/segfault during glyph rasterization.
- **Fix direction:** Prefer reading font bytes into owned buffers, or pin/validate file lifetime and handle mapping errors/reloads safely.

#### [render/text] #386 — `World` duplicates `FontManager`/text draw API
- **Severity:** High | **File:** `src/render/text.rs:55-81`
- **Problem:** Parallel text-render entry points (`draw_text` free fn vs `World::draw_text`) and debug-only coupling.
- **Impact:** API confusion and maintenance overhead.
- **Fix direction:** Consolidate to a single canonical text rendering interface/path.

#### [render/badges] #387 — Badge icon embedded as compile-time bytes
- **Severity:** High | **File:** `src/render/badges.rs:9`
- **Problem:** `include_bytes!` embeds PNG in binary; decoded lazily but always shipped.
- **Impact:** Binary bloat and unnecessary asset coupling.
- **Fix direction:** Draw procedurally or load from runtime assets/cache as appropriate.

#### [resolvers] #388 — Hardcodes all app capabilities to `true`
- **Severity:** High | **File:** `src/resolvers/capabilities.rs:13-18`
- **Problem:** No real capability detection; sets launch/focus/close/pin/window-list as universally supported.
- **Impact:** Incorrect UI affordances and failed actions.
- **Fix direction:** Derive capabilities from actual protocol/process/app metadata and update dynamically when possible.

#### [render/audio_player] #389 — Hardcoded colors in CPU renderer
- **Severity:** High | **File:** `src/render/audio_player.rs:112-117`
- **Problem:** RGB values are magic constants, ignoring theme/config.
- **Impact:** Visual inconsistency with user theme/other renderers.
- **Fix direction:** Source colors from shared theme/config parsing utilities.

#### [pointer/proximity] #390 — Proximity state may not integrate with GPU path
- **Severity:** High | **File:** `src/pointer/proximity.rs`
- **Problem:** Proximity-driven visibility logic may diverge between CPU and GPU rendering paths.
- **Impact:** Inconsistent show/hide behavior by render backend.
- **Fix direction:** Share a single proximity/visibility state and consumption path across renderers.

### Medium

#### [geometry] #391 — `Point::to_local_coords` truncates and clamps, losing precision
- **Severity:** Medium | **File:** `src/geometry/primitives.rs:18-22`
- **Problem:** Casts `f64` to `i32` with `max(0.0)` clamp.
- **Impact:** Sub-pixel precision loss/jitter; out-of-bounds masked.
- **Fix direction:** Return floating-point local coords or round explicitly; separate clamping from conversion.

#### [graphics] #392 — SVG animation loads all frames into memory at once
- **Severity:** Medium | **File:** `src/graphics/animation.rs:72-118`
- **Problem:** All frames parsed/rasterized eagerly into resident buffers.
- **Impact:** Memory footprint grows with frame count/size even when idle.
- **Fix direction:** Lazy-load, cache limited frames, or stream/cycle buffers.

#### [graphics] #393 — Fade comparison uses `f32::EPSILON`
- **Severity:** Medium | **File:** `src/graphics/fade.rs:52,66,97,123,127`
- **Problem:** Completion checks use machine epsilon, sensitive to drift.
- **Impact:** Animation may never settle/complete.
- **Fix direction:** Use domain-appropriate tolerance (e.g. ~1e-3 for alpha) and clamp-to-target on completion.

#### [handlers] #394 — Launcher path fallback returns non-existent system path
- **Severity:** Medium | **File:** `src/handlers/utils.rs:50`
- **Problem:** Returns default path even when missing.
- **Impact:** Failure deferred to execution time with poor diagnostics.
- **Fix direction:** Return `Option`/`Result` with clear lookup failure.

#### [models] #395 — `system.rs` monolithic model with many unused fields
- **Severity:** Medium | **File:** `src/models/system.rs:1-824`
- **Problem:** Large structs (e.g. `ProcessRecord`) allocate/populate fields never used.
- **Impact:** Memory/cognitive overhead.
- **Fix direction:** Prune unused fields or split into focused/lazy sub-structures.

#### [resolvers] #396 — Distribution resolver allocates lowercase path string
- **Severity:** Medium | **File:** `src/resolvers/distribution.rs:32-33`
- **Problem:** Allocates new lowercase `String` for checks.
- **Impact:** Repeated per-resolution allocations.
- **Fix direction:** Case-insensitive comparisons without full allocation where possible.

#### [resolvers] #397 — Environment resolver same path-allocation pattern
- **Severity:** Medium | **File:** `src/resolvers/environment.rs:31-33`
- **Problem:** Same as #396.
- **Impact:** Repeated allocations.
- **Fix direction:** Same as #396. [cluster]

#### [render/theme] #398 — Theme colors may differ between CPU and GPU paths
- **Severity:** Medium | **File:** `src/render/theme.rs`
- **Problem:** Separate/implicit color sources risk divergence.
- **Impact:** Visual inconsistency across render paths.
- **Fix direction:** Single shared theme/color provider for both paths.

#### [diagnostics] #399 — `SubsystemLogger` thin wrapper/hardcoded subsystem methods
- **Severity:** Medium | **File:** `src/diagnostics/logger.rs:72-127`
- **Problem:** Adds little value; some helpers ignore configured subsystem.
- **Impact:** Confusing abstraction/inconsistent labels.
- **Fix direction:** Remove wrapper or add real value (context/rate limiting) and consistent subsystem tagging.

#### [build] #400 — `build.rs` relies on `slint_build` env-var side effects
- **Severity:** Medium | **File:** `build.rs:8-13`
- **Problem:** Depends on undocumented internal ordering/side effect then overrides.
- **Impact:** Fragile builds on `slint_build` behavior changes.
- **Fix direction:** Set/manage generated include env vars explicitly and deterministically.

#### [error] #401 — `AppError` stores all errors as `String`
- **Severity:** Medium | **File:** `src/error.rs:47-80`
- **Problem:** Erases typed/source error information.
- **Impact:** Cannot handle errors programmatically without string parsing.
- **Fix direction:** Introduce typed variants/wrappers (`#[from]`/source) preserving causes.

#### [render/audio] #402 — `render_audio_surface` monolithic (~329 lines)
- **Severity:** Medium | **File:** `src/render/audio.rs:320-648`
- **Problem:** Single function handles all sections.
- **Impact:** Hard to maintain/test.
- **Fix direction:** Split into section render helpers (header/output/input/streams/footer).

#### [render/audio_player] #403 — `render` monolithic with deep branches
- **Severity:** Medium | **File:** `src/render/audio_player.rs:66-377`
- **Problem:** Large branched rendering function.
- **Impact:** Maintainability/testing difficulty.
- **Fix direction:** Split into `render_compact`/`render_expanded` and subcomponents.

#### [pointer] #404 — Window list state/timers reset on every hover change
- **Severity:** Medium | **File:** `src/pointer/motion.rs:196-206`
- **Problem:** Resets timers/PipeWire active row and interaction state when switching hovered app.
- **Impact:** Poor UX (lost scroll/selection context).
- **Fix direction:** Reset only on close/intentional transitions; preserve per-app state when switching.

#### [render/font] #405 — Tofu placeholder drawn with O(w×h) per-pixel border check
- **Severity:** Medium | **File:** `src/render/font.rs:276-286`
- **Problem:** Scans full glyph bitmap to draw border.
- **Impact:** Minor unnecessary work (cached per missing char).
- **Fix direction:** Draw top/bottom rows and side columns directly.

#### [render/text] #406 — `blit_glyph` performs redundant per-pixel bounds checks
- **Severity:** Medium | **File:** `src/render/text.rs:173-187`
- **Problem:** Two bounds checks per pixel inside inner loop.
- **Impact:** Branch overhead, especially near edges.
- **Fix direction:** Precompute clipped source/destination rect and iterate only valid region.

#### [graphics] #407 — `HIDE_FRAME_INTERVAL` dead/ineffective
- **Severity:** Medium | **File:** `src/graphics/hide.rs:11`
- **Problem:** Constant tied to disabled hide animation.
- **Impact:** Dead code/confusion.
- **Fix direction:** Remove or restore animation usage. [cluster with #378]

#### [render/badges] #408 — Alpha blending uses wider-than-needed integer intermediates
- **Severity:** Medium | **File:** `src/render/badges.rs:75-77`
- **Problem:** Uses `u32` intermediates where smaller type suffices.
- **Impact:** Minor clarity/intent issue (perf likely negligible).
- **Fix direction:** Use appropriately sized integer math and document blend formula.

#### [diagnostics] #409 — Possible circular dependency risk in diagnostics re-exports
- **Severity:** Medium | **File:** `src/diagnostics/mod.rs`
- **Problem:** Re-export/module organization may encourage cycles.
- **Impact:** Refactoring/build complexity risk.
- **Fix direction:** Enforce acyclic module layering and minimal re-exports.

#### [recovery] #410 — `RecoveryResult` granularity insufficient
- **Severity:** Medium | **File:** `src/recovery/mod.rs`
- **Problem:** Cannot express retry timing/strategy distinctions.
- **Impact:** Callers cannot choose immediate vs delayed retry appropriately.
- **Fix direction:** Extend enum (e.g. `RetryAfter(Duration)`, typed failure causes) and update handling.

### Low

#### [geometry] #411 — `Rect::contains` uses inclusive right/bottom edges
- **Severity:** Low | **File:** `src/geometry/primitives.rs:43-44`
- **Problem:** Uses `<=` for max edges.
- **Impact:** Edge points can belong to adjacent rects.
- **Fix direction:** Use half-open convention (`<`) or document current convention explicitly.

#### [graphics] #412 — Animation frame sort extracts all digits from filename
- **Severity:** Low | **File:** `src/graphics/animation.rs:84-91`
- **Problem:** Parsing strategy can misorder unusual names.
- **Impact:** Rare incorrect frame ordering.
- **Fix direction:** Parse intended numeric segment more robustly (e.g. trailing/index token).

#### [handlers] #413 — Launcher path test is conditional/weak
- **Severity:** Low | **File:** `src/handlers/utils.rs:60-65`
- **Problem:** Test skips assertions when conditions unmet.
- **Impact:** Regressions in fallback paths may pass unnoticed.
- **Fix direction:** Test fallback branches explicitly (temp dirs/mocked env/fs abstraction).

#### [models] #414 — `WindowId` ordering compares only protocol ID
- **Severity:** Low | **File:** `src/models/system.rs:55-57`
- **Problem:** `Ord`/equality semantics incomplete for full object identity.
- **Impact:** Incorrect ordering/collisions in sorted sets/maps.
- **Fix direction:** Compare full object identity (protocol + object/id fields as appropriate).

#### [resolvers] #415 — Distribution detection uses hardcoded/case-sensitive path patterns
- **Severity:** Low | **File:** `src/resolvers/distribution.rs:51-90`
- **Problem:** Limited substring checks for known launchers/paths.
- **Impact:** Missed detections in non-standard installs.
- **Fix direction:** Case-insensitive matching and additional env/metadata signals.

#### [render/audio] #416 — `short_name` allocates `Vec<char>` per call
- **Severity:** Low | **File:** `src/render/audio.rs:88`
- **Problem:** Collects chars to truncate.
- **Impact:** Repeated small allocations in render path.
- **Fix direction:** Use `char_indices`/iterator slicing without intermediate Vec.

#### [render/audio_player] #417 — `time_label` truncates seconds and omits hours
- **Severity:** Low | **File:** `src/render/audio_player.rs:513-515`
- **Problem:** Cast truncation (`59.9 -> 0:59`) and no hour formatting.
- **Impact:** Minor display inaccuracy/limitation.
- **Fix direction:** Round appropriately and support `H:MM:SS` when needed.

#### [diagnostics] #418 — `Subsystem` enum incomplete
- **Severity:** Low | **File:** `src/diagnostics/logger.rs:20-35`
- **Problem:** Missing subsystems (Bluetooth, Audio, Network, Rclone, Process, Cache, Resolver, Pointer, etc.).
- **Impact:** Inconsistent typed logging coverage.
- **Fix direction:** Extend enum or accept structured string/newtype subsystem identifiers.

#### [error] #419 — `color_eyre::install()` global init not idempotent-friendly
- **Severity:** Low | **File:** `src/error.rs:177-185`
- **Problem:** Repeated calls can fail due to global hook install.
- **Impact:** Startup/init fragility in repeated/test contexts.
- **Fix direction:** Guard with `OnceLock`/`Once` and treat already-installed as success.

#### [build] #420 — `build.rs` uses `unwrap()` on `slint_build::compile`
- **Severity:** Low | **File:** `build.rs:5-6`
- **Problem:** Panics with generic context on Slint compile errors.
- **Impact:** Poor diagnostics.
- **Fix direction:** Map errors with file-specific messages (`expect`/`context`).

---

## Suggested fix batches (for parallelizable work)

- **Batch A (trust & diagnostics):** #371, #383, #384, #410, #419
- **Batch B (pointer/input correctness):** #372, #373, #379, #390, #404
- **Batch C (font/text safety & perf):** #375, #376, #385, #386, #405, #406, #416
- **Batch D (CPU render hot paths):** #374, #380, #381, #382, #387, #389, #402, #403, #408, #417
- **Batch E (theme/capabilities/config):** #388, #389, #398, #394
- **Batch F (graphics/animation/hide):** #378, #392, #393, #407, #412
- **Batch G (models/resolvers/geometry):** #391, #395, #396, #397, #411, #414, #415
- **Batch H (tests/build/error/logging):** #377, #399, #400, #401, #409, #413, #418, #420

## Quick validation checklist per batch

- **Trust & diagnostics:** recovery actions perform real side effects and report accurate outcomes; health status updates over time; crash reports persisted with rotation and survive restart.
- **Pointer/input:** popup coordinate mapping correct for all popup types, no hit-test drift from config, O(1) surface lookup, consistent proximity/show-hide behavior across CPU/GPU paths.
- **Font/text:** no render-thread subprocess spawning, bounded glyph cache with stable eviction, safe font loading (no mmap UB), single text rendering API path.
- **CPU render:** no per-frame avoidable allocations in steady state, buffer clear/conversion/vector drawing paths optimized, visual output unchanged (or improved).
- **Theme/capabilities/config:** capabilities reflect reality, colors/theme sourced consistently from config/shared theme across renderers.
- **Graphics/animation/hide:** hide animation behaves correctly or is fully removed/gated; fade completes reliably; frame loading/memory bounded.
- **Tests/build/error:** tests assert behavior or are explicitly ignored with rationale, build emits clear errors, error types preserve causes, logger/init paths are robust and idempotent.
- **General:** `cargo check` passes, relevant `cargo test` suites pass, manual Wayland verification for pointer, popups, and rendering; no new hot-path allocations or blocking calls introduced.
