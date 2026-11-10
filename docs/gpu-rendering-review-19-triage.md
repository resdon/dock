# GPU Rendering Review Round 19 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-19.md` (issues #671–#710)  
**Scope reviewed:** `src/error.rs`, `src/errors/*`, `src/geometry/*`, `src/models.rs`, `src/models/system.rs`, `src/cache/*`, `src/resolvers/*` (icon_list, icon_stage, steam, window, capabilities, distribution, environment)  
**Totals:** 40 issues — 5 Critical, 16 High, 26 Medium, 0 Low  
**Cumulative (Rounds 1–19):** 710 issues — 90 Critical, 176 High, 265 Medium, 78 Low

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** (perf/scale + correctness first).
- Items marked **[cluster]** should be fixed together to avoid partial/inconsistent behavior.
- Add tests/guards when changing error semantics, cache lifecycle, or process traversal.
- Respect project priorities: event-driven, lowest CPU/RAM/I/O, no steady-state allocations, avoid spawn storms, explicit error handling where recoverability matters.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Cache lifecycle & persistence | 671, 685, 693, 694 | Cache deleted every launch (counterproductive); silent save/read/write failures; large-file memory reads |
| Process tree traversal / Steam/Gamescope | 672, 673, 706 | O(all processes) + `/proc` reads/EACCES, fragile .acf parsing, startup/event I/O spikes |
| Icon discovery/indexing duplication & perf | 674, 675, 686, 687, 705, 710 | WalkDir storms, duplicate implementations with different quality, 4 threads per query, per-line allocations, stack overflow on cycles, shard cleanup gaps |
| Model bloat & redundancy | 676, 677, 678, 679, 680, 681, 695 | Excess memory/clone churn, sync risk between booleans and enum statuses |
| Geometry/hit-test & UX defaults | 682, 683, 684 | Precision loss, inflexible hardcodes, developer-specific defaults |
| Error taxonomy/recoverability | 688, 689, 690, 701, 702, 708 | Misclassified recoverability, inconsistent strategies, lost error context |
| Resolver correctness & allocations | 696, 697, 698, 699, 700, 707 | O(n)/allocs on hot paths, broad matches/misclassification, ordering bugs |
| Minor safety/typing | 691, 692, 703, 704, 709 | unsafe, dead code, ambiguous APIs, type-safety gaps |

## Recommended fix order

1. **#671** — stop deleting the icon cache on every launch (gate by version/mismatch or remove). [cluster]
2. **#675, #686, #687** — consolidate duplicate indexers, replace per-query thread spawning with indexed lookup, and fix case-insensitive search without per-line allocations.
3. **#672, #673, #674, #706** — constrain process traversal to entry/children, avoid `/proc` scans over all PIDs, replace WalkDir depth-6 with cache/index/linicon, harden .acf parsing.
4. **#685, #693, #691, #705, #710** — fix silent persistence/cache errors, handle `setpriority` return/unsafe carefully, add recursion depth/cycle safety and shard cleanup on failure.
5. **#676, #677, #678, #679, #680, #681, #695** — reduce model bloat/redundant flags (store derived state or compact representations); consider sharing icon pixel buffers.
6. **#688, #689, #690, #701, #702, #708** — align error variants with types and `Recoverable`/strategy semantics.
7. **#682, #683, #684, #696, #697, #698, #699, #700, #703, #704, #707, #709** — geometry precision, config-driven defaults/dimensions, safer matching, ordering/types.

---

## Issues (sorted by severity, then area)

### Critical

#### [cache/lifecycle] #671 — Cache deleted on every launch
- **Severity:** Critical | **File:** `src/cache/mod.rs:16-23`
- **Problem:** `refresh_cache_on_launch()` unconditionally `remove_dir_all`s cached_icons and recreates it.
- **Impact:** Defeats caching; forces full re-resolution/rebuild every startup; extra I/O.
- **Fix direction:** Remove or gate behind cache version mismatch/invalidate flag; only clear when schema/version incompatible.
- **Review notes:** Matches observed behavior in current codebase.

#### [resolvers/steam/process] #672 — Steam resolver iterates all processes
- **Severity:** Critical | **File:** `src/resolvers/steam.rs:55-102`
- **Problem:** Walks every process and parent chains; reads `/proc/*/environ` and `/proc/*/cmdline` for descendants.
- **Impact:** CPU/I/O spike, many EACCES errors.
- **Fix direction:** Limit to entry PID + direct children or tracked PIDs; avoid global scan.
- **Review notes:** [cluster with #673].

#### [resolvers/steam/gamescope] #673 — Gamescope resolver reads /proc for all PIDs
- **Severity:** Critical | **File:** `src/resolvers/steam.rs:104-132`
- **Problem:** Triggered by broad name match, scans all processes reading `/proc/*/environ`.
- **Impact:** Permission noise and I/O spikes.
- **Fix direction:** Restrict to gamescope PID/known processes and require stronger signals.
- **Review notes:** [cluster with #672].

#### [resolvers/steam/icons] #674 — WalkDir depth 6 on hicolor theme
- **Severity:** Critical | **File:** `src/resolvers/steam.rs:242-256`
- **Problem:** Recursive tree traversal for single icon lookup.
- **Impact:** I/O spike per lookup.
- **Fix direction:** Use icon index/cache or linicon-based lookup.

#### [resolvers/cache/indexing] #675 — Duplicate icon indexer implementations
- **Severity:** Critical | **Files:** `src/resolvers/icon_list.rs`, `src/cache/icon_indexer.rs`
- **Problem:** Near-duplicate logic with different behavior (workers, nice priority, cache-age check, rename strategy).
- **Impact:** Maintenance divergence and inconsistent quality.
- **Fix direction:** Consolidate to single implementation (prefer `cache/icon_indexer.rs` with better behavior) and remove duplicate.
- **Review notes:** [cluster with #686,#687].

### High

#### [models/process] #676 — `ProcessRecord` stores full env `HashMap`
- **Severity:** High | **File:** `src/models/system.rs:625`
- **Problem:** Full environment retained per process.
- **Impact:** Memory waste at scale (~750KB for ~300 procs).
- **Fix direction:** Lazy-load on demand or store keys/filtered subset.

#### [models/window] #677 — `WindowRecord` redundant booleans
- **Severity:** High | **File:** `src/models/system.rs:684-688`
- **Problem:** `is_activated/is_minimized/is_foreground/is_background/is_locked` duplicate `status`.
- **Impact:** Sync risk and extra memory.
- **Fix direction:** Derive via methods or remove duplicates.

#### [models/app] #678 — `ApplicationStatus` many booleans
- **Severity:** High | **File:** `src/models/system.rs:443-453`
- **Problem:** 9 boolean flags, partially mutually exclusive/derivable.
- **Impact:** Complexity/inconsistency.
- **Fix direction:** Normalize to enum+flags or fewer canonical fields.

#### [models/process] #679 — `ProcessStatus` 10 boolean fields
- **Severity:** High | **File:** `src/models/system.rs:471-482`
- **Problem:** Similar to #678.
- **Impact:** Cognitive/memory overhead.
- **Fix direction:** Consolidate.

#### [models/tray] #680 — `TrayItem` holds icon pixel buffer
- **Severity:** High | **File:** `src/models.rs:41-61`
- **Problem:** `TrayIcon` with `Vec<u8>` stored per tray item.
- **Impact:** Memory duplication.
- **Fix direction:** Share cached icon data by key/id.

#### [models/media] #681 — `MediaWindowLink` 15 Option fields
- **Severity:** High | **File:** `src/models.rs:326-346`
- **Problem:** Heavy per-window embedded struct with many Optionals.
- **Impact:** Overhead/cloning cost.
- **Fix direction:** Split optional-heavy fields or store as `Option<MediaDetails>`.

#### [geometry/primitives] #682 — `to_local_coords` truncates f64→i32
- **Severity:** High | **File:** `src/geometry/primitives.rs:18-22`
- **Problem:** Truncation + `.max(0.0)` loses sub-pixel precision.
- **Impact:** Hit-test inaccuracy at fractional scales.
- **Fix direction:** Return f64/local float coords or round explicitly; separate clamping.

#### [geometry/popup] #683 — Hardcoded popup dimensions
- **Severity:** High | **File:** `src/geometry/popup.rs:26-30`
- **Problem:** Legacy constants with no config usage.
- **Impact:** Inflexible layout.
- **Fix direction:** Drive from config with sane defaults.

#### [cache/persistence] #684 — Hardcoded default pinned apps
- **Severity:** High | **File:** `src/cache/persistence.rs:30-35`
- **Problem:** Developer-specific defaults on missing pins file.
- **Impact:** Poor first-run UX.
- **Fix direction:** Empty list or configurable defaults.

#### [cache/persistence] #685 — `save_pinned_apps` ignores errors
- **Severity:** High | **File:** `src/cache/persistence.rs:38-46`
- **Problem:** `let _ =` on create_dir/write; empty else branch.
- **Impact:** Silent data loss.
- **Fix direction:** Log/report errors and consider atomic write/rollback.

#### [resolvers/icon_list] #686 — 4 threads per icon query
- **Severity:** High | **File:** `src/resolvers/icon_list.rs:166-269`
- **Problem:** Thread-per-shard per query; many file opens.
- **Impact:** Overhead/latency.
- **Fix direction:** Single-pass search, memory-mapped index, or prebuilt HashMap index.
- **Review notes:** [cluster with #675,#687].

#### [resolvers/icon_list] #687 — `to_lowercase()` per line
- **Severity:** High | **File:** `src/resolvers/icon_list.rs:144,230`
- **Problem:** Allocates per line on every search.
- **Impact:** Heap churn.
- **Fix direction:** Store normalized form or case-insensitive compare without allocation.
- **Review notes:** [cluster with #686].

### Medium

#### [errors] #688 — `AppError` uses `String` variants
- **Severity:** Medium | **File:** `src/error.rs:42-81`
- **Problem:** Lost source/error types/context.
- **Impact:** Harder programmatic handling/diagnostics.
- **Fix direction:** Prefer typed causes/`#[source]`, structured error kinds.

#### [errors/mod] #689 — `Recoverable` inconsistent with `recovery_strategy`
- **Severity:** Medium | **File:** `src/errors/mod.rs:55-76`
- **Problem:** DbusError has strategy but `is_recoverable` false (and coverage gaps).
- **Impact:** Inconsistent recovery logic.
- **Fix direction:** Align predicates and strategies.

#### [errors/gpu] #690 — `FrameTimeout` marked non-recoverable despite recovery strategy
- **Severity:** Medium | **File:** `src/errors/gpu.rs:48-55`
- **Problem:** Contradictory classification.
- **Impact:** Recovery logic may skip valid recovery.
- **Fix direction:** Mark recoverable or adjust strategy consistently.

#### [cache/icon_indexer] #691 — `unsafe libc::setpriority` without return check
- **Severity:** Medium | **File:** `src/cache/icon_indexer.rs:23-25,83-85`
- **Problem:** Silent failure on priority drop.
- **Impact:** No effect, unclear state.
- **Fix direction:** Check return and log on failure.

#### [cache/icon_indexer] #692 — Unused `_start` timer
- **Severity:** Medium | **File:** `src/cache/icon_indexer.rs:28`
- **Problem:** Dead code.
- **Impact:** Minor clutter.
- **Fix direction:** Remove or use for metrics.

#### [cache/mod] #693 — `save_cached_icon` ignores write errors
- **Severity:** Medium | **File:** `src/cache/mod.rs:41-50`
- **Problem:** Silent failures on disk full/perm issues.
- **Impact:** Silent cache corruption/incompleteness.
- **Fix direction:** Log and consider partial-write safety/atomic rename.

#### [cache/mod] #694 — `load_cached_icon` reads entire file
- **Severity:** Medium | **File:** `src/cache/mod.rs:85`
- **Problem:** `read_to_end` for large icons.
- **Impact:** Memory proportional to icon size.
- **Fix direction:** Size-validate first and use bounded reads/mmap carefully or stream.

#### [models/system] #695 — `ApplicationRecord` 17 fields
- **Severity:** Medium | **File:** `src/models/system.rs:723-739`
- **Problem:** Large nested struct, frequent clones.
- **Impact:** Complexity/memory/clone cost.
- **Fix direction:** Group related fields, reduce clones via references/arcs.

#### [models/system] #696 — `ResolutionTrace::add` O(n) `contains`
- **Severity:** Medium | **File:** `src/models/system.rs:295`
- **Problem:** Linear scan for deduplication.
- **Impact:** Minor overhead (small n).
- **Fix direction:** Use small set/bitmask or `HashSet`.

#### [resolvers/capabilities] #697 — `can_open_settings` substring + per-action allocation
- **Severity:** Medium | **File:** `src/resolvers/capabilities.rs:33-36`
- **Problem:** Lowercases each action and broad `contains`.
- **Impact:** Allocations and false matches.
- **Fix direction:** Case-insensitive match without allocation or exact/known action IDs.

#### [resolvers/distribution] #698 — Path matching with `contains`
- **Severity:** Medium | **File:** `src/resolvers/distribution.rs:51-90`
- **Problem:** Broad substring match on lowercased path.
- **Impact:** False positives.
- **Fix direction:** Match path components/executable basename or known prefixes.

#### [resolvers/environment] #699 — Defaults to `SystemPackage` when unknown
- **Severity:** Medium | **File:** `src/resolvers/environment.rs:94`
- **Problem:** Misclassifies source/manual builds.
- **Impact:** Incorrect packaging metadata.
- **Fix direction:** Default to `Unknown` when undetected.

#### [resolvers/icon_stage] #700 — Clones identifier
- **Severity:** Medium | **File:** `src/resolvers/icon_stage.rs:18`
- **Problem:** Unnecessary allocation per resolution.
- **Impact:** Minor churn.
- **Fix direction:** Borrow/clone only when needed.

#### [errors/wayland] #701 — `EventQueueBlocked` non-recoverable
- **Severity:** Medium | **File:** `src/errors/wayland.rs:44-49`
- **Problem:** Contradicts stated recovery strategy.
- **Impact:** Recovery logic may skip handling.
- **Fix direction:** Align recoverability.

#### [errors/pipewire] #702 — `StreamUnderrun` recovery strategy invalid for zero-idle dock
- **Severity:** Medium | **File:** `src/errors/pipewire.rs:69`
- **Problem:** "Reduce frame rate" not applicable.
- **Impact:** Misleading guidance.
- **Fix direction:** Recommend buffer/queue/throttling strategies consistent with event-driven rendering.

#### [geometry/popup] #703 — `PopupType::Custom` undocumented coordinate space
- **Severity:** Medium | **File:** `src/geometry/popup.rs:9`
- **Problem:** Ambiguous units/semantics vs other popup types.
- **Impact:** API confusion.
- **Fix direction:** Document logical pixels or use typed units.

#### [models] #704 — `TrayActionKind::Scroll` uses `&'static str` for orientation
- **Severity:** Medium | **File:** `src/models.rs:85`
- **Problem:** Type-unsafe string; allows arbitrary values.
- **Impact:** Validation burden.
- **Fix direction:** Replace with enum (`Horizontal`, `Vertical`).
- **Review notes:** I recently changed this to `&'static str` to avoid allocations (#619) — better than `String` but still not type-safe; enum is preferred if serialization/bindings allow.

#### [cache/icon_indexer] #705 — Recursive scan can stack overflow
- **Severity:** Medium | **File:** `src/cache/icon_indexer.rs:131-147`
- **Problem:** No depth limit/cycle detection (symlinks).
- **Impact:** Crash risk.
- **Fix direction:** Iterative DFS with `follow_links(false)`/visited set and depth cap.

#### [resolvers/steam] #706 — Fragile .acf parsing
- **Severity:** Medium | **File:** `src/resolvers/steam.rs:134-187`
- **Problem:** `split('"').nth(3)` assumptions break on malformed input.
- **Impact:** Incorrect resolution.
- **Fix direction:** Robust tokenizer/regex or proper key-value parser.

#### [resolvers/window] #707 — `order_index` from `protocol_id`
- **Severity:** Medium | **File:** `src/resolvers/window.rs:41`
- **Problem:** Protocol IDs not equivalent to z/order/creation order.
- **Impact:** Incorrect window ordering.
- **Fix direction:** Use compositor-provided ordering or track activation/stack order.

#### [errors/memory] #708 — `LeakDetected` marked non-recoverable
- **Severity:** Medium | **File:** `src/errors/memory.rs:43-48`
- **Problem:** Inconsistent with "investigate and fix" strategy.
- **Impact:** Recovery path skipped.
- **Fix direction:** Align classification.

#### [models/system] #709 — `OutputRecord.scale_factor` defaults to 1.0
- **Severity:** Medium | **File:** `src/models/system.rs:795`
- **Problem:** May be wrong on HiDPI before update.
- **Impact:** Visual sizing glitches on first render.
- **Fix direction:** Require/update from compositor or use safe fallback with warning.

#### [resolvers/icon_list] #710 — Orphan shard cleanup on panic/failure
- **Severity:** Medium | **File:** `src/resolvers/icon_list.rs:60-92`
- **Problem:** Worker panic leaves `.part` shards; merge not guarded.
- **Impact:** Disk clutter.
- **Fix direction:** Track shard files, clean up on error/panic, use temp+atomic rename.

---

## Suggested fix batches (for parallelizable work)

- **Batch A (cache lifecycle):** #671, #685, #693, #694
- **Batch B (icon indexing/perf):** #675, #686, #687, #705, #710
- **Batch C (process/Steam/Gamescope):** #672, #673, #674, #706
- **Batch D (models consolidation):** #676, #677, #678, #679, #680, #681, #695
- **Batch E (error semantics):** #688, #689, #690, #701, #702, #708
- **Batch F (resolvers/geometry/UX):** #682, #683, #684, #696, #697, #698, #699, #700, #703, #704, #707, #709
- **Batch G (minor safety):** #691, #692

## Quick validation checklist per batch

- **Cache:** No cache deletion on normal startup; versioned migration/invalidates cache correctly; `save_pinned_apps`/`save_cached_icon` report/log failures; cached icon reads validate headers/size.
- **Icon indexing:** Single indexer implementation used, no per-query thread storms, case-insensitive search allocation-free or bounded, recursive scan safe against symlink cycles/depth.
- **Steam/process:** Resolution limited to entry/children, no global `/proc` scans over all PIDs, Gamescope detection uses tighter scope, .acf parsing robust to formatting.
- **Models:** Redundant booleans removed/derived, shared icon buffers where possible, hot structs cloned less or represented compactly.
- **Errors:** `is_recoverable` and `recovery_strategy` consistent across error types, error payloads preserve context where feasible.
- **Resolvers/UX:** Hit-testing precision correct at fractional scales, popup dimensions config-driven, defaults not developer-specific, window ordering correct, packaging classification accurate.
- **General:** `cargo check` passes; targeted tests for cache/resolvers/geometry pass; no new hot-path allocations/spawn storms; silent failures replaced with explicit handling/logging where appropriate.
