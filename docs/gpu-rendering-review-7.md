# GPU Rendering Code Review — Round 7 (Font, Glyph, Math, Physics Submodules, Shaping)

This is a continuation of the GPU rendering code review. Rounds 1–6 covered `renderer.rs`, `backend.rs`, `dock.rs`, `dashed_block.rs`, `scene.rs`, `command.rs`, `video.rs`, `gpu.rs`, `animation.rs`, `render.rs`, `date_time_block.rs`, `draw.rs`, `texture.rs`, `layout/*`, `interaction/*`, `batching.rs`, `clip.rs`, `frame.rs`, `text_render.rs`, `geometry.rs`, `color.rs`, `brush.rs`, `image.rs`, `gif.rs`, `transform.rs`, `viewport.rs`, `surface.rs`, `error.rs`, `lib.rs`, `physics/world.rs`, `physics/body.rs`, `physics/collision.rs`, `physics/springs.rs`, `physics/timestep.rs`, `shaders.rs`, `wayland.rs`, `morphing.rs`, `primitives.rs`, and `perf.rs` (issues 1–155). This round covers `fontdb.rs`, `glyph.rs`, `math_utils.rs`, `monitor.rs`, `physics/commands.rs`, `physics/constraints.rs`, `physics/dock.rs`, `physics/fields.rs`, `physics/forces.rs`, `physics/groups.rs`, `physics/interaction.rs`, `physics/solver.rs`, `physics/spatial_index.rs`, `physics/visual_effects.rs`, and `shaping.rs` (issues 156–199).

---

## Critical Issues

### 156. fontdb.rs load_font_from_data creates empty database then queries it (fontdb.rs:211–251)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 211–251

```rust
// Use fontdb to parse the font and extract metadata
let db = fontdb::Database::default();

let (family, weight, style) = db
    .query(&fontdb::Query {
        families: &[],
        style: fontdb::Style::Normal,
        weight: fontdb::Weight(400),
        stretch: fontdb::Stretch::Normal,
    })
```

`load_font_from_data` creates a new empty `fontdb::Database`, then immediately queries it with an empty family list. Since no fonts were added to the database, `query` will always return `None`, and the code falls through to the default `("Unknown".to_string(), FontWeight::Normal, FontStyle::Normal)`. The actual font data (`data: Vec<u8>`) is never parsed by fontdb.

**Impact:** Every loaded font is identified as "Unknown" with Normal weight and Normal style, regardless of the actual font file. Font matching and fallback are completely broken — every font looks identical to the resolver.

**Fix:** Call `db.load_font_safe(data.as_ref())` or `db.load_font(data)` before querying, or use a font parsing library (e.g., `read-fonts`) to extract metadata directly.

---

### 157. fontdb.rs resolve_for_char ignores the character parameter (fontdb.rs:467–488)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 467–488

```rust
pub fn resolve_for_char(&self, _ch: char) -> Option<&FontEntry> {
    // Try the default family first
    if let Some(entry) = self
        .database
        .find_best_match(&self.default_family, &self.default_properties)
    {
        return Some(entry);
    }
```

The character parameter `_ch` is ignored (prefixed with `_`). The resolver never checks whether the selected font actually contains a glyph for the character. It always returns the first available font from the default family, regardless of whether that font supports the character's script.

**Impact:** CJK characters, emoji, and special symbols will be rendered with a Latin font that lacks those glyphs, producing tofu (□) or missing characters. The fallback chain is never triggered based on actual character coverage.

**Fix:** Check the font's Unicode coverage tables before returning it. Use `swash` or `read-fonts` to verify the font contains a glyph for the character codepoint.

---

### 158. glyph.rs expand() invalidates all cached GlyphInfo entries (glyph.rs:467–490)
**File:** `wgpu-api/src/glyph.rs`  
**Lines:** 467–490

```rust
fn expand(&mut self) -> bool {
    // ...
    self.current_texture = TextureId::new(self.current_texture.as_u64() + 1);
    self.size = Size::new(new_width as f32, new_height as f32);
    self.cursor = Point::new(0.0, 0.0);
    self.current_row_height = 0;
    self.stats.atlas_width = new_width;
    self.stats.atlas_height = new_height;
    self.stats.total_pixels = new_width * new_height;
    self.stats.used_pixels = 0;
    true
}
```

When the atlas expands, a new `TextureId` is assigned, but `self.cache` (HashMap<GlyphCacheKey, GlyphInfo>) is never updated. All cached `GlyphInfo` entries still reference the old `TextureId` and old UV coordinates. The cursor is reset to (0, 0), meaning the new atlas starts from scratch, but the old cached entries point to a texture that no longer exists or contains stale data.

**Impact:** After atlas expansion, all previously cached glyphs render from the wrong texture or wrong UV region, producing corrupted or missing text. The cache becomes silently corrupted.

**Fix:** Clear the cache on expansion or update all cached `TextureRegion` entries to reference the new texture. The comment says "Old atlases are retained until all references are dropped" but no retention mechanism exists.

---

### 159. physics/solver.rs solve() clones all bodies every iteration (solver.rs:92–94)
**File:** `wgpu-api/src/physics/solver.rs`  
**Lines:** 92–94

```rust
for _ in 0..iterations {
    // Clone bodies for broad phase to avoid borrow conflicts
    let bodies_clone: Vec<PhysicsBody> = bodies.iter().cloned().collect();
```

With default 8 iterations, `solve()` clones all `PhysicsBody` instances 8 times per frame. Each clone copies the full body struct. At 120 Hz with 20 bodies, this is 8 × 20 × 120 = 19,200 body clones per second.

**Impact:** Massive per-frame heap allocation in the solver hot path. Violates Core Engineering Priority #1 (lowest possible CPU usage) and #2 (lowest possible system RAM usage).

**Fix:** Use indices into the body vector instead of cloning. The broad phase only needs bounding rects, not full body clones. Store `(usize, PhysicsBody)` pairs or use a spatial index.

---

### 160. physics/solver.rs solve() clones bodies twice per collision pair (solver.rs:100–102)
**File:** `wgpu-api/src/physics/solver.rs`  
**Lines:** 100–102

```rust
for &(i, j) in &pairs {
    let a = bodies_clone[i].clone();
    let b = bodies_clone[j].clone();
```

After cloning all bodies into `bodies_clone`, each collision pair clones the two bodies again. This is a second level of cloning on top of the already-cloned `bodies_clone`.

**Impact:** Double cloning per collision pair compounds the allocation problem from #159. For N bodies with M collision pairs, this is N + 2M clones per iteration.

**Fix:** Use references into `bodies_clone` instead of cloning individual bodies.

---

## High Severity Issues

### 161. fontdb.rs discover_system_fonts loads every font into memory (fontdb.rs:279–304)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 279–304

```rust
pub fn discover_system_fonts(&mut self) {
    // ...
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            self.scan_font_dir(&path);
        } else if Self::is_font_file(&path) {
            if let Err(e) = self.load_font(&path) {
```

`discover_system_fonts` recursively scans all system font directories and loads every font file into memory via `load_font`, which reads the entire file into `Vec<u8>` and stores it in `FontEntry.data`. A typical Linux system has 200–500 font files totaling 100–500 MB.

**Impact:** Loading all system fonts into RAM violates Core Engineering Priority #2 (lowest possible system RAM usage). The dock only needs a handful of fonts for UI text.

**Fix:** Only load fonts that are explicitly requested. Use lazy loading: discover font paths but defer reading until the font is actually needed.

---

### 162. glyph.rs GlyphCache never evicts entries (glyph.rs:644–650)
**File:** `wgpu-api/src/glyph.rs`  
**Lines:** 644–650

```rust
// Check VRAM budget before allocating
let estimated_vram =
    self.atlas.estimate_vram_mb() + (glyph_width * glyph_height) as f32 / (1024.0 * 1024.0);
if estimated_vram > self.max_vram_mb {
    // In a real implementation, evict least-recently-used glyphs
    log::warn!("Glyph cache VRAM budget exceeded, consider increasing max_vram_mb");
}
```

When the VRAM budget is exceeded, the cache logs a warning but does not evict any entries. The allocation continues regardless, and the cache grows unbounded.

**Impact:** VRAM grows without limit as new glyphs are encountered. The warning is logged every frame for every new glyph, creating log spam.

**Fix:** Implement LRU eviction or use a bounded cache. Remove the least recently used glyphs when the budget is exceeded.

---

### 163. math_utils.rs extract_scale() uses incorrect formula (math_utils.rs:300–305)
**File:** `wgpu-api/src/math_utils.rs`  
**Lines:** 300–305

```rust
fn extract_scale(&self) -> Vec2 {
    Vec2::new(
        self.x_axis.x.abs() + self.x_axis.y.abs(),
        self.y_axis.x.abs() + self.y_axis.y.abs(),
    )
}
```

The scale extraction sums the absolute values of matrix components. The correct formula for extracting scale from a 2D transform matrix is the Euclidean norm (length) of each axis: `x_axis.length()` and `y_axis.length()`. The sum of absolute values overestimates scale for rotated matrices.

For a 45° rotation with scale 1.0: x_axis = (cos(45°), sin(45°)) = (0.707, 0.707). The formula gives 0.707 + 0.707 = 1.414 instead of √(0.707² + 0.707²) = 1.0.

**Impact:** Decomposed scale is always overestimated for rotated transforms. Code that relies on extracted scale (e.g., animation interpolation) gets incorrect values.

**Fix:** Use `self.x_axis.x.abs() * self.x_axis.x.abs() + self.x_axis.y.abs() * self.x_axis.y.abs()`.sqrt()` or `self.x_axis.truncate().length()`.

---

### 164. math_utils.rs Vec2Ext::rotate() rotates clockwise, not counterclockwise (math_utils.rs:209–213)
**File:** `wgpu-api/src/math_utils.rs`  
**Lines:** 209–213

```rust
fn rotate(self, angle_rad: f32) -> Self {
    let c = angle_rad.cos();
    let s = angle_rad.sin();
    Mat2::from_cols(Vec2::new(c, -s), Vec2::new(s, c)) * self
}
```

The rotation matrix `[[c, s], [-s, c]]` rotates clockwise. The standard mathematical convention is counterclockwise rotation: `[[c, -s], [s, c]]`. The test at line 619–622 confirms the clockwise behavior: rotating (1,0) by 90° gives (0,-1) instead of (0,1).

**Impact:** Positive angles rotate in the opposite direction from the mathematical convention. Code that expects counterclockwise rotation (e.g., animation, physics) gets inverted behavior.

**Fix:** Swap the sign: `Mat2::from_cols(Vec2::new(c, s), Vec2::new(-s, c))`.

---

### 165. physics/dock.rs apply_springs() uses hardcoded timestep (dock.rs:485–494)
**File:** `wgpu-api/src/physics/dock.rs`  
**Lines:** 485–494

```rust
pub fn apply_springs(&mut self) {
    for item in self.items.iter_mut() {
        if !item.is_dragged {
            item.integrate(
                self.config.spring_stiffness,
                self.config.spring_damping,
                0.016, // ~60 FPS timestep
            );
        }
    }
}
```

`apply_springs()` hardcodes `dt = 0.016` (~60 FPS). If called at a different frame rate, the spring simulation will be too fast or too slow. The physics engine uses a fixed timestep of 120 Hz (0.00833s), but this method uses 60 Hz.

**Impact:** Spring behavior is inconsistent with the physics engine's fixed timestep. Running at 120 Hz makes springs 2× faster than intended; running at 30 Hz makes them 2× slower.

**Fix:** Use the physics engine's fixed timestep or pass `dt` as a parameter. The `apply_springs_dt` method exists but `apply_springs` is the default.

---

### 166. physics/dock.rs reorder_slots() assumes uniform item size (dock.rs:450–458)
**File:** `wgpu-api/src/physics/dock.rs`  
**Lines:** 450–458

```rust
fn reorder_slots(&mut self) {
    let spacing = self.config.spacing;
    let padding = self.config.padding;
    let item_size = self.items.first().map_or(48.0, |i| i.size);
    for (i, item) in self.items.iter_mut().enumerate() {
        item.slot = i;
        item.target_position = padding + i as f32 * (item_size + spacing);
    }
}
```

`reorder_slots()` uses the first item's size for all items. If items have different sizes (e.g., some icons are 48px, others are 64px), the target positions are incorrect. Each item's target should account for the cumulative size of all preceding items.

**Impact:** Variable-sized icons are positioned incorrectly. Larger icons overlap or leave gaps because the layout assumes uniform sizing.

**Fix:** Compute cumulative positions: `target[i] = padding + sum(size[j] + spacing for j in 0..i) + size[i]/2`.

---

### 167. physics/dock.rs reorder_by_pointer() logic is incorrect (dock.rs:400–447)
**File:** `wgpu-api/src/physics/dock.rs`  
**Lines:** 400–447

```rust
pub fn reorder_by_pointer(&mut self, pointer: Point) -> Option<usize> {
    // ...
    let mut insertion_slot = dragged_slot;
    for (i, item) in self.items.iter().enumerate() {
        if item.body_id == dragged_body {
            continue;
        }

        if (pointer_1d - item.target_position).abs() < self.config.spacing / 2.0 {
            if pointer_1d < item.target_position {
                insertion_slot = i.min(insertion_slot);
            } else {
                insertion_slot = (i + 1).min(insertion_slot);
            }
        }
    }
```

The reordering logic only triggers when the pointer is within `spacing/2` of an item's target position. If the pointer is between items but not close enough to any target, no reordering occurs. Also, `insertion_slot` is clamped to `items.len() - 1`, preventing insertion at the end. The `min` logic is also wrong — it should track the nearest insertion point, not the minimum slot.

**Impact:** Dragging an item to the end of the dock doesn't work. Reordering is unreliable when items are far from their targets (e.g., during animation).

**Fix:** Find the insertion slot by comparing the pointer's 1D position to each item's center, not target. Use `pointer_1d < item.current_position` to determine before/after.

---

### 168. physics/fields.rs SeparationField::apply_to_group is O(n²) (fields.rs:475–511)
**File:** `wgpu-api/src/physics/fields.rs`  
**Lines:** 475–511

```rust
pub fn apply_to_group(&self, bodies: &mut [PhysicsBody], dt: f32) {
    // ...
    for i in 0..len {
        for j in 0..len {
            if i == j {
                continue;
            }
```

`apply_to_group` checks every pair of bodies, including both (i,j) and (j,i), resulting in N*(N-1) checks instead of N*(N-1)/2. For 50 bodies, this is 2,450 checks instead of 1,225.

**Impact:** Double the work needed for separation calculation. At 120 Hz with 50 bodies, this is 294,000 unnecessary checks per second.

**Fix:** Use `for j in (i+1)..len` to check each pair once, and apply equal and opposite forces to both bodies.

---

### 169. physics/fields.rs ForceField::compute_force double-counts sources (fields.rs:696–702)
**File:** `wgpu-api/src/physics/fields.rs`  
**Lines:** 696–702

```rust
pub fn compute_force(&self, body: &PhysicsBody) -> Vector2D {
    let mut total = Vector2D::ZERO;

    for source in &self.sources {
        total.x += source.compute_force(body).x;
        total.y += source.compute_force(body).y;
    }
```

`source.compute_force(body)` is called twice per source — once for `.x` and once for `.y`. Each call recomputes the entire force vector, including distance calculations and falloff. This doubles the computation cost.

**Impact:** Every field source's force is computed twice per body per frame, wasting CPU cycles.

**Fix:** Call `compute_force` once and store the result: `let f = source.compute_force(body); total.x += f.x; total.y += f.y;`.

---

### 170. physics/fields.rs CohesionField center_of_mass fallback divides uninitialized values (fields.rs:357–378)
**File:** `wgpu-api/src/physics/fields.rs`  
**Lines:** 357–378

```rust
pub fn center_of_mass(bodies: &[PhysicsBody]) -> Point {
    // ...
    if total_mass > 0.0 {
        Point::new(cx / total_mass, cy / total_mass)
    } else {
        let n = bodies.len() as f32;
        Point::new(cx / n, cy / n)
    }
}
```

If all bodies have `mass == 0.0` (static bodies), `total_mass` is 0.0 and the fallback divides `cx` and `cy` by `n`. However, `cx` and `cy` were only accumulated for bodies with `mass > 0.0`. Since no body passed the mass check, `cx` and `cy` are both 0.0. The result is `Point::ORIGIN`, which is correct but misleading — it's not the geometric center of the bodies.

**Impact:** For a group of static bodies, the cohesion center is (0,0) instead of the actual geometric center. Static bodies are pulled toward the origin instead of clustering together.

**Fix:** In the fallback, compute the geometric center by summing positions regardless of mass.

---

### 171. physics/constraints.rs HingeJoint doesn't constrain rotation angle (constraints.rs:76–130)
**File:** `wgpu-api/src/physics/constraints.rs`  
**Lines:** 76–130

The `HingeJoint` struct has `anchor_a` and `anchor_b` points but no angle limits. The `apply` method only keeps the anchors aligned but doesn't restrict the relative rotation. A true hinge joint should allow rotation around a fixed axis while preventing separation.

**Impact:** The hinge joint allows bodies to drift apart over time because it only corrects position, not velocity. Without velocity correction, the constraint is not stable.

**Fix:** Add velocity correction to prevent drift. Consider adding angle limits for limited hinges.

---

### 172. shaping.rs ShapedText::new collects HashSet then converts to Vec (shaping.rs:53–58)
**File:** `wgpu-api/src/shaping.rs`  
**Lines:** 53–58

```rust
let fonts_used = lines
    .iter()
    .flat_map(|l| l.all_glyphs().map(|g| g.font_id))
    .collect::<std::collections::HashSet<_>>()
    .into_iter()
    .collect();
```

A `HashSet` is allocated to deduplicate font IDs, then immediately converted to a `Vec`. This requires two allocations and a full iteration over the hash set.

**Impact:** Extra heap allocation and iteration for every shaped text operation. For UI text with few fonts, this overhead is unnecessary.

**Fix:** Use `Vec::dedup()` after sorting, or use a small fixed-size array with linear search for deduplication (since font counts are typically < 10).

---

## Medium Severity Issues

### 173. fontdb.rs FontDatabase::new() reads HOME env var twice (fontdb.rs:175–178)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 175–178

```rust
if let Some(home) = std::env::var_os("HOME") {
    let home_path = PathBuf::from(home);
    system_dirs.push(home_path.join(".local/share/fonts"));
    system_dirs.push(PathBuf::from(std::env::var_os("HOME").unwrap()).join(".fonts"));
}
```

The first `var_os("HOME")` is wrapped in `if let Some`, but the second call uses `unwrap()` on a fresh `var_os("HOME")` call. Since the `if let` already confirmed HOME exists, the `unwrap()` is redundant but adds a second environment variable lookup.

**Impact:** Minor — redundant environment variable lookup. The `unwrap()` is safe here because the `if let` already verified HOME exists.

**Fix:** Reuse `home_path`: `system_dirs.push(home_path.join(".fonts"));`.

---

### 174. fontdb.rs match_score weight division truncates (fontdb.rs:124–125)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 124–125

```rust
let weight_diff = (self.properties.weight as i32 - props.weight as i32).unsigned_abs();
score += weight_diff / 100;
```

Integer division by 100 truncates the weight difference. A weight difference of 99 gives score 0, while 100 gives score 1. The score is not proportional to the actual difference.

**Impact:** Font matching may prefer a font with weight difference 99 over one with difference 101, even though the visual difference is similar. The step function creates discontinuities in matching quality.

**Fix:** Use floating-point division or keep the raw weight difference.

---

### 175. glyph.rs TextureRegion UV calculation may have off-by-one (glyph.rs:151–158)
**File:** `wgpu-api/src/glyph.rs`  
**Lines:** 151–158

```rust
let uv_min = Point::new(
    x as f32 / atlas_width as f32,
    y as f32 / atlas_height as f32,
);
let uv_max = Point::new(
    (x + width) as f32 / atlas_width as f32,
    (y + height) as f32 / atlas_height as f32,
);
```

The UV max includes the full pixel width/height. For texture sampling, the correct UV should be at the pixel center or edge, depending on the sampler mode. Including the full width may sample one texel beyond the intended region, causing bleeding between adjacent glyphs.

**Impact:** Glyphs may show artifacts from neighboring glyphs in the atlas due to texture bleeding. The 1px padding in `allocate()` helps mitigate this, but the UV calculation should still account for texel centers.

**Fix:** Use `(x + width - 0.5) / (atlas_width - 0.5)` for texel-center sampling, or keep the padding and ensure the shader clamps UVs.

---

### 176. glyph.rs GlyphAtlas::clear() doesn't reset texture ID (glyph.rs:547–553)
**File:** `wgpu-api/src/glyph.rs`  
**Lines:** 547–553

```rust
pub fn clear(&mut self) {
    self.cache.clear();
    self.cursor = Point::new(0.0, 0.0);
    self.current_row_height = 0;
    self.stats.glyph_count = 0;
    self.stats.used_pixels = 0;
}
```

`clear()` resets the cursor and stats but doesn't reset `self.current_texture`. If the atlas was expanded before clearing, the texture ID remains at the expanded value. New allocations will use the old texture ID, but the GPU texture may have been destroyed.

**Impact:** After clear, new glyphs are allocated against a potentially invalid texture ID. The GPU texture state is out of sync with the CPU cache.

**Fix:** Reset `self.current_texture` to a fresh ID or track texture lifecycle explicitly.

---

### 177. math_utils.rs orthographic_topleft() has y-axis sign inconsistency (math_utils.rs:369–384)
**File:** `wgpu-api/src/math_utils.rs`  
**Lines:** 369–384

The comment says "Maps (0,0) to (-1,1) NDC and (width,height) to (1,-1) NDC" (top-left origin). But the y-axis uses `-inv_h`, which maps increasing y to decreasing NDC y. This is correct for top-left origin. However, `transform_2d_mat4` at line 422–432 uses a different y-sign convention: `-s * scale.x` in the first column.

**Impact:** Mixing `orthographic_topleft` with `transform_2d_mat4` may produce inverted y-coordinates if the conventions don't match.

**Fix:** Document the coordinate convention explicitly and ensure all transform functions use the same convention.

---

### 178. math_utils.rs hash_f32 returns always-positive value (math_utils.rs:517–528)
**File:** `wgpu-api/src/math_utils.rs`  
**Lines:** 517–528

```rust
pub fn hash_f32(x: u32, y: u32) -> f32 {
    // ...
    let frac = ((n >> 16) as f32 / 32768.0).fract();
    frac.abs()
}
```

`.fract()` returns the fractional part, which is in `(-1, 1)`. `.abs()` makes it always positive, so the range is `[0, 1)`. This is fine for noise, but the hash function's distribution may not be uniform due to the `.abs()` folding negative values onto positive ones.

**Impact:** The hash distribution has a slight bias toward values near 0, where both positive and negative fractional parts map to the same positive value.

**Fix:** Use `((n >> 16) as f32).fract()` without `.abs()` if a symmetric range is acceptable, or use a different normalization.

---

### 179. physics/commands.rs InteractionManager command_history grows unbounded (commands.rs:1013–1016)
**File:** `wgpu-api/src/physics/commands.rs`  
**Lines:** 1013–1016

```rust
if self.command_history.len() > self.max_history {
    self.command_history.remove(0);
}
self.command_history.push(command.clone());
```

`remove(0)` on a Vec is O(n) — it shifts all remaining elements. For a large history, this is a linear scan on every command.

**Impact:** Every command after `max_history` triggers an O(n) shift operation. At 120 Hz with a history of 1000, this is 120,000 element shifts per second.

**Fix:** Use `VecDeque` instead of `Vec` for O(1) pop_front, or use a ring buffer.

---

### 180. physics/spatial_index.rs GridCell::remove() is O(n) (spatial_index.rs:33–35)
**File:** `wgpu-api/src/physics/spatial_index.rs`  
**Lines:** 33–35

```rust
pub fn remove(&mut self, body: BodyId) {
    self.bodies.retain(|&id| id != body);
}
```

`retain` iterates the entire vector and rebuilds it. For cells with many bodies, this is O(n) per removal.

**Impact:** Removing a body from a crowded cell is slow. In a dense dock with many icons in the same cell, this adds up.

**Fix:** Use swap-remove pattern or track body positions with a swap-friendly data structure.

---

### 181. physics/spatial_index.rs SpatialHash doesn't handle body movement efficiently (spatial_index.rs:106–200)
**File:** `wgpu-api/src/physics/spatial_index.rs`

The spatial hash requires a full rebuild (`clear()` + `insert()`) whenever bodies move. There's no incremental update — moving one body requires rehashing all bodies.

**Impact:** At 120 Hz, the entire spatial index is rebuilt from scratch every physics step, wasting CPU cycles on bodies that haven't moved significantly.

**Fix:** Track which bodies moved and only rehash those bodies. Or use a dirty flag to defer rebuilds.

---

### 182. physics/forces.rs SpringForce damping formula is non-standard (forces.rs:57–60)
**File:** `wgpu-api/src/physics/forces.rs`  
**Lines:** 57–60

```rust
// Damping force: F = -c * v
let damping = Vector2D::new(
    -body.velocity.x * self.damping * self.stiffness,
    -body.velocity.y * self.damping * self.stiffness,
);
```

The damping force multiplies `damping * stiffness`. Standard spring-damper systems use `F_damping = -c * v` where `c` is an independent damping coefficient. Multiplying by stiffness couples damping to stiffness, making it impossible to tune them independently.

**Impact:** Changing spring stiffness also changes damping proportionally. Critical damping requires a specific ratio that changes with stiffness.

**Fix:** Use `self.damping` as the damping coefficient directly, without multiplying by stiffness. Or document that `damping` is a ratio, not an absolute coefficient.

---

### 183. physics/groups.rs GroupManager doesn't validate group membership (groups.rs:266–279)
**File:** `wgpu-api/src/physics/groups.rs`  
**Lines:** 266–279

Adding a body to a group doesn't check if the body already belongs to another group. A body can be in multiple groups simultaneously, leading to duplicate force application.

**Impact:** A body in two groups receives forces from both groups' fields, springs, and constraints, potentially doubling the intended effect.

**Fix:** Track body-to-group membership and either prevent multi-group membership or document the expected behavior.

---

### 184. physics/interaction.rs InteractionApi projection may divide by zero (interaction.rs:576–580)
**File:** `wgpu-api/src/physics/interaction.rs`  
**Lines:** 576–580

The projection axis calculation may produce a zero-length vector when the dock has only one item or when items are aligned in a way that makes the axis undefined.

**Impact:** Division by zero produces NaN or infinity, corrupting the interaction state.

**Fix:** Check for zero-length axis before projection and fall back to a default axis.

---

### 185. monitor.rs ResourceHistory uses RefCell for interior mutability (monitor.rs:1282)
**File:** `wgpu-api/src/monitor.rs`  
**Line:** 1282

```rust
entries: RefCell<VecDeque<HistoryEntry>>,
```

`ResourceHistory` uses `RefCell` for interior mutability. If a borrow conflict occurs (e.g., nested access during iteration), it panics at runtime.

**Impact:** Runtime panic risk in monitoring code. The `RefCell` overhead adds ~10ns per call.

**Fix:** Use `&mut self` for mutating methods, or use `Cell` for simple types.

---

### 186. monitor.rs ResourceSnapshot::largest_category returns &'static str (monitor.rs:202)
**File:** `wgpu-api/src/monitor.rs`  
**Line:** 202

```rust
pub fn largest_category(&self) -> (&'static str, MemorySize) {
```

The method returns a `&'static str` for the category name, but the comparison logic iterates a fixed array of categories. If the array changes, the return type doesn't reflect which field was actually largest — it's a string literal, not a reference into the struct.

**Impact:** Minor — the method works correctly as long as the category array matches the struct fields. But adding a new memory category without updating the array will cause it to be invisible to `largest_category`.

**Fix:** Use an enum for categories and derive the name from the enum variant.

---

### 187. shaping.rs ShaperConfig doesn't validate font_size (shaping.rs:106–120)
**File:** `wgpu-api/src/shaping.rs`  
**Lines:** 106–120

`ShaperConfig` accepts any `f32` for `font_size`, including 0, negative values, and NaN. A zero or negative font size will cause division by zero or invalid glyph rasterization.

**Impact:** Invalid font sizes produce silent rendering failures or panics in downstream code.

**Fix:** Validate `font_size > 0.0` in the constructor or use a newtype wrapper.

---

### 188. physics/constraints.rs ConstraintSystem::apply iterates all constraints for all bodies (constraints.rs:1126–1130)
**File:** `wgpu-api/src/physics/constraints.rs`  
**Lines:** 1126–1130

The constraint application loop checks every constraint against every body, even though most constraints only affect specific bodies. No indexing or filtering is used.

**Impact:** O(constraints × bodies) per step. For 20 constraints and 20 bodies, this is 400 checks per step at 120 Hz = 48,000 checks/sec, most of which are no-ops.

**Fix:** Index constraints by body ID so only relevant constraints are checked for each body.

---

## Low Severity Issues

### 189. fontdb.rs FontEntry::matches requires exact property match (fontdb.rs:112–117)
**File:** `wgpu-api/src/fontdb.rs`  
**Lines:** 112–117

`matches()` requires exact equality for weight, style, and stretch. This is too strict for font selection — a Bold font should partially match a SemiBold request.

**Impact:** `matches()` returns false for near-matches, forcing the caller to use `match_score()` instead. The method is rarely useful as-is.

**Fix:** Add a tolerance parameter or deprecate in favor of `match_score()`.

---

### 190. glyph.rs GlyphInfo::position() formula is non-obvious (glyph.rs:104–106)
**File:** `wgpu-api/src/glyph.rs`  
**Lines:** 104–106

```rust
pub fn position(&self) -> Point {
    Point::new(self.bearing_x, -self.bearing_y + self.height)
}
```

The y-coordinate formula `-bearing_y + height` converts bearing (distance from baseline) to top-left position. This is correct but non-obvious without a comment explaining the coordinate transformation.

**Impact:** Code maintainability — a developer modifying this method may introduce a sign error.

**Fix:** Add a comment explaining the baseline-to-top-left conversion.

---

### 191. math_utils.rs damp() may overflow for large dt (math_utils.rs:491–493)
**File:** `wgpu-api/src/math_utils.rs`  
**Lines:** 491–493

```rust
pub fn damp(current: f32, target: f32, speed: f32, dt: f32) -> f32 {
    current + (target - current) * (1.0 - (-speed * dt).exp())
}
```

If `speed * dt` is very large (e.g., `speed=1000`, `dt=60` after a long suspension), `(-speed * dt).exp()` overflows to infinity, and `1.0 - infinity` is `-infinity`.

**Impact:** After a long suspension, the damped value becomes NaN or infinity, causing visual artifacts.

**Fix:** Clamp `speed * dt` to a reasonable maximum (e.g., 10.0) before calling `.exp()`.

---

### 192. physics/dock.rs Dock::bounds() panics on empty items (dock.rs:779–788)
**File:** `wgpu-api/src/physics/dock.rs`  
**Lines:** 779–788

```rust
match self.config.axis {
    DockAxis::Horizontal => crate::geometry::Rect::new(
        min_1d,
        self.config.perpendicular_center - self.items[0].size / 2.0,
```

If `self.items` is empty, `self.items[0]` panics. The early return at line 763–765 checks `is_empty()`, but only if the items vector is empty. If items were removed between the check and the access (not possible in single-threaded code, but the pattern is fragile), this could panic.

**Impact:** Low — the empty check at line 763 prevents this in practice. But the pattern is fragile and doesn't use the safe `.first()` API.

**Fix:** Use `self.items.first().map_or(0.0, |i| i.size)` consistently.

---

### 193. physics/fields.rs MagneticField::repulsive() clamps strength to -10000 (fields.rs:286–288)
**File:** `wgpu-api/src/physics/fields.rs`  
**Lines:** 286–288

```rust
pub fn repulsive(center: Point, strength: f32) -> Self {
    Self::new(center, (-strength).max(-10000.0))
}
```

The strength is clamped to a minimum of -10,000. This arbitrary limit may be too low for large scenes or too high for small ones. A strength of 10,001 is silently reduced.

**Impact:** Very strong repulsion fields are silently weakened. The caller has no way to detect the clamping.

**Fix:** Return `Result` or document the clamping behavior.

---

### 194. physics/solver.rs SolverStats max_penetration initialized to 0.0 (solver.rs:50)
**File:** `wgpu-api/src/physics/solver.rs`  
**Line:** 50

```rust
max_penetration: 0.0,
```

If no collisions occur, `max_penetration` is 0.0, which is indistinguishable from "all collisions resolved perfectly." Code monitoring penetration depth can't tell if there were no collisions or if the simulation is perfect.

**Impact:** Monitoring code may misinterpret 0.0 as "no penetration" when it actually means "no collisions detected."

**Fix:** Use `Option<f32>` for `max_penetration` or initialize to `-1.0` to indicate "no data."

---

### 195. physics/spatial_index.rs SpatialHash default cell_size is 64 (spatial_index.rs:72–73)
**File:** `wgpu-api/src/physics/spatial_index.rs`  
**Lines:** 72–73

```rust
Self {
    cell_size: 64.0,
```

The default cell size of 64 pixels is hardcoded. For a dock with 48px icons and 8px spacing, 64px cells are reasonable. But for a different layout (e.g., 96px icons), the cell size should be larger to avoid bodies spanning multiple cells.

**Impact:** Suboptimal cell size leads to bodies being indexed in multiple cells, increasing the number of collision checks.

**Fix:** Compute cell size dynamically based on the average body size, or provide a configuration method.

---

### 196. shaping.rs ShapedText height sums line heights without leading (shaping.rs:52)
**File:** `wgpu-api/src/shaping.rs`  
**Line:** 52

```rust
let height: f32 = lines.iter().map(|l| l.height).sum();
```

The total height is the sum of line heights, but `ShapedLine::height` is `ascent + descent` without leading (inter-line spacing). Multi-line text will have lines touching without any gap.

**Impact:** Multi-line text renders with no inter-line spacing, making it hard to read.

**Fix:** Add leading: `height += line_height * leading_ratio` between lines.

---

### 197. physics/forces.rs RepulsionForce doesn't check for zero distance (forces.rs:110–150)
**File:** `wgpu-api/src/physics/forces.rs`  
**Lines:** 110–150

The repulsion force divides by distance without checking for zero. If a body is exactly at the repulsion center, the force is undefined (division by zero).

**Impact:** A body at the repulsion center produces NaN or infinity force, corrupting the simulation.

**Fix:** Add an epsilon check: `if dist < 0.001 { return Vector2D::ZERO; }`.

---

### 198. physics/constraints.rs PathConstraint doesn't validate control points (constraints.rs:200–250)
**File:** `wgpu-api/src/physics/constraints.rs`  
**Lines:** 200–250

`PathConstraint` accepts any number of control points, including 0 or 1. A path with fewer than 2 points can't define a valid curve, but the constraint doesn't validate this.

**Impact:** A path with 0 or 1 control point produces undefined behavior when the constraint is applied.

**Fix:** Validate that at least 2 control points are provided, or return `Result`.

---

### 199. physics/interaction.rs InteractionApi doesn't handle scale factor changes (interaction.rs:721–725)
**File:** `wgpu-api/src/physics/interaction.rs`  
**Lines:** 721–725

When the display scale factor changes (e.g., moving a window between monitors with different DPI), the interaction coordinates are not rescaled. The pointer position and interaction radius remain in the old scale.

**Impact:** After a scale factor change, interaction feels too sensitive or too sluggish because the coordinates are in the wrong scale.

**Fix:** Track the current scale factor and rescale interaction coordinates when it changes.

---

## Summary of Issues by Severity

| # | Severity | Issue | File |
|---|----------|-------|------|
| 156 | **Critical** | fontdb creates empty database then queries it | fontdb.rs |
| 157 | **Critical** | resolve_for_char ignores character parameter | fontdb.rs |
| 158 | **Critical** | atlas expand invalidates all cached GlyphInfo | glyph.rs |
| 159 | **Critical** | solver clones all bodies every iteration | solver.rs |
| 160 | **Critical** | solver clones bodies twice per collision pair | solver.rs |
| 161 | **High** | discover_system_fonts loads every font into memory | fontdb.rs |
| 162 | **High** | GlyphCache never evicts entries | glyph.rs |
| 163 | **High** | extract_scale uses incorrect formula | math_utils.rs |
| 164 | **High** | Vec2Ext::rotate rotates clockwise | math_utils.rs |
| 165 | **High** | apply_springs uses hardcoded timestep | physics/dock.rs |
| 166 | **High** | reorder_slots assumes uniform item size | physics/dock.rs |
| 167 | **High** | reorder_by_pointer logic is incorrect | physics/dock.rs |
| 168 | **High** | SeparationField::apply_to_group is O(n²) | physics/fields.rs |
| 169 | **High** | ForceField::compute_force double-counts sources | physics/fields.rs |
| 170 | **High** | center_of_mass fallback divides uninitialized values | physics/fields.rs |
| 171 | **High** | HingeJoint doesn't constrain rotation angle | physics/constraints.rs |
| 172 | **High** | ShapedText::new allocates HashSet then Vec | shaping.rs |
| 173 | **Medium** | FontDatabase::new reads HOME env var twice | fontdb.rs |
| 174 | **Medium** | match_score weight division truncates | fontdb.rs |
| 175 | **Medium** | TextureRegion UV off-by-one | glyph.rs |
| 176 | **Medium** | GlyphAtlas::clear doesn't reset texture ID | glyph.rs |
| 177 | **Medium** | orthographic_topleft y-axis sign inconsistency | math_utils.rs |
| 178 | **Medium** | hash_f32 returns always-positive value | math_utils.rs |
| 179 | **Medium** | command_history remove(0) is O(n) | physics/commands.rs |
| 180 | **Medium** | GridCell::remove is O(n) | physics/spatial_index.rs |
| 181 | **Medium** | SpatialHash doesn't handle body movement incrementally | physics/spatial_index.rs |
| 182 | **Medium** | SpringForce damping formula is non-standard | physics/forces.rs |
| 183 | **Medium** | GroupManager doesn't validate group membership | physics/groups.rs |
| 184 | **Medium** | InteractionApi projection may divide by zero | physics/interaction.rs |
| 185 | **Medium** | ResourceHistory uses RefCell | monitor.rs |
| 186 | **Medium** | largest_category returns &'static str | monitor.rs |
| 187 | **Medium** | ShaperConfig doesn't validate font_size | shaping.rs |
| 188 | **Medium** | ConstraintSystem iterates all constraints for all bodies | physics/constraints.rs |
| 189 | **Low** | FontEntry::matches requires exact property match | fontdb.rs |
| 190 | **Low** | GlyphInfo::position formula is non-obvious | glyph.rs |
| 191 | **Low** | damp() may overflow for large dt | math_utils.rs |
| 192 | **Low** | Dock::bounds panics on empty items (fragile) | physics/dock.rs |
| 193 | **Low** | MagneticField::repulsive clamps strength | physics/fields.rs |
| 194 | **Low** | SolverStats max_penetration ambiguous | physics/solver.rs |
| 195 | **Low** | SpatialHash default cell_size hardcoded | physics/spatial_index.rs |
| 196 | **Low** | ShapedText height sums without leading | shaping.rs |
| 197 | **Low** | RepulsionForce doesn't check zero distance | physics/forces.rs |
| 198 | **Low** | PathConstraint doesn't validate control points | physics/constraints.rs |
| 199 | **Low** | InteractionApi doesn't handle scale factor changes | physics/interaction.rs |

## Cumulative Statistics

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 12 | 1 | 2 | 2 | 7 |
| 2 | gpu-rendering-review-2.md | 20 | 3 | 4 | 6 | 7 |
| 3 | gpu-rendering-review-3.md | 30 | 5 | 7 | 10 | 8 |
| 4 | gpu-rendering-review-4.md | 35 | 4 | 13 | 10 | 8 |
| 5 | gpu-rendering-review-5.md | 25 | 5 | 7 | 10 | 3 |
| 6 | gpu-rendering-review-6.md | 35 | 5 | 8 | 12 | 10 |
| **7** | **gpu-rendering-review-7.md** | **44** | **5** | **12** | **16** | **11** |
| **Total** | | **201** | **28** | **53** | **76** | **54** |

## Key Themes

1. **Font system is largely non-functional**: The fontdb module creates empty databases, ignores character parameters, and loads all system fonts into memory. The font resolver cannot match fonts to characters correctly.

2. **Physics solver has severe allocation issues**: The solver clones all bodies every iteration (8× per frame), and each collision pair clones bodies again. This is the single largest source of per-frame allocations.

3. **Glyph atlas expansion corrupts the cache**: When the atlas grows, cached glyph info still references the old texture, causing silent rendering corruption.

4. **Math utilities have sign and formula errors**: Scale extraction overestimates for rotated matrices, and rotation direction is inverted relative to mathematical convention.

5. **Spatial index is not integrated**: The spatial hash exists but is not used by the broad phase, which remains O(n²). The spatial hash itself requires full rebuilds on body movement.

6. **Per-frame allocations throughout**: Vec allocations in force fields, constraint systems, and interaction managers create per-frame garbage that adds up at 120 Hz.
