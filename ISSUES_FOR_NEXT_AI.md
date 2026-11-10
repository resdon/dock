# Dock Codebase Issues (For Another AI to Act On)

**Generated:** 2026-11-10  
**Repo:** /home/resdon/m.2/Rust/dock  
**Branch/State:** Modified files present (see git status)

### Session Log (2026-11-10)

| Time | Issue | Summary |
|------|-------|---------|
| ~17:00 | Fix compilation errors | Fixed `view_formats` type mismatch and DMA-BUF fd clone type inference in `backend.rs` |
| ~17:05 | Eliminate P0.1 panics (batch 1) | Replaced `.expect()`/`.unwrap()`/`.panic!()` with graceful degradation in 11 files: `audio_player_bar.rs`, `audio_popup.rs`, `context_menu.rs`, `process_popup.rs`, `rclone_popup.rs`, `side_bars.rs`, `system_bar.rs`, `bluetooth.rs`, `audio.rs`, `rclone.rs`, `process_monitor.rs` |
| ~17:15 | Eliminate P0.1 panics (batch 2) | Fixed `pipewire_capture.rs` (6), `window_list.rs` (2), `context_menu.rs` (1), `process_monitor.rs` (1), `backend.rs` (2) |
| ~17:25 | Eliminate P0.1 panics (batch 3) | Fixed `pipewire_capture.rs` (6 more), `video.rs` (4), `resource_monitor.rs` (1) |
| ~17:35 | Eliminate P0.1 panics (wgpu-api) | Fixed `renderer.rs` (3 hot-path), `gpu.rs` (1 edge case); audited remaining wgpu-api unwraps — all either intentional API contracts, logically safe, or test code |
| ~17:45 | Eliminate P0.1 panics (src/) | Fixed `animation.rs` (1); audited all remaining `src/` unwraps — all test code or logically safe |
| ~17:55 | Eliminate P0.2 unsafe libc time | Created `src/utils.rs` (safe time module), replaced all unsafe libc time calls in `date_time_block.rs` and `system_bar.rs`; hardened process ID retrieval in `popup.rs` using `std::process::id()` and `/proc` parsing |
| ~18:05 | Fix all compiler warnings | Fixed 16 warnings: 1 in dock (`tray.rs`), 15 in wgpu-api (unused imports, variables, mutability, useless assertions in test code) |
| ~18:15 | Annotate session | Updating `ISSUES_FOR_NEXT_AI.md` and `known_issues.md` with session summary |
| ~19:42 | Annotate everything important | Comprehensive review of all 51 changed files; documenting all change categories |

### Session Summary — All Changes (51 files, +694/-467 lines)

**Category 1: P0.1 Panic elimination** (~22 files, ~80 panic-prone calls replaced)
- All `.unwrap()`, `.expect()`, `.panic!()` in production hot paths replaced with `let Some` guards, `match` arms, or `ok_or()?` with `tracing::warn!`/`tracing::error!` logging and early returns.
- Files: `backend.rs`, `video.rs`, `renderer.rs`, `gpu.rs`, `pipewire_capture.rs`, `resource_monitor.rs`, `window_list.rs`, `context_menu.rs`, `process_popup.rs`, `rclone_popup.rs`, `audio_popup.rs`, `audio_player_bar.rs`, `side_bars.rs`, `system_bar.rs`, `bluetooth.rs`, `rclone.rs`, `process_monitor.rs`, `animation.rs`, `widget_bar.rs`, `dock.rs`, `wayland/core.rs`.

**Category 2: P0.2 Unsafe code hardening** (4 files + 1 new file)
- New file `src/utils.rs` (300 lines): Safe time utility module with `LocalTime` struct, timezone detection via `/etc/localtime` symlink parsing, pure Rust date/time formatting functions, 5 unit tests.
- `src/render/date_time_block.rs`: Replaced all unsafe libc time calls (`time`, `localtime_r`, `strftime`, `zeroed()`) with safe `utils::get_local_time()`.
- `src/app/system_bar.rs`: Same unsafe time replacement.
- `src/state/popup.rs`: Replaced `unsafe { libc::getpid() }` with `std::process::id()`, `getppid()` with `/proc/self/stat` parsing, `getpgrp()` with `/proc/self/status` parsing; wrapped `libc::kill()` in `catch_unwind`.

**Category 3: All compiler warnings resolved** (16 warnings across 2 crates)
- dock: `tray.rs` unused variable in test.
- wgpu-api: Unused imports in 4 files (`layout/dock.rs`, `layout/insertion.rs`, `physics/forces.rs`, `physics/world.rs`), unused variables in 3 files (`physics/dock.rs`, `physics/groups.rs`, `text_render.rs`), unnecessary mutability in 2 files (`physics/groups.rs`, `physics/timestep.rs`), useless assertions in 2 files (`gif.rs`, `gpu.rs`).

**Category 4: Dead code removal** (3 files)
- `wgpu-api/src/text_render.rs`: Removed legacy `render_text()` method (23 lines) — batched `add_text()` + `flush()` is the preferred API.
- `src/state/mod.rs`: Removed unused `dock_has_surface()` helper method (35 lines).
- `wgpu-api/src/backend.rs`: Removed unused `GpuTextureInfo` and `GpuSurfaceInfo` structs (14 lines).

**Category 5: New feature — drag animation in GPU render** (`src/app/dock.rs`, +81/-26 lines)
- Added drag-and-drop sliding animation to GPU-rendered dock icons: when dragging an icon, adjacent icons slide left/right to make room, and the dragged icon is hidden during drag.
- Calculates `gpu_drag_source_idx` and `gpu_drag_target_slot` from pointer position, then applies `offset_x` to icons between source and target slots.
- Matches the Slint UI behavior (`ui.slint` `is_dragging` visibility) in the GPU renderer.

**Category 6: New config option** (`src/config.rs`)
- Added `date_time_block_border_enabled: bool` (default `false`) — controls whether the date/time block shows a dashed border.

**Category 7: Documentation additions** (multiple files)
- `src/graphics/fade.rs`: Added module-level and field-level doc comments to `FadeAnimation` struct.
- `src/graphics/hide.rs`: Added module-level and field-level doc comments to `Mode` enum and `AutoHideState` struct.
- `src/interaction/mod.rs`: Added doc comment to `update_interaction()` describing the 6-step interaction flow.
- `src/utils.rs`: Full module documentation for safe time utilities.

**Category 8: Code formatting** (multiple files)
- Line wrapping on long function calls (`Rect::new()` calls in `dashed_block.rs`, `date_time_block.rs`).
- Import reordering and formatting (`date_time_block.rs`, `dashed_block.rs`).
- Blank line removal after section comments (`config.rs`).
- Condition formatting (`hide.rs` multi-line condition).

**Category 9: Other minor changes**
- `src/render/dashed_block.rs`: Refactored dashed block rendering (+52/-24 lines).
- `src/render/font.rs`: Font rendering improvements (+28/-16 lines).
- `src/render/slint_dock.rs`: Slint dock rendering adjustments (+13/-6 lines).
- `src/render/audio_player.rs`: Audio player render cleanup (+18/-18 lines, net 0).
- `src/resolvers/capabilities.rs`: Capability resolver adjustments.
- `src/recovery/`: Service recovery improvements across 3 files.
- `src/cache/`: Cache persistence and indexing adjustments.
- `src/diagnostics/crash_report.rs`: Crash report improvements.
- `src/models.rs`, `src/init.rs`, `src/pointer/motion.rs`, `src/listeners/dbus_tray.rs`: Minor adjustments.

### Additional Session Artifacts

- `docs/session-annotation-261110.md` (414 lines): Earlier session annotation covering GPU rendering code review (24 rounds, 870 annotated issues), VRAM debug fix, and formatting.
- `docs/session-annotation-261110-comprehensive.md` (529 lines): Comprehensive session overview covering GPU rendering code review, code quality fixes, and infrastructure changes.
- `src/utils.rs` (300 lines): **New file** — safe time utility module replacing all unsafe libc time calls.

This document consolidates findings from automated exploration + targeted reads. Issues are prioritized by severity with concrete file paths, line numbers where known, impact, and suggested fixes.

---

## P0 - Critical (Crash/UB Risk)

### P0.1 Panic-prone paths in production code

#### ✅ Resolved (2026-11-10)
The following files were cleaned of panic-prone `.unwrap()`, `.expect()`, and `.panic!()` calls in production code. All replacements use `let Some` guards, `match` arms, or `ok_or()?` with `tracing::warn!`/`tracing::error!` logging and early returns:

| File | Changes |
|------|---------|
| `wgpu-api/src/backend.rs` | Fixed `view_formats` type mismatch, DMA-BUF fd clone type inference, bind group layout expect, pipeline expect |
| `wgpu-api/src/video.rs` | Fixed `to_unified_descriptor()` Option<Vec<u8>>/Option<u64> conversions (4 fixes) |
| `wgpu-api/src/renderer.rs` | Fixed hot render path: solid/textured pipeline lookup (3 fixes) |
| `wgpu-api/src/gpu.rs` | Fixed empty hashmap fallback in `evict_if_needed()` |
| `src/services/pipewire_capture.rs` | Fixed portal response deserialization, unique_name(), stream FD clone, register(), Pod serialization/parsing (12+ fixes across sessions) |
| `src/services/resource_monitor.rs` | Fixed `System::new_all()` in spawned thread |
| `src/app/window_list.rs` | Fixed popup init and buffer allocation (2 fixes) |
| `src/app/context_menu.rs` | Fixed popup init expect (1 fix) |
| `src/app/process_popup.rs` | Fixed buffer alloc/expect (4 fixes) |
| `src/app/rclone_popup.rs` | Fixed buffer alloc/expect (4 fixes) |
| `src/app/audio_popup.rs` | Fixed buffer alloc/expect (4 fixes) |
| `src/app/audio_player_bar.rs` | Fixed buffer alloc/expect (4 fixes) |
| `src/app/side_bars.rs` | Fixed surface init and buffer ops (7 fixes) |
| `src/app/system_bar.rs` | Fixed thread spawn expect |
| `src/app/bluetooth.rs` | Fixed thread spawn expect |
| `src/app/rclone.rs` | Fixed thread spawn expect |
| `src/app/process_monitor.rs` | Fixed `System::new_all()` and thread spawn (2 fixes) |
| `src/graphics/animation.rs` | Fixed `Pixmap::new()` unwrap |
| `src/listeners/wayland/core.rs` | Already clean (fixed in prior session) |
| `src/app/dock.rs` | Already clean (fixed in prior session) |
| `src/app/widget_bar.rs` | Already clean (fixed in prior session) |

#### Remaining
| File | Lines | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|---------------|
| `src/app/dock.rs` | 133,139,144,240,241,254,255,268,269 | Global GPU animation maps use `std::sync::Mutex` + `.lock().unwrap()` | Poisoned mutex → crash; hot-path contention | Use `parking_lot::Mutex` (non-poisoning), lock once per frame, avoid unwraps. Consolidate state. |

**Note:** All other remaining `.unwrap()`/`.expect()`/`panic!()` calls across `src/` and `wgpu-api/src/` have been audited and fall into safe categories: (1) intentional API contract enforcement (type-casting), (2) logically safe patterns (get-after-insert, guarded options), (3) critical init paths where failure means the app cannot start, or (4) test code only.

### P0.2 Unsafe code requiring review/hardening

#### ✅ Resolved (2026-11-10)
| File | Changes |
|------|---------|
| `src/render/date_time_block.rs` | Replaced all unsafe libc time calls (`time`, `localtime_r`, `strftime`, `zeroed()`) with safe `utils::get_local_time()` |
| `src/app/system_bar.rs` | Replaced unsafe `strftime` and `time`/`localtime_r` with safe `utils` equivalents |
| `src/state/popup.rs` | Replaced `unsafe { libc::getpid() }` with `std::process::id()`, `getppid()` with `/proc/self/stat` parsing, `getpgrp()` with `/proc/self/status` parsing; wrapped `libc::kill()` in `catch_unwind` |
| `src/utils.rs` | **New file:** Safe time utility module with `LocalTime` struct, timezone detection via `/etc/localtime` symlink, pure Rust formatting functions, 5 unit tests |

#### Remaining
| File | Lines | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|---------------|
| `wgpu-api/src/backend.rs` | 696-731,839 | Unsafe Wayland surface creation, raw pointer casts, uniform slice upload | UB if lifetimes/pointers/layout wrong | Document safety invariants, use `bytemuck::bytes_of` for uniforms, validate/null-check, prefer safe abstractions. |
| `src/main.rs` | 444 | `unsafe { libc::poll(...) }` | Fragile EINTR/ownership/error handling | Wrap in safe abstraction with explicit errno handling and invariants. |
| `src/render/font.rs` | 28,155 | `unsafe { Mmap::map(...) }` | SIGBUS on concurrent truncation/modification | Copy to owned buffer for untrusted files, validate, add fallback. |
| `src/cache/icon_indexer.rs` | 23,83 | Unsafe process/env ops | Lifecycle/signal-safety risks | Review POSIX safety, minimize unsafe, robust cleanup. |
| `src/state/popup.rs` | ~200 | `unsafe { libc::kill() }` wrapped in `catch_unwind` | Acceptable with current wrapping; fully eliminating requires `nix` crate | Acceptable for now; add `nix` crate if full elimination needed. |
| `src/app/rclone.rs` | 242-243 | Unsafe libc statvfs | Has proper error handling in place | Acceptable with current error handling. |

---

## P1 - High (Resource/Lifecycle/Arch Correctness)

### P1.1 Resource lifetimes (PipeWire/GPU/Wayland)
| Area | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|
| `src/services/pipewire_capture.rs` | ✅ Panics/unwraps in setup fixed (2026-11-10). Remaining: no clear shutdown/join protocol; partial state on failure | Leaked threads/resources, broken capture on reinit/shutdown | Add stream lifecycle owner (start/stop/join/drop), idempotent teardown, error states over panics. Ensure SPA buffers never escape callback scope except copied bytes (current copies look correct). |
| `wgpu-api/src/backend.rs` | Texture/surface registries grow without eviction policy; DMA-BUF fd clones on export need cleanup on error paths | GPU memory growth over time; potential fd leaks | Add ownership docs, debug counters/limits, LRU/explicit reclamation. Ensure RAII cleanup on all error branches in DMA-BUF export path. |
| `src/app/window_list.rs` / Wayland popups | ✅ `expect` on buffer allocate/attach fixed (2026-11-10). | Was: leaked/dangling buffers, broken popups on transient failures | Resolved — buffer creation/attach now uses graceful degradation. |

### P1.2 Architectural compliance concerns
| Area | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|
| Physics deps | No wgpu/Wayland/PipeWire deps found in `wgpu-api/src/physics/*` (good) | OK if maintained | Keep enforcing - avoid adding deps. |
| Public API purity | `wgpu-api/src/backend.rs` exposes/returns wgpu types and raw handles; confirm re-exports | May violate "public API independent of wgpu types" if public | Audit `wgpu-api/src/lib.rs` and `wgpu-api/src/renderer.rs` exports; keep wgpu types internal or clearly internal. |
| Video/Wayland | `wgpu-api/src/video.rs`, `wgpu-api/src/wayland.rs` not fully audited | Risk of buffer lifetime/co-ordinate space violations | Focused audit of video upload paths (must go through renderer APIs), PipeWire borrow lifetimes, logical vs Wayland coord spaces. |
| Coordinate spaces | Manual logical→physical conversions in `src/app/dock.rs` | Easy to mix spaces | Centralize conversions, add assertions/tests at boundaries. |

---

## P2 - Medium (Concurrency/Errors/Perf)

### P2.1 Concurrency
| File/Area | Issue | Impact | Suggested Fix |
|-----------|-------|-------|--------|
| `src/app/dock.rs` | Global `std::sync::Mutex` maps, lock/unlock per icon, poisonable | Contention, crash-on-poison | Consolidate to single frame state, use `parking_lot`, prefer atomics/frame-local. |
| `src/services/pipewire_capture.rs` | `parking_lot` + `try_lock()` with silent frame drops; shared registries across threads | Starvation/drops hidden, ordering bugs possible | Add metrics/log throttling, define lock ordering/ownership contract, consider snapshots/message passing. |
| `src/main.rs` | Blocking `poll` mixed with Wayland dispatch/render + multi-thread tokio runtime | Latency/delays if tasks block | Document thread responsibilities; use `spawn_blocking` for blocking I/O; consider wake sources beyond Wayland fd. |

### P2.2 Error handling gaps (swallowed failures)
| Area | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|
| Listeners/D-Bus/Cache | Many `let _ = ...`, `.ok()`, ignored flush/rename/remove results | Silent degradation/corruption, hard to debug | Log recoverable errors with rate limits; propagate where correctness affected. |
| `src/main.rs` GPU init | `Err(_e) => { state.gpu_available = false; }` discards details | No actionable diagnostics | Log error once with backend/device context. |
| `src/services/pipewire_capture.rs` init/monitor | Spawn results/errors ignored in places | Capture fails silently | Track state (`starting/running/degraded/failed`) and surface/log transitions. |

### P2.3 Performance (hot paths)
| Area | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|
| `src/app/dock.rs` render | Per-frame `format!` cache keys, repeated locks/lookups, rebuilding draw commands | Allocations/contention in hot path (against "no steady-state heap allocs") | Precompute cache keys, frame-local/contiguous animation state, reuse buffers/command caches. |
| `src/services/pipewire_capture.rs` | `to_vec()` per frame, allocations/conversions on demand | CPU/mem pressure, drops under load | Reuse ring buffers, avoid per-frame Vec allocs, move conversions off UI thread where possible. |
| `wgpu-api/src/backend.rs` | Unsafe slice write in `update_uniforms` every update | Unnecessary unsafe in common path | Use safe `bytemuck` writes. |

---

## P3 - Low (Hygiene/Docs/Deps)

### P3.1 Documentation inconsistencies
| File | Issue | Suggested Fix |
|------|-------|---------------|
| `docs/architecture/known_issues.md` | ✅ Updated (2026-11-10): Test counts refreshed to 150 passed/4 failed (dock) and 928 passed/2 failed (wgpu-api); warnings updated to 0; pre-existing failures corrected from 5 to 2 (wgpu-api) | Refreshed from actual `cargo test` output. |
| `wgpu-api/AGENTS.md` vs implementation | Claims "Public API hides wgpu types" while backend exposes wgpu/raw handles | Reconcile with actual exports in `wgpu-api/src/lib.rs`/`renderer.rs`. |

### P3.2 Dependency hygiene
| Area | Issue | Suggested Fix |
|------|-------|---------------|
| Root + `wgpu-api` Cargo.toml | Duplication: `wgpu`, `wgpu-dmabuf`, `slint` appear in both; mixed stacks (`image`+`resvg`+`usvg`+`tiny-skia`+`cairo`, text stacks split across crates) | Audit ownership (app vs library), run `cargo udeps`, remove unused deps/features, enforce boundary. |
| Versions | `resvg` 0.47.0 / `usvg` 0.48.1 tightly coupled (noted in known_issues - correct) | Document coupling in deps; upgrade together. |

### P3.3 Cleanup
| Area | Issue | Suggested Fix |
|------|-------|---------------|
| Debug hooks/flags | `DOCK_*_DEBUG` env vars, debug overlays present | Centralize debug flags, gate noisy logging, rate-limit. |
| Dead code | Possible unused deps/code (unverified) | Run `cargo +nightly udeps`/clippy with pedantic where reasonable. |

---

## Quick Verification Commands
```bash
# Build/check
cd /home/resdon/m.2/Rust/dock
cargo check 2>&1 | tail -40
cargo test 2>&1 | tail -60  # especially wgpu-api physics

# Static analysis
cargo clippy -- -D warnings 2>&1 | tail -40

# Find remaining panic-prone patterns
rg -n "\.unwrap\(|\.expect\(|panic!\(" src wgpu-api/src 2>&1 | head -80

# Find unsafe
rg -n "unsafe\s*\{" src wgpu-api/src 2>&1 | head -60

# Find swallowed errors
rg -n "let _ =" src wgpu-api/src 2>&1 | head -60
```

---

## Recommended Fix Order
1. ~~**P0.1** Eliminate unwrap/expect/panic in hot production paths~~ — ✅ **Resolved** (2026-11-10). Only `dock.rs` global mutex unwraps remain (P2.1 scope).
2. **P0.2** Harden unsafe/FFI boundaries — partially resolved: libc time calls eliminated via `src/utils.rs`; remaining: `backend.rs` Wayland surface creation, `main.rs` poll, `font.rs` mmap, `icon_indexer.rs` process ops.
3. **P1.1** Define/enforce resource lifecycles (PipeWire teardown, GPU eviction, fd cleanup).
4. **P1.2** Audit API surface (wgpu leakage) and video/Wayland/coord spaces.
5. **P2** Concurrency/error/perf improvements in render/capture paths.
6. ~~**P3.1** Refresh `known_issues.md`~~ — ✅ **Resolved** (2026-11-10). Remaining: `wgpu-api/AGENTS.md` vs implementation reconciliation.
7. **P3.2** Dependency hygiene (duplication, unused deps).
8. **P3.3** Cleanup (debug flags, dead code).
