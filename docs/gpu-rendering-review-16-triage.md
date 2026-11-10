# GPU Rendering Review Round 16 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-16.md` (issues #551–#595)  
**Scope reviewed:** `src/render/theme.rs`, `src/render/dashed_block.rs`, `src/render/date_time_block.rs`, `src/render/audio.rs`, `src/render/audio_player.rs`, `src/render/badges.rs`, `src/render/bluetooth.rs`, `src/render/font.rs`, `src/render/text.rs`, `src/state/popup.rs`, `src/resolvers/process.rs`, `src/services/monitor.rs`, `src/render/mod.rs`, `src/render/utils.rs`  
**Totals:** 45 issues — 7 Critical, 23 High, 15 Medium, 0 Low  
**Cumulative (Rounds 1–16):** 595 issues — 72 Critical, 128 High, 199 Medium, 78 Low

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** to remove event-thread I/O, render-path CPU waste, and dead/standalone code first.
- Items marked **[cluster]** should be fixed together to avoid inconsistent rendering or partial mitigations.
- Preserve project priorities/invariants: event-driven behavior, lowest CPU/RAM, no steady-state heap allocations, no render/event-thread blocking, bounded caches, and shared config/scale-aware behavior.
- For each fix, run relevant checks: `cargo check`, targeted `cargo test`, and manual Wayland verification for popups (audio/bluetooth/date-time/dashed blocks), icon/font rendering, hit-testing at 1x/2x scale, and window/app lifecycle flows.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Standalone/dead code in library paths | 551, 584, 586, 589, 590 | Dead weight, maintenance burden, confusing module boundaries |
| Event-time filesystem/process scans | 552, 553, 554, 555, 578, 580, 587, 588, 594, 595 | I/O/CPU spikes on window/app events, permission noise, latency |
| CPU render hot-path waste (per-pixel/f32/SVG parse) | 556, 557, 559, 560, 561, 562, 563, 564, 567, 568, 569, 570, 571, 575, 577, 582, 585 | High per-frame CPU cost across popup renderers; poor scaling |
| Per-frame allocations & cache growth | 565, 566, 572, 573, 574, 576, 581 | Heap churn, unbounded memory growth, first-use latency spikes |
| Subprocess/command overhead | 572, 579 | Blocking/latency on render or interaction paths |
| Color/theme duplication & rendering path consistency | 558, 583 | Drift risk, duplicated maintenance |
| Scale/hit-test correctness | 591, 592, 593 | Click/hover mismatches at non-1x scale |
| Minor/organizational issues | 589, 590 | Low-risk cleanup |

## Recommended fix order

1. **#551** — isolate/remove standalone `monitor.rs` binary logic from library build path (unblocks removal of related dead/expensive code). [cluster]
2. **#552, #553, #554, #555, #578, #580, #587, #588, #594, #595** — eliminate event-path `WalkDir`/full process scans/`/proc` reads and add caching/indexing/throttling. [cluster]
3. **#556, #557, #559, #560, #561, #562, #563, #564, #567, #568, #569, #570, #571, #577** — optimize popup/dashed/date-time/audio rendering (cache static SVGs, integer blending, scanline/primitive optimizations, avoid per-pixel f32 math). [cluster]
4. **#565, #566, #576** — remove per-frame string/vec allocations in date-time and bluetooth render paths. [cluster]
5. **#572, #573, #574, #575** — fix font fallback blocking, bound glyph cache, reduce first-miss mmap/tofu overhead. [cluster]
6. **#591, #592, #593** — make popup hit-testing scale-aware and consistent with render coordinates. [cluster]
7. **#558, #583, #584, #586, #581, #585** — consolidate color/badge/text paths and remove dead/debug code.
8. **#579, #589, #590** — minor interaction and module cleanup.

---

## Issues (sorted by severity, then area)

### Critical

#### [services/build-boundary] #551 — Standalone binary with `main()` embedded in library crate
- **Severity:** Critical | **File:** `src/services/monitor.rs:572-589`
- **Problem:** `monitor.rs` contains `fn main()` and Wayland dispatch app logic used as a standalone tool, but lives in the library crate/module graph.
- **Impact:** Dead/irrelevant code and dependencies compiled into dock library; confusing ownership and maintenance.
- **Fix direction:** Move to a separate binary target/crate or remove if obsolete; keep only reusable library components (if any) in `src/services`.
- **Notes:** [cluster with #552-#555,#594,#595] many issues become moot if removed/moved.

#### [services/icons] #552 — `WalkDir` depth 6 on icon directories per window event
- **Severity:** Critical | **File:** `src/services/monitor.rs:184-210`
- **Problem:** Traverses large icon trees (`/usr/share/icons`, pixmaps, home dirs) on event path.
- **Impact:** Major I/O latency spikes during window events.
- **Fix direction:** Use cached icon index/`linicon` lookups only; avoid recursive tree walks in hot/event paths.

#### [services/process] #553 — Full sysinfo process refresh on every `HandleEvent::Done`
- **Severity:** Critical | **File:** `src/services/monitor.rs:533-534`
- **Problem:** Refreshes all processes on frequent Wayland toplevel done events.
- **Impact:** CPU/I/O waste unrelated to actual icon resolution needs.
- **Fix direction:** Throttle refreshes and/or refresh only when required by specific resolution requests.

#### [services/process/procfs] #554 — Reads `/proc/<pid>/environ` for processes to find SteamAppId
- **Severity:** Critical | **File:** `src/services/monitor.rs:238-267`
- **Problem:** Scans descendants and reads environ broadly; fails for non-owned processes.
- **Impact:** Permission noise/errors and unnecessary I/O.
- **Fix direction:** Restrict to candidate Steam-related PIDs/ownership checks, cache results, avoid repeated scans.

#### [services/desktop] #555 — Walks/parses all `.desktop` files per window AppId event fallback
- **Severity:** Critical | **File:** `src/services/monitor.rs:91-143`
- **Problem:** Fallback does recursive scan and line-by-line parsing across XDG dirs.
- **Impact:** I/O spikes on window/app identification events.
- **Fix direction:** Build/cache desktop entry index (name/wmclass/startupwmclass -> metadata) and query it.

#### [render/audio_player] #556 — O(r²) per-pixel circle rendering for disc (11+ circles)
- **Severity:** Critical | **File:** `src/render/audio_player.rs:380-415`
- **Problem:** Filled circles drawn via full bounding-box scan with `x*x+y*y<=r*r` per circle.
- **Impact:** Large CPU cost per audio player render (~240k iterations before blend cost).
- **Fix direction:** Pre-render/cache static disc layers, use scanline/midpoint circle fill, or draw via GPU/shared primitives.

#### [render/audio_player] #557 — `blend()` uses f32 arithmetic per pixel
- **Severity:** Critical | **File:** `src/render/audio_player.rs:519-533`
- **Problem:** Per-channel u8↔f32 conversions and floating-point alpha math in inner loop.
- **Impact:** Significant CPU overhead across all blended shapes.
- **Fix direction:** Replace with integer alpha blending (`(src*a + dst*(255-a)) / 255` or `>>8` approximation) and reuse for all render primitives in module.
- **Notes:** [cluster with #569,#571,#563].

### High

#### [render/theme] #558 — Duplicate BGRA and ARGB color constants
- **Severity:** High | **File:** `src/render/theme.rs:6-24`
- **Problem:** Parallel constant sets for different byte orders.
- **Impact:** Drift/maintenance errors and wrong colors if only one set updated.
- **Fix direction:** Define canonical color representation and derive format-specific constants/helpers.

#### [render/theme] #559 — `fill_rounded()` is O(w×h) with per-pixel corner checks
- **Severity:** High | **File:** `src/render/theme.rs:29-78`
- **Problem:** Iterates entire rect and tests corner circle equation per pixel.
- **Impact:** CPU waste for rounded backgrounds.
- **Fix direction:** Use scanline spans per row (precompute corner coverage) or rasterize rounded rect more efficiently.

#### [render/theme] #560 — `stroke_rounded()` does O(w×h) with double containment checks
- **Severity:** High | **File:** `src/render/theme.rs:80-115`
- **Problem:** Calls inner/outer containment per pixel across full bounds.
- **Impact:** Compounded CPU waste for borders.
- **Fix direction:** Compute border spans/edges directly instead of full-area double containment.

#### [render/dashed_block] #561 — Per-pixel dashed border scans full canvas O(w×h)
- **Severity:** High | **File:** `src/render/dashed_block.rs:103-123`
- **Problem:** Iterates all pixels to draw border dashes.
- **Impact:** Unnecessary work, mostly interior pixels skipped.
- **Fix direction:** Draw only 4 edges with dash stepping.
- **Notes:** [cluster with #564].

#### [render/dashed_block] #562 — Parses/renders 4 embedded SVGs every frame
- **Severity:** High | **File:** `src/render/dashed_block.rs:143-153,250-285`
- **Problem:** `Tree::from_data` + `resvg::render` executed repeatedly for static icons.
- **Impact:** High CPU cost per render for static assets.
- **Fix direction:** Pre-render SVGs once (startup/config/size change) into cached RGBA buffers/pixmaps.

#### [render/dashed_block] #563 — Per-pixel SVG blending uses f32 alpha conversion
- **Severity:** High | **File:** `src/render/dashed_block.rs:180-203`
- **Problem:** u8→f32→u8 alpha math per pixel during blit.
- **Impact:** Avoidable CPU overhead.
- **Fix direction:** Use integer alpha blending and batch-friendly loops.

#### [render/date_time_block] #564 — Per-pixel dashed border scans full canvas O(w×h)
- **Severity:** High | **File:** `src/render/date_time_block.rs:160-179`
- **Problem:** Same full-canvas dashed border algorithm as dashed block.
- **Impact:** CPU waste on every render.
- **Fix direction:** Reuse shared edge-based dashed border helper. [cluster with #561].

#### [render/date_time_block] #565 — Calls `time`/`localtime_r`/`strftime` three times per render
- **Severity:** High | **File:** `src/render/date_time_block.rs:36-117`
- **Problem:** Separate time fetch/format calls for time/day-month/year in same frame.
- **Impact:** Redundant syscalls/libc calls per render.
- **Fix direction:** Fetch/convert once per render tick and derive all strings from same `tm`.

#### [render/date_time_block] #566 — Allocates formatted strings every render
- **Severity:** High | **File:** `src/render/date_time_block.rs:54-61,82-89,110-117`
- **Problem:** `strftime` output copied into new `Vec<u8>`/`String` each frame.
- **Impact:** Steady-state heap allocations (violates perf priority).
- **Fix direction:** Cache formatted strings and refresh only when minute/day/year changes; reuse buffers.

#### [render/audio] #567 — Per-pixel `rect()` fill O(w×h) with bounds checks
- **Severity:** High | **File:** `src/render/audio.rs:30-56`
- **Problem:** Pixel-by-pixel fill loop for solid rectangles.
- **Impact:** CPU waste in popup rendering.
- **Fix direction:** Use row fills/memset-like copies for clipped spans.

#### [render/audio] #568 — Audio popup background uses expensive rounded fill/stroke helpers
- **Severity:** High | **File:** `src/render/audio.rs:335-347`
- **Problem:** Relies on `theme::fill_rounded`/`stroke_rounded` (O(w×h) and double checks).
- **Impact:** Large per-render iteration count for popup background.
- **Fix direction:** Optimize theme helpers or cache background layer when size/theme unchanged. [cluster with #559,#560].

#### [render/audio_player] #569 — `rect()` blends per pixel (O(w×h) f32)
- **Severity:** High | **File:** `src/render/audio_player.rs:534-540`
- **Problem:** Solid/alpha rect draws call per-pixel `blend()`.
- **Impact:** CPU-heavy fills with floating-point math.
- **Fix direction:** Fast path for opaque/constant-alpha integer blending and span fills.

#### [render/audio_player] #570 — `line()` uses f32 interpolation instead of integer stepping
- **Severity:** High | **File:** `src/render/audio_player.rs:541-555`
- **Problem:** Per-step float division/multiply/round.
- **Impact:** Avoidable overhead for line drawing.
- **Fix direction:** Use Bresenham/integer DDA for axis-agnostic line rasterization.

#### [render/audio_player] #571 — `rounded()` is O(w×h) with per-pixel f32 blend
- **Severity:** High | **File:** `src/render/audio_player.rs:576-612`
- **Problem:** Full-area scan and blending for rounded rects.
- **Impact:** Very high cost for large backgrounds (e.g. expanded player).
- **Fix direction:** Scanline spans + integer blending; cache static layers where possible.

#### [render/font] #572 — Spawns `fc-match` subprocess per missing ASCII character
- **Severity:** High | **File:** `src/render/font.rs:109-133`
- **Problem:** On-demand subprocess spawn in font fallback path.
- **Impact:** Latency spikes (especially first render/missing glyphs); may block render thread if called there.
- **Fix direction:** Resolve/cache fallback font paths per script/language/charset at startup or on first miss per category, not per character.
- **Notes:** [cluster with #573,#574].

#### [render/font] #573 — Unbounded glyph cache
- **Severity:** High | **File:** `src/render/font.rs:22`
- **Problem:** `HashMap<(char,u32), CachedGlyph>` grows without eviction.
- **Impact:** Unbounded memory growth with diverse text/CJK.
- **Fix direction:** Bounded LRU/size-budget cache (count/bytes) with eviction.

#### [render/font] #574 — Memory-maps fallback font files on first miss
- **Severity:** High | **File:** `src/render/font.rs:149-178`
- **Problem:** Mmaps large fallback font files (e.g. CJK TTC) during miss handling.
- **Impact:** First-use latency spikes/page faults.
- **Fix direction:** Preload/cache fallback font faces/data handles; avoid repeated mmap setup on hot miss paths.

#### [render/font] #575 — Tofu placeholder draws by scanning all glyph pixels
- **Severity:** High | **File:** `src/render/font.rs:273-286`
- **Problem:** O(w×h) loop to set border pixels.
- **Impact:** Minor CPU waste (cached per glyph, but easy fix).
- **Fix direction:** Draw top/bottom rows and left/right columns directly.

#### [render/bluetooth] #576 — Allocates device vectors/strings every render frame
- **Severity:** High | **File:** `src/render/bluetooth.rs:138-178`
- **Problem:** Builds discovered/paired `Vec<DeviceEntry>` and per-device `SharedString`s each frame.
- **Impact:** Steady-state heap allocations/churn.
- **Fix direction:** Cache/reuse view models and update incrementally when snapshot changes.

#### [render/bluetooth] #577 — RGBA→BGRA conversion in per-pixel loop
- **Severity:** High | **File:** `src/render/bluetooth.rs:208-217`
- **Problem:** Scalar per-pixel channel swaps over large popup buffer.
- **Impact:** CPU waste (e.g. ~128k pixels/frame).
- **Fix direction:** Batch conversion with unchecked spans after bounds validation, SIMD, or reusable utility.

#### [state/popup] #578 — `close_application_completely` performs many `/proc` reads and process scans
- **Severity:** High | **File:** `src/state/popup.rs:65-157`
- **Problem:** Reads `/proc/<pid>/stat` per matching window, refreshes all processes, scans all processes again, etc.
- **Impact:** I/O/CPU spike on app close action.
- **Fix direction:** Batch/limit lookups, reuse existing process info, and avoid full system rescans where possible.
- **Notes:** [cluster with #580].

#### [state/popup] #579 — Spawns `steam -shutdown` subprocess
- **Severity:** High | **File:** `src/state/popup.rs:114`
- **Problem:** Launches external command synchronously in flow context.
- **Impact:** Latency and external dependency behavior on close.
- **Fix direction:** Prefer D-Bus/IPC integration if available, or make command non-blocking and guarded.

#### [state/popup] #580 — Full sysinfo process refresh on app close
- **Severity:** High | **File:** `src/state/popup.rs:127-128`
- **Problem:** Refreshes all processes after close.
- **Impact:** Unnecessary system-wide scan/CPU work.
- **Fix direction:** Refresh only affected PIDs/state or defer incremental update. [cluster with #578].

### Medium

#### [render/badges] #581 — Embedded decoded badge icon retained forever
- **Severity:** Medium | **File:** `src/render/badges.rs:9-10`
- **Problem:** `OnceLock<Option<(w,h,Vec<u8>)>>` keeps decoded pixels for process lifetime.
- **Impact:** Minor persistent memory use; inflexible asset handling.
- **Fix direction:** Acceptable if tiny; alternatively draw procedurally or allow shared/static buffer reuse strategy.

#### [render/badges] #582 — Per-pixel badge alpha blending
- **Severity:** Medium | **File:** `src/render/badges.rs:47-81`
- **Problem:** Pixel loop for small badge blits.
- **Impact:** Minor (small area), but can be simplified/optimized if reused.
- **Fix direction:** Keep if bounded; optional micro-optimization only.

#### [render/badges] #583 — Two badge rendering paths (canvas and Cairo)
- **Severity:** Medium | **File:** `src/render/badges.rs:84-135`
- **Problem:** Separate implementations for CPU/Cairo and canvas paths.
- **Impact:** Maintenance burden and divergence risk.
- **Fix direction:** Share badge drawing logic/data model across render backends where feasible.

#### [render/text] #584 — `World` struct and methods are dead code
- **Severity:** Medium | **File:** `src/render/text.rs:55-224`
- **Problem:** Unused text rendering API wrapper (~170 LOC) alongside free `draw_text`.
- **Impact:** Maintenance/cognitive overhead.
- **Fix direction:** Remove unused `World` implementation or consolidate callers onto it.

#### [render/text] #585 — `blit_glyph` bounds-checks every pixel
- **Severity:** Medium | **File:** `src/render/text.rs:164-218`
- **Problem:** Per-pixel target/bounds checks in inner loop.
- **Impact:** Minor overhead, especially for many glyphs.
- **Fix direction:** Precompute clipped source/dest rectangles and blit only valid region.

#### [render/text] #586 — `draw_blue_box` debug helper left in code
- **Severity:** Medium | **File:** `src/render/text.rs:83-107`
- **Problem:** Debug-only drawing method present in production path/codebase.
- **Impact:** Dead/debug noise.
- **Fix direction:** Remove with #584 or gate behind debug tooling.

#### [resolvers/process] #587 — Reads `/proc/<pid>/environ` during process resolution
- **Severity:** Medium | **File:** `src/resolvers/process.rs:41`
- **Problem:** Environment reads can fail with EACCES for other users' processes.
- **Impact:** Permission noise and unnecessary syscalls.
- **Fix direction:** Limit reads to same-UID/relevant processes and handle errors quietly/strategically.

#### [resolvers/process] #588 — `find_process` performs O(n) linear scan per resolve
- **Severity:** Medium | **File:** `src/resolvers/process.rs:81-129`
- **Problem:** Normalizes and compares against all processes without index.
- **Impact:** CPU overhead at scale (many processes/resolutions).
- **Fix direction:** Maintain/update lookup indexes (normalized name/exe -> PID candidates).

#### [render/mod] #589 — Module declaration/organization inconsistency
- **Severity:** Medium | **File:** `src/render/mod.rs:1-35`
- **Problem:** Declared/exported module set appears inconsistent with usage.
- **Impact:** Minor API/organization confusion.
- **Fix direction:** Align module visibility/exports with actual consumers and architecture.

#### [render/utils] #590 — HSL→RGB conversion noted as fine (no issue)
- **Severity:** Medium (informational in source) | **File:** `src/render/utils.rs:7-31`
- **Problem:** No functional/perf issue identified.
- **Impact:** None.
- **Fix direction:** No action required.

#### [render/audio_player/hit-test] #591 — Hardcoded hitbox coordinates ignore scale factor
- **Severity:** Medium | **File:** `src/render/audio_player.rs:36-64`
- **Problem:** Logical pixel ranges used without scale-aware mapping.
- **Impact:** Misaligned hit targets at non-1x DPI/scale.
- **Fix direction:** Use same coordinate space/scale transform as rendering; derive hit regions from layout metrics.

#### [render/audio/hit-test] #592 — Hardcoded hitbox coordinates ignore scale factor
- **Severity:** Medium | **File:** `src/render/audio.rs:9-28`
- **Problem:** Static logical constants for sliders/buttons/selectors.
- **Impact:** Hit-testing bugs at non-1x scale.
- **Fix direction:** Share layout/scale-aware geometry between render and hit-test. [cluster with #591,#593].

#### [render/dashed_block/hit-test] #593 — Bluetooth icon hit-test may diverge from rendered position/config
- **Severity:** Medium | **File:** `src/render/dashed_block.rs:33-49`
- **Problem:** Separate hit-test/render position derivations can drift (config/space mismatch).
- **Impact:** Potential missed or incorrect clicks.
- **Fix direction:** Single source of truth for icon rect/layout used by both render and hit-test.

#### [services/steam] #594 — Walks/parses all Steam `.acf` manifests on fallback
- **Severity:** Medium | **File:** `src/services/monitor.rs:281-330`
- **Problem:** Reads every manifest in steamapps dirs when env lookup fails.
- **Impact:** I/O spikes on game resolution.
- **Fix direction:** Index manifests, cache appid->path/name mappings, and update incrementally.
- **Notes:** [cluster with #551/#552 context if module retained].

#### [services/icons] #595 — Decodes/resizes images on every call without caching
- **Severity:** Medium | **File:** `src/services/monitor.rs:416-442`
- **Problem:** SVG/raster decode and resize repeated per event/request.
- **Impact:** CPU waste and latency.
- **Fix direction:** Cache decoded/resized icon buffers by (path, size/theme params).

---

## Suggested fix batches (for parallelizable work)

- **Batch A (remove/isolate standalone monitor path):** #551, #552, #553, #554, #555, #594, #595
- **Batch B (event/process/procfs efficiency):** #578, #580, #587, #588, #579
- **Batch C (popup render primitives & blending):** #556, #557, #559, #560, #567, #568, #569, #570, #571, #577
- **Batch D (static asset/date-time/dashed rendering):** #561, #562, #563, #564, #565, #566, #581
- **Batch E (font pipeline):** #572, #573, #574, #575
- **Batch F (bluetooth/text/badges cleanup):** #576, #582, #583, #584, #585, #586, #558
- **Batch G (hit-testing & module hygiene):** #591, #592, #593, #589, #590

## Quick validation checklist per batch

- **Monitor/event path:** no `WalkDir`/full `.desktop` scans on window events, no full process refresh per toplevel done event, Steam/env/proc reads bounded and cached, icon decode/resize cached.
- **Close/app actions:** `close_application_completely` avoids repeated system-wide scans; external commands are safe/non-blocking as designed.
- **Render hot paths:** static SVGs pre-rendered/cached, date-time strings cached by change granularity, no per-frame avoidable allocations, rounded/dashed/circle/line drawing uses efficient algorithms, integer blending where appropriate.
- **Font:** no per-character subprocess spawning, bounded glyph cache with eviction, reduced first-miss latency, fallback behavior correct.
- **Hit-testing:** audio/audio_player/dashed-block controls align with rendered geometry at 1x and fractional/high DPI scales.
- **Cleanup:** dead/debug code removed, theme color definitions single-source, badge/text rendering paths consolidated or clearly separated.
- **General:** `cargo check` passes; relevant tests pass; manual Wayland checks for popups, icons, fonts, hover/click targets, and app/window lifecycle show no regressions and reduced CPU/I/O churn.
