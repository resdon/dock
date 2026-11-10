# GPU Rendering Review Round 13 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-13.md` (issues #421–#460)  
**Scope reviewed:** Project infrastructure — `Cargo.toml` (root & `wgpu-api/`), `build.rs`, `scripts/`, `.github/workflows/ci.yml`, `Makefile`, `PKGBUILD`, `assets/`, and remaining wgpu-api modules `primitives.rs`, `morphing.rs`, `perf.rs`  
**Totals:** 40 issues — 8 Critical, 12 High, 16 Medium, 4 Low  
**Cumulative (Rounds 1–13):** 460 issues — 65 Critical, 114 High, 179 Medium, 78 Low

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** to fix correctness/packaging blockers and CI trust gaps first.
- Items marked **[cluster]** should be handled together for coherent dependency/build/packaging changes.
- Align with project priorities/invariants (`AGENTS.md`, ADR 005): lowest CPU/RAM, no idle waste, bounded resources, renderer owns GPU resources, `wgpu-api` public API independent of wgpu types, no hidden architectural dependencies.
- For each change, run relevant checks: `cargo check` / `cargo check -p wgpu-api`, `cargo test` (or targeted tests), `cargo clippy --workspace --all-targets`, packaging/build script dry runs as applicable, and manual Wayland run when runtime behavior is affected.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Dependency/binary bloat & feature gating | 421, 422, 423, 441, 442, 443, 451, 452, 453, 454 | Larger binary/RAM/compile time; unused or conflicting stacks in production |
| Architecture boundary violations & type safety | 444, 445, 446 | Main crate bypasses backend abstraction; weak IDs; popup geometry/input mismatch risk |
| Geometry/render correctness | 428 | Silent wrong shape output (`ellipse` renders circle) |
| Packaging & distribution (PKGBUILD/Makefile/scripts) | 424, 425, 426, 433, 434, 438, 439, 449, 450, 456, 457, 459 | Broken/non-reproducible packages, install/dist failures, wrong runtime assets/deps |
| CI/build reliability & signal quality | 432, 435, 436, 437, 455 | False green/constant red CI, poor diagnostics, missed warnings/links |
| Profiling/dead or half-finished infrastructure | 427, 429, 430, 431, 447, 448, 460 | Unused code, hot-path panic risk and O(n) operations if enabled |
| Caching/assets correctness | 440, 451 | Wrong icon selection due to cache key collisions; embedded asset inflexibility |

## Recommended fix order

1. **#428** — fix `ellipse()` geometry bug (correctness). 
2. **#444, #421, #422, #423** — enforce backend boundary and trim/gate heavy deps/features (architecture + bloat). [cluster]
3. **#424, #425, #426, #450, #438, #433, #449** — repair packaging/build/install and icon discovery reproducibility. [cluster]
4. **#435, #437, #432, #455, #436** — make CI/build checks trustworthy and actionable. [cluster]
5. **#440** — fix SVG cache key collisions (user-visible wrong icons).
6. **#429, #430, #431, #447, #448, #460** — address profiling hot-path risks, O(n) ops, and dead/unused profiling surface (integrate, gate, or trim). [cluster]
7. **#427, #439, #459** — remove/fix dead or dev-only binaries/scripts behavior.
8. **Remaining Medium/Low** — dependency cleanup, API hygiene, docs/packaging polish.

---

## Issues (sorted by severity, then area)

### Critical

#### [wgpu-api/primitives] #428 — `ellipse()` ignores `ry`, always renders a circle
- **Severity:** Critical | **File:** `wgpu-api/src/primitives.rs:124`
- **Problem:** Signature takes `ry` but implementation uses `rx` for a full circular arc (`_ry` unused).
- **Impact:** Silent geometry correctness bug; callers requesting non-circular ellipses get circles.
- **Fix direction:** Implement true ellipse path generation (parametric sampling, cubic Bezier approximation, or scaled arc segments) using both `rx` and `ry`; update `shape::ellipse()` and add tests for aspect ratios.
- **Notes:** Validate stroke/fill behavior and tessellation quality.

#### [deps/build] #421 — `tokio = { features = ["full"] }` bloats binary/RAM and broadens runtime surface
- **Severity:** Critical | **File:** `Cargo.toml:51`
- **Problem:** Enables all Tokio features (including multi-thread runtime, fs/net/process/etc.) though only a subset is needed.
- **Impact:** Larger binary/compile time, higher memory pressure, risk of accidental multi-thread runtime patterns conflicting with project constraints.
- **Fix direction:** Replace with minimal feature set required by actual usage (e.g. `rt`, `sync`, `time`, `macros` as applicable); verify at compile time.
- **Notes:** [cluster with #422,#423,#444 dependency hygiene].

#### [deps/features] #422 — `sixel-rs`/`icy_sixel` terminal deps linked into main Wayland binary
- **Severity:** Critical | **Files:** `Cargo.toml:35-36`, `src/lib.rs:122-134` (terminal graphics usage)
- **Problem:** Terminal image/sixel support compiled into production dock path.
- **Impact:** Unnecessary binary size/dependency surface for unused runtime feature.
- **Fix direction:** Gate crates and `terminal_graphics` module behind a dev/debug feature (off by default); ensure release builds exclude them.
- **Notes:** [cluster with #452,#453].

#### [deps] #423 — `cairo-rs` likely unused in GPU rendering path
- **Severity:** Critical | **File:** `Cargo.toml:53`
- **Problem:** Pulls Cairo + GLib/GObject stack despite wgpu/Slint rendering architecture.
- **Impact:** Significant binary/compile-time/dependency bloat without contributing to GPU path.
- **Fix direction:** Audit for `cairo` usage; remove if unused (and transitive deps become unnecessary), or isolate behind explicit feature only where required.
- **Notes:** Confirm no hidden runtime dependency before removal.

#### [packaging] #424 — PKGBUILD version mismatch with `Cargo.toml`
- **Severity:** Critical | **Files:** `PKGBUILD:3` (`pkgver=0.1.0`) vs `Cargo.toml:3` (`1.5.2`)
- **Problem:** Package metadata version diverges from crate version.
- **Impact:** Confusing installs/upgrades and incorrect version reporting.
- **Fix direction:** Sync `pkgver` to Cargo version (manual update policy or derive in packaging flow).
- **Notes:** Keep `pkgrel` semantics intact.

#### [packaging] #425 — PKGBUILD `source` uses directories and `SKIP` checksums
- **Severity:** Critical | **File:** `PKGBUILD:24`
- **Problem:** `source=('Cargo.toml' 'src' 'assets' 'scripts')` is not a valid/reliable makepkg source form; checksums skipped.
- **Impact:** Broken or non-reproducible packaging; no integrity verification.
- **Fix direction:** Use proper source strategy (git+tag/commit or release tarball) with real checksums.
- **Notes:** [cluster with #424,#426,#450].

#### [packaging] #426 — PKGBUILD generates icon list from build host during packaging
- **Severity:** Critical | **File:** `PKGBUILD:55`
- **Problem:** Runs `scripts/list_icons.sh` against host icon dirs at package time.
- **Impact:** Non-reproducible, host-specific package contents; wrong icons on target systems.
- **Fix direction:** Generate icon list at runtime or in install/post-install hooks on target machine; do not bake host scan into package payload.
- **Notes:** [cluster with #438].

#### [codebase hygiene] #427 — `src/services/test.rs` is a dead/meaningless standalone binary
- **Severity:** Critical | **File:** `src/services/test.rs:1-59` (and `Cargo.toml` bin entry if present)
- **Problem:** Binary counts desktop files into unused value, produces no useful output; name `test` is confusing.
- **Impact:** Dead code/build artifact and developer confusion.
- **Fix direction:** Remove binary target and file, or replace with a documented debug utility with real output and non-conflicting name.
- **Notes:** Check for references/scripts depending on it.

### High

#### [wgpu-api/perf] #429 — Profiling structs use `RefCell`, risking runtime panics
- **Severity:** High | **File:** `wgpu-api/src/perf.rs` (throughout, e.g. Stopwatch fields)
- **Problem:** Interior mutability via `RefCell` in profiling types (`Stopwatch`, `FrameRateAnalyzer`, `PerfCounter`, `Profiler`) can double-borrow.
- **Impact:** Panic risk if used in nested/hot render contexts.
- **Fix direction:** Use `Cell` for Copy state, `&mut self` APIs, or lock-free/borrow-free designs suitable for hot paths; avoid borrow checks in frame-critical code.
- **Notes:** [cluster with #447,#460].

#### [wgpu-api/perf] #430 — `FrameRateAnalyzer::record_frame()` uses `Vec::remove(0)` (O(n))
- **Severity:** High | **File:** `wgpu-api/src/perf.rs:309`
- **Problem:** Front removal shifts entire sample buffer each time limit exceeded.
- **Impact:** Avoidable per-frame CPU work when profiling enabled.
- **Fix direction:** Use `VecDeque` or fixed ring buffer for O(1) eviction.
- **Notes:** [cluster with #431].

#### [wgpu-api/perf] #431 — `JitterAnalyzer` uses same O(n) `remove(0)`
- **Severity:** High | **File:** `wgpu-api/src/perf.rs:1002`
- **Problem:** Same front-shift pattern as #430.
- **Impact:** Same O(n) overhead.
- **Fix direction:** Convert to `VecDeque`/ring buffer. [cluster with #430].

#### [build] #432 — `build.rs` panics on Slint compile errors with `unwrap()`
- **Severity:** High | **File:** `build.rs:5-6`
- **Problem:** No contextual error handling for `slint_build::compile`.
- **Impact:** Poor diagnostics during build failures.
- **Fix direction:** Use `expect("failed to compile ui.slint")` / file-specific messages or propagate errors clearly.
- **Notes:** [cluster with #420 context from prior rounds if touching build].

#### [make] #433 — Makefile build target checks for wrong binary path
- **Severity:** High | **File:** `Makefile:9`
- **Problem:** Tests for `dock` in CWD instead of built artifact (e.g. `target/release/dock`).
- **Impact:** Can skip builds or install wrong/empty file.
- **Fix direction:** Check correct release/debug artifact path and make install depend on successful build output.
- **Notes:** [cluster with #449,#456].

#### [make/dist] #434 — `dist` target hardcoded to `linux-amd64`
- **Severity:** High | **File:** `Makefile:36-46`
- **Problem:** Architecture/platform not parameterized.
- **Impact:** Limited/incorrect distribution artifacts for other targets (e.g. aarch64).
- **Fix direction:** Derive target triple/arch from `rustc -vV`, env vars, or make parameters.

#### [ci] #435 — `cargo tree -d` step always passes even with duplicates
- **Severity:** High | **File:** `.github/workflows/ci.yml:52`
- **Problem:** Command prints duplicates but exits 0; CI treats as success.
- **Impact:** False confidence; duplicate deps undetected.
- **Fix direction:** Fail when output non-empty (e.g. capture and test, or `test -z "$(cargo tree -d ...)"` with appropriate flags).
- **Notes:** Align `scripts/check.sh` behavior if duplicated.

#### [ci] #436 — No dedicated/strict wgpu-api clippy enforcement
- **Severity:** High | **File:** `.github/workflows/ci.yml`
- **Problem:** Workspace clippy may not surface crate-specific issues clearly/strictly.
- **Impact:** Missed warnings in backend crate.
- **Fix direction:** Add explicit `cargo clippy -p wgpu-api --all-targets -- -D warnings` (or agreed lint level) job/step.

#### [ci] #437 — Physics tests job red due to known pre-existing failures
- **Severity:** High | **File:** `.github/workflows/ci.yml:96`
- **Problem:** Runs `cargo test physics` without handling known failures.
- **Impact:** Permanent CI noise masks regressions.
- **Fix direction:** Track/fix known failures, or temporarily skip specific tests with issue links until fixed (prefer fixing).
- **Notes:** Keep visibility via TODO/issues; avoid blanket `continue-on-error`.

#### [scripts/assets] #438 — `scripts/list_icons.sh` hardcodes Flatpak appstream commit hash path
- **Severity:** High | **File:** `scripts/list_icons.sh:6`
- **Problem:** Commit-hash directory name is volatile across updates.
- **Impact:** Flatpak icon discovery breaks after Flathub updates.
- **Fix direction:** Discover latest/active appstream icons dir dynamically (glob/find newest matching path) instead of hardcoded hash.

#### [scripts] #439 — `scripts/launcher.sh` async temp cleanup is racy/leaky
- **Severity:** High | **File:** `scripts/launcher.sh:133`
- **Problem:** Background `(sleep 30; rm -rf ...) &` detaches; can orphan processes/dirs under rapid launches.
- **Impact:** Temp/resource leakage.
- **Fix direction:** Use `trap`-based cleanup tied to launcher lifecycle or delegate to launched app/runtime-safe temp handling.

#### [assets/cache] #440 — SVG raster cache key uses file stem only, causing collisions
- **Severity:** High | **File:** `src/lib.rs:67`
- **Problem:** Cache key `{stem}_{size}.png` ignores full path/content; different icons sharing name collide.
- **Impact:** Wrong icon rendered for different apps/sources.
- **Fix direction:** Include hash of canonical/full path (and optionally mtime/content hash/version) in cache key; ensure deterministic and bounded cache naming.

### Medium

#### [deps/runtime] #441 — `mimalloc` enabled unconditionally
- **Severity:** Medium | **File:** `Cargo.toml:85`
- **Problem:** Global allocator forced without proven benefit and potential interaction with wgpu/Vulkan allocators.
- **Impact:** Possible perf/memory fragmentation regressions.
- **Fix direction:** Gate behind feature flag and benchmark before enabling by default.

#### [deps] #442 — `half` crate likely unused in wgpu-api
- **Severity:** Medium | **File:** `wgpu-api/Cargo.toml:29`
- **Problem:** Dependency present without imports/use.
- **Impact:** Extra compile time/binary weight.
- **Fix direction:** Remove if unused; reintroduce only with FP16 implementation.

#### [text/rendering] #443 — `glyphon`/`cosmic-text`/`fontdb` coexist with custom text modules
- **Severity:** Medium | **File:** `wgpu-api/Cargo.toml:22-24`
- **Problem:** Parallel text rendering stacks alongside custom `text_render.rs`, `glyph.rs`, `shaping.rs`, `fontdb.rs`.
- **Impact:** Duplication, maintenance burden, inconsistency risk.
- **Fix direction:** Choose a single text pipeline and remove or isolate the other; document ownership and migration path.
- **Notes:** Coordinate with Round 7 font/glyph findings.

#### [architecture] #444 — Main crate depends directly on `wgpu`/`wgpu-dmabuf`
- **Severity:** Medium | **Files:** `Cargo.toml:58-59`, `wgpu-api/Cargo.toml:19-20`
- **Problem:** Main crate can use wgpu types directly, bypassing wgpu-api abstraction.
- **Impact:** Violates "All wgpu internals hidden inside the backend" invariant; tighter coupling.
- **Fix direction:** Remove direct wgpu deps from main crate where possible and expose needed abstractions in `wgpu-api`.
- **Notes:** [cluster with #421 cleanup pass on deps].

#### [wgpu-api/morphing] #445 — `MorphManager` uses raw `u64` node IDs
- **Severity:** Medium | **File:** `wgpu-api/src/morphing.rs:532`
- **Problem:** Untyped `HashMap<u64, ...>` key.
- **Impact:** ID/type confusion across subsystems.
- **Fix direction:** Introduce `NodeId`/newtype wrapper and update APIs.

#### [wgpu-api/morphing/wayland] #446 — `PopupAnimation` not integrated with Wayland popup lifecycle/surface size
- **Severity:** Medium | **File:** `wgpu-api/src/morphing.rs` (module behavior)
- **Problem:** Visual geometry animation can diverge from actual Wayland surface/input region.
- **Impact:** Input dead zones/mismatch when visuals differ from surface.
- **Fix direction:** Define contract: couple animation with surface configure/resize/input region updates, or clearly restrict to compositor-managed cases.

#### [wgpu-api/perf] #447 — Profiler infrastructure not integrated into render loop
- **Severity:** Medium | **File:** `wgpu-api/src/perf.rs` (throughout)
- **Problem:** `begin_phase/end_phase/time_phase` APIs unused in renderer/backend hot paths.
- **Impact:** Dead code; no runtime profiling signal.
- **Fix direction:** Integrate minimally behind feature flag or remove unused parts.
- **Notes:** [cluster with #429,#448,#460].

#### [wgpu-api/perf] #448 — Stress test config/result types exist without runner
- **Severity:** Medium | **File:** `wgpu-api/src/perf.rs:859-973`
- **Problem:** Incomplete feature (types only).
- **Impact:** Dead/half-implemented code.
- **Fix direction:** Implement runner or remove/gate types.

#### [make/install] #449 — `cp -r assets/24/*` fails when directory is empty
- **Severity:** Medium | **File:** `Makefile:27`
- **Problem:** Glob expansion fails if no files match.
- **Impact:** `make install` can fail.
- **Fix direction:** Handle empty dirs safely (e.g. conditional copy, `rsync`, or shell options) while preserving intended layout.

#### [packaging] #450 — PKGBUILD missing runtime deps (pipewire/libspa/fontconfig as needed)
- **Severity:** Medium | **File:** `PKGBUILD:8-18`
- **Problem:** Declared `depends` omit required runtime libraries/services.
- **Impact:** Installed package may not run or lack capture/font functionality.
- **Fix direction:** Add accurate runtime dependencies and split build vs runtime deps appropriately.

#### [assets/binary] #451 — Badge PNG embedded via `include_bytes!`
- **Severity:** Medium | **Files:** `src/render/badges.rs:9`, `assets/dialog-warning.png`
- **Problem:** Asset compiled into binary and not themeable/runtime-replaceable.
- **Impact:** Binary bloat/inflexibility.
- **Fix direction:** Draw procedurally or load from themed/runtime asset path with fallback.

#### [api] #452 — `terminal_graphics` exposed in public API
- **Severity:** Medium | **File:** `src/lib.rs:52`
- **Problem:** Debug/terminal-specific functionality re-exported publicly.
- **Impact:** API pollution; unstable/internal surface exposed.
- **Fix direction:** Make private or gate behind non-default feature. [cluster with #422,#453].

#### [deps/features] #453 — `base64` only needed for terminal/sixel debugging
- **Severity:** Medium | **Files:** `Cargo.toml:32`, `src/lib.rs:8`
- **Problem:** Dependency unconditionally included for dev-only path.
- **Impact:** Unnecessary production dependency.
- **Fix direction:** Gate behind same feature as terminal graphics/sixel support.

#### [wgpu-api/features] #454 — `gif` support present but known problematic (eager decode)
- **Severity:** Medium | **File:** `wgpu-api/Cargo.toml:28`
- **Problem:** GIF decoding has known memory issues (per prior round findings).
- **Impact:** Half-implemented/unsafe-for-scale feature.
- **Fix direction:** Fix decoder memory behavior or gate feature off by default until fixed.

#### [ci/docs] #455 — `cargo doc` in CI doesn't fail on broken intra-doc links
- **Severity:** Medium | **File:** `.github/workflows/ci.yml:122`
- **Problem:** Doc build lacks strict link checking flags.
- **Impact:** Broken documentation links can merge unnoticed.
- **Fix direction:** Enable `-D rustdoc::broken_intra_doc_links` (and consider `--document-private-items` only if desired).

### Low

#### [make] #456 — `BINARY_NAME` hardcoded in Makefile
- **Severity:** Low | **File:** `Makefile:2`
- **Problem:** Not derived from Cargo package metadata.
- **Impact:** Drift if binary name changes.
- **Fix direction:** Derive from `cargo metadata`/Cargo.toml or centralize variable.

#### [packaging] #457 — PKGBUILD license set to `custom` without valid identifier
- **Severity:** Low | **File:** `PKGBUILD:7`
- **Problem:** Non-specific license field.
- **Impact:** Packaging policy/compliance ambiguity.
- **Fix direction:** Set correct SPDX/Arch license identifier(s) and install license file if required.

#### [scripts/qa] #458 — `scripts/check.sh` relies on `cargo machete` (false positives)
- **Severity:** Low | **File:** `scripts/check.sh:131-141`
- **Problem:** Known false positives for some dependency kinds.
- **Impact:** Noisy failures.
- **Fix direction:** Tune usage, whitelist known cases, or replace/supplement with `cargo-udeps` in appropriate environments.

#### [scripts] #459 — `scripts/launcher.sh` contains dev-specific `taskman` special case
- **Severity:** Low | **File:** `scripts/launcher.sh:138-143`
- **Problem:** Hardcoded local binary path behavior in production launcher.
- **Impact:** Unexpected dev-only behavior/leakage.
- **Fix direction:** Remove or gate behind explicit debug env flag.

#### [wgpu-api/perf] #460 — `perf.rs` is very large (1467 LOC) with unused/partial utilities
- **Severity:** Low | **File:** `wgpu-api/src/perf.rs`
- **Problem:** Broad profiling/stress-test surface not fully used.
- **Impact:** Maintenance/compile burden.
- **Fix direction:** Trim to active functionality or gate behind `profiling`/`perf` feature.

---

## Suggested fix batches (for parallelizable work)

- **Batch A (correctness):** #428, #440
- **Batch B (deps & architecture boundaries):** #421, #422, #423, #441, #442, #443, #444, #452, #453, #454
- **Batch C (packaging & install):** #424, #425, #426, #433, #434, #449, #450, #456, #457
- **Batch D (CI/build/scripts):** #432, #435, #436, #437, #438, #439, #455, #458, #459
- **Batch E (profiling & dead infrastructure):** #427, #429, #430, #431, #447, #448, #460
- **Batch F (assets/API/types/popup integration):** #445, #446, #451

## Quick validation checklist per batch

- **Correctness:** `ellipse(cx,cy,rx,ry)` renders correct aspect ratios; SVG cache never returns wrong icon for same stem/different path.
- **Deps & architecture:** release build excludes dev-only terminal/sixel/base64 paths when gated; main crate does not depend on raw wgpu internals; minimal tokio features compile and run.
- **Packaging & install:** PKGBUILD builds reproducibly from valid source with correct version/deps; `make build/install/dist` work on clean trees and empty asset cases; icon discovery not host/hash-dependent.
- **CI/build/scripts:** `cargo tree -d` fails on duplicates, Slint/build errors are contextual, doc link checks enforced, clippy covers `wgpu-api`, physics CI status reflects reality; launcher temp cleanup has no leaks.
- **Profiling:** no `RefCell` panic risk in hot paths if enabled, sample buffers O(1), unused profiling code removed or behind feature with clear integration point.
- **Assets/API/types:** public API surface limited appropriately, node IDs typed, popup animation contract respects Wayland surface/input lifecycle, badge rendering/assets behave as intended.
- **General:** `cargo check` and `cargo check -p wgpu-api` pass; `cargo test`/targeted tests pass; `cargo clippy --workspace --all-targets` clean per policy; no new steady-state allocations/blocking in hot paths.
