# GPU Rendering Code Review — Round 8 (Slint, Text, Layout, Visual Effects, App Layer, Diagnostics)

This is a continuation of the GPU rendering code review. Rounds 1–7 covered `renderer.rs`, `backend.rs`, `dock.rs`, `dashed_block.rs`, `scene.rs`, `command.rs`, `video.rs`, `gpu.rs`, `animation.rs`, `render.rs`, `date_time_block.rs`, `draw.rs`, `texture.rs`, `layout/*`, `interaction/*`, `batching.rs`, `clip.rs`, `frame.rs`, `text_render.rs`, `geometry.rs`, `color.rs`, `brush.rs`, `image.rs`, `gif.rs`, `transform.rs`, `viewport.rs`, `surface.rs`, `error.rs`, `lib.rs`, `physics/*`, `shaders.rs`, `wayland.rs`, `morphing.rs`, `primitives.rs`, `perf.rs`, `fontdb.rs`, `glyph.rs`, `math_utils.rs`, `monitor.rs`, and `shaping.rs` (issues 1–199). This round covers `slint.rs`, `text.rs`, `layout.rs` (root), `physics/visual_effects.rs`, `src/app/icon_load.rs`, `src/app/draw.rs`, and `src/diagnostics/gpu_validation.rs` (issues 200–240).

---

## Critical Issues

### 200. slint.rs SlintWgpuAdapter doesn't share wgpu device/queue (slint.rs:830-848)
**File:** `wgpu-api/src/slint.rs`  
**Lines:** 830-848

```rust
pub fn select_backend(&mut self) -> Result<(), String> {
    #[cfg(feature = "slint-wgpu-30")]
    {
        use slint::wgpu_30::WGPUConfiguration;
        use slint::BackendSelector;

        BackendSelector::new()
            .require_wgpu_30(WGPUConfiguration::default())
            .select()
            .map_err(|e| format!("Failed to select Slint wgpu backend: {:?}", e))?;
```

The documentation claims "Slint renders into a shared wgpu rendering context, potentially avoiding an additional GPU composition step." However, `WGPUConfiguration::default()` creates an independent wgpu instance, not a shared one. The `WGPUConfiguration::Manual` variant exists for sharing a device/queue, but it's never used. The `new()` function takes `&Renderer` as a parameter but ignores it entirely (prefixed with `_`).

**Impact:** Slint creates its own wgpu device and queue, doubling GPU resource usage. The "shared context" mode (Option B) described in the module documentation is not implemented. Two independent wgpu instances compete for GPU memory and driver resources.

**Fix:** Use `WGPUConfiguration::Manual` to share the renderer's device and queue with Slint, or remove the misleading documentation about shared contexts.

---

### 201. slint.rs DockWidget::hit_test() doesn't account for scale factor (slint.rs:325-335)
**File:** `wgpu-api/src/slint.rs`  
**Lines:** 325-335

```rust
pub fn hit_test(&self, point: Point) -> Option<i32> {
    for (i, item) in self.dock_items.iter().enumerate() {
        let x = self.item_x(i as i32) as f32 + item.offset_x;
        let y = 0.0;
        let rect = Rect::new(x, y, self.icon_size as f32, self.dock_height as f32);
        if rect.contains(&point) {
            return Some(i as i32);
        }
    }
    None
}
```

The hit test uses `self.icon_size` (logical pixels, e.g., 48) for the rect width, but the `point` parameter may be in physical pixels if the caller doesn't convert. At 2x scale, a click at physical pixel (100, 30) would be tested against a rect at logical x=56 with width 48, which maps to physical x=112 with width 96. The hit test would fail because the coordinate spaces don't match.

**Impact:** Hit testing fails at non-1.0 scale factors. Clicking on an icon at 2x scale may miss or hit the wrong icon.

**Fix:** Document the coordinate space of `point` and add a `scale_factor` parameter, or convert the point to logical pixels before testing.

---

### 202. slint.rs DockWidget::hit_test() ignores animated icon position (slint.rs:327)
**File:** `wgpu-api/src/slint.rs`  
**Line:** 327

```rust
let x = self.item_x(i as i32) as f32 + item.offset_x;
```

The hit test only accounts for `offset_x` (drag animation), but ignores `icon_x` and `icon_y` (scale animation offset). When `anim_scale` changes, the icon is repositioned by `icon_x` and `icon_y` to stay centered. The hit test doesn't include these offsets.

**Impact:** During scale animation, the hit test rect doesn't match the rendered icon position. Clicking near the edge of a scaled icon may miss.

**Fix:** Include `item.icon_x` in the hit test rect calculation.

---

### 203. src/app/draw.rs — window_ids rebuilt and sorted every frame (draw.rs:72-86)
**File:** `src/app/draw.rs`  
**Lines:** 72-86

```rust
let mut window_ids: Vec<(String, u32)> = self
    .windows
    .iter()
    .filter_map(|(id, win)| {
        if win.title.is_empty() {
            None
        } else {
            Some((win.title.clone(), id.0.protocol_id()))
        }
    })
    .collect();
window_ids.sort_by(|(a_title, a_id), (b_title, b_id)| {
    a_title.cmp(b_title).then(a_id.cmp(b_id))
});
pw.update_window_ids(&window_ids);
```

Every frame, all window titles are cloned into a new Vec, then sorted. With 50 windows, this allocates 50 String clones per frame. At 60 FPS, that's 3,000 String allocations per second. The `update_window_ids` call likely replaces the entire internal map.

**Impact:** Per-frame heap allocations in the hot path violate Core Engineering Priority #2 (lowest possible system RAM usage). String cloning is expensive.

**Fix:** Cache the window_ids map and only update when windows change. Use a dirty flag set by window add/remove events.

---

### 204. src/app/draw.rs — canonical_app_ids rebuilt every frame (draw.rs:93-112)
**File:** `src/app/draw.rs`  
**Lines:** 93-112

```rust
let canonical_app_ids: HashMap<String, String> = self
    .windows
    .values()
    .map(|win| {
        let raw_id = win
            .app_id
            .as_ref()
            .map(|id| id.to_string())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                if !win.title.is_empty() {
                    win.title.clone()
                } else {
                    "Unknown".to_string()
                }
            });
        let canonical = self.canonical_running_app_id(&raw_id);
        (raw_id, canonical)
    })
    .collect();
```

Every frame, a new HashMap is allocated and populated by iterating all windows. Each window's app_id is cloned to a String. The `canonical_running_app_id` method is called for every window every frame.

**Impact:** Per-frame HashMap allocation and String cloning. With 50 windows, this is 50 String allocations + 50 HashMap insertions per frame. At 60 FPS, that's 3,000 HashMap insertions per second.

**Fix:** Cache the canonical_app_ids map and invalidate only when pinned_apps or windows change.

---

### 205. src/app/draw.rs — running_by_app rebuilt every frame per dock (draw.rs:142-164)
**File:** `src/app/draw.rs`  
**Lines:** 142-164

```rust
let mut running_by_app: HashMap<String, Vec<&WindowState>> = HashMap::new();
for win in &ordered_windows {
    let raw_id = win
        .app_id
        .as_ref()
        .map(|id| id.to_string())
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| {
            if !win.title.is_empty() {
                win.title.clone()
            } else {
                "Unknown".to_string()
            }
        });
    let id = canonical_app_ids.get(&raw_id).cloned().unwrap_or(raw_id);
    running_by_app.entry(id).or_default().push(*win);
}
```

For each dock instance, a new HashMap is built from scratch. Each window's app_id is cloned. Then each group is sorted by title. This is O(n) HashMap construction + O(n log n) sorting per dock per frame.

**Impact:** Massive per-frame allocations. With 2 docks and 50 windows, this is 100 String clones + 2 HashMap builds + 2 sorts per frame.

**Fix:** Incrementally update running_by_app when windows are added/removed. Use a dirty flag.

---

## High Severity Issues

### 206. slint.rs SlintWgpuAdapter::new() ignores renderer parameter (slint.rs:815)
**File:** `wgpu-api/src/slint.rs`  
**Line:** 815

```rust
pub fn new(_renderer: &crate::renderer::Renderer) -> Result<Self, String> {
    // The adapter is a marker type — the actual integration happens
    // via BackendSelector::require_wgpu_30() which we call in select_backend().
    // The renderer's device/queue are shared with Slint via WGPUConfiguration::Manual.
    Ok(Self { initialized: false })
}
```

The `_renderer` parameter is ignored. The comment claims "the renderer's device/queue are shared with Slint via WGPUConfiguration::Manual" but the code uses `WGPUConfiguration::default()`. The renderer reference is never stored.

**Impact:** The adapter cannot share GPU resources with Slint. The function signature is misleading — it accepts a renderer but doesn't use it.

**Fix:** Either store the renderer reference and use `WGPUConfiguration::Manual`, or remove the renderer parameter.

---

### 207. slint.rs WidgetRegistry::unregister_component() doesn't clean up textures (slint.rs:945-947)
**File:** `wgpu-api/src/slint.rs`  
**Lines:** 945-947

```rust
pub fn unregister_component(&mut self, id: WidgetId) {
    self.components.remove(&id);
}
```

When a component is unregistered, its `SlintComponent` is removed from the HashMap, but if the component has an associated `TextureId`, the GPU texture is never released. The texture remains allocated in VRAM.

**Impact:** VRAM leak — unregistering a component leaves its GPU texture orphaned. Over time, repeated register/unregister cycles accumulate orphaned textures.

**Fix:** Before removing the component, check if it has a texture and release it through the backend's `remove_texture()` method.

---

### 208. text.rs Text::estimate_width() uses hardcoded 0.6 factor (text.rs:259-263)
**File:** `wgpu-api/src/text.rs`  
**Lines:** 259-263

```rust
pub fn estimate_width(&self) -> f32 {
    let char_count = self.content.chars().count() as f32;
    let avg_char_width = self.style.font_size * 0.6;
    let letter_spacing = self.style.letter_spacing * char_count.max(0.0);
    char_count * avg_char_width + letter_spacing
}
```

The average character width is estimated as `font_size * 0.6`, which assumes a proportional Latin font. This is incorrect for monospace fonts (where char width ≈ font_size), CJK fonts (where char width ≈ font_size), and emoji (which may be wider). The `estimate_width` is used by `wrap_lines()` to determine line breaks.

**Impact:** Text wrapping is incorrect for non-Latin scripts and monospace fonts. CJK text wraps too early (0.6× the actual width). Monospace text wraps at ~60% of available space.

**Fix:** Use font metrics from the actual font file to estimate character width, or provide a per-font average width factor.

---

### 209. text.rs Text::wrap_lines() creates temporary Text objects per word (text.rs:290-295)
**File:** `wgpu-api/src/text.rs`  
**Lines:** 290-295

```rust
let test_style = self.style.clone();
let test_text = Text {
    content: test_line.clone(),
    style: test_style,
    ..self.clone()
};
if test_text.estimate_width() <= max_w {
```

For every word in the text, a new `Text` object is created by cloning the entire style and content. This allocates a new String and clones the TextStyle (which contains an optional Font with a path String). Then `estimate_width()` is called, which only needs the char count and font size.

**Impact:** Excessive allocations during text wrapping. For a 100-word paragraph, this creates 100 temporary Text objects with cloned Strings.

**Fix:** Compute the estimate inline without creating a Text object. Pass the char count and font size directly to a helper function.

---

### 210. text.rs TextStyle shadow is defined but never rendered (text.rs:138)
**File:** `wgpu-api/src/text.rs`  
**Line:** 138

```rust
/// Text shadow offset and color.
pub shadow: Option<(Point, Color, f32)>,
```

The `shadow` field stores offset, color, and blur radius. The `with_shadow()` builder method exists. But the GPU backend's text rendering path (`DrawCommand::Text` in `draw_commands_to_gpu_impl`) never reads or applies the shadow. Text shadows are invisible on the GPU path.

**Impact:** Text shadows defined in TextStyle are silently ignored on GPU rendering. The CPU/Slint path may render them correctly, creating a visual mismatch.

**Fix:** Implement text shadow rendering in the GPU backend, or document that shadows are not supported on the GPU path.

---

### 211. visual_effects.rs VisualTransform::apply_position() applies scale incorrectly (visual_effects.rs:145-149)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 145-149

```rust
pub fn apply_position(&self, base: Point) -> Point {
    Point::new(
        base.x * self.scale.width + self.translation.x,
        base.y * self.scale.height + self.translation.y,
    )
}
```

This scales the base position, then adds translation. But the correct transform for rendering is: first translate, then scale relative to the center. The current formula scales the position coordinates themselves, which distorts the position for non-origin bases. For example, a base at (100, 100) with scale (1.5, 1.5) and translation (0, 0) becomes (150, 150) — the position moves away from the origin, which is incorrect for a scale transform centered on the object.

**Impact:** Visual transforms applied to positions produce incorrect results. Objects scale away from the origin instead of scaling in place.

**Fix:** Use `Point::new(base.x + self.translation.x, base.y + self.translation.y)` for translation-only, or implement proper center-based scaling.

---

### 212. visual_effects.rs VisualTransform::compose() doesn't handle rotation correctly (visual_effects.rs:153-165)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 153-165

```rust
pub fn compose(&self, other: &VisualTransform) -> Self {
    Self {
        translation: Point::new(
            self.translation.x * other.scale.width + other.translation.x,
            self.translation.y * other.scale.height + other.translation.y,
        ),
        scale: Size::new(
            self.scale.width * other.scale.width,
            self.scale.height * other.scale.height,
        ),
        rotation: self.rotation + other.rotation,
    }
}
```

The composition adds rotations and multiplies scales, but doesn't account for rotation affecting the translation or scale axes. When `other` has a non-zero rotation, `self.translation` should be rotated by `other.rotation` before being scaled and translated. The current formula treats rotation as independent, which is only correct when one of the rotations is zero.

**Impact:** Composing transforms with rotation produces incorrect results. The translation is not rotated, so the final position is wrong.

**Fix:** Rotate `self.translation` by `other.rotation` before applying scale and translation. Rotate scale axes if non-uniform scaling is involved.

---

### 213. visual_effects.rs collision_effects uses linear search by BodyId (visual_effects.rs:447-455, 460-462)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 447-455, 460-462

```rust
if let Some((_, existing)) = self
    .collision_effects
    .iter_mut()
    .find(|(id, _)| *id == body)
```

Every collision lookup iterates the entire `collision_effects` vector. With N bodies and M collision effects, each lookup is O(M). The `register_collision`, `compute_collision_squash`, and `get_collision_glow` methods all do linear searches.

**Impact:** O(n) lookup for collision effects. With many bodies, this becomes a performance bottleneck in the visual effects update path.

**Fix:** Use a `HashMap<BodyId, CollisionEffect>` or a `Hashbrown` hash map for O(1) lookups.

---

### 214. visual_effects.rs drag_states uses linear search by BodyId (visual_effects.rs:570-576, 588-590)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 570-576, 588-590

```rust
if let Some((_, state)) = self.drag_states.iter_mut().find(|(id, _)| *id == body)
```

Same issue as #213 — linear search through `drag_states` vector for every drag-related operation.

**Impact:** O(n) lookup for drag states. Compounds with collision effect lookups.

**Fix:** Use a `HashMap<BodyId, DragVisualState>`.

---

### 215. visual_effects.rs glow and shadow effects are computed but never rendered (visual_effects.rs:650-675)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 650-675

The `compute_glow_alpha` and `compute_shadow_elevation` methods compute visual effect values, but there's no corresponding GPU rendering path. The GPU backend's `draw_commands_to_gpu_impl` doesn't have a glow or shadow effect for icons. The `DrawCommand` enum doesn't include glow or shadow parameters.

**Impact:** The visual effects system computes glow and shadow values that are never used. The physics-to-visual pipeline is incomplete — the visual properties exist but have no rendering backend.

**Fix:** Add glow and shadow support to `DrawCommand` and the GPU backend, or document that these effects are not yet rendered.

---

### 216. src/app/icon_load.rs — background thread blocks on image decode (icon_load.rs:37-38)
**File:** `src/app/icon_load.rs`  
**Lines:** 37-38

```rust
if let Some((width, _height, rgba)) =
    crate::terminal_graphics::load_image_raw_rgba(path, ICON_SOURCE_SIZE)
```

The background thread calls `load_image_raw_rgba` synchronously, which decodes the image (PNG/SVG) on the thread. SVG decoding in particular can be slow, as it requires rasterization. The single-threaded design means only one icon is loaded at a time.

**Impact:** Icon loading is serialized. If 10 icons need to be loaded, they're loaded one-by-one, blocking the background thread. SVG icons take longer to decode than PNG.

**Fix:** Use a thread pool for parallel icon loading, or use an async decoder.

---

### 217. src/app/icon_load.rs — no deduplication of concurrent requests (icon_load.rs:67-69)
**File:** `src/app/icon_load.rs`  
**Lines:** 67-69

```rust
pub fn request(&self, app_id: String) {
    let _ = self.tx.send(IconLoadRequest { app_id });
}
```

If the same `app_id` is requested multiple times before the first request completes, the icon will be loaded multiple times. The `mpsc::channel` has no deduplication. The caller (`draw.rs:42-44`) checks `icon_cache.contains_key`, but there's a race: the check passes, the request is sent, but before the result arrives, another frame checks and sends another request.

**Impact:** Duplicate icon loads waste CPU and bandwidth. The result channel receives duplicate results for the same app_id.

**Fix:** Track pending requests in a `HashSet<String>` and skip duplicates. Remove from the set when the result arrives.

---

## Medium Severity Issues

### 218. layout.rs Shadow type is defined but not used in GPU rendering (layout.rs:274-341)
**File:** `wgpu-api/src/layout.rs`  
**Lines:** 274-341

The `Shadow` struct defines `x`, `y`, `blur`, `spread`, and `color`. The `expansion()` method computes the bounding box expansion. But the GPU backend never renders shadows. The `DrawCommand::Rectangle` doesn't have a shadow field. The `Shadow` type is a data-only struct with no rendering implementation.

**Impact:** Shadow configuration is silently ignored on the GPU path. UI elements that should have shadows render flat.

**Fix:** Implement shadow rendering in the GPU backend (e.g., as a blurred rectangle behind the main shape), or remove the type if shadows are not planned.

---

### 219. layout.rs CornerRadius is defined but not used in GPU rendering (layout.rs:168-272)
**File:** `wgpu-api/src/layout.rs`  
**Lines:** 168-272

The `CornerRadius` struct provides per-corner radius control. But the GPU backend's `draw_commands_to_gpu_impl` ignores `radius` on `DrawCommand::Rectangle` (as noted in Round 1, issue #3). The `CornerRadius` type has no connection to the GPU rendering path.

**Impact:** Rounded corners are not rendered on GPU. All rectangles render as sharp-cornered.

**Fix:** Implement rounded rectangle rendering in the GPU backend using a SDF (signed distance field) approach or tessellated vertices.

---

### 220. src/app/draw.rs — is_animating hardcoded to false (draw.rs:121)
**File:** `src/app/draw.rs`  
**Line:** 121

```rust
// All animations disabled — is_animating always false
let is_animating = false;
```

The comment says "All animations disabled." The `is_animating` flag is passed to `draw_main_dock`, which likely uses it to determine whether to render animation frames. With it hardcoded to `false`, no animations trigger redraws.

**Impact:** Animations are completely disabled. The dock never renders animation frames, even when bounce or click animations are active.

**Fix:** Compute `is_animating` from the actual animation state (e.g., `self.bounce_app_id.is_some() || self.click_app_id.is_some()`).

---

### 221. src/app/draw.rs — first_frame_status_pending always false (draw.rs:114, 332-334)
**File:** `src/app/draw.rs`  
**Lines:** 114, 332-334

```rust
let first_frame_status_pending = false;
// ...
if first_frame_status_pending {
    self.needs_redraw = true;
}
```

The variable is hardcoded to `false`, so the `if` block at line 332 is dead code. The `needs_redraw` is never set by this path.

**Impact:** Dead code. The first-frame status pending logic never triggers a redraw.

**Fix:** Remove the dead code or implement the logic correctly.

---

### 222. src/app/draw.rs — canonical_app_ids and raw_id duplication (draw.rs:93-112, 142-157)
**File:** `src/app/draw.rs`  
**Lines:** 93-112, 142-157

The `raw_id` computation (lines 144-155) duplicates the same logic from lines 97-108. Both blocks check `app_id`, fall back to `title`, and then to `"Unknown"`. This logic appears in at least 3 places across the codebase.

**Impact:** Code duplication increases maintenance burden and risk of inconsistency. If the fallback logic changes, all copies must be updated.

**Fix:** Extract a `WindowState::effective_app_id()` method and use it everywhere.

---

### 223. gpu_validation.rs — GpuValidator doesn't integrate with wgpu error scopes (gpu_validation.rs:38-105)
**File:** `src/diagnostics/gpu_validation.rs`  
**Lines:** 38-105

The documentation shows `push_error_scope()` and `pop_error_scope()` methods, but the `GpuValidator` struct only has `record_error()` and `record_warning()`. There are no error scope methods. The validator is a simple counter, not an actual wgpu error scope monitor.

**Impact:** The GPU validator cannot detect wgpu errors automatically. It requires manual `record_error()` calls, which defeats the purpose of automated error detection.

**Fix:** Implement `push_error_scope()` and `pop_error_scope()` using `wgpu::Device::push_error_scope()` and `pop_error_scope()`, or rename the struct to reflect its actual capabilities.

---

### 224. gpu_validation.rs — GpuValidationError is never constructed (gpu_validation.rs:24-31)
**File:** `src/diagnostics/gpu_validation.rs`  
**Lines:** 24-31

The `GpuValidationError` struct is defined with `label`, `message`, and `error_count` fields, but it's never instantiated anywhere in the code. The `GpuValidator` uses `record_error()` with string parameters, not `GpuValidationError` instances.

**Impact:** Dead code. The struct exists but is never used.

**Fix:** Remove the struct or use it in the error recording path.

---

### 225. gpu_validation.rs — UncapturedErrorHandler is never installed (gpu_validation.rs:121-145)
**File:** `src/diagnostics/gpu_validation.rs`  
**Lines:** 121-145

The `UncapturedErrorHandler` has a `handle()` method, but there's no `install()` method that calls `wgpu::set_uncaptured_error_handler()`. The handler is created but never connected to wgpu's error reporting system.

**Impact:** Uncaptured GPU errors are not handled. The handler exists but is never invoked by wgpu.

**Fix:** Add an `install()` method that calls `wgpu::set_uncaptured_error_handler()` with a closure that delegates to the handler.

---

### 226. slint.rs DashConfig::dash_positions() allocates Vec per call (slint.rs:417-427)
**File:** `wgpu-api/src/slint.rs`  
**Lines:** 417-427

```rust
pub fn dash_positions(&self, window_count: i32) -> Vec<f32> {
    let count = window_count.min(5).max(1) as i32;
    let dash_w = self.dash_width(count);
    let mut positions = Vec::with_capacity(count as usize);
    let mut x = self.x_offset;
    for _ in 0..count {
        positions.push(x);
        x += dash_w + self.gap;
    }
    positions
}
```

Every call allocates a new Vec. Since window_count is capped at 5, the maximum size is 5 elements. This is called per icon per frame.

**Impact:** Per-frame allocation for dash positions. With 20 icons, that's 20 Vec allocations per frame.

**Fix:** Return a fixed-size array `[f32; 5]` with a count, or use `SmallVec` to avoid heap allocation for small counts.

---

### 227. slint.rs BluetoothState adapter_name is hardcoded (slint.rs:582)
**File:** `wgpu-api/src/slint.rs`  
**Line:** 582

```rust
adapter_name: "archlinux".to_string(),
```

The default adapter name is hardcoded to "archlinux". This should be read from the system's Bluetooth adapter name.

**Impact:** The Bluetooth widget shows "archlinux" as the adapter name on all systems, regardless of the actual hostname.

**Fix:** Read the hostname from `/etc/hostname` or use `hostname::get()` at initialization.

---

### 228. visual_effects.rs GroupVisualEffects thresholds are hardcoded (visual_effects.rs:743-746)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 743-746

```rust
Self {
    show_connecting_lines: avg_speed > 200.0 || max_deformation > 0.4,
    line_alpha: (avg_speed / 500.0).min(0.3),
    shared_motion_blur: (avg_speed / 200.0).min(0.5),
    collective_glow: (max_deformation * 0.5_f32).min(0.3_f32),
}
```

The thresholds (200.0, 0.4, 500.0, 0.3, etc.) are hardcoded magic numbers with no configuration. These values were tuned for a specific scenario and may not work for different dock sizes or animation speeds.

**Impact:** Group visual effects may trigger too early or too late depending on the dock configuration. No way to tune the behavior.

**Fix:** Move these thresholds to `VisualEffectsConfig` with documented defaults.

---

## Low Severity Issues

### 229. slint.rs DockItem uses i32 for app_id and index_on_dock (slint.rs:114-116)
**File:** `wgpu-api/src/slint.rs`  
**Lines:** 114-116

```rust
/// Application ID.
pub app_id: i32,
/// Index position on the dock.
pub index_on_dock: i32,
```

Using `i32` for an ID and index is unusual. `app_id` should be a `String` (matching the app_id used elsewhere in the codebase). `index_on_dock` should be `usize` since it's a non-negative index.

**Impact:** Type mismatch with the rest of the codebase, which uses `String` for app_id. Numeric conversion errors possible.

**Fix:** Use `String` for `app_id` and `usize` for `index_on_dock`.

---

### 230. slint.rs DockItem badge_color is a hex string (slint.rs:110)
**File:** `wgpu-api/src/slint.rs`  
**Line:** 110

```rust
/// Badge color (hex string, e.g., "#dc2626").
pub badge_color: String,
```

The badge color is stored as a hex string, requiring parsing every time it's used. The `Color` type exists and should be used directly.

**Impact:** String parsing overhead for badge color. No compile-time type safety.

**Fix:** Use `Color` type instead of `String`.

---

### 231. text.rs Text::estimate_width() doesn't handle empty string correctly (text.rs:262)
**File:** `wgpu-api/src/text.rs`  
**Line:** 262

```rust
let letter_spacing = self.style.letter_spacing * char_count.max(0.0);
```

`char_count` is always non-negative (it's cast from `usize`), so `.max(0.0)` is a no-op. For an empty string, `char_count` is 0, and the result is 0.0, which is correct. But the `.max(0.0)` suggests the author expected negative values, which is impossible.

**Impact:** No functional impact, but the code is misleading.

**Fix:** Remove the `.max(0.0)` call.

---

### 232. text.rs Text::wrap_lines() doesn't handle consecutive whitespace (text.rs:284)
**File:** `wgpu-api/src/text.rs`  
**Line:** 284

```rust
for word in paragraph.split_whitespace() {
```

`split_whitespace()` collapses all whitespace, including multiple spaces, tabs, and newlines. Consecutive spaces in the original text are lost.

**Impact:** Text with intentional multiple spaces (e.g., indentation, formatting) loses its spacing.

**Fix:** Use a word-wrapping algorithm that preserves whitespace, or document that whitespace is normalized.

---

### 233. layout.rs Insets::is_empty() uses <= 0.0, not == 0.0 (layout.rs:108)
**File:** `wgpu-api/src/layout.rs`  
**Line:** 108

```rust
pub fn is_empty(&self) -> bool {
    self.top <= 0.0 && self.right <= 0.0 && self.bottom <= 0.0 && self.left <= 0.0
}
```

Negative insets are considered "empty." This is unusual — negative insets represent outward expansion (outset), not emptiness. A `Insets::new(-5.0, -5.0, -5.0, -5.0)` would return `true` for `is_empty()`.

**Impact:** Negative insets are incorrectly treated as empty. Code that checks `is_empty()` before applying insets may skip valid outward expansion.

**Fix:** Use `== 0.0` or add an epsilon for floating-point comparison.

---

### 234. visual_effects.rs MotionState::is_at_rest() threshold uses length_squared for velocity but not for angular_velocity (visual_effects.rs:112-116)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 112-116

```rust
pub fn is_at_rest(&self, threshold: f32) -> bool {
    self.velocity.length_squared() < threshold
        && self.acceleration.length_squared() < threshold
        && self.angular_velocity.abs() < threshold
}
```

The threshold is compared against `length_squared()` for velocity and acceleration, but against the raw value for `angular_velocity`. This means the threshold has different units for different checks. A threshold of `0.01` means `speed² < 0.01` (speed < 0.1) but `|angular_velocity| < 0.01`.

**Impact:** The threshold parameter is confusing — it doesn't represent the same physical quantity for all checks. Setting the threshold to account for squared values makes the angular velocity check too strict.

**Fix:** Use `length()` for all checks, or document that the threshold is in squared units for linear quantities.

---

### 235. visual_effects.rs CollisionEffect decay formula is Euler approximation (visual_effects.rs:254-262)
**File:** `wgpu-api/src/physics/visual_effects.rs`  
**Lines:** 254-262

```rust
pub fn decay(&mut self, dt: f32, decay_rate: f32) {
    if !self.active {
        return;
    }
    self.intensity *= (1.0 - decay_rate * dt).max(0.0);
```

The decay uses a first-order Euler approximation: `intensity *= (1 - rate * dt)`. The exact solution for exponential decay is `intensity *= (-rate * dt).exp()`. For large `dt` values, the Euler approximation can produce negative values (clamped to 0.0) or inaccurate decay.

**Impact:** For large timesteps, the decay is inaccurate. The `.max(0.0)` clamp hides the Euler instability.

**Fix:** Use `self.intensity *= (-decay_rate * dt).exp()` for accurate exponential decay.

---

### 236. src/app/draw.rs — box_size and spacing hardcoded (draw.rs:37-38)
**File:** `src/app/draw.rs`  
**Lines:** 37-38

```rust
let box_size = 48;
let spacing = 8;
```

These values are hardcoded in `draw()` but should come from `DockConfig`. The config likely has `icon_size` and `item_spacing` fields, but they're not used here.

**Impact:** Changing the config's icon size or spacing doesn't affect the draw frame. The hardcoded values override configuration.

**Fix:** Read `box_size` and `spacing` from `self.config`.

---

### 237. src/app/draw.rs — ordered_windows sorted by order_index (draw.rs:140)
**File:** `src/app/draw.rs`  
**Line:** 140

```rust
ordered_windows.sort_by_key(|win| win.order_index);
```

Windows are sorted by `order_index`, but the `running_by_app` groups are then sorted by title (line 163). This means the window order within an app group is by title, not by `order_index`. The two sorts may conflict with user expectations.

**Impact:** Windows within the same app are ordered alphabetically by title, not by the user's focus order or z-order.

**Fix:** Preserve `order_index` sorting within app groups, or document that title sorting takes precedence.

---

### 238. gpu_validation.rs — uses SeqCst ordering for all atomics (gpu_validation.rs:54, 66, 78, 83, 93, 94)
**File:** `src/diagnostics/gpu_validation.rs`  
**Lines:** 54, 66, 78, 83, 93, 94

All atomic operations use `Ordering::SeqCst`, the strongest ordering. For simple counters that don't need synchronization with other data, `Relaxed` ordering would suffice and be faster.

**Impact:** Unnecessary memory fence overhead on every error/warning record and count load. On weakly-ordered architectures (ARM), this is measurable.

**Fix:** Use `Ordering::Relaxed` for counters that don't synchronize with other data.

---

### 239. slint.rs WidgetRegistry uses AtomicU64 with SeqCst (slint.rs:928)
**File:** `wgpu-api/src/slint.rs`  
**Line:** 928

```rust
let id = WidgetId(self.next_widget_id.fetch_add(1, Ordering::SeqCst));
```

Widget ID generation uses `SeqCst` ordering. Since widget IDs are monotonically increasing and don't need synchronization with other data, `Relaxed` ordering is sufficient.

**Impact:** Unnecessary memory fence on every widget registration.

**Fix:** Use `Ordering::Relaxed`.

---

### 240. src/app/icon_load.rs — ICON_SOURCE_SIZE hardcoded to 48 (icon_load.rs:36)
**File:** `src/app/icon_load.rs`  
**Line:** 36

```rust
const ICON_SOURCE_SIZE: u32 = 48;
```

The icon source size is hardcoded to 48px. If the dock configuration uses a different icon size (e.g., 64px), the icons will be loaded at 48px and scaled up, resulting in blurry icons.

**Impact:** Icons are always loaded at 48px regardless of the configured icon size. Higher-resolution displays with larger icon sizes will show blurry icons.

**Fix:** Make `ICON_SOURCE_SIZE` configurable or read it from the dock config.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 200 | **Critical** | SlintWgpuAdapter doesn't share wgpu device | Two independent wgpu instances |
| 201 | **Critical** | DockWidget hit_test ignores scale factor | Clicks miss at non-1.0 scale |
| 202 | **Critical** | DockWidget hit_test ignores animated position | Clicks miss during animation |
| 203 | **Critical** | window_ids rebuilt every frame | 3,000 String allocs/sec |
| 204 | **Critical** | canonical_app_ids rebuilt every frame | 3,000 HashMap insertions/sec |
| 205 | **Critical** | running_by_app rebuilt every frame | 100 String clones per frame |
| 206 | **High** | SlintWgpuAdapter ignores renderer param | Can't share GPU resources |
| 207 | **High** | unregister_component doesn't clean textures | VRAM leak on unregister |
| 208 | **High** | estimate_width uses hardcoded 0.6 factor | Wrong wrapping for CJK/monospace |
| 209 | **High** | wrap_lines creates Text per word | Excessive allocations |
| 210 | **High** | TextStyle shadow never rendered | Visual mismatch GPU vs CPU |
| 211 | **High** | apply_position scales incorrectly | Objects scale from origin |
| 212 | **High** | compose doesn't handle rotation | Wrong composed transforms |
| 213 | **High** | collision_effects linear search | O(n) lookup per body |
| 214 | **High** | drag_states linear search | O(n) lookup per body |
| 215 | **High** | Glow/shadow computed but not rendered | Incomplete visual pipeline |
| 216 | **Medium** | Icon load blocks on decode | Serialized icon loading |
| 217 | **Medium** | No deduplication of icon requests | Duplicate loads |
| 218 | **Medium** | Shadow type not used in GPU | Shadows invisible on GPU |
| 219 | **Medium** | CornerRadius not used in GPU | Rounded corners invisible |
| 220 | **Medium** | is_animating hardcoded false | Animations disabled |
| 221 | **Medium** | first_frame_status_pending dead code | Dead code path |
| 222 | **Medium** | raw_id logic duplicated 3x | Maintenance burden |
| 223 | **Medium** | GpuValidator no error scopes | Can't auto-detect GPU errors |
| 224 | **Medium** | GpuValidationError never constructed | Dead code |
| 225 | **Medium** | UncapturedErrorHandler never installed | Uncaptured errors lost |
| 226 | **Medium** | dash_positions allocates Vec | 20 Vec allocs/frame |
| 227 | **Medium** | adapter_name hardcoded | Wrong name on non-Arch |
| 228 | **Medium** | GroupVisualEffects thresholds hardcoded | No tuning possible |
| 229 | **Low** | DockItem app_id is i32 | Type mismatch |
| 230 | **Low** | badge_color is hex string | Parsing overhead |
| 231 | **Low** | char_count.max(0.0) is no-op | Misleading code |
| 232 | **Low** | wrap_lines collapses whitespace | Lost formatting |
| 233 | **Low** | is_empty treats negative as empty | Wrong for outset |
| 234 | **Low** | is_at_rest mixes units | Confusing threshold |
| 235 | **Low** | CollisionEffect Euler decay | Inaccurate for large dt |
| 236 | **Low** | box_size/spacing hardcoded | Config ignored |
| 237 | **Low** | Window sort order conflict | Unexpected ordering |
| 238 | **Low** | SeqCst on all atomics | Unnecessary fences |
| 239 | **Low** | SeqCst on widget ID | Unnecessary fence |
| 240 | **Low** | ICON_SOURCE_SIZE hardcoded | Blurry at larger sizes |

---

## Cumulative Summary

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 1 | 2 | 2 | 3 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 3 |
| 3 | gpu-rendering-review-3.md | 33–60 | 5 | 6 | 8 | 7 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 13 | 21 | 7 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 12 | 11 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| **8** | **gpu-rendering-review-8.md** | **200–240** | **6** | **12** | **11** | **12** |
| **Total** | | **240** | — | — | — | — |

The earlier rounds' severity subtotals do not consistently add up to their numbered findings, so cumulative severity totals are intentionally omitted rather than repeated as precise counts.

## Round 8 Verification

The findings were checked against the current implementation. Confirmed defects were fixed; several findings describe intentional subsystem boundaries or unsupported features rather than implementation bugs.

| Findings | Disposition |
|----------|-------------|
| 200, 206 | Slint and `wgpu-api` intentionally use independent devices because the renderer owns private backend resources. Corrected the module diagram and adapter docs; the adapter no longer accepts an unused renderer parameter. |
| 201–202 | Fixed hit testing to use logical coordinates, account for animated icon bounds, and provide an explicit physical-pixel helper. |
| 203–205 | Window IDs and running-app grouping are cached and invalidated with dock/app state changes; frame data now passes IDs rather than rebuilding per-dock `WindowState` references. Configured icon size and spacing are used. |
| 207 | Unregistering returns the associated `TextureId`; the caller that owns the renderer remains responsible for releasing it. Automatic GPU resource destruction is outside this registry's ownership. |
| 208, 210, 215, 218–219 | Approximate text measurement and missing shadow/glow/rounded-corner rendering remain limitations; these are not silently represented as implemented. |
| 209, 211–214 | Removed temporary text/style clones, corrected transform application/composition semantics, and changed collision/drag effect lookup to maps. |
| 216–217 | Icon decoding runs on the background loader thread; pending requests are already deduplicated by app state. The single worker can serialize decode throughput but does not block the UI thread. |
| 220–221 | Animation scheduling now follows pending triggers and active Slint/GPU animations; removed the dead first-frame redraw branch. |
| 222 | Consolidated repeated raw window app-ID fallback logic in one helper. |
| 223, 225 | The validator and uncaptured-error wrapper do not install wgpu scopes/callbacks because they do not own a device; clarified that boundary in API docs. |
| 224 | `record_error` now returns the populated `GpuValidationError`. |
| 226 | Added a fixed-storage dash-position API; the allocating convenience method remains available but has no hot-path call sites. |
| 227–228 | Replaced the host-specific adapter label with a generic default. Visual-effect thresholds remain fixed policy values, not configuration knobs. |
| 229–230, 232, 237 | Integer widget identifiers, string colors, whitespace handling, and title-based stream matching are current API/behavior choices; the title ordering keeps duplicate-title PipeWire matching deterministic. |
| 231, 233–235, 238–239 | Removed the no-op width clamp, corrected negative-inset emptiness and rest/decay behavior, and relaxed independent counter/ID atomics. |
| 240 | Icon loading now receives the configured icon size instead of always using 48 px. |
