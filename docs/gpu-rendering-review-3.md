# GPU Rendering Code Review — Round 3 (Scene Graph, Commands, Video, Animation, App Layer)

This is a continuation of the GPU rendering code review. Rounds 1 and 2 covered issues 1–32. This round adds issues 33–60: **28 numbered findings**, not 30. Across the three review files, the numbered findings total 60, not 62. The initial severity labels and impacts below have not all been validated; see the corrections at the end.

---

## Critical Issues

### 33. Scene graph traversal ignores z-index within siblings (scene.rs:771-791)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 771-791

`traverse_visible` uses a stack-based DFS that visits children in insertion order. It does **not** sort children by `z_index` before visiting. The `visible_nodes` method (line 794) sorts all nodes globally by z-index, but this destroys parent-child hierarchy — a child with z_index=0 from parent A may render before a child with z_index=1 from parent B, even if parent A should be entirely behind parent B.

**Impact:** Z-ordering is incorrect when multiple groups have overlapping children. The `visible_nodes` method is used for hit testing, not rendering, so the actual render path may not use it at all.

**Fix:** Sort children by z_index within each group during traversal, or use a painter's algorithm that respects hierarchy.

---

### 34. Scene hit test uses bounding rect only, not actual content (scene.rs:841-854)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 841-854

```rust
pub fn hit_test(&self, point: Point) -> Option<NodeId> {
    let mut nodes = self.visible_nodes();
    nodes.reverse();
    for id in nodes {
        if let Some(node) = self.nodes.get(&id) {
            let rect = node.style.bounding_rect();
            if rect.contains(&point) {
                return Some(id);
            }
        }
    }
    None
}
```

Hit testing uses `bounding_rect()` which is the node's position + size. It does not account for:
- **Transforms**: Rotation, scale, or anchor point are ignored
- **Content shape**: A circle or rounded rect hit-tests as a full rectangle
- **Parent transforms**: Child coordinates are not transformed to world space
- **Opacity**: Invisible nodes (opacity=0) are filtered by `visible_nodes`, but semi-transparent nodes still pass

**Impact:** Click detection is inaccurate for rotated/scaled nodes and for non-rectangular shapes. Clicking outside a circular icon but within its bounding box will still register as a hit.

---

### 35. Command enum has 60+ variants but GPU path handles ~6 (command.rs:25-518 vs renderer.rs:1000-1200)
**File:** `wgpu-api/src/command.rs` lines 25-518, `wgpu-api/src/renderer.rs`

The `Command` enum defines 60+ drawing primitives: Circle, Ellipse, Line, DashedLine, Arc, Star, Triangle, Diamond, Polygon, CubicBezier, QuadraticBezier, Ring, ProgressBar, Separator, Dot, Checkmark, Cross, GradientCircle, PatternFill, Glow, Blur, Vignette, Noise, Scanlines, Pixelate, ColorMatrix, HueRotate, BrightnessContrast, Saturate, Invert, Grayscale, Sepia, OpacityMask, Composite, DropShadow, InnerShadow, Outline, MultiStroke, GradientStroke, TextOutline, TextShadow, TextGradient, TextWithBackground, TextWithStroke, TextGlow, TextBlur, TextMask, TextGradientMask, TextPatternFill, TextNoise, TextScanlines, TextPixelate, TextColorMatrix, TextHueRotate, TextBrightnessContrast, TextSaturate, TextInvert, TextGrayscale, TextSepia, TextOpacityMask, TextComposite, TextDropShadow, TextInnerShadow, TextOutlineStroke, TextMultiStroke, TextGradientStroke.

The GPU backend in `renderer.rs` handles only: Rectangle, Image, Text, Video, PushClip, PopClip, SetTransform, SetOpacity. All other 50+ variants are **completely unimplemented** in the GPU path.

**Impact:** Any code that uses these Command variants will silently fail to render on the GPU path. The API suggests capabilities that don't exist.

---

### 36. CommandBuffer state (transform, opacity, clip) never applied to commands (command.rs:525-536)
**File:** `wgpu-api/src/command.rs`  
**Lines:** 525-536

```rust
pub struct CommandBuffer {
    commands: Vec<Command>,
    transform_stack: Vec<Transform>,
    blend_mode: BlendMode,
    opacity: f32,
    clip_path: Option<Path>,
}
```

`CommandBuffer` maintains `transform_stack`, `blend_mode`, `opacity`, and `clip_path` fields. However, these are **never applied** to the commands in `self.commands`. The `transform()`, `pop_transform()`, `set_blend_mode()`, `set_opacity()`, `clip()`, and `reset_clip()` methods push commands, but nothing compiles these state changes into the actual draw commands.

**Impact:** Transform, opacity, blend mode, and clip state tracked by CommandBuffer is dead code. Commands issued after `transform()` will not be transformed.

---

## High Severity Issues

### 37. Video YUV format conversion returns None (video.rs:356-361)
**File:** `wgpu-api/src/video.rs`  
**Lines:** 356-361

```rust
VideoFormat::Nv12 | VideoFormat::I420 | VideoFormat::Nv16 | VideoFormat::Yuy2 => {
    // For now, return None — YUV conversion requires a proper
    // implementation. In production, use a SIMD-accelerated
    // converter or let the GPU handle it via a shader.
    None
}
```

NV12, I420, NV16, and YUY2 are the **most common** PipeWire video formats. Screen capture and window capture typically use NV12. When `to_rgba()` returns `None`, the video frame is silently dropped.

**Impact:** Video capture from most PipeWire streams will fail silently. The dock's screen capture feature is non-functional for the most common video formats.

---

### 38. Video DMA-BUF path (Path A) is unimplemented stub (video.rs:381-395)
**File:** `wgpu-api/src/video.rs`  
**Lines:** 381-395

```rust
pub enum FrameBufferSource {
    DmaBuf,  // Path A — future
    CpuUpload,  // Path B — current
}
```

The `DmaBuf` variant exists but has no implementation. The `FrameBuffer` struct has `data: Option<Vec<u8>>` for CPU upload, but no fields for DMA-BUF file descriptors, offsets, strides, or modifiers. The entire zero-copy GPU sharing path is a stub.

**Impact:** Video must go through CPU upload (Path B), which requires copying pixel data and uploading to GPU. This is slower and uses more memory than DMA-BUF sharing.

---

### 39. VideoManager texture reuse may cause stale textures (video.rs:733+)
**File:** `wgpu-api/src/video.rs`  
**Lines:** 733+

The VideoManager reuses GPU textures when dimensions and format haven't changed. However, if a stream is disconnected and reconnected with the same dimensions, the old texture may still be referenced by a `VideoNode` that hasn't been updated. The texture reuse logic doesn't invalidate old references.

**Impact:** Stale texture references after stream reconnect can cause rendering artifacts or crashes.

---

### 40. CubicBezier easing ignores x1, x2 control points (animation.rs:97-104)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 97-104

```rust
Self::CubicBezier(_x1, y1, _x2, y2) => {
    let one_minus_t = 1.0 - t;
    one_minus_t * one_minus_t * one_minus_t * y1
        + 3.0 * one_minus_t * one_minus_t * t * y2
```

The `CubicBezier` variant stores 4 control points `(x1, y1, x2, y2)`, but the implementation **ignores x1 and x2** (prefixed with `_`). A proper cubic bezier easing requires solving for t given the input x, then computing y. The current code treats y1 and y2 as weights in a simplified polynomial, which does not produce a correct cubic bezier curve.

**Impact:** Custom easing functions using CubicBezier produce incorrect interpolation curves. Animations with custom easing will not match the intended timing.

---

### 41. DrawCommand state commands ignored in GPU path (render.rs:173-189, renderer.rs:1173-1178)
**File:** `wgpu-api/src/render.rs` lines 173-189, `wgpu-api/src/renderer.rs` lines 1173-1178

`DrawCommand` defines `PushClip`, `PopClip`, `SetTransform`, and `SetOpacity`. In `renderer.rs`, these are matched but **completely ignored**:

```rust
crate::render::DrawCommand::PushClip { .. }
| crate::render::DrawCommand::PopClip
| crate::render::DrawCommand::SetTransform { .. }
| crate::render::DrawCommand::SetOpacity { .. } => {
    // These are state commands handled during scene traversal
}
```

The comment says "handled during scene traversal", but the GPU path (`draw_commands_to_gpu_impl`) receives pre-compiled draw commands. There is no scene traversal in the GPU path — it directly processes draw commands.

**Impact:** Clipping, transforms, and opacity changes have no effect in the GPU rendering path. Content that should be clipped, rotated, or faded will render incorrectly.

---

### 42. date_time_block GPU coordinates not scaled (date_time_block.rs:62-176)
**File:** `src/app/date_time_block.rs`  
**Lines:** 62-176

The GPU path builds draw commands using **logical pixel** coordinates (e.g., `block_width as f32 / 2.0` for center X). Then it calls `render_to_dmabuf_scaled` with `scale as f32`. However, the dashed border positions use `pos as f32` where `pos` is in logical pixels. The text positions use config values in logical pixels.

In contrast, `dock.rs` pre-scales all coordinates to physical pixels before calling `render_to_dmabuf` (which uses scale=1.0). This inconsistency means:
- `dock.rs`: Physical pixel coords, scale=1.0
- `date_time_block.rs`: Logical pixel coords, scale=dock_scale
- `dashed_block.rs`: Logical pixel coords, scale=dock_scale

**Impact:** If `render_to_dmabuf_scaled` is not used consistently, or if the scale factor differs between calls, the date_time_block will render at incorrect positions or sizes.

---

## Medium Severity Issues

### 43. Scene node removal doesn't clean up orphaned children properly (scene.rs:666-693)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 666-693

`remove_node_recursive` removes descendants recursively before removing the parent node. Each descendant is itself removed, so no surviving child retains a pointer to the removed node in the normal tree case.

**Status:** The reported dangling-parent issue is not reproduced by the normal recursive removal path.

---

### 44. Scene add_child did not reject cycles or missing nodes (scene.rs:696-721)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 696-721

The original implementation permitted self-parenting and ancestor cycles, and it could detach a child before discovering that the requested parent did not exist. The sequential mutable borrows themselves were not a borrow-checker or runtime defect.

**Status:** Fixed: `add_child` now rejects missing nodes, direct self-parenting, and ancestor cycles before changing either node. Regression tests cover self/ancestor cycles and no-mutation behavior for invalid IDs.

---

### 45. Scene visible_nodes sorts globally, affecting hit-test ordering (scene.rs:794-801)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 794-801

```rust
pub fn visible_nodes(&self) -> Vec<NodeId> {
    let mut result = Vec::new();
    self.traverse_visible(|node| {
        result.push(node.id);
    });
    result.sort_by_key(|id| self.nodes.get(id).map(|n| n.style.z_index).unwrap_or(0));
    result
}
```

This sorts all visible nodes globally by z-index, ignoring parent-child relationships. A child with z_index=0 from a parent with z_index=10 will be sorted before a child with z_index=5 from a parent with z_index=0. This breaks the hierarchical z-ordering model.

**Impact:** `visible_nodes()` is used by `hit_test()`, so global sorting can make hit-test order inconsistent with tree traversal. The renderer's `render_scene()` uses `traverse_visible()` directly; this sort does not itself determine that rendering order. This overlaps with finding #33.

---

### 46. VideoFrame to_rgba allocates new Vec per frame (video.rs:302-364)
**File:** `wgpu-api/src/video.rs`  
**Lines:** 302-364

Every call to `to_rgba()` allocates a new `Vec<u8>` for the output. For a 1920x1080 frame at 30fps, this is ~8MB allocated and freed 30 times per second. The BGRx and BGRA paths use `chunks_exact` which is slow compared to SIMD.

**Impact:** High CPU usage and memory pressure from per-frame allocations for video format conversion.

---

### 47. VideoFrame is_valid uses >= instead of == (video.rs:292-295)
**File:** `wgpu-api/src/video.rs`  
**Lines:** 292-295

```rust
pub fn is_valid(&self) -> bool {
    let expected = self.format.estimate_buffer_size(self.width, self.height);
    self.data.len() >= expected
}
```

The check uses `>=` which allows extra bytes beyond the expected size. While this is lenient, it means corrupted frames with extra data will pass validation. More importantly, `estimate_buffer_size` for NV12/I420 uses `base * 3 / 2` which may not match the actual buffer size due to stride alignment.

**Impact:** The `>=` check permits trailing/padding bytes and does not validate per-plane stride layout. Since `VideoFrame` has no stride metadata, this check cannot establish whether a padded frame's rows are interpreted correctly. Allowing extra bytes is not itself invalid.

---

### 48. Animation system has no frame-rate independence (animation.rs:183+)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 183+

The `Animation` struct has `elapsed_time` that accumulates per-update. However, there's no fixed timestep or interpolation between physics steps. If the update rate varies (e.g., 60fps vs 120fps), animations will progress at different speeds.

**Impact:** The main `Animation::update` advances progress by `dt / duration`, so it is time-based rather than update-count-based. Fixed-step simulation/interpolation is a separate concern; this finding does not show refresh-rate-dependent animation speed.

---

### 49. PulseAnimation, RotateAnimation, FadeAnimation are standalone types (animation.rs:409-523)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 409-523

These are separate structs (`PulseAnimation`, `RotateAnimation`, `FadeAnimation`) that are not integrated with the `Animation` system. They have their own `update()` methods and don't use `AnimationManager`. This creates a parallel animation system that doesn't benefit from the main system's features (pausing, completion callbacks, etc.).

**Impact:** Code duplication and inconsistent animation behavior. These special animations can't be paused, cancelled, or chained with other animations.

---

### 50. draw.rs requests icon loads every frame (draw.rs:41-45)
**File:** `src/app/draw.rs`  
**Lines:** 41-45

```rust
for app_id in &apps_in_dock {
    if !self.icon_cache.contains_key(app_id) {
        self.request_icon_load(app_id.to_string());
    }
}
```

Every frame, the draw function iterates all apps in the dock and checks if their icon is cached. If not, it requests a load. This is O(N) per frame where N is the number of pinned apps. The `request_icon_load` function may spawn async tasks or queue work.

**Impact:** The per-frame cache check is O(number of pinned apps), but `request_icon_load` suppresses duplicate requests using `pending_icon_searches`. This is repeated lookup work, not repeated task spawning or duplicate loads.

---

### 51. draw.rs builds window ID map every frame (draw.rs:66-87)
**File:** `src/app/draw.rs`  
**Lines:** 66-87

```rust
if let Some(ref pw) = self.pipewire_capture {
    let mut window_ids: Vec<(String, u32)> = self.windows.iter()...collect();
    window_ids.sort_by(|(a_title, a_id), (b_title, b_id)| ...);
    pw.update_window_ids(&window_ids);
}
```

Every frame, the entire window list is iterated, filtered, collected into a Vec, and sorted. Then `update_window_ids` is called, which likely replaces the internal map. This is O(W log W) per frame where W is the number of windows.

**Impact:** This allocates and sorts on each draw. The code documents that refresh as intentional so the PipeWire monitor sees current title-to-ID mappings and deterministic ordering. It is a profiling candidate, not by itself a correctness bug.

---

### 52. canonical_app_ids built every frame (draw.rs:93-111)
**File:** `src/app/draw.rs`  
**Lines:** 93-111

```rust
let canonical_app_ids: HashMap<String, String> = self.windows.values().map(|win| {
    let raw_id = win.app_id.as_ref()...;
    let canonical = self.canonical_running_app_id(&raw_id);
    (raw_id, canonical)
}).collect();
```

Every frame, a new HashMap is built mapping raw app IDs to canonical app IDs. This involves string allocations and HashMap insertions for every window.

**Impact:** Per-draw allocation and lookup work proportional to the number of windows. Whether caching is worthwhile depends on redraw frequency and profiling; this is not evidence of a rendering failure.

---

## Low Severity Issues

### 53. Scene node_count returns HashMap len, not tree depth (scene.rs:754-756)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 754-756

`node_count()` returns `self.nodes.len()`. This is the total number of nodes, not the number of root nodes or the tree depth. The name is ambiguous — it could mean "number of nodes" or "number of top-level nodes".

**Impact:** API ambiguity.

---

### 54. Scene clear doesn't reset next_id (scene.rs:764-767)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 764-767

```rust
pub fn clear(&mut self) {
    self.nodes.clear();
    self.roots.clear();
}
```

`clear()` removes all nodes but doesn't reset `next_id`. New nodes after clear will have IDs continuing from where they left off. This is generally fine, but if the scene is reused many times, `next_id` could overflow (u64, so unlikely in practice).

**Impact:** Negligible in practice, but inconsistent with the semantic of "clear".

---

### 55. Scene IntoIterator doesn't preserve tree order (scene.rs:863-870)
**File:** `wgpu-api/src/scene.rs`  
**Lines:** 863-870

```rust
fn into_iter(self) -> Self::IntoIter {
    self.nodes.into_values()
}
```

Iterating over the scene yields nodes in HashMap order, which is arbitrary. Parents and children are not guaranteed to be adjacent or in any particular order.

**Impact:** Code that iterates the scene expecting tree order will behave unpredictably.

---

### 56. CommandBuffer text() defaults position to ORIGIN (command.rs:632-639)
**File:** `wgpu-api/src/command.rs`  
**Lines:** 632-639

```rust
pub fn text(&mut self, content: &str, size: f32, color: &Color) {
    let text = Text::new(content).with_style(TextStyle::new().with_size(size).with_color(*color));
    self.commands.push(Command::Text {
        text,
        position: Point::ORIGIN,  // ← always (0, 0)
    });
}
```

The `text()` method always positions text at (0, 0). The caller must use `text_at()` to position text elsewhere. This is a footgun — text rendered with `text()` will appear at the origin.

**Impact:** Text positioned at (0, 0) when the caller forgets to use `text_at()`.

---

### 57. VideoFrame to_rgba for BGRx sets alpha to 255 (video.rs:337)
**File:** `wgpu-api/src/video.rs`  
**Line:** 337

```rust
out.push(255); // A
```

BGRx format has no alpha channel. The conversion sets alpha to 255 (fully opaque). However, some compositors use the x padding for alpha information. Hardcoding 255 may lose transparency information.

**Impact:** BGRx has no alpha channel; the padding byte is not defined as transparency. Setting alpha to 255 is the conventional opaque conversion, not a demonstrated loss of valid alpha data.

---

### 58. Animation easing functions use simplified formulas (animation.rs:57-104)
**File:** `wgpu-api/src/animation.rs`  
**Lines:** 57-104

The easing functions (`EaseIn`, `EaseOut`, `EaseInOut`, `Back`, `Bounce`, `Elastic`) use simplified polynomial formulas rather than the standard CSS/WebKit easing curves. For example, `EaseIn` uses `t * t` (quadratic) instead of the standard cubic ease-in. `EaseOut` uses `t * (2.0 - t)` which is not the standard ease-out curve.

**Impact:** The quadratic ease-in/ease-out and piecewise bounce functions are valid easing choices, not necessarily incorrect substitutes for a CSS preset. The custom `CubicBezier` issue is separately covered by finding #40.

---

### 59. RenderPassInfo texture_count not tracked (render.rs:363-393)
**File:** `wgpu-api/src/render.rs`  
**Lines:** 363-393

`RenderPassInfo` has a `texture_count` field, but there's no code that actually counts unique textures during rendering. The field is set by the constructor, but the constructor is called with hardcoded or estimated values.

**Status:** Not a defect in the scene-rendering path: `render_scene()` counts unique texture IDs in a `HashSet` and passes that count to `RenderPassInfo::new`. The other constructor call is explicitly supplied by its caller.

---

### 60. FrameState transform_stack not used in GPU path (render.rs:222-223, renderer.rs)
**File:** `wgpu-api/src/render.rs` lines 222-223

`FrameState` maintains a `transform_stack` and `clip_stack`. However, the GPU rendering path (`draw_commands_to_gpu_impl`) doesn't use `FrameState` at all — it processes `DrawCommand` directly. The transform and clip stacks in `FrameState` are only used if the scene graph traversal path is active, which it isn't for the dock's GPU rendering.

**Impact:** Transform and clip state in FrameState is dead code for the GPU path.

## Validation corrections

The review labels several observations as rendering failures even though they are duplicate findings, public-API limitations, performance candidates, or expected behavior. Current assessment:

| Finding | Current assessment |
|---|---|
| 33 | Scene traversal preserves insertion order and does not apply z-index. Confirmed ordering limitation; finding #45 is related but affects hit testing, not `render_scene()` ordering. |
| 34 | Bounding-box hit testing does not apply transforms or content shape. Confirmed limitation; assess separately from GPU drawing. |
| 35 | `Command` and `DrawCommand` are distinct APIs. This is not a switch that silently drops 50 variants in `draw_commands_to_gpu_impl`; the `CommandBuffer` API has no current conversion path to that renderer. |
| 36 | `CommandBuffer` records state commands, but its state fields are not applied by this type. This is an API integration limitation, not evidence that GPU draw commands of those types are silently discarded. |
| 37 | Confirmed: current `VideoFrame::to_rgba()` does not convert the listed YUV formats. |
| 38 | DMA-BUF import is explicitly a future path in the video module; CPU upload is the documented current implementation and tradeoff. |
| 39 | Stale-texture behavior after reconnect is not demonstrated. The cache is keyed by `StreamId`; stream removal/reuse lifecycle must be established before treating this as a defect. |
| 40 | Confirmed: `CubicBezier` ignores its x control points and does not solve the curve's x parameter. |
| 41 | Duplicate of round 2 finding #16: the GPU draw loop ignores clip, transform, and opacity commands. |
| 42 | Not demonstrated: date/time commands use logical coordinates with `render_to_dmabuf_scaled`, consistently with the dashed block. |
| 43 | Not reproduced: recursive node removal removes descendants as well as their parent links. |
| 44 | Fixed: `add_child` validates both nodes and rejects self/ancestor cycles before mutating the scene; tests cover rejected operations. |
| 45 | Global z-index sorting affects hit-test order, not the `render_scene()` traversal; overlaps with #33. |
| 46 | Confirmed allocation cost: conversion creates a new RGBA vector per frame. The impact depends on active streams, resolution, and format. |
| 47 | `>=` allows trailing/padding bytes; that is not itself invalid. The current frame type lacks stride metadata, so this check cannot validate padded plane layouts. |
| 48 | Not a demonstrated issue: the main animation update uses elapsed `dt`, not update count. |
| 49 | Standalone animation helpers are an integration/design limitation, not necessarily duplicated behavior or a rendering defect. |
| 50 | Cache lookup repeats per draw, but `request_icon_load` deduplicates pending requests. No repeated task-spawn defect established. |
| 51–52 | Per-draw allocation/sort work is present; its cost and caching value require profiling. The window map refresh is documented as intentional for current PipeWire monitor behavior. |
| 53–56 | API naming/iteration/default-position concerns; not GPU rendering failures. `text()` explicitly offers `text_at()` for positioned text. |
| 57 | BGRx has no alpha channel; converting its padding byte to opaque alpha is conventional. |
| 58 | The listed quadratic and bounce formulas are valid easing choices; no standard-curve compatibility contract is documented. |
| 59 | Not a defect: `render_scene()` counts unique texture IDs before constructing `RenderPassInfo`. |
| 60 | Confirmed: `FrameState` transform/clip stacks are not translated into GPU draw state; overlaps with findings #16 and #41. |

Findings 16, 17, 33, 34, 37, 40, 41, 46, and 60 describe confirmed limitations or behavior, with overlaps noted above. The initial severity labels should not be treated as final severity.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 33 | **Critical** | Scene traversal ignores z-index within siblings | Incorrect render order |
| 34 | **Critical** | Hit test uses bounding rect only | Inaccurate click detection |
| 35 | **Critical** | 50+ Command variants unimplemented in GPU | Silent render failures |
| 36 | **Critical** | CommandBuffer state never applied | Transforms, opacity, clip don't work |
| 37 | **High** | YUV format conversion returns None | Video capture non-functional |
| 38 | **High** | DMA-BUF path unimplemented | Slower video, more memory |
| 39 | **High** | Video texture reuse may cause stale refs | Artifacts after reconnect |
| 40 | **High** | CubicBezier ignores x1, x2 | Wrong easing curves |
| 41 | **High** | DrawCommand state commands ignored | Clipping, transforms, opacity broken |
| 42 | **High** | date_time_block coords not scaled consistently | Wrong position/size |
| 43 | **Medium** | Node removal doesn't clean orphans | Dangling parent pointers |
| 44 | **Medium** | add_child has no self-parenting check | Cycle risk |
| 45 | **Medium** | visible_nodes sorts globally | Broken hierarchical z-order |
| 46 | **Medium** | to_rgba allocates per frame | CPU and memory pressure |
| 47 | **Medium** | is_valid uses >= not == | Stride alignment issues |
| 48 | **Medium** | No frame-rate independence | Animations speed varies |
| 49 | **Medium** | Standalone animation types | Code duplication |
| 50 | **Medium** | Icon loads requested every frame | Unnecessary per-frame work |
| 51 | **Medium** | Window ID map rebuilt every frame | O(W log W) per frame |
| 52 | **Medium** | canonical_app_ids rebuilt every frame | Per-frame heap allocations |
| 53 | **Low** | node_count ambiguous | API ambiguity |
| 54 | **Low** | clear doesn't reset next_id | Inconsistent semantics |
| 55 | **Low** | IntoIterator doesn't preserve order | Unpredictable iteration |
| 56 | **Low** | text() defaults to ORIGIN | Footgun for text positioning |
| 57 | **Low** | BGRx alpha hardcoded to 255 | Lost transparency |
| 58 | **Low** | Easing formulas non-standard | Different feel |
| 59 | **Low** | texture_count not tracked | Inaccurate stats |
| 60 | **Low** | FrameState unused in GPU path | Dead code |

---

## Cross-Reference with Previous Rounds

- **Round 1** (`gpu-rendering-review.md`): Issues 1–12, covering renderer.rs, backend.rs, dock.rs, dashed_block.rs
- **Round 2** (`gpu-rendering-review-2.md`): Issues 13–32, covering renderer.rs, backend.rs, shaders.rs, command.rs, render.rs
- **Round 3** (this file): Issues 33–60, covering scene.rs, command.rs (deep), video.rs, gpu.rs, animation.rs, render.rs (deep), date_time_block.rs, draw.rs

**Total issues found: 60** (4 Critical in this round, 5 High, 12 Medium, 9 Low)

### Cumulative Severity Distribution

| Severity | Round 1 | Round 2 | Round 3 | Total |
|----------|---------|---------|---------|-------|
| Critical | 1 | 3 | 4 | 8 |
| High | 4 | 4 | 5 | 13 |
| Medium | 2 | 6 | 12 | 20 |
| Low | 5 | 7 | 9 | 21 |
| Info | 0 | 0 | 0 | 0 |
| **Total** | **12** | **20** | **30** | **62** |

### Key Themes Across All Rounds

1. **GPU buffer/texture leaks**: Per-frame allocations without cleanup (issues 1, 13, 26)
2. **State commands ignored**: Clip, transform, opacity not applied in GPU path (issues 16, 41)
3. **Coordinate space confusion**: Mixed logical/physical pixel conventions (issues 2, 20, 24, 42)
4. **Video pipeline incomplete**: YUV conversion missing, DMA-BUF unimplemented (issues 12, 18, 37, 38)
5. **Rounded corners/stroke ignored**: Visual mismatch between GPU and CPU paths (issues 3, 17)
6. **Dead code**: Unused structs, functions, and command variants (issues 30, 31, 32, 35, 60)
7. **Per-frame allocations**: Unnecessary heap work every frame (issues 26, 46, 50, 51, 52)
8. **Scene graph not used**: The scene graph exists but the GPU path bypasses it entirely (issues 33, 34, 45, 60)
