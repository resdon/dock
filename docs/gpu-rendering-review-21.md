# GPU Rendering Code Review — Round 21 (Wayland, Physics Springs, Morphing, Primitives, Profiling, Tray)

This is a continuation of the GPU rendering code review. Rounds 1–20 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, documentation, render submodules, state modules, services, physics interaction, groups, input handling, popup rendering, tray rendering, error handling, geometry, models, cache, resolvers, scene graph, text, Slint bridge, library root, registry, and config (issues 1–750). This round covers: `wgpu-api/src/wayland.rs`, `wgpu-api/src/physics/springs.rs`, `wgpu-api/src/morphing.rs`, `wgpu-api/src/primitives.rs`, `wgpu-api/src/perf.rs`, and `src/render/tray.rs` — issues 751–790.

---

## Critical Issues

### 751. wayland.rs — `process_input_event` FocusChange iterates all windows (lines 809-812)
**File:** `wgpu-api/src/wayland.rs` lines 809-812

```rust
InputEvent::FocusChange { window_id, focused } => {
    for window in self.windows.values_mut() {
        window.on_focus_changed(*focused && window.id == *window_id);
    }
}
```

Every focus change event iterates ALL windows to update focus state. For a single focus change, this is O(n) where n is the number of windows. The correct approach is to look up the target window directly and update only it.

**Impact:** O(n) per focus event instead of O(1). With 100 windows, this is 100 iterations per focus change.

**Fix:** Use `self.windows.get_mut(window_id)` to update only the target window. Clear focus on the previously focused window if needed.

---

### 752. wayland.rs — `CloseRequest` iterates all windows to find focused one (lines 814-820)
**File:** `wgpu-api/src/wayland.rs` lines 814-820

```rust
InputEvent::CloseRequest => {
    for window in self.windows.values_mut() {
        if window.focused {
            window.close();
        }
    }
}
```

A close request from the compositor iterates all windows to find the focused one. There is no `focused_window_id` field, so the compositor's close request — which targets a specific window — is resolved by scanning all windows.

**Impact:** O(n) per close request. The `CloseRequest` variant of `InputEvent` carries no `window_id`, so the compositor cannot specify which window to close.

**Fix:** Add `window_id` to `CloseRequest` or track `focused_window_id` in `WaylandConnection`.

---

### 753. wayland.rs — `WaylandConnection` has no actual Wayland socket or event queue (lines 700-712)
**File:** `wgpu-api/src/wayland.rs` lines 700-712

```rust
pub struct WaylandConnection {
    next_window_id: AtomicU64,
    next_popup_id: AtomicU64,
    windows: HashMap<WaylandWindowId, WaylandWindow>,
    popups: HashMap<WaylandPopupId, WaylandPopup>,
    modifiers: ModifiersState,
}
```

`WaylandConnection` stores no `wayland_client::Connection`, no `Display`, no event queue, and no file descriptor. It is a pure-Rust state machine that simulates Wayland window management without actually connecting to a Wayland compositor. The actual Wayland integration lives in `src/listeners/wayland/` and `src/init.rs`, completely separate from this module.

**Impact:** Architecture confusion — `WaylandConnection` is a mock/stub that shadows the real Wayland client code. Application code using this module gets no actual Wayland functionality.

**Fix:** Either integrate this with the real Wayland client or rename it to `WindowManager` to reflect its actual role.

---

### 754. perf.rs — `Stopwatch` uses `RefCell` for interior mutability (lines 47-52)
**File:** `wgpu-api/src/perf.rs` lines 47-52

```rust
pub struct Stopwatch {
    start: RefCell<Option<Instant>>,
    elapsed: RefCell<Duration>,
}
```

Every `start()`, `stop()`, `reset()`, and `elapsed()` call borrows a `RefCell`. If a stopwatch is used from multiple threads (despite not being `Send`), or if nested borrows occur, this panics. The `RefCell` pattern is used throughout `Profiler`, `FrameRateAnalyzer`, `PerfCounter`, and `JitterAnalyzer`.

**Impact:** Panic risk on nested borrows. Performance overhead of `RefCell` borrow checking on every timing call in the hot path.

**Fix:** Use `&mut self` methods or `UnsafeCell` with proper `Send`/`Sync` bounds.

---

### 755. perf.rs — `FrameRateAnalyzer.record_frame` uses `Vec::remove(0)` (line 309)
**File:** `wgpu-api/src/perf.rs` line 309

```rust
if times.len() > self.max_samples {
    times.remove(0);
}
```

`Vec::remove(0)` shifts all remaining elements by one position, making this O(n) per call. At 60 FPS with 360 samples, this shifts 359 elements every frame after the buffer fills.

**Impact:** O(n) per frame after buffer fills — ~360 element shifts per frame at 60 FPS.

**Fix:** Use a circular buffer or `VecDeque::pop_front()`.

---

## High Severity Issues

### 756. wayland.rs — `WaylandWindow` derives `Clone` (line 257)
**File:** `wgpu-api/src/wayland.rs` line 257

```rust
#[derive(Debug, Clone)]
pub struct WaylandWindow {
```

Cloning a `WaylandWindow` clones `WaylandWindowConfig` which contains `title: String`, `background: Option<Color>`, and `parent: Option<WaylandWindowId>`. The `Viewport` field is also cloned. For a dock with multiple windows, this is unnecessary heap allocation.

**Impact:** Per-clone heap allocation for window title and config.

**Fix:** Remove `Clone` or use `Arc` for shared config.

---

### 757. wayland.rs — `WaylandPopup` derives `Clone` (line 449)
**File:** `wgpu-api/src/wayland.rs` line 449

```rust
#[derive(Debug, Clone)]
pub struct WaylandPopup {
```

Same issue as `WaylandWindow` — `WaylandPopupConfig` contains `PopupAnchor` which has `Rect`, `PopupGravity`, and `Point`.

**Impact:** Per-clone allocation for popup config.

---

### 758. wayland.rs — `KeyEvent` derives `Clone` (line 511)
**File:** `wgpu-api/src/wayland.rs` line 511

```rust
#[derive(Debug, Clone)]
pub struct KeyEvent {
```

`KeyEvent` contains `ModifiersState` which is `Copy`, but the struct derives `Clone` unnecessarily.

**Impact:** Minor — `KeyEvent` could derive `Copy` instead of `Clone`.

**Fix:** Derive `Copy` for `KeyEvent`, `PointerEvent`, and `TouchEvent`.

---

### 759. wayland.rs — `PointerEvent` derives `Clone` (line 554)
**File:** `wgpu-api/src/wayland.rs` line 554

`PointerEvent` contains `PointerEventType` which has `PointerButton` and `PointerButtonState` — all `Copy`. The struct should derive `Copy`.

**Fix:** Derive `Copy` for `PointerEvent`.

---

### 760. wayland.rs — `TouchEvent` derives `Clone` (line 660)
**File:** `wgpu-api/src/wayland.rs` line 660

Same as above — all fields are `Copy`.

**Fix:** Derive `Copy` for `TouchEvent`.

---

### 761. wayland.rs — `InputEvent` derives `Clone` (line 673)
**File:** `wgpu-api/src/wayland.rs` line 673

`InputEvent` is an enum with variants that include `KeyEvent`, `PointerEvent`, and `TouchEvent`. Since these inner types could be `Copy`, the enum could also be `Copy` for most variants. However, `Configure` and `FocusChange` contain `WaylandWindowId` which is `Copy`. The `CloseRequest` variant is unit-like.

**Fix:** Make `InputEvent` `Copy` by making all inner types `Copy`.

---

### 762. wayland.rs — `VisualPopup` derives `Clone` (line 868)
**File:** `wgpu-api/src/wayland.rs` line 868

`VisualPopup` contains `Color` (Copy), `Point` (Copy), `Size` (Copy), and `Option<PopupShadow>` (Copy). The struct should derive `Copy`.

**Fix:** Derive `Copy` for `VisualPopup`.

---

### 763. wayland.rs — `OutputInfo` derives `Clone` (line 972)
**File:** `wgpu-api/src/wayland.rs` line 972

`OutputInfo` contains `name: String` and `description: String`. These are the only non-Copy fields. For a system with 2-3 monitors, this is minor.

**Impact:** Per-clone allocation for monitor name and description strings.

---

### 764. physics/springs.rs — `SpringChain.apply` creates HashMap per call (lines 266-302)
**File:** `wgpu-api/src/physics/springs.rs` lines 266-302

```rust
pub fn apply(&self, bodies_map: &mut std::collections::HashMap<BodyId, PhysicsBody>, dt: f32) {
    let mut force_map: std::collections::HashMap<BodyId, crate::geometry::Vector2D> =
        std::collections::HashMap::new();
```

Every call to `apply()` creates a new `HashMap` to accumulate forces. At 120 Hz physics timestep, this is 120 HashMap allocations per second.

**Impact:** Per-physics-step heap allocation.

**Fix:** Accept a pre-allocated force buffer or use a Vec of (BodyId, Vector2D) pairs.

---

### 765. physics/springs.rs — `SpringConstraint.compute_force` uses `sqrt` (line 73)
**File:** `wgpu-api/src/physics/springs.rs` line 73

```rust
let current_length = (dx * dx + dy * dy).sqrt();
```

`sqrt()` is called in `compute_force`, `compute_force_to_point`, and `is_at_rest`. At 120 Hz with N springs, this is 120×N sqrt calls per second.

**Impact:** CPU cost of sqrt in the hot physics path.

**Fix:** Use `sqrt` only when needed; consider reciprocal sqrt or length-squared comparison where possible.

---

### 766. morphing.rs — `MorphManager` uses three separate HashMaps (lines 530-537)
**File:** `wgpu-api/src/morphing.rs` lines 530-537

```rust
pub struct MorphManager {
    animations: std::collections::HashMap<u64, GeometryAnimation>,
    size_animations: std::collections::HashMap<u64, SizeAnimation>,
    popup_animations: std::collections::HashMap<u64, PopupAnimation>,
}
```

Three separate HashMaps mean three separate lookups for `stop()`, `advance()`, and `is_idle()`. A node can have a geometry, size, and popup animation simultaneously, leading to fragmented state.

**Impact:** Three HashMap lookups per operation instead of one.

**Fix:** Use a single HashMap with an enum value type.

---

### 767. morphing.rs — `PopupAnimation.evaluate` recomputes from/to rects every call (lines 464-489)
**File:** `wgpu-api/src/morphing.rs` lines 464-489

```rust
pub fn evaluate(&self) -> (Rect, f32) {
    let t = (self.elapsed / self.duration).clamp(0.0, 1.0);
    let e = self.easing.compute(t);
    let from_rect = if self.appearing { self.anchor } else { self.target };
    let to_rect = if self.appearing { self.target } else { self.anchor };
```

The `from_rect` and `to_rect` are determined by `self.appearing` on every call. This is a branch in the hot path.

**Impact:** Branch misprediction on every evaluate call.

**Fix:** Store `from_rect` and `to_rect` as fields set at construction time.

---

### 768. primitives.rs — `Path.rounded_rect` uses arcs with cubic bezier approximation (lines 76-121)
**File:** `wgpu-api/src/primitives.rs` lines 76-121

The `rounded_rect` method constructs a path using `arc()` commands, which store start/end angles but are not rasterized by the GPU backend. The GPU backend's `draw_commands_to_gpu_impl` ignores `Path` types entirely — rectangles are drawn as `DrawCommand::Rectangle`.

**Impact:** `rounded_rect` path is constructed but never used by GPU rendering. The GPU path draws sharp rectangles.

**Fix:** Integrate path rasterization into GPU backend or use GPU-friendly rounded rectangles.

---

### 769. primitives.rs — `Path.ellipse` uses cubic bezier approximation with KAPPA constant (lines 124-155)
**File:** `wgpu-api/src/primitives.rs` lines 124-155

The ellipse is approximated using 4 cubic Bezier segments with the KAPPA constant (0.5522847498307936). This is a standard approximation, but the resulting path is never rasterized by the GPU backend.

**Impact:** Dead code — ellipse paths are constructed but not rendered on GPU.

---

### 770. primitives.rs — `Path` commands Vec grows unbounded (line 16)
**File:** `wgpu-api/src/primitives.rs` line 16

```rust
pub commands: Vec<PathCommand>,
```

Every `move_to`, `line_to`, `arc`, `cubic_bezier_to`, etc. pushes to the commands Vec. For complex paths, this can grow large. The `append()` method extends the Vec with another path's commands.

**Impact:** Per-path allocation proportional to command count.

**Fix:** Pre-allocate with a reasonable capacity or use a smallVec pattern.

---

### 771. perf.rs — `Profiler` uses `RefCell` for all internal state (lines 646-659)
**File:** `wgpu-api/src/perf.rs` lines 646-659

```rust
pub struct Profiler {
    phases: RefCell<HashMap<RenderPhase, PhaseTiming>>,
    frame_rate: FrameRateAnalyzer,
    counters: RefCell<HashMap<String, PerfCounter>>,
    active_phase: RefCell<Option<(RenderPhase, Instant)>>,
    enabled: RefCell<bool>,
    frame_count: RefCell<u64>,
}
```

Every method on `Profiler` borrows a `RefCell`. `begin_phase()`, `end_phase()`, `record_frame()`, `set_counter()`, `counter()`, `phase_timing()`, `all_phases()`, `snapshot()`, `report()` — all use `borrow()` or `borrow_mut()`. This is 6 `RefCell` instances, each with borrow-check overhead on every call.

**Impact:** Performance overhead on every profiling call in the hot render path. Panic risk on nested borrows.

**Fix:** Use `&mut self` or make Profiler interior-mutable with a single `RefCell` wrapping the entire state.

---

### 772. perf.rs — `JitterAnalyzer.record` uses `Vec::remove(0)` (line 1002)
**File:** `wgpu-api/src/perf.rs` line 1002

```rust
if times.len() > self.max_samples {
    times.remove(0);
}
```

Same O(n) issue as `FrameRateAnalyzer`. With 60 samples, this shifts 59 elements per frame.

**Impact:** O(n) per frame — 60 element shifts per frame.

**Fix:** Use `VecDeque` or circular buffer.

---

### 773. perf.rs — `Profiler.counter()` clones PerfCounter (line 746)
**File:** `wgpu-api/src/perf.rs` line 746

```rust
pub fn counter(&self, name: &str) -> PerfCounter {
    let mut counters = self.counters.borrow_mut();
    counters
        .entry(name.to_string())
        .or_insert_with(|| PerfCounter::new(name))
        .clone()
}
```

`counter()` returns a cloned `PerfCounter`. The clone shares `RefCell` state with the original, so `set()` on the clone updates the stored counter. However, `name.to_string()` allocates a new String per call.

**Impact:** Per-call String allocation.

**Fix:** Accept `&str` and use `Cow` or intern the name.

---

### 774. perf.rs — `Profiler.set_counter()` allocates String per call (line 756)
**File:** `wgpu-api/src/perf.rs` line 756

```rust
let counter = counters
    .entry(name.to_string())
    .or_insert_with(|| PerfCounter::new(name));
```

`name.to_string()` allocates a new String for every `set_counter()` call. At 60 FPS with 5 counters, this is 300 String allocations per second.

**Impact:** Per-call heap allocation.

**Fix:** Use `&'static str` for counter names or intern the strings.

---

### 775. perf.rs — `Profiler.snapshot()` clones entire phases HashMap (line 802)
**File:** `wgpu-api/src/perf.rs` line 802

```rust
phases: self.phases.borrow().clone(),
```

Taking a snapshot clones the entire phases HashMap, including all `PhaseTiming` entries. This is O(phases) allocation.

**Impact:** Per-snapshot heap allocation.

**Fix:** Return a reference or use `Arc` for shared snapshot data.

---

### 776. perf.rs — `Profiler.report()` allocates Vec and Strings (lines 829-846)
**File:** `wgpu-api/src/perf.rs` lines 829-846

```rust
pub fn report(&self) -> String {
    let phases: Vec<String> = self.phases.borrow().values().map(|p| p.report()).collect();
    let counters: Vec<String> = self.counters.borrow().values().map(|c| c.report()).collect();
```

Every call to `report()` allocates a Vec of Strings for phases and counters, then joins them. This is O(phases + counters) allocations.

**Impact:** Per-report heap allocation.

**Fix:** Use `write!` to a pre-allocated buffer or return a `Cow<str>`.

---

## Medium Severity Issues

### 777. wayland.rs — `WaylandWindowConfig` default size is 800×600 (line 190)
**File:** `wgpu-api/src/wayland.rs` line 190

```rust
size: Size::new(800.0, 600.0),
```

The default window size is 800×600, which is inappropriate for a dock. The dock is typically 400-800px wide and 60px tall.

**Impact:** Inconsistency — default size doesn't match dock's actual dimensions.

**Fix:** Use dock-appropriate defaults or require explicit size.

---

### 778. wayland.rs — `WaylandPopupConfig` default anchor rect is zero-sized (lines 419-423)
**File:** `wgpu-api/src/wayland.rs` lines 419-423

```rust
anchor: PopupAnchor::new(
    Rect::new(0.0, 0.0, 0.0, 0.0),
    PopupGravity::default(),
    Point::new(0.0, 0.0),
),
```

The default anchor is a zero-sized rect at (0,0). A popup created with default config will be anchored at the origin, which may not be visible.

**Impact:** Popups with default config appear at (0,0).

---

### 779. wayland.rs — `destroy_window` removes popups but doesn't clean up dismissed ones (line 748)
**File:** `wgpu-api/src/wayland.rs` line 748

```rust
self.popups.retain(|_, popup| popup.parent != id);
```

When a window is destroyed, all its popups are removed, including dismissed ones. Dismissed popups are already invisible and could be cleaned up lazily.

**Impact:** Minor — dismissed popups are retained until the parent window is destroyed.

**Fix:** Clean up dismissed popups periodically or on the next event.

---

### 780. wayland.rs — `dismiss_popups_for_window` dismisses and removes in one pass (lines 786-795)
**File:** `wgpu-api/src/wayland.rs` lines 786-795

```rust
pub fn dismiss_popups_for_window(&mut self, window_id: WaylandWindowId) {
    self.popups.retain(|_, popup| {
        if popup.parent == window_id {
            popup.dismiss();
            false
        } else {
            true
        }
    });
}
```

The method dismisses and removes popups in a single `retain()` call. The `dismiss()` call is redundant since the popup is immediately removed from the HashMap.

**Impact:** Redundant state mutation before removal.

**Fix:** Remove directly without calling `dismiss()`.

---

### 781. wayland.rs — `popup_count()` filters active popups on every call (line 851)
**File:** `wgpu-api/src/wayland.rs` line 851

```rust
pub fn popup_count(&self) -> usize {
    self.popups.values().filter(|p| p.is_active()).count()
}
```

Every call iterates all popups and filters by `is_active()`. This is O(n) per call.

**Impact:** O(n) per call instead of O(1).

**Fix:** Maintain an active popup count or use a separate HashMap for active popups.

---

### 782. physics/springs.rs — `SpringConstraint` derives `Clone` (line 13)
**File:** `wgpu-api/src/physics/springs.rs` line 13

```rust
#[derive(Debug, Clone)]
pub struct SpringConstraint {
```

`SpringConstraint` contains `BodyId` (Copy), `Option<BodyId>` (Copy), `Option<Point>` (Copy), and f32 fields. The struct should derive `Copy`.

**Fix:** Derive `Copy` for `SpringConstraint`.

---

### 783. physics/springs.rs — `DistanceConstraint` derives `Clone` (line 163)
**File:** `wgpu-api/src/physics/springs.rs` line 163

Same issue — all fields are `Copy`.

**Fix:** Derive `Copy` for `DistanceConstraint`.

---

### 784. physics/springs.rs — `SpringChain` derives `Clone` (line 223)
**File:** `wgpu-api/src/physics/springs.rs` line 223

`SpringChain` contains `Vec<BodyId>` which is not `Copy`. The `Clone` derive is correct here, but the Vec is cloned on every clone operation.

**Impact:** Per-clone allocation for body IDs.

---

### 785. physics/springs.rs — `RadialSpringSystem` derives `Clone` (line 306)
**File:** `wgpu-api/src/physics/springs.rs` line 306

All fields are `Copy` (Point, f32). Should derive `Copy`.

**Fix:** Derive `Copy` for `RadialSpringSystem`.

---

### 786. morphing.rs — `GeometryAnimation` derives `Clone` and `PartialEq` (line 62)
**File:** `wgpu-api/src/morphing.rs` line 62

`GeometryAnimation` contains `Rect` (Copy), `f32` (Copy), and `Easing` (Copy). Should derive `Copy`.

**Fix:** Derive `Copy` for `GeometryAnimation`.

---

### 787. morphing.rs — `SizeAnimation` derives `Clone` and `PartialEq` (line 199)
**File:** `wgpu-api/src/morphing.rs` line 199

Same issue — all fields are `Copy`.

**Fix:** Derive `Copy` for `SizeAnimation`.

---

### 788. morphing.rs — `PositionAnimation` derives `Clone` and `PartialEq` (line 273)
**File:** `wgpu-api/src/morphing.rs` line 273

Same issue — all fields are `Copy`.

**Fix:** Derive `Copy` for `PositionAnimation`.

---

### 789. morphing.rs — `PopupAnimation` derives `Clone` and `PartialEq` (line 395)
**File:** `wgpu-api/src/morphing.rs` line 395

All fields are `Copy` (Rect, f32, Easing, bool). Should derive `Copy`.

**Fix:** Derive `Copy` for `PopupAnimation`.

---

### 790. src/render/tray.rs — `blend_pixel` uses f32 alpha blending per pixel (lines 122-133)
**File:** `src/render/tray.rs` lines 122-133

```rust
fn blend_pixel(canvas: &mut [u8], index: usize, color: [u8; 3], alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    canvas[index] = (color[2] as f32 * a + canvas[index] as f32 * inv).round() as u8;
    canvas[index + 1] = (color[1] as f32 * a + canvas[index + 1] as f32 * inv).round() as u8;
    canvas[index + 2] = (color[0] as f32 * a + canvas[index + 2] as f32 * inv).round() as u8;
    canvas[index + 3] = 255;
}
```

Every pixel blend converts u8→f32, multiplies, adds, rounds, and converts back to u8. This is 6 f32→u8 conversions, 4 f32 multiplications, and 3 f32 additions per pixel. The `draw_status_arrow` function calls this in nested loops over every pixel of the arrow shape.

**Impact:** Per-pixel f32 operations without SIMD. For a 24×24 icon area, this is 576 pixels × ~15 f32 ops = ~8,640 f32 operations per icon.

**Fix:** Use integer alpha blending or SIMD intrinsics.

---

## Summary of Issues by Severity

 #  | Severity | Issue | Impact |
----|----------|-------|--------|
 751 | **Critical** | FocusChange iterates all windows | O(n) per focus event |
 752 | **Critical** | CloseRequest scans all windows | O(n) per close, no window_id |
 753 | **Critical** | WaylandConnection has no Wayland socket | Mock/stub, no real Wayland |
 754 | **Critical** | Stopwatch uses RefCell | Panic risk, overhead |
 755 | **Critical** | record_frame uses Vec::remove(0) | O(n) per frame |
 756 | **High** | WaylandWindow derives Clone | Per-clone allocation |
 757 | **High** | WaylandPopup derives Clone | Per-clone allocation |
 758 | **High** | KeyEvent derives Clone, not Copy | Unnecessary clone |
 759 | **High** | PointerEvent derives Clone, not Copy | Unnecessary clone |
 760 | **High** | TouchEvent derives Clone, not Copy | Unnecessary clone |
 761 | **High** | InputEvent derives Clone, not Copy | Unnecessary clone |
 762 | **High** | VisualPopup derives Clone, not Copy | Unnecessary clone |
 763 | **High** | OutputInfo derives Clone | Per-clone allocation |
 764 | **High** | SpringChain.apply creates HashMap | Per-physics-step allocation |
 765 | **High** | SpringConstraint uses sqrt | CPU cost in hot path |
 766 | **High** | MorphManager uses 3 HashMaps | 3 lookups per operation |
 767 | **High** | PopupAnimation branch on appearing | Branch misprediction |
 768 | **High** | Path.rounded_rect not rendered on GPU | Dead code for GPU path |
 769 | **High** | Path.ellipse not rendered on GPU | Dead code for GPU path |
 770 | **High** | Path commands Vec grows unbounded | Per-path allocation |
 771 | **High** | Profiler uses 6 RefCells | Overhead, panic risk |
 772 | **High** | JitterAnalyzer uses Vec::remove(0) | O(n) per frame |
 773 | **High** | Profiler.counter() clones PerfCounter | Per-call String alloc |
 774 | **High** | Profiler.set_counter() allocates String | Per-call heap alloc |
 775 | **High** | Profiler.snapshot() clones HashMap | Per-snapshot alloc |
 776 | **High** | Profiler.report() allocates Vec+Strings | Per-report alloc |
 777 | **Medium** | WaylandWindowConfig default 800x600 | Wrong for dock |
 778 | **Medium** | PopupConfig default anchor zero-sized | Popups at origin |
 779 | **Medium** | destroy_window doesn't clean dismissed | Retained dismissed popups |
 780 | **Medium** | dismiss_popups redundant dismiss() | Redundant mutation |
 781 | **Medium** | popup_count() O(n) filter | O(n) per call |
 782 | **Medium** | SpringConstraint derives Clone, not Copy | Unnecessary clone |
 783 | **Medium** | DistanceConstraint derives Clone, not Copy | Unnecessary clone |
 784 | **Medium** | SpringChain derives Clone | Per-clone allocation |
 785 | **Medium** | RadialSpringSystem derives Clone, not Copy | Unnecessary clone |
 786 | **Medium** | GeometryAnimation derives Clone, not Copy | Unnecessary clone |
 787 | **Medium** | SizeAnimation derives Clone, not Copy | Unnecessary clone |
 788 | **Medium** | PositionAnimation derives Clone, not Copy | Unnecessary clone |
 789 | **Medium** | PopupAnimation derives Clone, not Copy | Unnecessary clone |
 790 | **Medium** | blend_pixel f32 per-pixel ops | No SIMD, slow blending |

---

## Cumulative Summary

 Round | File | Issues | New Running Total |
-------|------|--------|-------------------|
 1 | gpu-rendering-review.md | 1-12 | 12 |
 2 | gpu-rendering-review-2.md | 13-32 | 32 |
 3 | gpu-rendering-review-3.md | 33-60 | 62 |
 4 | gpu-rendering-review-4.md | 61-95 | 97 |
 5 | gpu-rendering-review-5.md | 96-120 | 120 |
 6 | gpu-rendering-review-6.md | 121-155 | 155 |
 7 | gpu-rendering-review-7.md | 156-199 | 199 |
 8 | gpu-rendering-review-8.md | 200-240 | 240 |
 9 | gpu-rendering-review-9.md | 241-280 | 280 |
 10 | gpu-rendering-review-10.md | 281-330 | 330 |
 11 | gpu-rendering-review-11.md | 331-370 | 370 |
 12 | gpu-rendering-review-12.md | 371-420 | 420 |
 13 | gpu-rendering-review-13.md | 421-460 | 460 |
 14 | gpu-rendering-review-14.md | 461-505 | 505 |
 15 | gpu-rendering-review-15.md | 506-550 | 550 |
 16 | gpu-rendering-review-16.md | 551-595 | 595 |
 17 | gpu-rendering-review-17.md | 596-630 | 630 |
 18 | gpu-rendering-review-18.md | 631-670 | 670 |
 19 | gpu-rendering-review-19.md | 671-710 | 710 |
 20 | gpu-rendering-review-20.md | 711-750 | 750 |
 **21** | **gpu-rendering-review-21.md** | **751-790** | **790** |

**Cumulative severity across all 21 rounds:**
- Critical: 100 (was 95 + 5 new)
- High: 222 (was 196 + 26 new)
- Medium: 321 (was 295 + 26 new)
- Low: 78 (no new low issues this round)
- **Total: 790 issues**

### Key Themes from Round 21

1. **WaylandConnection is a mock**: The entire `WaylandConnection` struct has no actual Wayland socket, display, or event queue. It is a pure-Rust state machine that simulates window management. The real Wayland integration lives in `src/listeners/wayland/`. This creates a dual architecture where the wgpu-api module's Wayland types are disconnected from the actual compositor.

2. **O(n) operations in Wayland event handling**: `FocusChange` iterates all windows instead of looking up the target directly. `CloseRequest` has no `window_id` and scans all windows. `popup_count()` filters all popups on every call. These are O(n) where O(1) is achievable.

3. **RefCell everywhere in profiling**: `Stopwatch`, `Profiler`, `FrameRateAnalyzer`, `PerfCounter`, and `JitterAnalyzer` all use `RefCell` for interior mutability. This adds borrow-check overhead to every timing call in the hot render path and introduces panic risk on nested borrows.

4. **Vec::remove(0) in hot paths**: Both `FrameRateAnalyzer` and `JitterAnalyzer` use `Vec::remove(0)` to maintain a circular buffer. This is O(n) per frame, shifting all elements. A `VecDeque` or true circular buffer would be O(1).

5. **Per-call String allocations in Profiler**: `counter()`, `set_counter()`, `snapshot()`, and `report()` all allocate Strings or clone HashMaps. At 60 FPS, this is hundreds of allocations per second in the profiling path.

6. **Clone vs Copy misuse**: 10+ types in wayland.rs, physics/springs.rs, and morphing.rs derive `Clone` when all fields are `Copy`. This adds unnecessary clone overhead where a simple copy would suffice.

7. **Path primitives not rendered on GPU**: `rounded_rect()`, `ellipse()`, and other path construction methods in primitives.rs create `Path` objects with `PathCommand` vectors, but the GPU backend's `draw_commands_to_gpu_impl` never rasterizes paths. The GPU path uses `DrawCommand::Rectangle` for all shapes, ignoring rounded corners.

8. **SpringChain allocates HashMap per physics step**: At 120 Hz, `SpringChain::apply()` creates a new HashMap to accumulate forces. This is 120 HashMap allocations per second, violating the "no steady-state heap allocations" rule.

9. **Per-pixel f32 blending in tray renderer**: `blend_pixel()` converts u8→f32, performs alpha blending in f32, and converts back to u8 for every pixel. No SIMD, no integer blending. For a 24×24 icon, this is ~8,640 f32 operations.

10. **MorphManager uses 3 HashMaps**: Three separate HashMaps for geometry, size, and popup animations mean three lookups per operation. A single HashMap with an enum value type would be more efficient.
