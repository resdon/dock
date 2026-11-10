# GPU Rendering Code Review — Round 4 (Texture, Layout, Interaction, Batching, Clip, Frame, Text)

This is a continuation of the GPU rendering code review. Rounds 1–3 cover issues 1–60; this round adds issues 61–95 (35 numbered findings). The descriptions and initial severity classifications below are not all confirmed. See the validation corrections at the end.

---

## Critical Issues

### 61. picking.rs depth used HashMap iteration index, and pick() never selected a body (picking.rs)
**File:** `wgpu-api/src/interaction/picking.rs`  
**Lines:** 90, 113

```rust
for (idx, (body_id, body)) in bodies.iter().enumerate() {
    // ...
    let depth = idx;  // ← HashMap iteration index
```

The `depth` used for hit testing was the **HashMap iteration index**, which is arbitrary and non-deterministic. Moreover, `best_depth` was initialized to `usize::MAX` and only replaced when `depth >= best_depth`; for any feasible collection this condition is false, so `pick()` returned `None` even for a hit.

**Status:** The defect exists in this source file, but `wgpu-api/src/interaction/` has no module declaration in `lib.rs` or a `mod.rs`; these files are not compiled or reachable in the current crate. The picker also has no visual z-order input, so ranking by a hash-map index cannot establish topmost visual order.

**Tests:** Coverage verifies a hit is selected, overlapping IDs use stable ordering, and `pick_all()` returns that order.

---

### 62. picking.rs AABB radius estimate uses diagonal, not edge distance (picking.rs:224-233)
**File:** `wgpu-api/src/interaction/picking.rs`  
**Lines:** 224-233

```rust
fn estimate_radius(body: &PhysicsBody) -> f32 {
    match body.shape {
        CollisionShape::Circle { radius } => radius,
        CollisionShape::AABB { half_extents } => {
            (half_extents.width * half_extents.width
                + half_extents.height * half_extents.height)
                .sqrt()
        }
    }
}
```

For an AABB, the "radius" is computed as the diagonal distance from center to corner. This overestimates the hit area — a point near the edge midpoint will pass the expanded hit test even though it's outside the actual shape. The correct approach is to use the distance from center to the nearest edge in the direction of the point.

**Status:** The oversized AABB expansion exists in the same uncompiled interaction source. No runtime effect in the current crate was established.

**Fix:** Use the actual `contains_point` method for the shape, or compute the correct distance from center to edge in the point's direction.

---

### 63. insertion.rs BodyId assignment decoupled from index (insertion.rs:72-73, 87-89)
**File:** `wgpu-api/src/layout/insertion.rs`  
**Lines:** 72-73, 87-89

```rust
let body_id = BodyId::new(self.next_id);  // ← increments independently
self.next_id += 1;
// ...
for i in (index + 1)..=count {
    displaced.push(BodyId::new(i as u64));  // ← assumes sequential IDs from 1
}
```

`next_id` starts at 1 and increments on each insert. But `displaced` bodies are identified using `BodyId::new(i as u64)` where `i` is the index. After one insert and one remove, `next_id` is 2, but the remaining bodies still have IDs 1, 2, 3, etc. The `displaced` list will reference wrong bodies or non-existent bodies.

Similarly, `arrangement.rs` (line 47) uses `BodyId::new((i + 1) as u64)` assuming sequential IDs starting from 1.

**Impact:** After multiple insert/remove cycles, body IDs become inconsistent. Redistribution, animation, and displacement operations target wrong bodies or miss bodies entirely.

**Fix:** Maintain a mapping from index to BodyId. Don't assume BodyId equals index+1.

---

### 64. commands.rs Redistribute has dead code — empty inner loop (commands.rs:169-188)
**File:** `wgpu-api/src/interaction/commands.rs`  
**Lines:** 169-188

```rust
CommandType::Redistribute => {
    let body_vec: Vec<PhysicsBody> = bodies.values().cloned().collect();
    let len = body_vec.len();
    let repulsion = BodyRepulsion::new(self.strength, self.radius / 2.0);
    for i in 0..len {
        for j in (i + 1)..len {
            // Can't mutably borrow twice; use IDs
        }
    }
    // Simplified: apply repulsion from center
    let repulsion = RepulsionForce::new(
        self.origin,
        self.strength * 0.5,
        self.radius,
    );
    for (_, body) in bodies.iter_mut() {
        repulsion.apply(body, dt);
    }
}
```

The code clones all bodies into a Vec, creates a `BodyRepulsion` instance, and then has an **empty inner loop** with a comment explaining why it can't work. Then it falls back to a simplified `RepulsionForce` from the center, which is not the same as mutual body repulsion.

**Impact:** The Redistribute command does not perform mutual body repulsion as intended. It pushes all bodies away from a single center point, which doesn't spread them evenly. The cloned Vec and BodyRepulsion are wasted allocations.

**Fix:** Implement the mutual repulsion properly using IDs to avoid the borrow issue, or remove the dead code and document the simplified behavior.

---

## High Severity Issues

### 65. drag.rs hardcoded thresholds ignore config (drag.rs:125, 109)
**File:** `wgpu-api/src/interaction/drag.rs`  
**Lines:** 125, 109

```rust
if !self.committed && self.drag_distance > 5.0 {  // ← hardcoded, ignores config.drag_threshold
// ...
if speed > 5000.0 {  // ← hardcoded, ignores config.max_velocity
```

`DragInfo::update()` hardcodes the drag commit threshold (5.0) and velocity clamp (5000.0), ignoring `DragConfig::drag_threshold` and `DragConfig::max_velocity` respectively. The config fields exist but are never read by the DragInfo methods.

**Impact:** Changing `DragConfig` has no effect on drag behavior. The drag threshold and velocity clamp are fixed at compile time.

**Fix:** Pass config values to `DragInfo::update()` or store a reference to the config in DragInfo.

---

### 66. drag.rs end_drag discards state update (drag.rs:237)
**File:** `wgpu-api/src/interaction/drag.rs`  
**Line:** 237

```rust
info.state = DragState::Dropping;
```

After `let info = self.drag_info.take()?;` on line 209, `info` is a local copy. Setting `info.state = DragState::Dropping` modifies the local copy, which is then dropped. The `self.drag_info` is `None` at this point. The Dropping state is never observed.

**Status:** The assignment updated only a local value after `self.drag_info.take()` had removed the active drag. No caller could observe that transition; `end_drag()` returns the body ID and clears drag state.

**Fix:** Removed the unobservable assignment; drag state is cleared when ending the drag.

---

### 67. drag.rs velocity_history uses O(n) remove(0) (drag.rs:197)
**File:** `wgpu-api/src/interaction/drag.rs`  
**Line:** 197

```rust
if self.velocity_history.len() > 10 {
    self.velocity_history.remove(0);  // ← O(n) shift
}
```

`Vec::remove(0)` shifts all elements by one position, making this O(n). While n=10 is small, this pattern appears in multiple files (commands.rs line 239, drag.rs line 197).

**Impact:** Minor performance cost per drag update. Use a circular buffer or `VecDeque` for O(1) eviction.

---

### 68. gestures.rs hardcoded thresholds ignore config (gestures.rs:257, 384)
**File:** `wgpu-api/src/interaction/gestures.rs`  
**Lines:** 257, 384

```rust
if time_since_last_tap < self.config.double_tap_interval as f64 && dist < 20.0 {  // ← 20.0 hardcoded
// ...
if distance > 10.0 && !self.state.long_press_triggered {  // ← 10.0 hardcoded
```

The double-tap distance threshold (20.0px) and pan distance threshold (10.0px) are hardcoded, ignoring any config setting. These thresholds don't scale with the device pixel ratio.

**Impact:** On high-DPI displays, 20px is very small for a double-tap tolerance. On low-DPI displays, 10px for pan may be too sensitive.

**Fix:** Add `double_tap_distance` and `pan_threshold` to `GestureConfig`.

---

### 69. gestures.rs GesturePhase test assumes default discriminant values (gestures.rs:582-585)
**File:** `wgpu-api/src/interaction/gestures.rs`  
**Lines:** 582-585

```rust
#[test]
fn test_gesture_phase() {
    assert_eq!(GesturePhase::Started as u8, 0);
    assert_eq!(GesturePhase::Updated as u8, 1);
    assert_eq!(GesturePhase::Ended as u8, 2);
    assert_eq!(GesturePhase::Cancelled as u8, 3);
}
```

Casting a fieldless enum to an integer is safe Rust. Without a representation attribute, however, its discriminants are not an ABI guarantee; the test only verifies the current definition's compiler-assigned values.

**Impact:** This is not undefined behavior. The test would be brittle only if it is intended to assert a stable external numeric representation.

**Recommendation:** Add `#[repr(u8)]` only if these numeric values are part of a serialization/FFI contract; otherwise test behavior using variants.

---

### 70. input.rs is_left_held identical to is_left_pressed (input.rs:257-265)
**File:** `wgpu-api/src/interaction/input.rs`  
**Lines:** 257-265

```rust
pub fn is_left_pressed(&self) -> bool {
    self.left_button == MouseButtonState::Pressed
}
pub fn is_left_held(&self) -> bool {
    // In a real implementation, this would track held state across frames
    self.left_button == MouseButtonState::Pressed
}
```

Both methods return the same value. The comment acknowledges this is not implemented. `MouseButtonState` is `Pressed`/`Released`, not a held state tracker. There's no way to distinguish "just pressed this frame" from "held for multiple frames."

**Impact:** Code using `is_left_held()` gets the same result as `is_left_pressed()`. No frame-delta detection for button presses.

**Fix:** Track previous button state and provide `is_left_just_pressed()` and `is_left_held()` correctly.

---

### 71. input.rs KeyInputEvent uses String for key_code (input.rs:107-117)
**File:** `wgpu-api/src/interaction/input.rs`  
**Lines:** 107-117

```rust
pub struct KeyInputEvent {
    pub key_code: String,  // ← String allocation per key event
```

Every keyboard event allocates a `String` for the key code. Key codes are a fixed, known set (ArrowUp, Enter, Escape, etc.). Using `String` means per-event heap allocation and string comparison instead of enum matching.

**Impact:** Per-key-event heap allocation. Should use a typed enum or `Key` from `wayland-client` or `winit`.

---

### 72. batching.rs stroke renders as filled rect, not an outline (batching.rs:233-265)
**File:** `wgpu-api/src/batching.rs`  
**Lines:** 233-265

```rust
// Stroke as a separate command if present
if let Some(stroke_color) = stroke {
    if *stroke_width > 0.0 {
        let stroke_rect = Rect::new(
            rect.x - stroke_width / 2.0,
            rect.y - stroke_width / 2.0,
            rect.width + stroke_width,
            rect.height + stroke_width,
        );
        let instance = QuadInstance::solid(
            [stroke_rect.x, stroke_rect.y],
            [stroke_rect.width, stroke_rect.height],
            [stroke_color.r, stroke_color.g, stroke_color.b, stroke_color.a],
        );
```

The stroke is rendered as a **filled rectangle** slightly larger than the original rect, placed behind it (lower z-order). This does not produce a stroke outline — it produces a colored halo/bleed behind the fill. The fill rectangle then covers the center, leaving only the edges visible, but the corners are square, not rounded, and the stroke width is not correctly applied.

**Impact:** The stroke quad is added after the fill at a larger size, so for the same batch it covers the fill instead of forming an outline. Corners are square and the inside/outside stroke semantics are not implemented.

**Fix:** Render the stroke as four edge rectangles or use a proper stroke shader.

---

### 73. batching.rs text rendering is a placeholder quad (batching.rs:301-324)
**File:** `wgpu-api/src/batching.rs`  
**Lines:** 301-324

```rust
// Text is rendered as a placeholder quad for now.
// In the full implementation, text shaping produces multiple
// glyph quads that are batched by atlas texture.
let key = BatchKey::new(
    PipelineType::Text,
    None, // Glyph atlas texture
    self.blend_mode,
    self.clip_index,
);
let instance = QuadInstance::solid(
    [position.x, position.y],
    [*font_size, *font_size],
    [color.r, color.g, color.b, color.a],
);
```

Text commands are converted into a solid-color square of size `font_size × font_size` at the text position. This is not actual text rendering — it's a colored box. The `text_render.rs` module exists with proper glyphon integration, but `batching.rs` bypasses it entirely.

**Impact:** `BatchSorter` produces a placeholder square for text. This does not mean all current renderer text is a square: `Renderer::draw_commands_to_gpu_impl` routes text through `TextManager`; the batching path is a separate, incomplete route.

**Fix:** Integrate `TextManager` with the batching pipeline, or have `BatchSorter` delegate text commands to `TextManager`.

---

### 74. batching.rs PushClip tracked but never applied to GPU (batching.rs:354-361)
**File:** `wgpu-api/src/batching.rs`  
**Lines:** 354-361

```rust
DrawCommand::PushClip { clip_rect: _ } => {
    self.clip_index += 1;
}
DrawCommand::PopClip => {
    if self.clip_index > 0 {
        self.clip_index -= 1;
    }
}
```

The clip index is incremented/decremented and stored in the `BatchKey`, but nothing actually applies the scissor rectangle to the GPU render pass. The `clip_rect` parameter is ignored (`_`). The GPU backend never calls `render_pass.set_scissor_rect()`.

**Impact:** Clipping has no effect. Content that should be clipped renders outside its intended bounds.

**Fix:** Store the actual clip rectangle and apply it via `set_scissor_rect()` in the GPU backend before drawing commands with that clip index.

---

### 75. batching.rs SetTransform completely ignored (batching.rs:371-374)
**File:** `wgpu-api/src/batching.rs`  
**Lines:** 371-374

```rust
DrawCommand::SetTransform { .. } => {
    // Transform is resolved on the CPU before batching
    // so it doesn't affect batch keys
}
```

The comment claims transforms are resolved on the CPU, but there is no code that does this resolution. The transform is silently discarded. Any draw command issued after `SetTransform` will render at its original position, not transformed.

**Impact:** Transforms have no effect. Rotated, scaled, or translated content renders at the wrong position.

**Fix:** Either implement CPU-side transform resolution (apply transform to subsequent command coordinates) or pass transforms through to the GPU via the uniform buffer.

---

### 76. texture.rs TextureId is opaque but not zero-safe (texture.rs:9-20)
**File:** `wgpu-api/src/texture.rs`  
**Lines:** 9-20

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u64);
impl From<TextureConfig> for Texture {
    fn from(config: TextureConfig) -> Self {
        Self::new(TextureId::new(0), config)  // ← ID 0
    }
}
```

`TextureId::new(0)` is used as a default/placeholder value in `From<TextureConfig>`. There's no `TextureId::INVALID` or `Option<TextureId>` pattern. Code that checks for "no texture" may accidentally match a legitimately allocated texture with ID 0.

**Impact:** ID zero is constructible and used by a few placeholder/test paths, but the API does not define zero as a null sentinel or as invalid. This is a type/API convention risk, not an observed collision with the renderer's allocator (which starts at 1).

**Fix:** Reserve ID 0 as invalid and start allocation from 1, or use `Option<TextureId>`.

---

### 77. texture.rs Texture struct duplicates size information (texture.rs:182-195)
**File:** `wgpu-api/src/texture.rs`  
**Lines:** 182-195

```rust
pub struct Texture {
    pub id: TextureId,
    pub config: TextureConfig,  // ← contains width, height
    pub size: Size,             // ← duplicates width, height
```

`Texture` stores both `config` (which contains `width` and `height`) and `size` (which is derived from the same values). The `size` field is set in `Texture::new()` from `config.width` and `config.height`, creating redundant state.

**Impact:** Since both fields are public and `config` is mutable, changing its dimensions can make `size()` disagree with `width()`/`height()`. The redundant size is also a convenience representation, so the issue is the mutable invariant rather than the extra few bytes.

**Fix:** Remove `size` field and derive it from `config`, or remove `width`/`height` from `config`.

---

### 78. texture.rs uploaded flag is unreliable (texture.rs:190, 211-217)
**File:** `wgpu-api/src/texture.rs`  
**Lines:** 190, 211-217

```rust
pub uploaded: bool,
pub fn from_image(id: TextureId, image: &Image) -> Self {
    // ...
    let mut texture = Self::new(id, config);
    texture.uploaded = true;  // ← marked uploaded but no GPU upload happens here
    texture
}
```

`from_image` marks the texture as uploaded, but no actual GPU upload occurs in this function. The `uploaded` flag is set optimistically. The actual upload happens later in `Renderer::upload_texture_from_pixels`, which is a separate step. If the upload fails or is skipped, the flag is still `true`.

**Impact:** The `uploaded` flag appears to describe GPU upload status, but `from_image` has only the CPU-side image descriptor and does not upload data. No renderer path currently consults this flag, so the stated render failure is not established.

**Fix:** Only set `uploaded = true` after the GPU upload actually succeeds.

---

## Medium Severity Issues

### 79. text_render.rs loads system fonts on every creation (text_render.rs:60-64)
**File:** `wgpu-api/src/text_render.rs`  
**Lines:** 60-64

```rust
pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
    let mut font_system = FontSystem::new();
    {
        let fontdb = font_system.db_mut();
        fontdb.load_system_fonts();  // ← scans /usr/share/fonts every time
    }
```

Every `TextManager` instance scans the system fonts. `Renderer::new` currently creates one manager per renderer; this can repeat for multiple Renderer instances, but not once per frame.

**Impact:** Potential startup cost when multiple Renderer instances are created. The stated hundreds-of-milliseconds estimate is unmeasured.

**Fix:** Share a single `FontSystem` across all `TextManager` instances using `Arc`.

---

### 80. text_render.rs color conversion loses precision (text_render.rs:132-137)
**File:** `wgpu-api/src/text_render.rs`  
**Lines:** 132-137

```rust
&Attrs::new().color(Color::rgba(
    (item.color[0] * 255.0) as u8,
    (item.color[1] * 255.0) as u8,
    (item.color[2] * 255.0) as u8,
    (item.color[3] * 255.0) as u8,
)),
```

The `TextItem` color is `[f32; 4]` expected in the range 0.0–1.0. Converting to `u8` truncates rather than rounds; this is a small quantization difference, not a systematic renderer darkening issue.

**Impact:** A value of 0.75 maps to 191; rounding 191.25 also yields 191. The original example did not show a one-level discrepancy.

**Fix:** Use `(item.color[0] * 255.5) as u8` or `(item.color[0] * 255.0).round() as u8`.

---

### 81. text_render.rs legacy render_text defeats batching (text_render.rs:193-214)
**File:** `wgpu-api/src/text_render.rs`  
**Lines:** 193-214

```rust
pub fn render_text(&mut self, text: &str, ...) {
    self.add_text(TextItem { ... });
    self.flush(viewport_width, viewport_height, render_pass);  // ← flushes immediately
}
```

The legacy `render_text` method adds one text item and immediately flushes, defeating the batching mechanism. Each call to `render_text` triggers a full prepare+render cycle.

**Impact:** Rendering N text items via `render_text` produces N prepare+render cycles instead of 1. This is extremely inefficient.

**Fix:** Deprecate `render_text` and require callers to use `add_text` + `flush`.

---

### 82. FrameInfo constructor supplies a 60 Hz default (frame.rs:36)
**File:** `wgpu-api/src/frame.rs`  
**Line:** 36

```rust
delta_time: 0.016, // ~60 FPS default
```

`FrameInfo::new()` initializes `delta_time` to 0.016 and `start_time` to 0.0. `FrameInfo` is not currently produced by the renderer's frame loop; it is a metadata type whose timing fields must be populated by a caller that uses it.

**Impact:** Callers relying on the constructor's timing defaults receive placeholder values. This is not evidence that the renderer's animations use fixed 60 Hz timing.

**Fix:** Compute `delta_time` from the difference between the current and previous frame's `start_time`.

---

### 83. FrameInfo start_time defaults to zero (frame.rs:34)
**File:** `wgpu-api/src/frame.rs`  
**Line:** 34

```rust
start_time: 0.0,
```

The `start_time` is initialized to zero and there is no renderer integration that updates it.

**Impact:** The value is only meaningful if the caller explicitly updates it; current in-crate code does not consume it.

**Fix:** Set `start_time` to `std::time::Instant::now().elapsed().as_secs_f64()` or similar in the render loop.

---

### 84. clip.rs ClipStack uses RefCell unnecessarily (clip.rs:139)
**File:** `wgpu-api/src/clip.rs`  
**Line:** 139

```rust
effective: RefCell<Option<ClipRect>>,
```

`ClipStack` uses `RefCell` for interior mutability of the cached effective clip. However, `effective_clip()` takes `&self`, and the cache is invalidated on `push`/`pop`/`clear` which take `&mut self`. The `RefCell` is only needed because `effective_clip()` wants to cache without `&mut self`.

**Impact:** `RefCell` adds runtime borrow checking overhead and risks panic if borrowed mutably while already borrowed. The cache could be recomputed on-demand since the stack is typically small.

**Fix:** Remove `RefCell` and recompute the effective clip on each call, or use a `&mut self` method.

---

### 85. clip.rs ScissorRect never applied to GPU (clip.rs:452-532, renderer.rs)
**File:** `wgpu-api/src/clip.rs`  
**Lines:** 452-532

`ScissorRect` provides a complete API for GPU scissor rectangles, including `from_clip()`, `intersection()`, and `contains()`. However, the GPU backend in `renderer.rs` never calls `render_pass.set_scissor_rect()`. The scissor test is never enabled.

**Impact:** Scissor clipping is a complete implementation that is never used. Content that should be clipped renders outside its bounds.

**Fix:** Apply `ScissorRect` in the GPU backend before drawing commands within a clip region.

---

### 86. gpu.rs QuadInstance SIZE constant may not match shader layout (gpu.rs:116)
**File:** `wgpu-api/src/gpu.rs`  
**Line:** 116

```rust
pub const SIZE: u32 = std::mem::size_of::<Self>() as u32;
```

`QuadInstance` is `#[repr(C)]` with 2 + 2 + 2 + 2 + 4 = 12 f32 fields = 48 bytes. The comment says "32 bytes per instance (aligned to 16-byte boundary)" which is incorrect — it's 48 bytes. If the shader expects a different layout, the instance buffer will be misinterpreted.

**Impact:** The comment is wrong (32 vs 48 bytes). If the shader was written for 32-byte instances, the data will be misaligned and render garbage.

**Fix:** Verify the shader matches the Rust struct layout. Update the comment to reflect the actual size.

---

### 87. layout/projection.rs NodeId assignment can collide (projection.rs:171-178)
**File:** `wgpu-api/src/layout/projection.rs`  
**Lines:** 171-178

```rust
.enumerate()
.map(|(i, body)| {
    let projection = self.project_body(body, node_size);
    let mut node = Node::new(NodeId::new(i as u64), content.clone());
```

`create_nodes` assigns `NodeId` based on the enumeration index of `bodies.values()`. Since `HashMap` iteration order is arbitrary, the same body can get different NodeIds across calls. Also, if called multiple times, NodeIds will collide with existing nodes.

**Impact:** Node IDs are non-deterministic and can collide. Scene graph operations that rely on stable NodeIds will fail.

**Fix:** Use the body's ID to derive the node ID, or maintain a separate counter.

---

### 88. layout/projection.rs body_to_node mapping overwritten (projection.rs:176)
**File:** `wgpu-api/src/layout/projection.rs`  
**Line:** 176

```rust
self.body_to_node.insert(body.id, i as u64);
```

Every call to `create_nodes` overwrites the `body_to_node` mapping. If the body order changes (HashMap iteration), the mapping becomes inconsistent. There's no way to unregister old mappings.

**Impact:** Stale mappings point to wrong nodes. No cleanup of old mappings.

**Fix:** Clear `body_to_node` before creating nodes, or use a proper registration/unregistration pattern.

---

### 89. layout/projection.rs deformation formula has redundant operations (projection.rs:116)
**File:** `wgpu-api/src/layout/projection.rs`  
**Line:** 116

```rust
(speed / 1000.0 * self.config.max_deformation * 1000.0).min(self.config.max_deformation)
```

This simplifies to `speed * self.config.max_deformation`.min(max_deformation)`. The `/ 1000.0 * 1000.0` cancels out. The intent seems to be normalizing speed to a 0-1 range, but the multiplication undoes the division.

**Impact:** The deformation scales linearly with speed, capped at `max_deformation`. The formula works but is confusing and suggests the author intended a different normalization.

**Fix:** Simplify to `(speed * self.config.max_deformation).min(self.config.max_deformation)` or implement the intended normalization correctly.

---

### 90. layout/arrangement.rs assumes sequential BodyIds (arrangement.rs:47)
**File:** `wgpu-api/src/layout/arrangement.rs`  
**Line:** 47

```rust
let body_id = BodyId::new((i + 1) as u64);
```

`redistribute` assumes bodies have sequential IDs starting from 1. If bodies were created by `InsertionManager` (which uses `next_id`), the IDs may not be sequential after insert/remove cycles.

**Impact:** Same as issue #63 — redistribution targets wrong bodies after multiple insert/remove operations.

---

### 91. interaction/commands.rs CommandQueue uses O(n) eviction (commands.rs:239)
**File:** `wgpu-api/src/interaction/commands.rs`  
**Line:** 239

```rust
if self.commands.len() >= self.max_size {
    self.commands.remove(0);  // ← O(n)
}
```

Same pattern as issue #67. `Vec::remove(0)` shifts all elements.

**Impact:** O(n) per push when queue is full. Use `VecDeque` for O(1) eviction.

---

### 92. interaction/input.rs MouseButtonState can't distinguish pressed from held (input.rs:24-30)
**File:** `wgpu-api/src/interaction/input.rs`  
**Lines:** 24-30

```rust
pub enum MouseButtonState {
    Pressed,
    Released,
}
```

With only two states, there's no way to track whether a button was just pressed this frame or has been held for multiple frames. The `is_left_held` method acknowledges this limitation.

**Impact:** Can't implement "click" vs "drag" distinction at the input layer. Callers must track state themselves.

**Fix:** Add a `Held` state or track previous state in `InputState`.

---

### 93. batching.rs SetOpacity changes blend mode but doesn't modify colors (batching.rs:362-370)
**File:** `wgpu-api/src/batching.rs`  
**Lines:** 362-370

```rust
DrawCommand::SetOpacity { opacity } => {
    if *opacity < 1.0 {
        self.blend_mode = BlendMode::Alpha;
    } else {
        self.blend_mode = BlendMode::Opaque;
    }
}
```

When opacity is set, only the blend mode changes. The actual color alpha of subsequent commands is not modified. If a command has `color.a = 1.0` and opacity is 0.5, the command still renders with full alpha because the color isn't multiplied by the opacity.

**Impact:** `SetOpacity` only enables alpha blending but doesn't actually make subsequent commands more transparent. Content renders at full opacity even when `SetOpacity { opacity: 0.5 }` is called.

**Fix:** Track current opacity and multiply it into the color alpha of subsequent commands.

---

### 94. gpu.rs PipelineType enum has Composite variant but no composite pipeline implementation
**File:** `wgpu-api/src/gpu.rs`  
**Lines:** 182-192

```rust
pub enum PipelineType {
    Solid2D,
    TexturedQuad,
    Text,
    Composite,  // ← documented but not implemented
}
```

The `Composite` pipeline is documented as "for opacity, transitions, and blending" but there is no corresponding shader or pipeline creation code for it. Commands that would use the Composite pipeline fall through or are handled by other pipelines.

**Impact:** The Composite pipeline type exists but has no backend support. Code that tries to use it will fail or fall back incorrectly.

**Fix:** Implement the Composite pipeline or remove the variant.

---

### 95. text_render.rs TextManager holds Arc<wgpu::Device> violating renderer ownership
**File:** `wgpu-api/src/text_render.rs`  
**Lines:** 47-49, 80-81

```rust
device: Arc<wgpu::Device>,
queue: Arc<wgpu::Queue>,
// ...
device: Arc::new(device.clone()),
queue: Arc::new(queue.clone()),
```

Per the project guidelines, "The Renderer struct owns the wgpu device and queue. No other subsystem creates or destroys wgpu resources directly." `TextManager` holds its own `Arc` clones of the device and queue, bypassing the renderer's ownership model. It creates wgpu resources (`Cache`, `TextAtlas`, `TextRenderer`, `Viewport`) directly.

**Impact:** Violates the critical architectural invariant of renderer ownership. Multiple independent wgpu resource owners can cause resource conflicts, ordering issues, and make it impossible to track GPU resource lifetimes.

**Fix:** Have `TextManager` borrow the device/queue through the renderer's API, not hold direct Arc references.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 61 | **Critical** | Uncompiled picker depth uses HashMap index | `pick()` would always return `None` if compiled and used |
| 62 | **Critical** | Uncompiled AABB hit expansion uses diagonal | Oversized hit area if used |
| 63 | **Critical** | BodyId decoupled from index | Wrong bodies after insert/remove |
| 64 | **Critical** | Redistribute has dead code | Mutual repulsion doesn't work |
| 65 | **High** | drag.rs hardcoded thresholds | Config has no effect |
| 66 | **High** | drag.rs end_drag state assignment | Unobservable transition in uncompiled source |
| 67 | **High** | velocity_history O(n) eviction | Minor perf cost |
| 68 | **High** | gestures.rs hardcoded thresholds | Doesn't scale with DPI |
| 69 | **High** | GesturePhase integer cast | Safe cast; representation not promised |
| 70 | **High** | is_left_held == is_left_pressed | Can't detect just-pressed |
| 71 | **High** | KeyInputEvent uses String | Per-key heap allocation |
| 72 | **High** | stroke renders as filled rect | Not a proper outline |
| 73 | **High** | BatchSorter text is placeholder quad | Batching path lacks glyph rendering |
| 74 | **High** | PushClip never applied to GPU | Clipping doesn't work |
| 75 | **High** | SetTransform ignored | Transforms don't work |
| 76 | **High** | TextureId zero convention | No null sentinel contract |
| 77 | **High** | Texture duplicates size | Public mutable fields can diverge |
| 78 | **High** | uploaded flag semantics | Not consulted by renderer |
| 79 | **Medium** | load_system_fonts per instance | Slow TextManager creation |
| 80 | **Medium** | color truncation darkens text | Slightly wrong colors |
| 81 | **Medium** | render_text defeats batching | N prepare+render cycles |
| 82 | **Medium** | delta_time hardcoded to 60fps | Wrong animation speed |
| 83 | **Medium** | start_time never set | Timing calculations broken |
| 84 | **Medium** | ClipStack uses RefCell | Unnecessary overhead |
| 85 | **Medium** | ScissorRect never applied | Scissor clipping unused |
| 86 | **Medium** | QuadInstance SIZE comment wrong | Misleading documentation |
| 87 | **Medium** | NodeId assignment non-deterministic | Colliding node IDs |
| 88 | **Medium** | body_to_node overwritten | Stale mappings |
| 89 | **Medium** | deformation formula redundant | Confusing code |
| 90 | **Medium** | arrangement assumes sequential IDs | Same as #63 |
| 91 | **Medium** | CommandQueue O(n) eviction | Same as #67 |
| 92 | **Medium** | MouseButtonState too simple | Can't distinguish click/drag |
| 93 | **Medium** | SetOpacity doesn't modify colors | Opacity has no visual effect |
| 94 | **Medium** | Composite pipeline unimplemented | Dead pipeline type |
| 95 | **Medium** | TextManager stores shared device handles | Not an ownership violation |

---

## Cross-Reference with Previous Rounds

- **Round 1** (`gpu-rendering-review.md`): Issues 1–12, renderer.rs, backend.rs, dock.rs, dashed_block.rs
- **Round 2** (`gpu-rendering-review-2.md`): Issues 13–32, renderer.rs, backend.rs, shaders.rs, command.rs, render.rs
- **Round 3** (`gpu-rendering-review-3.md`): Issues 33–60, scene.rs, command.rs, video.rs, gpu.rs, animation.rs, render.rs, date_time_block.rs, draw.rs
- **Round 4** (this file): Issues 61–95, texture.rs, layout/ submodules, interaction/ submodules, batching.rs, clip.rs, frame.rs, text_render.rs, gpu.rs

## Validation corrections

| Finding | Current assessment |
|---|---|
| 61 | Confirmed in source but inactive: `pick()` initializes best depth to `usize::MAX`, so it never selects a candidate. The `interaction` directory is not declared as a crate module, so this is not current runtime behavior. It also has no visual z-index input. |
| 62 | Confirmed in source but inactive: expanded AABB hit tests use a circumscribed-circle radius. The source module is not part of the compiled crate. |
| 63, 90 | Sequential-ID assumptions in insertion/arrangement are present; this is one coupled layout identity issue, not two independent findings. |
| 64 | Dead clone/empty-loop code and center-based repulsion exist in the uncompiled `interaction/commands.rs`; no active command behavior is established. |
| 65, 68 | Hardcoded drag/gesture thresholds are in uncompiled interaction modules. The DPI impact is not established; config omission would matter only if these modules are wired in. |
| 66 | The `Dropping` assignment modifies a local after taking the `DragInfo`, but this is in an uncompiled module and cannot affect current crate behavior. |
| 67, 91 | `Vec::remove(0)` is O(n), but the collections are bounded (10 samples and a default queue of 64); both are in uncompiled interaction sources. Low-impact optimization, not a high-severity issue. |
| 69 | Integer casts of fieldless Rust enums are safe. Without `repr`, numeric discriminants are not an ABI guarantee; the original undefined-behavior claim was false. |
| 70, 92 | The uncompiled input source only represents current pressed/released status; it cannot represent a just-pressed edge separately. |
| 71 | The uncompiled input source uses `String` key codes. This allocates, but key events are low frequency and this is not a rendering issue. |
| 72 | Confirmed in `BatchSorter`: its expanded stroke quad follows the fill and is not a true outline. The sorter is not currently used by the renderer's direct draw-command path. |
| 73 | Confirmed only for `BatchSorter`: it creates a placeholder quad. The active renderer text path uses `TextManager`; do not generalize this to all GPU text. |
| 74, 75, 93 | Confirmed limitations of `BatchSorter` state handling. `BatchSorter` is not currently called by the renderer, so these do not describe the active direct GPU path; the direct renderer's ignored state commands are already recorded in findings #16/#41. |
| 76 | ID zero is constructible, but it is not documented as a null sentinel and the renderer allocator starts at 1. No current collision/failure was demonstrated. |
| 77 | Public mutable config dimensions can diverge from the separately stored `size`. The redundant representation is only a risk if callers mutate the public config. |
| 78 | `from_image` marks a CPU-side descriptor uploaded, but the flag is not consulted by current renderer code. The claimed render attempt against a missing GPU texture was not established. |
| 79 | Font scanning occurs per `TextManager` construction (currently one per `Renderer`), not per frame. The stated time cost is unmeasured. |
| 80 | Conversion truncates to 8-bit values; the stated 0.75-to-192 example is incorrect (191.25 rounds to 191). This is minor quantization, not a proven visible defect. |
| 81 | The convenience method flushes immediately, which prevents combining multiple calls through that method; the normal `add_text`/`flush` path supports batching. |
| 82, 83 | `FrameInfo` has placeholder timing defaults and is not currently driven by the renderer frame loop. No active animation frame-rate bug was established from this struct. |
| 84 | `RefCell` supports caching `effective_clip()` under `&self`; recomputing may be simpler, but overhead/panic risk is not demonstrated in the current call pattern. |
| 85 | Duplicate of clipping limitations in #16/#41. `ScissorRect` is not wired to the active renderer. |
| 86 | The comment's 32-byte instance size is wrong; the actual struct is 48 bytes and an existing test asserts that. No shader-layout mismatch was established in this review. |
| 87, 88 | `create_nodes()` uses unstable enumeration-derived NodeIds and leaves mappings for bodies absent from later calls. Confirmed identity/mapping risks; mapping to a live `Scene` may collide with its allocator. |
| 89 | Algebraically simplifies to `min(speed * max_deformation, max_deformation)`. It is redundant/confusing, not a demonstrated behavior bug. |
| 94 | `Composite` is a declared batching pipeline kind without current backend implementation. The `BatchSorter` has no active renderer integration, so this is an incomplete API/planned feature rather than a current pipeline fallback defect. |
| 95 | Not an ownership violation: `Renderer` owns `TextManager`, and cloned wgpu handles refer to the same device/queue. TextManager's internal resources are subordinate renderer resources. |

**Numbered findings: 95** (issues 1–95). Round 4 adds 35 entries: 4 initially labeled Critical, 14 High, and 17 Medium. Several entries are duplicates, risks, or disproven claims; the labels are not validated severity ratings.

**Module integration note:** `wgpu-api/src/interaction/{commands,drag,gestures,input,picking}.rs` are not declared from `lib.rs` and have no `interaction/mod.rs`. Their findings describe unused source files, not compiled crate behavior; the targeted cargo test filters matched zero tests. No changes were retained in those files.

### Numbered entries by round

| Round | Entries |
|-------|---------|
| 1 | 12 |
| 2 | 20 |
| 3 | 28 |
| 4 | 35 |
| **Total** | **95** |

The previous severity totals were arithmetically inconsistent with the numbered ranges (and round 3 has 28 entries, not 30). Cumulative severity totals are omitted because classifications are being corrected.

### Key Themes Across All Rounds

1. **GPU buffer/texture allocations**: Per-draw/per-frame allocations require profiling; the reviewed wgpu buffer handles are RAII-managed (issues 1, 13, 26)
2. **State commands ignored**: PushClip, PopClip, SetTransform, SetOpacity tracked but not applied (issues 27, 41, 74, 75, 93)
3. **Coordinate space confusion**: Mixed logical/physical pixel conventions across modules (issues 2, 42, 82)
4. **BodyId/index mismatch**: Layout and interaction modules assume sequential BodyIds that break after insert/remove (issues 63, 90)
5. **Hardcoded values ignoring config**: Drag, gesture, and frame modules hardcode thresholds that should be configurable (issues 65, 68, 82)
6. **Hit testing incorrect**: AABB expansion was oversized; Picker selection used an unstable index and previously never chose a candidate (issues 34, 61, 62)
7. **Text rendering incomplete**: Batching uses placeholder quads; TextManager exists but isn't integrated (issues 73, 81, 95)
8. **Dead code and stubs**: Many features are defined but not implemented (issues 35, 36, 38, 64, 94)
9. **Architecture ownership**: TextManager is renderer-owned and holds shared device/queue handles; no ownership violation was established (issue 95)
10. **O(n) operations in hot paths**: Vec::remove(0) in velocity history and command queue (issues 67, 91)

### Files Reviewed in Round 4

| File | Issues |
|------|--------|
| `texture.rs` | 76, 77, 78 |
| `layout/arrangement.rs` | 90 |
| `layout/dock.rs` | (none new) |
| `layout/insertion.rs` | 63 |
| `layout/projection.rs` | 87, 88, 89 |
| `interaction/commands.rs` | 64, 91 |
| `interaction/drag.rs` | 65, 66, 67 |
| `interaction/gestures.rs` | 68, 69 |
| `interaction/input.rs` | 70, 71, 92 |
| `interaction/picking.rs` | 61, 62 |
| `batching.rs` | 72, 73, 74, 75, 93 |
| `clip.rs` | 84, 85 |
| `frame.rs` | 82, 83 |
| `text_render.rs` | 79, 80, 81, 95 |
| `gpu.rs` | 86, 94 |
