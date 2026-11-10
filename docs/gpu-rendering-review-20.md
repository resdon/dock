# GPU Rendering Code Review — Round 20 (Scene Graph, Text, Slint Bridge, Library Root, Registry, Config)

This is a continuation of the GPU rendering code review. Rounds 1–19 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, documentation, render submodules, state modules, services, physics interaction, groups, input handling, popup rendering, tray rendering, error handling, geometry, models, cache, and resolvers (issues 1–710). This round covers: `wgpu-api/src/scene.rs`, `wgpu-api/src/text.rs`, `wgpu-api/src/slint.rs`, `wgpu-api/src/lib.rs`, `src/registry.rs`, and `src/config.rs` — issues 711–750.

---

## Critical Issues

### 711. scene.rs — `hit_test` ignores node transforms (scale, rotation, anchor) (lines 861-874)
**File:** `wgpu-api/src/scene.rs` lines 861-874

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

`bounding_rect()` returns `Rect::new(position.x, position.y, size.width, size.height)` — it completely ignores `scale`, `rotation`, and `anchor` from `NodeStyle`. A node scaled to 2x will be hit-tested at its unscaled size. A rotated node will be hit-tested as if unrotated.

**Impact:** Hit testing is incorrect for any node with non-identity transform. Clicking on a scaled icon will miss.

**Fix:** Apply the node's transform matrix to the hit test point before checking against the bounding rect, or compute the transformed bounding rect.

---

### 712. scene.rs — `hit_test` uses parent-local coordinates only, not world coordinates (lines 861-874)
**File:** `wgpu-api/src/scene.rs` lines 861-874

The hit test checks each node's `bounding_rect()` which is in parent-local coordinates. For nested nodes (children of groups), the hit test point should be transformed into the node's local coordinate space. Currently, a child node at position (10, 10) inside a group at position (100, 100) will be hit-tested at (10, 10) instead of (110, 110).

**Impact:** Nested nodes are never hit correctly. Popups and grouped UI elements are unclickable.

**Fix:** Accumulate the world transform as you traverse the tree and transform the hit point into each node's local space.

---

### 713. scene.rs — `visible_nodes()` z-index sort is unstable (lines 814-821)
**File:** `wgpu-api/src/scene.rs` lines 814-821

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

`sort_by_key` is stable in Rust, but nodes with the same z-index retain their traversal order. However, the traversal order is DFS from roots, which means siblings are visited in insertion order. The issue is that `traverse_visible` uses a stack (LIFO), so children are pushed in reverse order and popped in forward order. But the z-index sort then reorders across all nodes, not just siblings. Two nodes in different subtrees with the same z-index will be ordered by traversal order, which may not match visual stacking.

**Impact:** Z-ordering across different subtrees is non-deterministic for same z-index values.

**Fix:** Use `sort_by` with a comparison that includes parent path for tie-breaking.

---

### 714. scene.rs — Scene graph is completely bypassed by GPU rendering path (entire file)
**File:** `wgpu-api/src/scene.rs` (1062 lines)

The entire scene graph module — `Scene`, `Node`, `NodeStyle`, `NodeContent`, hit testing, traversal, z-ordering — is **never used** by the actual GPU rendering path. In `src/app/dock.rs`, the GPU path constructs `DrawCommand` vectors directly, bypassing the scene graph entirely. The scene graph is a well-designed retained mode system that sits unused.

**Impact:** 1062 lines of dead code. The scene graph was designed to be the core of the rendering architecture, but the GPU path uses an immediate-mode approach with `DrawCommand` arrays.

**Fix:** Either integrate the scene graph into the GPU rendering path or remove it.

---

### 715. slint.rs — `SlintWgpuAdapter` doesn't share wgpu device with wgpu-api renderer (lines 807-857)
**File:** `wgpu-api/src/slint.rs` lines 807-857

```rust
pub struct SlintWgpuAdapter {
    initialized: bool,
}
```

The adapter has no wgpu device, queue, or instance. When `select_backend()` is called, it tells Slint to use wgpu, but Slint creates its own independent wgpu device. The wgpu-api renderer also creates its own device. This means two separate Vulkan devices exist, doubling VRAM usage and preventing texture sharing.

**Impact:** Architecture invariant violation — the renderer should own the wgpu device, but Slint creates a second one. This defeats the purpose of the adapter.

**Fix:** Pass the renderer's device and queue to Slint via the adapter, or render Slint to a texture that the renderer consumes.

---

## High Severity Issues

### 716. scene.rs — `traverse_visible` stack-based DFS doesn't respect z-order (lines 791-811)
**File:** `wgpu-api/src/scene.rs` lines 791-811

```rust
pub fn traverse_visible<F>(&self, mut visit: F)
where
    F: FnMut(&Node),
{
    let mut stack = Vec::new();
    for root_id in &self.roots {
        stack.push(*root_id);
    }
    while let Some(id) = stack.pop() {
        if let Some(node) = self.nodes.get(&id) {
            if node.is_visible() {
                visit(node);
            }
            for child_id in node.children.iter().rev() {
                stack.push(*child_id);
            }
        }
    }
}
```

The traversal visits nodes in DFS order, not z-order. A child with z-index 100 is visited before a sibling with z-index 1, because DFS visits children before moving to the next sibling. The `visible_nodes()` method sorts by z-index after traversal, but `traverse_visible` itself does not.

**Impact:** Any code that uses `traverse_visible` directly (not through `visible_nodes`) will get incorrect render order.

**Fix:** Sort children by z-index before pushing to the stack, or use a different traversal algorithm.

---

### 717. scene.rs — `remove_node_recursive` clones children Vec (lines 666-693)
**File:** `wgpu-api/src/scene.rs` lines 666-693

```rust
fn remove_node_recursive(&mut self, id: NodeId) {
    let children: Vec<NodeId> = self
        .nodes
        .get(&id)
        .map(|node| node.children.clone())
        .unwrap_or_default();
```

Every recursive removal clones the children vector. For a deep tree, this is O(depth × avg_children) allocations.

**Impact:** Per-removal heap allocation proportional to tree depth.

**Fix:** Use a stack-based approach that avoids cloning.

---

### 718. scene.rs — `Node.data` is `Option<String>` (line 462)
**File:** `wgpu-api/src/scene.rs` line 462

```rust
pub data: Option<String>,
```

User data attached to nodes is a String. This is a catch-all that could hold anything. For a dock with 50 nodes, each holding a 50-byte string, this is 2.5 KB of rarely-used data.

**Impact:** Memory waste and type safety — any string can be stored, no structure.

**Fix:** Use a typed enum or remove the field if unused.

---

### 719. text.rs — `estimate_width` uses hardcoded 0.6 multiplier (lines 259-264)
**File:** `wgpu-api/src/text.rs` lines 259-264

```rust
pub fn estimate_width(&self) -> f32 {
    let char_count = self.content.chars().count() as f32;
    let avg_char_width = self.style.font_size * 0.6;
    let letter_spacing = self.style.letter_spacing * char_count;
    char_count * avg_char_width + letter_spacing
}
```

The average character width is estimated as `font_size * 0.6`, which is a rough approximation. For monospace fonts, this is wrong. For CJK fonts, this is very wrong. The estimate can be off by 50% or more.

**Impact:** Text wrapping and layout based on this estimate will be incorrect for many fonts.

**Fix:** Use actual font metrics or a more sophisticated estimation.

---

### 720. text.rs — `wrap_lines` allocates String per word (lines 277-314)
**File:** `wgpu-api/src/text.rs` lines 277-314

```rust
for word in paragraph.split_whitespace() {
    let word_chars = word.chars().count() as f32;
    // ...
    current_line.push_str(word);
```

Every word creates a new String allocation via `push_str`. For a 100-word paragraph, this is 100 String allocations. The `lines.push(std::mem::take(&mut current_line))` also allocates per line.

**Impact:** Per-wrap heap allocation — violates "no steady-state heap allocations" rule.

**Fix:** Pre-allocate a buffer and use string slices.

---

### 721. text.rs — `wrap_lines` doesn't handle CJK or other non-whitespace-separated scripts (lines 282-285)
**File:** `wgpu-api/src/text.rs` lines 282-285

```rust
for word in paragraph.split_whitespace() {
```

CJK text has no whitespace between characters. The entire CJK paragraph becomes one "word" and wraps as a single line, potentially overflowing the max width.

**Impact:** CJK text wrapping is broken.

**Fix:** Use grapheme-based wrapping for scripts without whitespace.

---

### 722. slint.rs — `DockItem` has 22 fields (lines 90-140)
**File:** `wgpu-api/src/slint.rs` lines 90-140

`DockItem` stores name, icon, running, loading, badge_count, badge_color, window_count, app_id, index_on_dock, focused, open_animation, pinned, minimized, is_hovered, is_dragging, offset_x, anim_scale, icon_width, icon_height, icon_x, icon_y, and 2 more. Many of these are derivable: `icon_width` = `icon_size * anim_scale`, `icon_x` = `(slot_size - icon_width) / 2`, etc.

**Impact:** Memory waste and consistency risk — derived fields can drift from their source values.

**Fix:** Compute derived fields on-demand via accessor methods.

---

### 723. slint.rs — `DockWidget::hit_test` doesn't account for animation scale in hit area (lines 317-327)
**File:** `wgpu-api/src/slint.rs` lines 317-327

```rust
pub fn hit_test(&self, point: Point) -> Option<i32> {
    for (i, item) in self.dock_items.iter().enumerate() {
        let x = self.item_x(i as i32) as f32 + item.offset_x + item.icon_x;
        let y = item.icon_y;
        let rect = Rect::new(x, y, item.icon_width, item.icon_height);
        if rect.contains(&point) {
            return Some(i as i32);
        }
    }
    None
}
```

The hit test uses `icon_x` and `icon_y` which are computed by `set_anim_scale`. When `anim_scale` is 1.25, `icon_x` becomes negative (centering offset). The hit rect is shifted left by the centering offset, which is correct for visual positioning but means the hit area moves during animation. A user clicking near the edge of an icon may miss as it animates.

**Impact:** Hit testing is inconsistent during animation — the hit area shifts as the icon scales.

**Fix:** Use the slot's bounding box (48×48) for hit testing, not the animated icon's bounding box.

---

### 724. slint.rs — `badge_text()` allocates String per call (lines 195-201)
**File:** `wgpu-api/src/slint.rs` lines 195-201

```rust
pub fn badge_text(&self) -> String {
    if self.badge_count > 9 {
        "9+".to_string()
    } else {
        self.badge_count.to_string()
    }
}
```

Every call allocates a new String. For a dock with 10 icons, this is 10 String allocations per frame if called during rendering.

**Impact:** Per-frame heap allocation.

**Fix:** Return `&'static str` for "9+" and use a small buffer for the numeric case.

---

### 725. slint.rs — `BluetoothState` has 24 fields (lines 537-582)
**File:** `wgpu-api/src/slint.rs` lines 537-582

`BluetoothState` stores power_on, scanning, minimized, settings_open, adapter_name, sweep_cos, sweep_sin, dot1_x, dot1_y, dot2_x, dot2_y, dot3_x, dot3_y, scroll_offset, paired_scroll_offset, scroll_thumb_y, scroll_thumb_h, paired_scroll_thumb_y, paired_scroll_thumb_h, last_action, footer_text, connected_count, and 2 more. The animation state (sweep_cos, sweep_sin, dot positions) is stored as separate fields instead of a single angle.

**Impact:** Cognitive complexity and memory waste.

**Fix:** Store a single angle for sweep animation and compute cos/sin on-demand.

---

### 726. slint.rs — `WidgetRegistry` stores `DockWidget` and `BluetoothWidget` as `Option` (lines 907-916)
**File:** `wgpu-api/src/slint.rs` lines 907-916

```rust
pub struct WidgetRegistry {
    dock_widget: Option<DockWidget>,
    bluetooth_widget: Option<BluetoothWidget>,
}
```

These are `Option` but the registry is created without them, meaning every access returns `None` until explicitly set. The dock widget is the primary widget and should always exist.

**Impact:** API confusion — callers must handle `None` for a widget that should always exist.

**Fix:** Make `dock_widget` a required field in the constructor.

---

### 727. lib.rs — 300+ re-exports create a flat namespace with naming conflicts (lines 117-364)
**File:** `wgpu-api/src/lib.rs` lines 117-364

The crate root re-exports over 300 types from all modules into a single flat namespace. This creates naming conflicts:
- `DockItem` from `slint` vs `PhysicsDockItem` from `physics` (aliased)
- `AttractionConstraint` vs `PhysicsAttractionConstraint` (aliased)
- `SpringConstraint` vs `PhysicsSpringConstraint` (aliased)
- `Dock` vs `PhysicsDock` (aliased)
- `DockConfig` vs `PhysicsDockConfig` (aliased)
- `DockDragState` vs `PhysicsDockDragState` (aliased)

The aliases (`as Physics...`) are a workaround for the naming conflict, not a solution.

**Impact:** Cognitive complexity — importing from `wgpu_api::` pulls in 300+ types. IDE autocomplete is overwhelmed.

**Fix:** Use module-based imports instead of flat re-exports.

---

### 728. lib.rs — Public API purity invariant violated by re-exporting wgpu-adjacent types (lines 204-210)
**File:** `wgpu-api/src/lib.rs` lines 204-210

```rust
pub use gpu::{
    BlendMode as GpuBlendMode, BufferCache, BufferType, CachedBuffer, CachedGlyph, CachedPipeline,
    CachedTexture, FrameStats, GpuBackendConfig, GpuResources, GpuUniformBuffer,
    OrthographicProjection, PipelineCache, PipelineKey, PipelineType, PresentationError,
    QuadInstance, SurfaceAlphaMode, SurfacePresentationConfig, SurfacePresentationState,
    SurfacePresenter, TextureCache,
};
```

The AGENTS.md states: "The `wgpu-api` public API is **independent of wgpu types**." However, `GpuBackendConfig`, `GpuResources`, `GpuUniformBuffer`, `OrthographicProjection`, `PipelineCache`, `PipelineKey`, `PipelineType`, `PresentationError`, `QuadInstance`, `SurfaceAlphaMode`, `SurfacePresentationConfig`, `SurfacePresentationState`, `SurfacePresenter`, and `TextureCache` are all GPU backend internals that should not be part of the public API.

**Impact:** Architecture invariant violation — application code can access GPU backend internals directly.

**Fix:** Make these types `pub(crate)` or remove from re-exports.

---

### 729. registry.rs — `SystemRegistry` derives `Clone` (line 32)
**File:** `src/registry.rs` line 32

```rust
#[derive(Default, Debug, Clone)]
pub struct SystemRegistry {
```

Cloning `SystemRegistry` clones all HashMaps, including `windows`, `applications`, `processes`, `outputs`, and all index maps. For 300 processes, 100 windows, and 50 applications, this is a massive clone operation.

**Impact:** Performance — cloning the registry is O(n) over all entries.

**Fix:** Remove `Clone` derive and use `Arc` or references where sharing is needed.

---

### 730. registry.rs — `refresh_application_status` clones all window records (lines 594-624)
**File:** `src/registry.rs` lines 594-624

```rust
pub fn refresh_application_status(&mut self, app_id: &AppId) {
    let (window_ids, process_count) = if let Some(app) = self.applications.get(app_id) {
        (app.windows.clone(), app.processes.len())
    } else {
        return;
    };
    let windows: Vec<WindowRecord> = window_ids
        .iter()
        .filter_map(|id| self.windows.get(id).cloned())
        .collect();
```

`app.windows.clone()` clones the Vec of WindowIds. Then `self.windows.get(id).cloned()` clones each `WindowRecord` (which has 20+ fields including `MediaWindowLink` with 15 Option fields). For an app with 10 windows, this is 10 full `WindowRecord` clones.

**Impact:** Per-status-refresh heap allocation proportional to window count.

**Fix:** Use references instead of cloning.

---

## Medium Severity Issues

### 731. registry.rs — `push_unique` is O(n) linear scan (lines 20-23)
**File:** `src/registry.rs` lines 20-23

```rust
fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}
```

Every call to `push_unique` scans the entire vector. For a frequently-called function like `attach_window_indexes`, this is O(n) per index update.

**Impact:** Performance — O(n) scan on every index attachment.

**Fix:** Use a HashSet for uniqueness checking or maintain sorted order for binary search.

---

### 732. registry.rs — 10+ HashMap indexes must stay in sync (lines 33-52)
**File:** `src/registry.rs` lines 33-52

```rust
pub windows_by_app: HashMap<AppId, Vec<WindowId>>,
pub windows_by_process: HashMap<ProcessId, Vec<WindowId>>,
pub windows_by_output: HashMap<OutputId, Vec<WindowId>>,
pub windows_by_status: HashMap<WindowStatus, Vec<WindowId>>,
pub windows_by_priority: HashMap<WindowPriority, Vec<WindowId>>,
pub processes_by_app: HashMap<AppId, Vec<ProcessId>>,
pub processes_by_binary_path: HashMap<PathBuf, Vec<ProcessId>>,
pub processes_by_binary_name: HashMap<String, Vec<ProcessId>>,
```

Every window or process registration must update all relevant indexes. If any index is missed, the registry becomes inconsistent. The `validate_indexes` method (line 629) can detect corruption, but it's a debug/test method, not a runtime invariant.

**Impact:** Maintenance risk — adding a new index requires updating all attach/detach methods.

**Fix:** Use a single source of truth and compute indexes on-demand, or use a transactional approach.

---

### 733. registry.rs — `register_process` clones the process record (lines 163-184)
**File:** `src/registry.rs` lines 163-184

```rust
pub fn register_process(&mut self, mut process: ProcessRecord, app_id: Option<AppId>) {
    // ...
    if let Some(process) = self.processes.get(&pid).cloned() {
        self.attach_process_indexes(&process);
    }
}
```

After inserting the process, it's cloned again for `attach_process_indexes`. The `ProcessRecord` has 22 fields including `env_vars: HashMap<String, String>`.

**Impact:** Per-registration heap allocation proportional to process record size.

**Fix:** Pass a reference to the inserted record instead of cloning.

---

### 734. registry.rs — `register_window` clones the window record (lines 263-275)
**File:** `src/registry.rs` lines 263-275

```rust
pub fn register_window(&mut self, window: WindowRecord) {
    let id = window.id.clone();
    if let Some(existing) = self.windows.remove(&id) {
        self.detach_window_indexes(&existing);
    }
    self.windows.insert(id.clone(), window);
    if let Some(window) = self.windows.get(&id).cloned() {
        self.attach_window_indexes(&window);
    }
}
```

Same pattern as `register_process` — the window is cloned after insertion. `WindowRecord` has 20+ fields.

**Impact:** Per-registration heap allocation.

**Fix:** Same as #733.

---

### 735. registry.rs — `set_process_binary` clones the process record (lines 515-527)
**File:** `src/registry.rs` lines 515-527

```rust
pub fn set_process_binary(&mut self, pid: ProcessId, binary: BinaryContext) {
    // ...
    let snapshot = {
        let process = self.ensure_process(pid);
        process.binary = binary;
        process.status.running = true;
        process.clone()
    };
    self.attach_process_indexes(&snapshot);
}
```

Explicit `.clone()` of the process record.

**Impact:** Same as #733.

---

### 736. registry.rs — `set_window_priority` removes and re-registers the window (lines 529-536)
**File:** `src/registry.rs` lines 529-536

```rust
pub fn set_window_priority(&mut self, id: ObjectId, priority: WindowPriority) {
    let window_id = WindowId(id);
    if let Some(mut window) = self.windows.remove(&window_id) {
        self.detach_window_indexes(&window);
        window.priority = priority;
        self.register_window(window);
    }
}
```

Changing a single field (priority) requires removing the window, detaching all indexes, and re-registering with all indexes re-attached. This is O(indexes × entries) for a single field change.

**Impact:** Performance — updating priority is as expensive as full re-registration.

**Fix:** Update only the affected index (windows_by_priority) instead of all indexes.

---

### 737. registry.rs — `touch_window_status` removes and re-registers the window (lines 538-550)
**File:** `src/registry.rs` lines 538-550

Same pattern as `set_window_priority` — changing status requires full detach and re-attach.

**Impact:** Same as #736.

---

### 738. scene.rs — `NodeStyle.rotation` is in degrees, not radians (line 40)
**File:** `wgpu-api/src/scene.rs` line 40

```rust
/// Rotation in degrees (clockwise).
pub rotation: f32,
```

The comment says degrees, but the GPU shader and transform matrix typically use radians. If the rotation is used directly in a matrix computation without conversion, the result will be wrong.

**Impact:** Correctness risk — degrees vs radians confusion.

**Fix:** Document clearly and convert at the boundary.

---

### 739. scene.rs — `NodeStyle.anchor` is unused in hit testing or rendering (line 46)
**File:** `wgpu-api/src/scene.rs` line 46

```rust
/// Anchor point within the node (0..1, where (0,0) is top-left).
pub anchor: Point,
```

The anchor point is defined but not used in `bounding_rect()`, `hit_test()`, or any rendering code. It's a no-op field.

**Impact:** Dead code — anchor has no effect.

**Fix:** Implement anchor-based positioning or remove the field.

---

### 740. scene.rs — `Node.is_visible()` checks `self.style.visible` twice (lines 518-520)
**File:** `wgpu-api/src/scene.rs` lines 518-520

```rust
pub fn is_visible(&self) -> bool {
    self.style.visible && !self.style.is_invisible()
}
```

`is_invisible()` returns `!self.visible || self.opacity <= 0.0`. So `is_visible()` becomes `self.visible && !(!self.visible || self.opacity <= 0.0)` which simplifies to `self.visible && self.visible && self.opacity > 0.0` = `self.visible && self.opacity > 0.0`. The double check of `self.visible` is redundant.

**Impact:** Minor — the logic is correct but confusing.

**Fix:** Simplify to `self.visible && self.opacity > 0.0`.

---

### 741. scene.rs — `Scene.add_node` doesn't validate parent existence (lines 559-570)
**File:** `wgpu-api/src/scene.rs` lines 559-570

```rust
pub fn add_node(&mut self, node: Node) -> NodeId {
    let id = node.id;
    if let Some(parent) = node.parent {
        if let Some(parent_node) = self.nodes.get_mut(&parent) {
            parent_node.children.push(id);
        }
    } else {
        self.roots.push(id);
    }
    self.nodes.insert(id, node);
    id
}
```

If the parent doesn't exist, the node is silently added as an orphan — it's in `self.nodes` but not in any parent's children list and not in `self.roots`. The node is unreachable.

**Impact:** Silent data loss — orphaned nodes are never traversed or rendered.

**Fix:** Return an error if the parent doesn't exist, or add the node as a root.

---

### 742. scene.rs — `Scene.clear()` doesn't reset `next_id` (lines 784-787)
**File:** `wgpu-api/src/scene.rs` lines 784-787

```rust
pub fn clear(&mut self) {
    self.nodes.clear();
    self.roots.clear();
}
```

After clearing, `next_id` retains its value. This is correct for avoiding ID reuse, but it means the ID counter grows unbounded over time if the scene is repeatedly cleared and repopulated.

**Impact:** Minor — `next_id` is a u64, so it would take a very long time to overflow.

---

### 743. text.rs — `TextStyle` default font size is 14px (line 146)
**File:** `wgpu-api/src/text.rs` line 146

```rust
font_size: 14.0,
```

The default font size is hardcoded to 14px. On high-DPI displays, this may be too small. The dock's actual font sizes are configured elsewhere (e.g., 12px for dock items).

**Impact:** Inconsistency — the default doesn't match the dock's actual font sizes.

**Fix:** Make the default configurable or match the dock's actual font size.

---

### 744. text.rs — `Text.truncation_suffix` defaults to "..." (line 229)
**File:** `wgpu-api/src/text.rs` line 229

```rust
truncation_suffix: "...".to_string(),
```

The truncation suffix is a String, not a `&'static str`. Every `Text::new()` call allocates a String for "...".

**Impact:** Per-text allocation.

**Fix:** Use `Option<&'static str>` or `Cow<'static, str>`.

---

### 745. text.rs — `Font` struct stores path as String (line 79)
**File:** `wgpu-api/src/text.rs` line 79

```rust
pub path: String,
```

The font path is a String. For a dock that uses a single font, this is fine. But for multiple fonts, each `Font` instance stores its own path String.

**Impact:** Per-font allocation.

**Fix:** Use `&'static str` for system fonts or `Cow<'static, str>` for dynamic paths.

---

### 746. slint.rs — `DockWidget` default colors are hardcoded (lines 251-258)
**File:** `wgpu-api/src/slint.rs` lines 251-258

```rust
hover_color: Color::rgba(0.0, 0.9, 1.0, 1.0), // #00E5FF
dash_focused_color: Color::rgba(0.0, 0.9, 1.0, 1.0), // #00E5FF
dash_inactive_color: Color::rgba(0.61, 0.64, 0.69, 1.0), // #9CA3AF
background_color: Color::rgba(0.11, 0.11, 0.13, 1.0), // #1D1C22
border_color: Color::rgba(0.30, 0.29, 0.33, 1.0),     // #4C4954
```

These colors are duplicated from `src/config.rs` defaults. If the config changes, the slint.rs defaults become stale.

**Impact:** Duplication — two sources of truth for the same colors.

**Fix:** Read from config or define in a single place.

---

### 747. slint.rs — `DashConfig.dash_positions()` allocates Vec (lines 420-424)
**File:** `wgpu-api/src/slint.rs` lines 420-424

```rust
pub fn dash_positions(&self, window_count: i32) -> Vec<f32> {
    let mut positions = [0.0; 5];
    let count = self.dash_positions_into(window_count, &mut positions);
    positions[..count].to_vec()
}
```

A fixed-size array is used, then sliced and converted to a Vec. This allocates a new Vec per call. The `dash_positions_into` method (line 429) avoids allocation, but `dash_positions` is the public API.

**Impact:** Per-call heap allocation.

**Fix:** Deprecate `dash_positions` in favor of `dash_positions_into`.

---

### 748. slint.rs — `DeviceEntry` stores button_text and status as Strings (lines 461-465)
**File:** `wgpu-api/src/slint.rs` lines 461-465

```rust
pub button_text: String,
pub status: String,
pub last_action: String,
```

These are derived from the device's state (connected, paired, etc.) but stored as Strings. Every state change allocates a new String.

**Impact:** Per-state-change allocation.

**Fix:** Use an enum for state and derive the text on-demand.

---

### 749. lib.rs — `interaction/` module not declared or re-exported (lines 76-111)
**File:** `wgpu-api/src/lib.rs` lines 76-111

The `interaction/` directory contains 5 files (commands.rs, drag.rs, gestures.rs, input.rs, picking.rs) but is not declared as a module in `lib.rs`. The interaction types are re-exported from `animation.rs` instead (e.g., `GestureState`, `GestureType`, `InteractionConfig`, `InteractionManager`).

**Impact:** Module organization mismatch — the interaction module exists but is not declared. The types are re-exported from animation, which is semantically incorrect.

**Fix:** Declare `pub mod interaction;` and re-export from the correct module.

---

### 750. config.rs — 1533 lines, single file for all configuration (entire file)
**File:** `src/config.rs` (1533 lines)

The entire configuration system — dock layout, animations, popups, timing, badges, time box, tray box, and all default values — is in a single 1533-line file. This includes `DockConfig` struct with 100+ fields, default value functions, load/save functions, and migration logic.

**Impact:** Cognitive complexity — understanding the config requires navigating 1533 lines. Adding a new config option requires finding the right section in a massive file.

**Fix:** Split into modules: `dock.rs`, `animation.rs`, `popup.rs`, `timing.rs`, `badge.rs`, etc.

---

## Summary of Issues by Severity

| #  | Severity | Issue | Impact |
|----|----------|-------|--------|
| 711 | **Critical** | hit_test ignores transforms | Clicks miss scaled/rotated nodes |
| 712 | **Critical** | hit_test uses parent-local coords | Nested nodes unclickable |
| 713 | **Critical** | z-index sort unstable across subtrees | Non-deterministic render order |
| 714 | **Critical** | Scene graph bypassed by GPU path | 1062 lines of dead code |
| 715 | **Critical** | SlintWgpuAdapter doesn't share device | Two Vulkan devices, doubled VRAM |
| 716 | **High** | traverse_visible ignores z-order | Incorrect render order |
| 717 | **High** | remove_node_recursive clones children | Per-removal allocation |
| 718 | **High** | Node.data is Option<String> | Memory waste, no type safety |
| 719 | **High** | estimate_width hardcoded 0.6 multiplier | Wrong text layout for many fonts |
| 720 | **High** | wrap_lines allocates per word | Per-wrap heap allocation |
| 721 | **High** | wrap_lines doesn't handle CJK | CJK text wrapping broken |
| 722 | **High** | DockItem 22 fields | Derived fields can drift |
| 723 | **High** | hit_test shifts during animation | Inconsistent hit area |
| 724 | **High** | badge_text() allocates per call | Per-frame allocation |
| 725 | **High** | BluetoothState 24 fields | Cognitive complexity |
| 726 | **High** | WidgetRegistry Options for required widgets | API confusion |
| 727 | **High** | 300+ re-exports, flat namespace | IDE autocomplete overwhelmed |
| 728 | **High** | Public API purity violated | GPU internals exposed |
| 729 | **High** | SystemRegistry derives Clone | Massive clone operation |
| 730 | **High** | refresh_application_status clones windows | Per-refresh allocation |
| 731 | **Medium** | push_unique O(n) linear scan | Performance on index updates |
| 732 | **Medium** | 10+ HashMap indexes must sync | Maintenance risk |
| 733 | **Medium** | register_process clones record | Per-registration allocation |
| 734 | **Medium** | register_window clones record | Per-registration allocation |
| 735 | **Medium** | set_process_binary clones record | Per-update allocation |
| 736 | **Medium** | set_window_priority full re-register | O(indexes × entries) for single field |
| 737 | **Medium** | touch_window_status full re-register | Same as #736 |
| 738 | **Medium** | rotation in degrees, not radians | Correctness risk |
| 739 | **Medium** | anchor point unused | Dead code |
| 740 | **Medium** | is_visible() redundant check | Confusing logic |
| 741 | **Medium** | add_node silent orphan | Unreachable nodes |
| 742 | **Medium** | clear() doesn't reset next_id | Unbounded ID counter |
| 743 | **Medium** | default font size 14px hardcoded | Inconsistency |
| 744 | **Medium** | truncation_suffix is String | Per-text allocation |
| 745 | **Medium** | Font path is String | Per-font allocation |
| 746 | **Medium** | DockWidget colors duplicated | Two sources of truth |
| 747 | **Medium** | dash_positions() allocates Vec | Per-call allocation |
| 748 | **Medium** | DeviceEntry strings for state | Per-state-change allocation |
| 749 | **Medium** | interaction/ module not declared | Module organization mismatch |
| 750 | **Medium** | config.rs 1533 lines | Cognitive complexity |

---

## Cumulative Summary

| Round | File | Issues | New Running Total |
|-------|------|--------|-------------------|
| 1 | gpu-rendering-review.md | 1-12 | 12 |
| 2 | gpu-rendering-review-2.md | 13-32 | 32 |
| 3 | gpu-rendering-review-3.md | 33-60 | 62 |
| 4 | gpu-rendering-review-4.md | 61-95 | 97 |
| 5 | gpu-rendering-review-5.md | 96-120 | 120 |
| 6 | gpu-rendering-review-6.md | 121-155 | 155 |
| 7 | gpu-rendering-review-7.md | 156-199 | 199 |
| 8 | gpu-rendering-review-8.md | 200-240 | 240 |
| 9 | gpu-rendering-review-9.md | 241-280 | 280 |
| 10 | gpu-rendering-review-10.md | 281-330 | 330 |
| 11 | gpu-rendering-review-11.md | 331-370 | 370 |
| 12 | gpu-rendering-review-12.md | 371-420 | 420 |
| 13 | gpu-rendering-review-13.md | 421-460 | 460 |
| 14 | gpu-rendering-review-14.md | 461-505 | 505 |
| 15 | gpu-rendering-review-15.md | 506-550 | 550 |
| 16 | gpu-rendering-review-16.md | 551-595 | 595 |
| 17 | gpu-rendering-review-17.md | 596-630 | 630 |
| 18 | gpu-rendering-review-18.md | 631-670 | 670 |
| 19 | gpu-rendering-review-19.md | 671-710 | 710 |
| **20** | **gpu-rendering-review-20.md** | **711-750** | **750** |

**Cumulative severity across all 20 rounds:**
- Critical: 95 (was 90 + 5 new)
- High: 196 (was 176 + 20 new)
- Medium: 295 (was 265 + 30 new)
- Low: 78 (no new low issues this round)
- **Total: 750 issues**

### Key Themes from Round 20

1. **Scene graph is dead code**: The entire 1062-line scene graph module is bypassed by the GPU rendering path. The hit testing, z-ordering, and traversal are all non-functional because no code uses them.

2. **Hit testing is fundamentally broken**: Even if the scene graph were used, `hit_test` ignores transforms (scale, rotation, anchor) and uses parent-local coordinates instead of world coordinates. Nested nodes would be unclickable.

3. **Slint integration is disconnected**: `SlintWgpuAdapter` doesn't share the wgpu device, meaning two Vulkan devices exist. The adapter is a no-op that selects Slint's wgpu backend but doesn't integrate with wgpu-api's renderer.

4. **Registry clones everything**: `SystemRegistry` derives `Clone`, and every registration method clones the full record. `refresh_application_status` clones all window records for a single app. This is O(n) heap allocation on every event.

5. **Flat namespace with 300+ re-exports**: The crate root re-exports every type from every module into a single flat namespace, creating naming conflicts resolved by awkward aliases (`PhysicsDockItem`, `PhysicsSpringConstraint`).

6. **Public API purity violated**: GPU backend internals (`GpuBackendConfig`, `GpuResources`, `GpuUniformBuffer`, etc.) are re-exported at the crate root, violating the invariant that the public API should be independent of wgpu types.

7. **Per-operation allocations everywhere**: `badge_text()` allocates per call, `wrap_lines` allocates per word, `dash_positions()` allocates per call, `DeviceEntry` stores state as Strings. These violate the "no steady-state heap allocations" rule.

8. **Registry index maintenance is fragile**: 10+ HashMap indexes must stay in sync. Changing a single field (priority, status) requires full detach and re-attach of all indexes, making it O(indexes × entries) for a single field change.
