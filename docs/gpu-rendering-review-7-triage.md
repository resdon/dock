# GPU Rendering Review Round 7 — Triage for AI Agents

**Source:** `docs/gpu-rendering-review-7.md` (issues #156–#199)  
**Scope reviewed:** `fontdb.rs`, `glyph.rs`, `math_utils.rs`, `monitor.rs`, `physics/commands.rs`, `physics/constraints.rs`, `physics/dock.rs`, `physics/fields.rs`, `physics/forces.rs`, `physics/groups.rs`, `physics/interaction.rs`, `physics/solver.rs`, `physics/spatial_index.rs`, `physics/visual_effects.rs`, `shaping.rs`  
**Totals:** 44 issues — 5 Critical, 12 High, 16 Medium, 11 Low

## How to use this file

- Work top-to-bottom by **Recommended Fix Order** to address blockers and highest ROI first.
- Items marked **[cluster]** should be fixed together in the same change where practical.
- Verify each fix with targeted checks: `cargo check -p wgpu-api` / `cargo test -p wgpu-api <module>` and visual inspection where rendering/text/physics behavior is affected.
- Respect project invariants from `AGENTS.md`: physics is pure Rust and fixed-timestep, renderer owns GPU resources, no per-frame avoidable allocations in hot paths, no coordinate-space mixing without explicit conversion.

## Executive summary

| Theme | Issues | Risk |
|---|---:|---|
| Font/text pipeline broken or inefficient | 156, 157, 161, 172, 173, 174, 187, 189, 196 | Text rendering/fallback incorrect; high RAM use |
| Glyph atlas correctness/lifetime | 158, 162, 175, 176, 190 | Silent text corruption, VRAM growth, stale texture refs |
| Math/transform correctness | 163, 164, 177, 178, 191 | Incorrect scale/rotation/projection behavior, NaN risk |
| Physics solver hot-path allocations | 159, 160 | Major CPU/RAM churn every frame (Priority #1/#2 violation) |
| Dock layout/interaction correctness | 165, 166, 167, 184, 192, 199 | Incorrect animation/reordering/interaction at runtime |
| Force/field correctness & perf | 168, 169, 170, 182, 193, 197 | Wrong forces, wasted compute, NaN/div-by-zero risk |
| Constraints/groups/indexing gaps | 171, 180, 181, 183, 188, 194, 195, 198 | Unstable constraints, O(n²)/O(n) operations, unused index |
| Misc correctness/maintainability | 179, 185, 186 | O(n) history ops, runtime panic risk, fragile APIs |

## Recommended fix order

1. **#158 Glyph atlas expansion corrupts cache** — prevents silent text corruption after growth.
2. **#156, #157 Font resolution/parsing** — unblock correct font identity and glyph fallback. [cluster]
3. **#159, #160 Solver cloning** — remove dominant per-frame allocations in physics hot path. [cluster]
4. **#163, #164 Math transform formulas** — fix foundational geometry/math used broadly.
5. **#165, #166, #167 Dock spring/layout/reorder** — restore correct dock behavior.
6. **#168, #169, #170, #182, #197 Force/field fixes** — correctness + perf in force application.
7. **#161, #162, #172, #176 Text/glyph resource management** — memory/VRAM budgets and cache lifecycle.
8. **#171, #188, #180, #181, #183, #195 Constraints/index/groups** — stability and scalability improvements.
9. **Remaining Medium/Low** — hardening, validation, edge cases, API clarity.

---

## Issues (sorted by severity, then subsystem)

### Critical

#### [font] #156 — `fontdb` creates empty database then queries it
- **Severity:** Critical | **File:** `wgpu-api/src/fontdb.rs:211–251`
- **Problem:** `load_font_from_data` creates an empty `fontdb::Database` and queries it without loading the provided font data; query returns `None`, falling back to `"Unknown"` / Normal / Normal.
- **Impact:** All loaded fonts misidentified; font matching/fallback broken.
- **Fix direction:** Parse/load `data` into the database (or parse metadata directly with `read-fonts`/equivalent) before querying; derive family/weight/style from actual font.
- **Notes:** Coordinate with #157 and #161 for a coherent font pipeline.

#### [font] #157 — `resolve_for_char` ignores the character parameter
- **Severity:** Critical | **File:** `wgpu-api/src/fontdb.rs:467–488`
- **Problem:** `_ch` unused; resolver returns default-family match without checking glyph/Unicode coverage.
- **Impact:** Missing-glyph/tofu for CJK/emoji/symbols; fallback chain not driven by character coverage.
- **Fix direction:** Check codepoint coverage (e.g. via `swash`/`read-fonts` cmap) and iterate fallback candidates until one supports `ch`.
- **Notes:** [cluster with #156].

#### [glyph] #158 — Atlas expand invalidates all cached `GlyphInfo` entries
- **Severity:** Critical | **File:** `wgpu-api/src/glyph.rs:467–490`
- **Problem:** On expand, `current_texture` changes and cursor/size reset but cache entries keep old `TextureId`/UVs; no retention/remap of old atlas data.
- **Impact:** After expansion, cached glyphs render from wrong texture/region → corrupted or missing text.
- **Fix direction:** Either (a) clear/invalidate glyph cache and force re-rasterization/upload to new atlas, or (b) implement multi-atlas retention with stable region references; ensure GPU texture lifecycle matches CPU state.
- **Notes:** High user-visible correctness bug; fix before tuning eviction (#162).

#### [physics/solver] #159 — `solve()` clones all bodies every iteration
- **Severity:** Critical | **File:** `wgpu-api/src/physics/solver.rs:92–94`
- **Problem:** `bodies.iter().cloned().collect()` runs each solver iteration (default 8/frame) to avoid borrow conflicts.
- **Impact:** Large per-frame heap allocations/CPU churn; violates perf/memory priorities.
- **Fix direction:** Refactor to index-based access and/or separate read-only broad-phase data (AABBs/ids) from mutable resolution; avoid full-body clones.
- **Notes:** [cluster with #160].

#### [physics/solver] #160 — `solve()` clones bodies twice per collision pair
- **Severity:** Critical | **File:** `wgpu-api/src/physics/solver.rs:100–102`
- **Problem:** After cloning into `bodies_clone`, each pair clones `a` and `b` again.
- **Impact:** Compounds #159 allocation pressure.
- **Fix direction:** Borrow/refer to pair elements by index; write results back through indices.
- **Notes:** [cluster with #159].

### High

#### [font] #161 — `discover_system_fonts` loads every font into memory
- **Severity:** High | **File:** `wgpu-api/src/fontdb.rs:279–304`
- **Problem:** Recursively scans system dirs and eagerly reads all font files into `FontEntry.data`.
- **Impact:** 100s of MB RAM on typical systems; violates memory priority.
- **Fix direction:** Discover/metadata-index paths lazily; load font bytes only when selected/needed (with caching and eviction if required).
- **Notes:** May require API/lifecycle changes in font entry storage.

#### [glyph] #162 — `GlyphCache` never evicts entries
- **Severity:** High | **File:** `wgpu-api/src/glyph.rs:644–650`
- **Problem:** VRAM budget overrun only logs a warning; allocation continues.
- **Impact:** Unbounded VRAM growth and log spam.
- **Fix direction:** Implement LRU/size-bounded eviction (and atlas/texture cleanup strategy) when budget exceeded; make behavior deterministic.
- **Notes:** Depends on atlas/texture management decisions from #158/#176.

#### [math] #163 — `extract_scale()` uses incorrect formula
- **Severity:** High | **File:** `wgpu-api/src/math_utils.rs:300–305`
- **Problem:** Uses sum of absolute axis components instead of axis vector lengths (Euclidean norm).
- **Impact:** Scale overestimated for rotated transforms (e.g. 45° scale 1 becomes ~1.414), breaking decomposition/animation consumers.
- **Fix direction:** Use `x_axis.truncate().length()` / `y_axis.truncate().length()` (or equivalent) per 2D affine convention.
- **Notes:** Add/update unit tests for rotated scales.

#### [math] #164 — `Vec2Ext::rotate()` rotates clockwise
- **Severity:** High | **File:** `wgpu-api/src/math_utils.rs:209–213`
- **Problem:** Rotation matrix/sign convention produces clockwise rotation; contradicts standard counterclockwise math convention and can surprise callers.
- **Impact:** Inverted rotation behavior in animation/physics/math consumers.
- **Fix direction:** Correct matrix to standard CCW convention and update dependent tests/callers that relied on old behavior.
- **Notes:** Audit call sites for expectations before changing.

#### [physics/dock] #165 — `apply_springs()` uses hardcoded timestep
- **Severity:** High | **File:** `wgpu-api/src/physics/dock.rs:485–494`
- **Problem:** Hardcodes `dt = 0.016` despite engine fixed timestep (120 Hz).
- **Impact:** Spring speed inconsistent with frame/update rate and physics config.
- **Fix direction:** Accept/pass `dt` (or use engine fixed timestep constant) as the default path; deprecate hardcoded variant if needed.
- **Notes:** Align with `apply_springs_dt` if it exists.

#### [physics/dock] #166 — `reorder_slots()` assumes uniform item size
- **Severity:** High | **File:** `wgpu-api/src/physics/dock.rs:450–458`
- **Problem:** Uses first item's size for all target positions.
- **Impact:** Variable-sized icons overlap or gap incorrectly.
- **Fix direction:** Compute cumulative targets from each item's actual size (and spacing/padding) in slot order.
- **Notes:** [cluster with #167].

#### [physics/dock] #167 — `reorder_by_pointer()` logic incorrect
- **Severity:** High | **File:** `wgpu-api/src/physics/dock.rs:400–447`
- **Problem:** Triggers only near targets (`spacing/2`), clamps insertion preventing end placement, and uses flawed `min` tracking.
- **Impact:** Unreliable drag-reorder, especially at ends or mid-animation.
- **Fix direction:** Derive insertion index from pointer 1D position relative to item centers/current positions; allow insertion at `0..=len-1/end` as appropriate.
- **Notes:** [cluster with #166]; validate with animated positions.

#### [physics/fields] #168 — `SeparationField::apply_to_group` is O(n²) with duplicate pair checks
- **Severity:** High | **File:** `wgpu-api/src/physics/fields.rs:475–511`
- **Problem:** Checks both (i,j) and (j,i).
- **Impact:** Double separation work per step.
- **Fix direction:** Iterate `j in (i+1)..len` and apply symmetric/opposite forces once per pair.
- **Notes:** Ensure force accumulation/sign remains correct.

#### [physics/fields] #169 — `ForceField::compute_force` double-computes each source
- **Severity:** High | **File:** `wgpu-api/src/physics/fields.rs:696–702`
- **Problem:** Calls `source.compute_force(body)` separately for `.x` and `.y`.
- **Impact:** Wasted CPU per source/body/frame.
- **Fix direction:** Compute once into a local vector and accumulate both components.
- **Notes:** Trivial perf fix.

#### [physics/fields] #170 — `center_of_mass` fallback returns origin for zero-mass bodies
- **Severity:** High | **File:** `wgpu-api/src/physics/fields.rs:357–378`
- **Problem:** Fallback divides unaccumulated `cx,cy` by count when all masses are zero, yielding origin instead of geometric center.
- **Impact:** Static-body groups pulled toward (0,0).
- **Fix direction:** Compute geometric mean of positions in fallback (sum positions / n), independent of mass accumulation path.
- **Notes:** Define behavior for empty slice.

#### [physics/constraints] #171 — `HingeJoint` doesn't constrain rotation/velocity drift
- **Severity:** High | **File:** `wgpu-api/src/physics/constraints.rs:76–130`
- **Problem:** Position-only anchor correction; no velocity/angular correction (and no angle limits).
- **Impact:** Drift/instability over time.
- **Fix direction:** Add velocity-level correction/impulse or Baumgarte stabilization as appropriate; consider optional angle limits.
- **Notes:** Keep within project physics architecture (pure/fixed-step).

#### [text/shaping] #172 — `ShapedText::new` allocates `HashSet` then `Vec`
- **Severity:** High | **File:** `wgpu-api/src/shaping.rs:53–58`
- **Problem:** Dedup via HashSet converted to Vec causes extra allocations/iteration.
- **Impact:** Unnecessary churn on shaping path.
- **Fix direction:** Use small-vector/linear dedup for few fonts, or sort+dedup a Vec; avoid intermediate HashSet when font count is small.
- **Notes:** Low-risk perf cleanup.

### Medium

#### [font] #173 — `FontDatabase::new()` reads `HOME` twice
- **Severity:** Medium | **File:** `wgpu-api/src/fontdb.rs:175–178`
- **Problem:** Repeated `var_os("HOME")` lookup despite existing `home_path`.
- **Impact:** Minor redundant work.
- **Fix direction:** Reuse `home_path` for `.fonts`.

#### [font] #174 — `match_score` weight division truncates
- **Severity:** Medium | **File:** `wgpu-api/src/fontdb.rs:124–125`
- **Problem:** Integer `/ 100` creates step/discontinuity in score.
- **Impact:** Near-match ranking can be unintuitive.
- **Fix direction:** Preserve finer granularity (float or scaled integer) in scoring.

#### [glyph] #175 — `TextureRegion` UV calculation may cause bleeding/off-by-one
- **Severity:** Medium | **File:** `wgpu-api/src/glyph.rs:151–158`
- **Problem:** UV max at full extent; texel sampling can bleed without careful center/clamp handling.
- **Impact:** Possible glyph edge artifacts.
- **Fix direction:** Use texel-center aware UVs and/or half-texel inset consistent with sampler/filter/padding strategy.
- **Notes:** Validate against existing 1px padding.

#### [glyph] #176 — `GlyphAtlas::clear()` doesn't reset texture ID/lifecycle state
- **Severity:** Medium | **File:** `wgpu-api/src/glyph.rs:547–553`
- **Problem:** Cursor/stats reset but `current_texture` (and GPU-side state assumptions) not reset.
- **Impact:** CPU/GPU atlas state can desync after clear/expand.
- **Fix direction:** Reset/recreate texture identity and ensure GPU resources are released/rebound appropriately via renderer-owned APIs.
- **Notes:** [cluster with #158/#162 design].

#### [math] #177 — `orthographic_topleft()` / transform y-axis convention inconsistency
- **Severity:** Medium | **File:** `wgpu-api/src/math_utils.rs:369–384` (and related transforms)
- **Problem:** Potential mismatch between projection and 2D transform sign conventions.
- **Impact:** Inverted/misaligned rendering when mixed.
- **Fix direction:** Document canonical coordinate convention and make all matrix helpers consistent; add tests.

#### [math] #178 — `hash_f32` biases distribution via `abs()`
- **Severity:** Medium | **File:** `wgpu-api/src/math_utils.rs:517–528`
- **Problem:** Folding via `.abs()` skews values toward 0.
- **Impact:** Non-uniform procedural/noise behavior.
- **Fix direction:** Normalize to desired range without bias (e.g. map unsigned bits to `[0,1)` directly).

#### [physics/commands] #179 — `InteractionManager` `command_history` `remove(0)` is O(n)
- **Severity:** Medium | **File:** `wgpu-api/src/physics/commands.rs:1013–1016`
- **Problem:** Vec front-removal shifts elements on every bounded insert.
- **Impact:** Avoidable CPU overhead at high command rates.
- **Fix direction:** Use `VecDeque` or ring buffer.

#### [physics/spatial_index] #180 — `GridCell::remove()` is O(n)
- **Severity:** Medium | **File:** `wgpu-api/src/physics/spatial_index.rs:33–35`
- **Problem:** `retain` scans full cell.
- **Impact:** Slow removals in dense cells.
- **Fix direction:** Swap-remove with index tracking if order irrelevant.

#### [physics/spatial_index] #181 — `SpatialHash` requires full rebuild on movement
- **Severity:** Medium | **File:** `wgpu-api/src/physics/spatial_index.rs:106–200`
- **Problem:** No incremental update; clear+reinsert pattern.
- **Impact:** Repeated full rehashing at 120 Hz.
- **Fix direction:** Track previous cells per body and update only moved bodies; or skip rebuild when unchanged.
- **Notes:** Pairs with #195 and broad-phase integration.

#### [physics/forces] #182 — `SpringForce` damping coupled to stiffness
- **Severity:** Medium | **File:** `wgpu-api/src/physics/forces.rs:57–60`
- **Problem:** Damping multiplied by stiffness (`-v * damping * stiffness`).
- **Impact:** Non-standard tuning; changing stiffness alters damping behavior.
- **Fix direction:** Use independent damping coefficient or clearly define/document ratio semantics and update callers/config.
- **Notes:** Behavior change; verify feel/regressions.

#### [physics/groups] #183 — `GroupManager` doesn't validate group membership
- **Severity:** Medium | **File:** `wgpu-api/src/physics/groups.rs:266–279`
- **Problem:** Body can join multiple groups; forces/constraints may apply multiple times.
- **Impact:** Duplicate/unintended forces.
- **Fix direction:** Enforce single-group membership or track/ref-count membership and document multi-group semantics explicitly.

#### [physics/interaction] #184 — Interaction projection may divide by zero
- **Severity:** Medium | **File:** `wgpu-api/src/physics/interaction.rs:576–580`
- **Problem:** Zero-length axis possible (single item/alignment).
- **Impact:** NaN/inf corrupts interaction state.
- **Fix direction:** Guard axis length with epsilon and fallback axis.

#### [monitor] #185 — `ResourceHistory` uses `RefCell`
- **Severity:** Medium | **File:** `wgpu-api/src/monitor.rs:1282`
- **Problem:** Interior mutability can panic on borrow conflicts.
- **Impact:** Runtime panic risk in diagnostics/monitoring paths.
- **Fix direction:** Prefer `&mut self` API or safer cell structure for accessed fields.

#### [monitor] #186 — `largest_category` returns `&'static str` from fixed comparison
- **Severity:** Medium | **File:** `wgpu-api/src/monitor.rs:202`
- **Problem:** String/category mapping can drift from struct fields.
- **Impact:** New categories may be omitted silently.
- **Fix direction:** Use enum/typed category identifiers derived from fields.

#### [text/shaping] #187 — `ShaperConfig` doesn't validate `font_size`
- **Severity:** Medium | **File:** `wgpu-api/src/shaping.rs:106–120`
- **Problem:** Accepts 0/negative/NaN.
- **Impact:** Division-by-zero/invalid rasterization downstream.
- **Fix direction:** Validate/clamp or return `Result`/newtype invariant.

#### [physics/constraints] #188 — `ConstraintSystem::apply` checks all constraints for all bodies
- **Severity:** Medium | **File:** `wgpu-api/src/physics/constraints.rs:1126–1130`
- **Problem:** No body-to-constraint indexing.
- **Impact:** O(constraints × bodies) mostly no-op work.
- **Fix direction:** Maintain per-body constraint lists/indices.

### Low

#### [font] #189 — `FontEntry::matches` requires exact property match
- **Severity:** Low | **File:** `wgpu-api/src/fontdb.rs:112–117`
- **Problem:** Exact equality too strict vs scored matching.
- **Impact:** Limited usefulness; near-matches rejected.
- **Fix direction:** Add tolerance or deprecate in favor of `match_score()`.

#### [glyph] #190 — `GlyphInfo::position()` formula non-obvious
- **Severity:** Low | **File:** `wgpu-api/src/glyph.rs:104–106`
- **Problem:** `-bearing_y + height` baseline-to-top-left conversion undocumented.
- **Impact:** Maintenance/sign-error risk.
- **Fix direction:** Add clarifying doc comment (and unit test if helpful).

#### [math] #191 — `damp()` may overflow for large `dt`
- **Severity:** Low | **File:** `wgpu-api/src/math_utils.rs:491–493`
- **Problem:** Large `speed*dt` makes `exp` behavior produce unstable results after long pauses.
- **Impact:** NaN/infinity artifacts after suspension.
- **Fix direction:** Clamp `speed * dt` to safe max before `exp`.

#### [physics/dock] #192 — `Dock::bounds()` indexing pattern fragile on empty items
- **Severity:** Low | **File:** `wgpu-api/src/physics/dock.rs:779–788`
- **Problem:** Uses `self.items[0]` despite emptiness concerns; pattern not robust.
- **Impact:** Potential panic if invariants change.
- **Fix direction:** Use `first()`/safe fallbacks consistently.

#### [physics/fields] #193 — `MagneticField::repulsive()` silently clamps strength
- **Severity:** Low | **File:** `wgpu-api/src/physics/fields.rs:286–288`
- **Problem:** Clamps to `-10000.0` without signaling.
- **Impact:** Unexpected weakening of strong fields.
- **Fix direction:** Document clamp or return validated/adjusted result.

#### [physics/solver] #194 — `SolverStats.max_penetration` ambiguous when zero
- **Severity:** Low | **File:** `wgpu-api/src/physics/solver.rs:50`
- **Problem:** `0.0` used for both "no collisions" and "no penetration".
- **Impact:** Ambiguous diagnostics/monitoring.
- **Fix direction:** Use `Option<f32>` or sentinel like `-1.0`.

#### [physics/spatial_index] #195 — `SpatialHash` default `cell_size` hardcoded
- **Severity:** Low | **File:** `wgpu-api/src/physics/spatial_index.rs:72–73`
- **Problem:** Fixed 64px may be suboptimal for other icon/layout sizes.
- **Impact:** Extra broad-phase checks when bodies span cells.
- **Fix direction:** Derive/configure from average item/body size.

#### [text/shaping] #196 — `ShapedText` height sums line heights without leading
- **Severity:** Low | **File:** `wgpu-api/src/shaping.rs:52`
- **Problem:** No inter-line spacing/leading.
- **Impact:** Multi-line text lines touch.
- **Fix direction:** Include line gap/leading in total height calculation.

#### [physics/forces] #197 — `RepulsionForce` doesn't check zero distance
- **Severity:** Low | **File:** `wgpu-api/src/physics/forces.rs:110–150`
- **Problem:** Division by distance without epsilon guard.
- **Impact:** NaN/infinity force if body at center.
- **Fix direction:** Early-return zero/epsilon-clamped force when `dist` is near zero.

#### [physics/constraints] #198 — `PathConstraint` doesn't validate control points
- **Severity:** Low | **File:** `wgpu-api/src/physics/constraints.rs:200–250`
- **Problem:** Accepts < 2 control points.
- **Impact:** Undefined behavior on apply.
- **Fix direction:** Validate input (>=2) and return error/assert invariant.

#### [physics/interaction] #199 — Interaction API doesn't handle scale factor changes
- **Severity:** Low | **File:** `wgpu-api/src/physics/interaction.rs:721–725`
- **Problem:** Pointer/radius coordinates not rescaled on DPI/scale change.
- **Impact:** Interaction sensitivity incorrect after monitor/scale changes.
- **Fix direction:** Track scale factor and transform interaction coordinates/radii on change.

---

## Suggested fix batches (for parallelizable work)

- **Batch A (text correctness):** #156, #157, #158, #175, #176, #190
- **Batch B (text perf/memory):** #161, #162, #172, #173, #174, #187, #189, #196
- **Batch C (math correctness):** #163, #164, #177, #178, #191
- **Batch D (solver perf):** #159, #160, #194
- **Batch E (dock behavior):** #165, #166, #167, #192
- **Batch F (forces/fields):** #168, #169, #170, #182, #193, #197
- **Batch G (constraints/groups/spatial):** #171, #180, #181, #183, #188, #195, #198
- **Batch H (interaction/monitor):** #179, #184, #185, #186, #199

## Quick validation checklist per batch

- **Text/glyph:** font loading resolves known families, fallback covers non-Latin sample, atlas resize does not corrupt previously rendered glyphs, no unbounded cache growth.
- **Math:** unit tests for rotated scale extraction and rotation direction; projection/transform consistency in rendered output.
- **Physics/solver:** no per-iteration body clones, stable collision resolution, no NaN/infs under edge cases (zero distance, empty groups).
- **Dock/interaction:** correct spring timing at fixed step, variable-size layout spacing, drag reorder to ends, correct behavior across scale-factor changes.
- **General:** `cargo check -p wgpu-api` passes; relevant module tests pass; no new hot-path allocations introduced.
