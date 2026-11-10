# GPU Rendering Review Round 17 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-17.md` (issues #596–#630)  
**Scope reviewed:** `wgpu-api/src/physics/interaction.rs`, `wgpu-api/src/physics/groups.rs`, `src/interaction/click.rs`, `src/interaction/drag.rs`, `src/interaction/scroll.rs`, `src/interaction/timers.rs`  
**Totals (from source issue list):** 35 issues — 5 Critical, 16 High, 14 Medium, 0 Low  
**Cumulative (as reported in source):** 630 issues across rounds 1–17 (source notes cumulative deltas; use issue-level counts above for this round)

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** to fix interaction correctness and hot-path behavior first.
- Items marked **[cluster]** should be fixed together to avoid partial refactors or inconsistent behavior.
- Preserve project invariants: physics is pure/fixed-timestep and must not depend on wgpu; no per-event avoidable allocations in hot paths; event-driven behavior; config-driven paths/values (no user-specific hardcodes); bounded/sane complexity in input handlers.
- For each fix, run relevant checks: `cargo check`, `cargo check -p wgpu-api`, targeted `cargo test` (physics/interaction where available), and manual Wayland verification for drag/release, dock icon rearrange, click routing (dock/popups/context menu/window list), and scroll behavior.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Physics interaction correctness & API design | 596, 601, 602, 603, 621, 622, 623 | Incorrect drag feel/launch, global side effects, fragile drag state, jittery inserts |
| Physics/group scalability & data model | 597, 598, 604, 605, 606, 607, 608, 609, 624, 625, 626, 627 | O(n²)/mutex-heavy ops, string-tag allocations, linear lookups, dead/incomplete behavior |
| Click/input handler complexity & duplication | 599, 610, 611, 612, 613, 614, 615, 616, 628, 629 | Unmaintainable monolith, repeated scans/recomputation per event, coordinate inconsistencies |
| Config/path and runtime hygiene | 600, 617, 618, 619, 620, 630 | Broken installs, disk I/O on every drag, per-event allocations, misplaced/unused state |

## Recommended fix order

1. **#596, #601, #602** — fix drag physics correctness and state isolation (velocity basis, per-body spring params, explicit drag state). [cluster]
2. **#597, #598, #605, #606, #607, #608, #609** — replace string tags/O(n)/O(n²) group operations with typed IDs and indexed structures. [cluster]
3. **#599, #610, #611, #612, #615, #616** — decompose click handler and eliminate repeated popup/surface scans per event. [cluster]
4. **#603, #613, #614, #617, #618** — reduce unnecessary interaction side effects and per-release/per-drag recomputation/I-O.
5. **#600, #619, #620, #628, #629, #630** — config/path/constants/state cleanup for correctness and maintainability.
6. **#604, #621, #622, #623, #624, #625, #626, #627** — API completeness, dead/duplicate code, and physics behavior refinements.

---

## Issues (sorted by severity, then area)

### Critical

#### [wgpu-api/physics/interaction] #596 — `drag_move` derives release velocity from fixed physics timestep
- **Severity:** Critical | **File:** `wgpu-api/src/physics/interaction.rs:270-287`
- **Problem:** Body is teleported to pointer and velocity computed as `delta / dt` using fixed timestep (~1/120s), not wall-clock pointer motion.
- **Impact:** Small pointer moves produce extreme velocities on release (unrealistic/supersonic icon launches).
- **Fix direction:** Track pointer samples with wall-clock timestamps (or smoothed velocity estimator) and set release velocity from real pointer motion; clamp/sanitize velocity.
- **Notes:** Keep physics integration fixed-timestep; separate input velocity estimation from simulation step.

#### [wgpu-api/physics/interaction] #597 — `disperse` is O(n²) with per-pair mutex acquisitions
- **Severity:** Critical | **File:** `wgpu-api/src/physics/interaction.rs:607-638`
- **Problem:** All pairs in region checked; each pair does multiple `get_body_mut` lock/unlock operations.
- **Impact:** Severe CPU/lock contention scaling poorly with group size.
- **Fix direction:** Collect relevant body snapshots/indices once, use spatial hashing/grid or neighbor queries, batch force accumulation, and apply updates with minimal locking.
- **Notes:** [cluster with #621].

#### [wgpu-api/physics/interaction] #598 — `cut`/`merge` use string-based group tagging
- **Severity:** Critical | **File:** `wgpu-api/src/physics/interaction.rs:508-587`
- **Problem:** Group membership encoded in `body.tag` strings (`format!`, string compare, full scans).
- **Impact:** Heap allocations and O(bodies × tag length) operations per group operation.
- **Fix direction:** Use typed `GroupId`/membership field or side tables (`HashMap<BodyId, GroupId>`, per-group member sets) for O(1) checks/updates.
- **Notes:** [cluster with #607,#606].

#### [src/interaction/click] #599 — 1,036-line monolithic click handler
- **Severity:** Critical | **File:** `src/interaction/click.rs` (entire file)
- **Problem:** Single function handles press/release for dock, context menu, window list, multiple popups/bars/widgets with deeply nested logic.
- **Impact:** High maintenance risk, difficult reasoning/testing, easy to introduce routing regressions.
- **Fix direction:** Split into per-surface/per-popup handlers (modules/functions/trait-dispatched handlers) with shared hit-test/routing helpers and clear event flow.
- **Notes:** [cluster with #610-#616,#629].

#### [src/interaction/click/config] #600 — Hardcoded Bluetooth widget binary path
- **Severity:** Critical | **File:** `src/interaction/click.rs:183`
- **Problem:** Uses user-specific dev path (`~/m.2/Rust/.../widget-test`).
- **Impact:** Feature fails on other systems/installations.
- **Fix direction:** Resolve executable from config, `$PATH`, or standard install locations with graceful fallback/error reporting.

### High

#### [wgpu-api/physics/interaction] #601 — Drag hardcodes spring stiffness/damping at world level
- **Severity:** High | **File:** `wgpu-api/src/physics/interaction.rs:255-268`
- **Problem:** `drag`/`drag_release` mutate world-wide spring parameters.
- **Impact:** Dragging one body changes physics behavior for all bodies.
- **Fix direction:** Make drag spring/constraint parameters per-body (or per-interaction override) without global mutation.
- **Notes:** [cluster with #596,#602].

#### [wgpu-api/physics/interaction] #602 — Drag state inferred from magic number (`spring_stiffness > 500.0`)
- **Severity:** High | **File:** `wgpu-api/src/physics/interaction.rs:314-320`
- **Problem:** `is_dragging` detection depends on parameter threshold.
- **Impact:** Fragile/brittle state tracking; breaks if tuning changes.
- **Fix direction:** Track explicit drag state (flag/map keyed by body id/interaction id).

#### [wgpu-api/physics/interaction] #603 — `insert` repels all neighbors within `2× body size`
- **Severity:** High | **File:** `wgpu-api/src/physics/interaction.rs:460-502`
- **Problem:** Broad repel radius affects many bodies in dense layouts.
- **Impact:** Inserting one icon causes global jitter instead of local rearrangement.
- **Fix direction:** Limit to immediate neighbors/insertion gap participants, use directional displacement, or constrain impulse magnitude/radius adaptively.

#### [wgpu-api/physics/groups] #604 — `arrange_horizontal` is a no-op that clears state
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:294-301`
- **Problem:** Clears members/rest positions without rebuilding arrangement.
- **Impact:** Destructive dead behavior; callers lose group state.
- **Fix direction:** Implement arrangement correctly or remove/unexport method until implemented.

#### [wgpu-api/physics/groups] #605 — `remove_member` uses O(n) vector removals
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:133-138`
- **Problem:** Shifts `members` and `rest_positions` independently via `Vec::remove`.
- **Impact:** Linear-time removals and potential index coupling bugs.
- **Fix direction:** Use indexed map/swap-remove with consistent bookkeeping, or store member structs together.

#### [wgpu-api/physics/groups] #606 — `contains`, `rest_position`, `set_rest_position` are O(n)
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:141-163`
- **Problem:** Linear scans for membership/position lookups.
- **Impact:** Avoidable overhead for frequent operations.
- **Fix direction:** Use `HashMap<BodyId, Point>`/index map for O(1) average access.

#### [wgpu-api/physics/groups] #607 — `find_group_for_body` is O(groups × members)
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:393-398`
- **Problem:** Iterates all groups and performs linear contains checks.
- **Impact:** Poor scaling with many groups/members.
- **Fix direction:** Maintain reverse index `BodyId -> GroupId`.

#### [wgpu-api/physics/groups] #608 — `split_at` clones config for both halves
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:258-282`
- **Problem:** Duplicates config data during split.
- **Impact:** Unnecessary allocations/copies.
- **Fix direction:** Share immutable config (`Arc`) or move/borrow as appropriate.

#### [wgpu-api/physics/groups] #609 — `BodyGroup` derives `Clone` with duplicated vectors
- **Severity:** High | **File:** `wgpu-api/src/physics/groups.rs:85`
- **Problem:** Implicit deep copies of member/rest-position collections.
- **Impact:** Hidden allocation/copy costs at call sites.
- **Fix direction:** Remove/limit `Clone`, provide explicit clone/copy method, or use shared structures.

#### [src/interaction/click] #610 — Multiple `any()` dock scans per press event
- **Severity:** High | **File:** `src/interaction/click.rs:76-95`
- **Problem:** Separate iterations to detect popup/surface types.
- **Impact:** Repeated O(docks×popups) checks per press.
- **Fix direction:** Resolve surface target once (single pass/map lookup) and branch on result.

#### [src/interaction/click] #611 — Same duplicated surface checks in Release path
- **Severity:** High | **File:** `src/interaction/click.rs:411-519`
- **Problem:** Mirrors press-side repeated scans.
- **Impact:** Duplicate work per release event.
- **Fix direction:** Share routing/hit-target resolution logic between press and release. [cluster with #610].

#### [src/interaction/click] #612 — Catch-all popup pass performs redundant third scan
- **Severity:** High | **File:** `src/interaction/click.rs:959-1008`
- **Problem:** Final loop rechecks popup surfaces after specific handlers already ran.
- **Impact:** Extra iterations and complex control flow.
- **Fix direction:** Centralize dispatch with definitive handled/not-handled result to avoid fallthrough rescans.

#### [src/interaction/click] #613 — Context menu geometry rebuilt on every release
- **Severity:** High | **File:** `src/interaction/click.rs:687-773`
- **Problem:** Recomputes full popup/menu geometry per release while open.
- **Impact:** Redundant computation on input events.
- **Fix direction:** Cache geometry/layout on open/config/scale change and reuse for hit-testing.

#### [src/interaction/click] #614 — Window list sorting/HashMap rebuild on every release
- **Severity:** High | **File:** `src/interaction/click.rs:777-956`
- **Problem:** Rebuilds title map and re-sorts windows per release event.
- **Impact:** Per-event allocations and string work.
- **Fix direction:** Maintain cached/sorted view updated on window/title changes, not on click release.

#### [src/interaction/click] #615 — `event_on_dock_surface` recomputed in Press and Release
- **Severity:** High | **File:** `src/interaction/click.rs:69-72,674-677`
- **Problem:** Same surface classification repeated.
- **Impact:** Redundant scans.
- **Fix direction:** Compute once per event and pass through handler pipeline.

#### [src/interaction/click] #616 — Panel-toggle close-all logic duplicated 3×
- **Severity:** High | **File:** `src/interaction/click.rs:129-143,209-226,236-253`
- **Problem:** Repeated code to close other panels when opening one.
- **Impact:** Easy to miss cases when adding panels; maintenance burden.
- **Fix direction:** Extract helper (e.g. `close_other_panels(except)`) and centralize panel state transitions.

### Medium

#### [src/interaction/drag] #617 — Linear searches in `pinned_apps`
- **Severity:** Medium | **File:** `src/interaction/drag.rs:50-51`
- **Problem:** Two separate linear string searches.
- **Impact:** Minor per-drag overhead and duplicated lookup.
- **Fix direction:** Use indexed lookup/set or single pass to locate needed entries.

#### [src/interaction/drag] #618 — `save_pinned_apps` called on every drag release
- **Severity:** Medium | **File:** `src/interaction/drag.rs:77,81`
- **Problem:** Persists to disk on each drag release.
- **Impact:** Unnecessary disk I/O and potential wear/latency.
- **Fix direction:** Debounce/coalesce saves or persist only when order/state actually changed.

#### [src/interaction/scroll] #619 — Scroll orientation allocates `String` per event
- **Severity:** Medium | **File:** `src/interaction/scroll.rs:65`
- **Problem:** `orientation.to_string()` heap-allocates on every scroll.
- **Impact:** Avoidable per-event allocation.
- **Fix direction:** Use enum/`&'static str`/`Copy` representation.

#### [src/interaction/timers] #620 — Timer state stored in `menu_state`
- **Severity:** Medium | **File:** `src/interaction/timers.rs:17-121`
- **Problem:** Timers for multiple unrelated subsystems live in menu-specific state container.
- **Impact:** Poor cohesion, confusing ownership/lifecycle.
- **Fix direction:** Move timers into dedicated timer/runtime state module or owning subsystems.

#### [wgpu-api/physics/interaction] #621 — `attract`/`repel` clone body IDs into temporary `Vec`
- **Severity:** Medium | **File:** `wgpu-api/src/physics/interaction.rs:354-382`
- **Problem:** Allocates per call proportional to bodies in region.
- **Impact:** Heap churn in interaction operations.
- **Fix direction:** Iterate over borrowed slices/iterators or reuse scratch buffers.

#### [wgpu-api/physics/interaction] #622 — `concentrate` duplicates `attract` logic
- **Severity:** Medium | **File:** `wgpu-api/src/physics/interaction.rs:593-605`
- **Problem:** Reimplemented behavior instead of reuse.
- **Impact:** Maintenance/divergence risk.
- **Fix direction:** Delegate to shared helper/`attract` with parameters.

#### [wgpu-api/physics/interaction] #623 — `apply_force` naming/signature ambiguity
- **Severity:** Medium | **File:** `wgpu-api/src/physics/interaction.rs:388-397`
- **Problem:** Same method name used for trait/body operations with different meanings/signatures.
- **Impact:** API confusion and misuse risk.
- **Fix direction:** Rename for clarity (`apply_impulse`, `apply_force_to_region`, etc.) and document units/semantics.

#### [wgpu-api/physics/groups] #624 — `GroupBehavior` variants unimplemented
- **Severity:** Medium | **File:** `wgpu-api/src/physics/groups.rs:36-46`
- **Problem:** `Rigid`, `Chain`, `Free` defined but not implemented.
- **Impact:** Dead/incomplete API surface.
- **Fix direction:** Implement supported behaviors or reduce enum to implemented variants and document roadmap.

#### [wgpu-api/physics/groups] #625 — `center_of_mass` performs repeated map lookups
- **Severity:** Medium | **File:** `wgpu-api/src/physics/groups.rs:166-185`
- **Problem:** Per-member HashMap lookups during aggregation.
- **Impact:** Avoidable overhead in group computations.
- **Fix direction:** Iterate stored member data directly or use contiguous structures.

#### [wgpu-api/physics/groups] #626 — `apply_boundary` ignores rotation
- **Severity:** Medium | **File:** `wgpu-api/src/physics/groups.rs:228-255`
- **Problem:** Boundary constraint handled only on independent X/Y extents.
- **Impact:** Incorrect behavior for rotated bodies/groups.
- **Fix direction:** Account for oriented bounds/AABB of rotated shape or document axis-aligned limitation.

#### [wgpu-api/physics/groups] #627 — `merge` ignores overlapping rest positions
- **Severity:** Medium | **File:** `wgpu-api/src/physics/groups.rs:285-291`
- **Problem:** When body exists in both groups, incoming/target rest position handling drops data silently.
- **Impact:** Silent state loss/inconsistent merged layout.
- **Fix direction:** Define deterministic merge policy (keep existing, override, or rebase) and apply explicitly.

#### [src/interaction/click] #628 — Button codes duplicated across files
- **Severity:** Medium | **File:** `src/interaction/click.rs:19-21`
- **Problem:** `BTN_LEFT/RIGHT/MIDDLE` constants repeated in multiple modules.
- **Impact:** Drift/maintenance risk.
- **Fix direction:** Centralize input/button constants in shared module.

#### [src/interaction/click] #629 — Inconsistent hit-testing coordinate transforms
- **Severity:** Medium | **File:** `src/interaction/click.rs:687-773,777-956`
- **Problem:** Context menu and window list use different coordinate transform paths.
- **Impact:** Scale/position hit-test bugs and inconsistent behavior.
- **Fix direction:** Use shared coordinate conversion helpers and single source of layout geometry.

#### [src/interaction/scroll] #630 — Unused parameters in scroll handler
- **Severity:** Medium | **File:** `src/interaction/scroll.rs:22-25`
- **Problem:** `_apps_in_dock`, `_running_by_app`, `_scale_factor`, `_layout` unused.
- **Impact:** API noise/inconsistency; may indicate incomplete behavior.
- **Fix direction:** Remove unused params, wire them into behavior, or document why retained.

---

## Suggested fix batches (for parallelizable work)

- **Batch A (drag/physics correctness):** #596, #601, #602, #603
- **Batch B (groups/indexing/perf):** #597, #598, #605, #606, #607, #608, #609, #621, #625
- **Batch C (click handler architecture):** #599, #610, #611, #612, #613, #614, #615, #616, #629
- **Batch D (interaction hygiene/config):** #600, #617, #618, #619, #620, #628, #630
- **Batch E (group API completeness):** #604, #622, #623, #624, #626, #627

## Quick validation checklist per batch

- **Drag/physics:** drag start/move/release feels stable (no supersonic launches), only dragged body affected, explicit drag state tracked, insert affects local neighbors only.
- **Groups/perf:** no string-tag group membership, O(1)/indexed body-group lookups, no O(n²) all-pairs mutex-heavy disperse path for large regions, no avoidable per-op allocations.
- **Click routing:** single-pass surface/target resolution, correct press/release handling for dock/context menu/window list/popups, cached geometry/sorted lists reused across events, consistent hit-testing coordinates/scale.
- **Interaction hygiene:** Bluetooth/widget paths config-driven, no disk writes on every drag release, no per-scroll string allocations, timer/input constants/state placed coherently.
- **Group API:** no destructive no-op methods, merge/split semantics deterministic, boundary/behavior behavior documented or implemented.
- **General:** `cargo check` and `cargo check -p wgpu-api` pass; relevant tests pass; manual Wayland interaction checks show correct behavior with no new hot-path allocations.
