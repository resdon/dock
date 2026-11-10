# Dock Codebase Issues (For Another AI to Act On)

**Generated:** 2026-11-10  
**Repo:** /home/resdon/m.2/Rust/dock  
**Branch/State:** Modified files present (see git status)

This document consolidates findings from automated exploration + targeted reads. Issues are prioritized by severity with concrete file paths, line numbers where known, impact, and suggested fixes.

---

## P0 - Critical (Crash/UB Risk)

### P0.1 Panic-prone paths in production code
| File | Lines | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|---------------|
| `src/app/dock.rs` | 133,139,144,240,241,254,255,268,269 | Global GPU animation maps use `std::sync::Mutex` + `.lock().unwrap()` | Poisoned mutex → crash; hot-path contention | Use `parking_lot::Mutex` (non-poisoning), lock once per frame, avoid unwraps. Consolidate state. |
| `src/app/window_list.rs` | 134,276,382,713,721 | `unwrap()`/`expect()` in window list/Wayland buffer paths | Crash on protocol/state/allocation mismatch | Propagate `Result`/`Option`, degrade gracefully (close popup/skip frame), log once. |
| `src/app/widget_bar.rs` | 38,60,223,231 | `expect()` on globals/init/buffers | Hard crash on compositor/env differences | Fallible init/render; surface recoverable errors. |
| `wgpu-api/src/backend.rs` | 383,459 | `expect("shared bind group layout must be initialized")` | Library panic on invariant violation | Encode invariant in types/state or return `Result`. |
| `wgpu-api/src/backend.rs` | 1011 | `fd.try_clone().map_err(...).unwrap()` in DMA-BUF export | FD clone failure becomes panic despite Result API | Propagate clone error to caller. |
| `src/services/pipewire_capture.rs` | ~1068, ~1550 | `panic!("No session path...")`, `.register().unwrap()` | Crashes capture thread on malformed portal response/setup failure | Return typed errors, tear down partial state, mark stream unavailable. |
| `src/listeners/wayland/core.rs` | 277 | `expect("Failed to secure pointer handle")` | Crashes on compositor/input variance | Degrade input features gracefully. |

### P0.2 Unsafe code requiring review/hardening
| File | Lines | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|---------------|
| `wgpu-api/src/backend.rs` | 696-731,839 | Unsafe Wayland surface creation, raw pointer casts, uniform slice upload | UB if lifetimes/pointers/layout wrong | Document safety invariants, use `bytemuck::bytes_of` for uniforms, validate/null-check, prefer safe abstractions. |
| `src/main.rs` | 444 | `unsafe { libc::poll(...) }` | Fragile EINTR/ownership/error handling | Wrap in safe abstraction with explicit errno handling and invariants. |
| `src/render/font.rs` | 28,155 | `unsafe { Mmap::map(...) }` | SIGBUS on concurrent truncation/modification | Copy to owned buffer for untrusted files, validate, add fallback. |
| `src/cache/icon_indexer.rs` | 23,83 | Unsafe process/env ops | Lifecycle/signal-safety risks | Review POSIX safety, minimize unsafe, robust cleanup. |
| `src/state/popup.rs` | 119,149 | Unsafe-related flows | Needs review | Audit lifetimes/signals. |
| `src/app/system_bar.rs` | 294-303 | Unsafe libc time/statvfs | Error-prone, duplicated | Centralize safe wrappers. |
| `src/app/rclone.rs` | 242-243 | Unsafe libc calls | Duplicated patterns | Centralize safe wrappers. |
| `src/render/date_time_block.rs` | 37,38,39,43,65,66,67,71,93,94,95,99 | Unsafe libc time calls | Duplicated patterns | Centralize safe wrappers. |

---

## P1 - High (Resource/Lifecycle/Arch Correctness)

### P1.1 Resource lifetimes (PipeWire/GPU/Wayland)
| Area | Issue | Impact | Suggested Fix |
|------|-------|-------|--------|
| `src/services/pipewire_capture.rs` | Panics/unwraps in setup; no clear shutdown/join protocol; partial state on failure | Leaked threads/resources, broken capture on reinit/shutdown | Add stream lifecycle owner (start/stop/join/drop), idempotent teardown, error states over panics. Ensure SPA buffers never escape callback scope except copied bytes (current copies look correct). |
| `wgpu-api/src/backend.rs` | Texture/surface registries grow without eviction policy; DMA-BUF fd clones on export need cleanup on error paths | GPU memory growth over time; potential fd leaks | Add ownership docs, debug counters/limits, LRU/explicit reclamation. Ensure RAII cleanup on all error branches in DMA-BUF export path. |
| `src/app/window_list.rs` / Wayland popups | `expect` on buffer allocate/attach; no fallback | Leaked/dangling buffers, broken popups on transient failures | Make buffer creation/attach transactional and recoverable. |

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
| `docs/architecture/known_issues.md` | States "334 passed, 5 failures", "0 errors, ~20 warnings", "Last verified 2026-11-10"; lists specific test failures; may be stale vs current codebase | Refresh from actual `cargo test`/`cargo check`/dep tree. Revalidate the 5 pre-existing failures. Update resolved section as needed. |
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
1. **P0.1** Eliminate unwrap/expect/panic in hot production paths (`dock.rs`, `window_list.rs`, `widget_bar.rs`, `pipewire_capture.rs`, `backend.rs`).
2. **P0.2** Harden unsafe/FFI boundaries (bytemuck, safe wrappers for libc/mmap/poll, document invariants).
3. **P1.1** Define/enforce resource lifecycles (PipeWire teardown, GPU eviction, fd cleanup).
4. **P1.2** Audit API surface (wgpu leakage) and video/Wayland/coord spaces.
5. **P2** Concurrency/error/perf improvements in render/capture paths.
6. **P3** Refresh docs and dependency hygiene.
