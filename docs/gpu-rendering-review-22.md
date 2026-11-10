# GPU Rendering Code Review — Round 22 (Core Types, Icon Resolution, Graphics, Pointer, State)

This is a continuation of the GPU rendering code review. Rounds 1–21 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, documentation, render submodules, state modules, services, physics interaction, groups, input handling, popup rendering, tray rendering, error handling, geometry, models, cache, resolvers, scene graph, text, Slint bridge, library root, registry, config, Wayland, physics springs, morphing, primitives, and profiling (issues 1–790). This round covers: `src/lib.rs`, `src/types.rs`, `src/resolvers/icon.rs`, `src/graphics/animation.rs`, `src/graphics/fade.rs`, `src/graphics/hide.rs`, `src/pointer/handler.rs`, `src/pointer/motion.rs`, `src/pointer/coordinates.rs`, `src/pointer/proximity.rs`, `src/state/interaction.rs`, and `src/state/dock.rs` — issues 791–830.

---

## Critical Issues

### 791. lib.rs — `terminal_graphics` module is a standalone binary feature in a library crate (lines 58-142)
**File:** `src/lib.rs` lines 58-142

The `terminal_graphics` module contains `load_image_raw_rgba()` which rasterizes SVGs using resvg, caches them as PNG, and `generate_terminal_image_string()` which encodes RGBA pixels to base64 and formats them as ITU T.327 terminal graphics sequences. This module uses `base64`, `image`, `resvg`, `tiny_skia`, `dirs`, and `walkdir` — all pulled into the library crate. The `generate_terminal_image_string` function is never called by the application and is designed for terminal-based image display, not for a Wayland dock.

**Impact:** Binary bloat — base64 encoding, terminal graphics sequence generation, and the entire SVG rasterization pipeline are dead code in a Wayland application. The `terminal_graphics` module adds ~85 lines of unused code and pulls in dependencies (base64 engine) that are never used by the dock.

**Fix:** Move `terminal_graphics` to a separate binary crate or remove it entirely. Keep `load_image_raw_rgba` if used by the dock, but remove `generate_terminal_image_string`.

---

### 792. lib.rs — `load_image_raw_rgba` rasterizes SVG on every call when cache misses (lines 88-116)
**File:** `src/lib.rs` lines 88-116

When an SVG icon is loaded and the cache doesn't have a pre-rasterized PNG, the function reads the SVG file, parses it with `usvg::Tree::from_data`, creates a `tiny_skia::Pixmap`, and renders it. This happens on the main thread, blocking the event loop. The cache key is a hash of the SVG path and target size, but the cache is written asynchronously — the `img.save(&cache_path)` call is fire-and-forget.

**Impact:** Main thread blocking during SVG rasterization. If multiple SVG icons are loaded simultaneously (e.g., on startup), the event loop is blocked for each one sequentially.

**Fix:** Rasterize SVGs in a background thread or use the existing `icon_load` worker pool.

---

### 793. types.rs — `DockInstance` has 22 public fields (lines 232-274)
**File:** `src/types.rs` lines 232-274

```rust
pub struct DockInstance {
    pub surface: LayerSurface,
    pub output: WlOutput,
    pub width: u32,
    pub requested_width: u32,
    pub height: u32,
    pub current_buffer: Option<Buffer>,
    pub scale_notifier: Option<WpFractionalScaleV1>,
    pub scale_factor: f64,
    pub hover_popup: Option<PopupSurface>,
    pub menu_popup: Option<PopupSurface>,
    pub configured: bool,
    pub status_strip_rendered: bool,
    pub context_menu_popup: Option<PopupSurface>,
    pub window_list_popup: Option<PopupSurface>,
    pub audio_popup: Option<PopupSurface>,
    pub bluetooth_popup: Option<PopupSurface>,
    pub rclone_popup: Option<PopupSurface>,
    pub process_popup: Option<PopupSurface>,
    pub tray_bar: Option<PopupSurface>,
    pub status_widget_bar: Option<PopupSurface>,
    pub audio_player_bar: Option<PopupSurface>,
    pub audio_player_expanded: bool,
    pub system_status_bar: Option<PopupSurface>,
    pub date_time_block_surface: Option<DateTimeBlockSurface>,
    pub dashed_block_surface: Option<DashedBlockSurface>,
    pub dock_state: Option<DockState>,
    pub hover_fade: FadeAnimation,
    pub menu_fade: FadeAnimation,
    pub window_list_fade: FadeAnimation,
    pub active_hovered_app: Option<(usize, String)>,
}
```

22 public fields mean any module can mutate any field directly. The 10 `Option<PopupSurface>` fields could be a HashMap or enum. The 3 `FadeAnimation` fields could be a struct. The 2 block surfaces could be grouped.

**Impact:** No encapsulation — any module can set `dock.status_strip_rendered = true` without going through a controlled interface. Adding a new popup requires modifying `DockInstance` in 10+ places (motion.rs surface classification, proximity.rs, etc.).

**Fix:** Group related fields: `Popups` struct with all popup surfaces, `FadeAnimations` struct, `BlockSurfaces` struct. Make fields private with accessor methods.

---

### 794. types.rs — `PopupState` has 30+ fields (lines 415-447)
**File:** `src/types.rs` lines 415-447

```rust
pub struct PopupState<T> {
    pub target_app_id: Option<String>,
    pub target_window: Option<ObjectId>,
    pub target_tray_item: Option<String>,
    pub anchor_override: Option<(i32, i32)>,
    pub is_open: bool,
    pub items: T,
    pub waiting_for_initial_release: bool,
    pub just_opened: bool,
    pub cursor_moved: bool,
    pub fade: FadeAnimation,
    pub last_debug_print: Instant,
    pub no_motion_timer: Option<Instant>,
    pub dock_timer: Option<Instant>,
    pub popup_timer: Option<Instant>,
    pub pointer_inside_popup: bool,
    pub x: i32,
    pub y: i32,
    pub opened_by_button: Option<u32>,
    pub consecutive_false_count: u32,
    pub last_pointer_x: f64,
    pub last_pointer_y: f64,
    pub consecutive_no_motion_count: u32,
    pub consecutive_on_dock_count: u32,
    pub consecutive_on_context_menu_count: u32,
    pub consecutive_on_window_list_count: u32,
    pub context_menu_timer: Option<Instant>,
    pub window_list_timer: Option<Instant>,
}
```

30 fields including debug-only fields (`last_debug_print`), motion tracking fields (`consecutive_*_count`), and timer fields. Many of these are dead or rarely used.

**Impact:** Memory waste — each `PopupState` instance is ~200 bytes on the heap. The `consecutive_*_count` fields suggest state machine logic that should be encapsulated, not exposed as public fields.

**Fix:** Remove debug fields in release builds. Group timer fields. Encapsulate motion tracking in a sub-struct.

---

### 795. resolvers/icon.rs — `get_icon_path` creates `sysinfo::System` per Steam icon call (lines 203-209)
**File:** `src/resolvers/icon.rs` lines 203-209

```rust
if app_id.starts_with("steam_icon_") {
    let sys_scanner = sysinfo::System::new().expect("Failed to create sysinfo System");
    if let Some((_, _, icon_path)) = resolve_steam_game_details(app_id, "", &sys_scanner, None) {
        return Some(icon_path);
    }
}
```

Every time a Steam icon is resolved, a new `sysinfo::System` is created. `sysinfo::System::new()` does not refresh process data, so it's a minimal instance. However, `resolve_steam_game_details` likely iterates all processes, and creating a new System instance per call is wasteful.

**Impact:** Per-Steam-icon sysinfo allocation. If multiple Steam games are in the dock, this is called multiple times.

**Fix:** Cache the `sysinfo::System` instance or pass it from the caller.

---

## High Severity Issues

### 796. types.rs — `PopupSurface` stores `cached_content` Vec<u8> (line 180)
**File:** `src/types.rs` line 180

```rust
pub cached_content: Vec<u8>,
```

Each popup surface stores a full pixel buffer of the last rendered frame. For a 300×200 popup at 4 bytes/pixel, this is 240KB per popup. With 10 popup surfaces, this is 2.4MB of cached pixels that may never be used.

**Impact:** Memory waste — cached content is used for fade frames to avoid re-rendering, but the cache is never invalidated when the popup content changes structurally (e.g., window list items change).

**Fix:** Use a hash of the popup content to invalidate the cache. Limit cache size.

---

### 797. types.rs — `DockInstance` has 10 `Option<PopupSurface>` fields (lines 242-264)
**File:** `src/types.rs` lines 242-264

Every popup surface is a separate field on `DockInstance`. Adding a new popup requires:
1. Adding a field to `DockInstance`
2. Updating `motion.rs` surface classification (11 boolean flags)
3. Updating `proximity.rs` if needed
4. Updating render code
5. Updating initialization code

**Impact:** O(1) becomes O(n) for every new popup. The surface classification in `motion.rs` is already 80 lines of repetitive code.

**Fix:** Use a `HashMap<PopupType, PopupSurface>` or an enum. Surface classification becomes a single lookup.

---

### 798. pointer/motion.rs — Surface classification uses 11 boolean flags (lines 20-110)
**File:** `src/pointer/motion.rs` lines 20-110

```rust
let mut is_context_menu_surface = false;
let mut is_window_list_surface = false;
let mut is_hover_popup_surface = false;
let mut is_audio_popup_surface = false;
let mut is_bluetooth_popup_surface = false;
let mut is_rclone_popup_surface = false;
let mut is_process_popup_surface = false;
let mut is_widget_bar_surface = false;
let mut is_audio_player_surface = false;
let mut is_tray_bar_surface = false;
let mut is_bottom_status_surface = false;
```

Each pointer event iterates all docks and checks 11 popup surfaces per dock. This is O(docks × popups) per event. With 1 dock and 11 popups, this is 11 comparisons per event.

**Impact:** O(docks × popups) per pointer event. Adding a new popup requires adding a new boolean flag and a new `if` block.

**Fix:** Store popup surfaces in a HashMap keyed by surface ID. Lookup is O(1).

---

### 799. pointer/motion.rs — `is_special_surface` is a tautology (line 124)
**File:** `src/pointer/motion.rs` line 124

```rust
let is_special_surface = is_popup_surface;
```

`is_special_surface` is assigned the exact value of `is_popup_surface`. The variable is never modified between assignment and use. This is a leftover from a refactoring where `is_special_surface` once included additional conditions.

**Impact:** Dead code — `is_special_surface` adds no value over `is_popup_surface`.

**Fix:** Replace `is_special_surface` with `is_popup_surface` everywhere.

---

### 800. pointer/motion.rs — Drag detection uses hardcoded threshold (line 210)
**File:** `src/pointer/motion.rs` line 210

```rust
if dist_sq > 25.0 {
```

Drag starts when the pointer moves more than 5 pixels (sqrt(25) = 5). This threshold is not configurable and doesn't account for scale factor. At 2x scale, 5 logical pixels = 10 physical pixels, but the threshold is still 25 physical pixel-squared.

**Impact:** Inconsistent drag detection across scale factors.

**Fix:** Scale the threshold by `scale_factor` or use logical pixels consistently.

---

### 801. pointer/coordinates.rs — `map_coordinates` ignores scale factor (line 14)
**File:** `src/pointer/coordinates.rs` line 14

```rust
pub fn map_coordinates(
    event: &PointerEvent,
    state: &AppState,
    dock_surface_ptr: Option<&WlSurface>,
    _scale_factor: f32,  // ← unused
) -> (f32, f32) {
```

The `scale_factor` parameter is unused (prefixed with `_`). Pointer coordinates from Wayland are in surface-local coordinates, which are already scaled. However, the function doesn't document whether the return value is in logical or physical pixels.

**Impact:** Ambiguous coordinate space — callers must know whether the result is logical or physical pixels. The unused parameter suggests incomplete implementation.

**Fix:** Document the coordinate space of the return value. Use or remove the scale factor parameter.

---

### 802. pointer/proximity.rs — `pointer_between_icon_and_window` hardcodes layout (line 156)
**File:** `src/pointer/proximity.rs` line 156

```rust
let anchor_x = coordinates::calculate_popup_anchor(
    state,
    app_id,
    apps_in_dock,
    phys_width,
    dock_scale as f64,
    (dock_height, 48, 8, 0),  // ← hardcoded: height, box_size, spacing, start_offset
);
```

The layout tuple `(dock_height, 48, 8, 0)` hardcodes icon size (48), spacing (8), and start offset (0). These values should come from the config, not be hardcoded. If `config.icon_size` is 64, the proximity check uses 48, causing a mismatch.

**Impact:** Proximity check disagrees with actual icon layout when config differs from defaults.

**Fix:** Pass the actual layout from the config or from the caller.

---

### 803. graphics/fade.rs — `apply_alpha_to_canvas` modifies only alpha channel (lines 153-167)
**File:** `src/graphics/fade.rs` lines 153-167

```rust
pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
    let alpha = (self.current_alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as u16;
    // ...
    for chunk in canvas.chunks_exact_mut(4) {
        chunk[3] = ((chunk[3] as u16 * alpha + 127) / 255) as u8;
    }
}
```

Only the alpha channel is modified. RGB channels are untouched. This is correct for pre-multiplied alpha compositing, but the function name suggests it applies alpha to the entire pixel.

**Impact:** Confusing — the function doesn't premultiply RGB by alpha, it only scales the alpha channel. If the caller expects premultiplied alpha, the result is wrong.

**Fix:** Rename to `apply_alpha_channel` or document that RGB is not premultiplied.

---

### 804. graphics/hide.rs — `apply_alpha_to_canvas` modifies all 4 channels (lines 142-157)
**File:** `src/graphics/hide.rs` lines 142-157

```rust
pub fn apply_alpha_to_canvas(&self, canvas: &mut [u8]) {
    let alpha = (self.current_alpha.clamp(0.0, 1.0) * 255.0 + 0.5) as u16;
    // ...
    for chunk in canvas.chunks_exact_mut(4) {
        chunk[0] = ((chunk[0] as u16 * alpha + 127) / 255) as u8;
        chunk[1] = ((chunk[1] as u16 * alpha + 127) / 255) as u8;
        chunk[2] = ((chunk[2] as u16 * alpha + 127) / 255) as u8;
        chunk[3] = ((chunk[3] as u16 * alpha + 127) / 255) as u8;
    }
}
```

All 4 channels (RGBA) are multiplied by alpha. This is different from `fade.rs` which only modifies the alpha channel. The two `apply_alpha_to_canvas` methods have different behavior despite the same name and signature.

**Impact:** Inconsistent alpha blending — `FadeAnimation::apply_alpha_to_canvas` scales only alpha, while `AutoHideState::apply_alpha_to_canvas` scales all channels. Callers using the wrong one get incorrect visual results.

**Fix:** Unify the two methods or rename them to reflect their different behavior.

---

### 805. graphics/fade.rs & hide.rs — Duplicate fade animation logic
**File:** `src/graphics/fade.rs` (190 lines) and `src/graphics/hide.rs` (159 lines)

Both modules implement nearly identical fade animation logic:
- `current_alpha`, `target_alpha`, `last_update`, `duration_secs`
- `ALPHA_EPSILON` constant (1e-3)
- `FADE_FRAME_INTERVAL` / `HIDE_FRAME_INTERVAL` (41,666,667 ns = 24 FPS)
- `tick()` method with identical interpolation logic
- `apply_alpha_to_canvas()` method
- `is_fully_hidden()` and `is_fully_visible()` methods

The only differences are `FadeAnimation` uses `min_alpha`/`max_alpha` while `AutoHideState` uses `HIDDEN_ALPHA` (0.2) hardcoded.

**Impact:** ~350 lines of duplicated logic. Bug fixes in one are not applied to the other. The different `apply_alpha_to_canvas` behavior (issue #804) is a direct consequence of this duplication.

**Fix:** Extract a `FadeAnimator` struct that both use. Unify `apply_alpha_to_canvas`.

---

### 806. graphics/hide.rs — `HIDDEN_ALPHA` is 0.2, not 0.0 (line 7)
**File:** `src/graphics/hide.rs` line 7

```rust
pub const HIDDEN_ALPHA: f32 = 0.2; // Adjust target alpha here (0.2 = 80% transparency)
```

When the dock is "hidden", it's not fully transparent — it's 20% opaque. This means the dock is always partially visible, even in "hidden" mode. The comment says "80% transparency" which is correct, but the constant name `HIDDEN_ALPHA` suggests full invisibility.

**Impact:** Dock never fully hides — 20% alpha remains visible. This is intentional for the "reveal strip" but the naming is misleading.

**Fix:** Rename to `HIDDEN_REVEAL_ALPHA` or document that "hidden" means semi-transparent, not invisible.

---

### 807. state/dock.rs — `window_count_for_app` iterates all windows (lines 171-179)
**File:** `src/state/dock.rs` lines 171-179

```rust
pub fn window_count_for_app(&self, app_id: &str) -> usize {
    self.windows
        .values()
        .filter(|window| {
            let raw_id = raw_window_app_id(window);
            self.canonical_running_app_id(&raw_id) == app_id
        })
        .count()
}
```

Every call iterates ALL windows, computes `raw_window_app_id` (which clones strings), and calls `canonical_running_app_id` (which does case-insensitive comparisons and hardcoded app rules). This is O(n) where n is the number of windows.

**Impact:** O(n) per call with string allocations. Called from `motion.rs` on every hover event and from `proximity.rs` on every proximity check.

**Fix:** Cache the window count per app or maintain a `HashMap<String, usize>`.

---

### 808. state/dock.rs — `canonical_running_app_id` has hardcoded app rules (lines 94-153)
**File:** `src/state/dock.rs` lines 94-153

```rust
if lowered.contains("amnezia") {
    Some("AmneziaVPN")
} else if lowered.contains("qbittorrent") {
    Some("org.qbittorrent.qBittorrent")
}
// ...
if normalized.contains("wezterm")
// ...
if (normalized.eq_ignore_ascii_case("steamwebhelper")
    || normalized.eq_ignore_ascii_case("steamwebhelper-bin"))
// ...
if lowered == "bluetooth" || lowered == "widget-test" {
```

Hardcoded app ID canonicalization for AmneziaVPN, qBittorrent, WezTerm, Steam, and Bluetooth. Each call does substring matching and case-insensitive comparisons.

**Impact:** O(n) string operations per call. Adding a new app requires a code change and recompilation.

**Fix:** Move to config file. Use a lookup table.

---

### 809. state/interaction.rs — Dead code in `update_hover_and_hide_timers` (lines 175-195)
**File:** `src/state/interaction.rs` lines 175-195

```rust
pub fn update_hover_and_hide_timers(&mut self) -> bool {
    let state_changed = false;
    /*
    // 1. Immediately dismiss hover popup when pointer leaves surface
    if !self.interaction.pointer_inside && self.hover_state.is_visible {
        self.hover_state.is_visible = false;
        self.hover_state.app_id = None;
        self.hover_state.last_leave_time = None;
        self.needs_redraw = true;
        state_changed = true;
    }
    */
    // ...
    state_changed
}
```

The function always returns `false`. The only non-commented code sets `is_pointer_near` but doesn't affect the return value. The commented-out code was the original functionality.

**Impact:** Dead function — always returns false, does minimal work. Callers checking the return value get incorrect results.

**Fix:** Remove the function or restore its functionality.

---

### 810. state/dock.rs — `get_app_id_at_location` uses hardcoded BOX_SIZE and SPACING (lines 199-204)
**File:** `src/state/dock.rs` lines 199-204

```rust
let total_icons_width =
    total_items as f64 * BOX_SIZE + total_items.saturating_sub(1) as f64 * SPACING;
```

`BOX_SIZE` is 48.0 and `SPACING` is 8.0 (module-level constants). If `config.icon_size` is 64, hit testing uses 48, causing a mismatch between visual layout and clickable area.

**Impact:** Hit testing disagrees with visual layout when config differs from defaults.

**Fix:** Use `config.icon_size` and `config.spacing` instead of module constants.

---

## Medium Severity Issues

### 811. lib.rs — `svg_cache_path` uses DefaultHasher (lines 66-83)
**File:** `src/lib.rs` lines 66-83

```rust
fn svg_cache_path(svg_path: &Path, target_size: u32) -> Option<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    svg_path.hash(&mut hasher);
    target_size.hash(&mut hasher);

    let cache_key = format!("{:x}", hasher.finish());
```

`DefaultHasher` uses SipHash, which is not stable across Rust versions. Cache keys generated by one version of the dock may not match those generated by another version after a Rust upgrade.

**Impact:** Cache invalidation on Rust upgrade — all cached SVGs are re-rasterized.

**Fix:** Use a stable hash function (e.g., SHA-256 truncated) or clear the cache on version change.

---

### 812. lib.rs — `load_image_raw_rgba` uses `image::open` for non-SVG (lines 118-125)
**File:** `src/lib.rs` lines 118-125

```rust
let img = image::open(path).ok()?;
let scaled = img.resize_exact(
    target_size,
    target_size,
    image::imageops::FilterType::Lanczos3,
);
```

`image::open` probes the file to detect its format, then decodes it. For PNG icons, this is correct. For other formats (WebP, AVIF, etc.), the `image` crate may not support them. The `ok()?` silently returns `None` on unsupported formats.

**Impact:** Silent failure for unsupported image formats. No error logging.

**Fix:** Log unsupported formats. Support more formats.

---

### 813. types.rs — `DockRenderState` holds `&'a mut FontManager` (line 103)
**File:** `src/types.rs` line 103

```rust
pub font_manager: &'a mut FontManager,
```

A mutable reference to `FontManager` in a render state struct. If `DockRenderState` is cloned or shared across threads, this violates Rust's aliasing rules. The struct is not `Send` or `Sync` because of the mutable reference.

**Impact:** Cannot use `DockRenderState` in multi-threaded rendering. Font loading blocks the render path.

**Fix:** Use `Arc<Mutex<FontManager>>` or pass font data by value.

---

### 814. types.rs — `InteractionState` stores `WlSurface` (line 138)
**File:** `src/types.rs` line 138

```rust
pub focused_surface: Option<WlSurface>,
```

`WlSurface` is a Wayland proxy object. Storing it in interaction state means the state cannot be serialized or debugged easily. The `Clone` derive on `InteractionState` clones the proxy, which is a shallow copy.

**Impact:** `InteractionState::Clone` is a shallow copy of Wayland proxies. Two clones share the same underlying Wayland object.

**Fix:** Store `ObjectId` instead of `WlSurface` for identification.

---

### 815. types.rs — `MenuItemType::FocusWindow` stores `ZwlrForeignToplevelHandleV1` (line 383)
**File:** `src/types.rs` line 383

```rust
FocusWindow(ZwlrForeignToplevelHandleV1),
```

A Wayland protocol proxy is stored directly in a menu item type. This prevents `MenuItemType` from being `Send` or serializable. The proxy must be used on the same thread as the Wayland event loop.

**Impact:** Context menu items cannot be processed off the main thread.

**Fix:** Store `ObjectId` and look up the handle when needed.

---

### 816. resolvers/icon.rs — `find_icon_by_name` parses all .desktop files (lines 23-67)
**File:** `src/resolvers/icon.rs` lines 23-67

To find an icon by name, the function iterates all `.desktop` files in `/usr/share/applications` and `~/.local/share/applications`, parsing each one line by line. For a system with 500 .desktop files, this opens and parses 500 files.

**Impact:** O(n) file I/O per call. Called during icon resolution, which happens on startup and when apps are launched.

**Fix:** Cache the .desktop file index. Use the icon index from `icon_indexer.rs`.

---

### 817. resolvers/icon.rs — `find_icon_path` uses WalkDir without depth limit (lines 149-159)
**File:** `src/resolvers/icon.rs` lines 149-159

```rust
pub fn find_icon_path(root_dir: &str, target_name: &str) -> Option<PathBuf> {
    for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
```

`WalkDir` recursively traverses the entire directory tree. For `/usr/share/icons/hicolor`, this can be thousands of files across many subdirectories. No depth limit, no parallelization.

**Impact:** Slow icon resolution — WalkDir traverses the entire icon theme directory for each icon.

**Fix:** Use the icon index from `icon_indexer.rs`. Set a depth limit.

---

### 818. resolvers/icon.rs — `get_icon_path` searches 10 directories sequentially (lines 236-253)
**File:** `src/resolvers/icon.rs` lines 236-253

```rust
let search_paths = [
    format!("{}/.local/share/icons/hicolor/scalable/apps", home),
    format!("{}/.local/share/icons/hicolor/512x512/apps", home),
    format!("{}/.local/share/icons/hicolor/256x256/apps", home),
    format!("{}/.local/share/icons/hicolor/128x128/apps", home),
    format!("{}/.local/share/icons/hicolor/48x48/apps", home),
    "/usr/share/icons/hicolor/scalable/apps".to_string(),
    "/usr/share/icons/hicolor/512x512/apps".to_string(),
    "/usr/share/icons/hicolor/256x256/apps".to_string(),
    "/usr/share/icons/hicolor/128x128/apps".to_string(),
    "/usr/share/pixmaps".to_string(),
];
```

10 directories are searched sequentially, each with a full WalkDir traversal. For a single icon, this is 10 directory traversals.

**Impact:** 10× WalkDir per icon. For 10 icons, this is 100 directory traversals.

**Fix:** Use the icon index. Search in parallel.

---

### 819. graphics/animation.rs — `load_svg_animation_sequence` sorts by digit extraction (lines 80-92)
**File:** `src/graphics/animation.rs` lines 80-92

```rust
entries.sort_by_key(|e| {
    e.path()
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| {
            s.chars()
                .filter(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .ok()
        })
        .unwrap_or(0)
});
```

Frame sorting extracts all ASCII digits from the filename and parses them as a u32. Files without digits get priority 0. Files like `frame10.svg` and `frame2.svg` are sorted correctly (10 > 2), but `loading-a.svg` and `loading-b.svg` both get priority 0 and are unordered.

**Impact:** Animation frames without numeric filenames are in undefined order.

**Fix:** Fall back to lexicographic sort for files without digits.

---

### 820. graphics/animation.rs — `IconAnimation.update` uses modulo for looping (line 42)
**File:** `src/graphics/animation.rs` line 42

```rust
self.current_frame = (self.current_frame + 1) % self.frames.len();
```

The animation loops indefinitely using modulo. There's no way to stop the animation or mark it as completed. The `is_active` field is set at construction and never changed.

**Impact:** Animations run forever. No way to stop them except dropping the `IconAnimation` instance.

**Fix:** Add a `looping` field. Add a `stop()` method.

---

### 821. pointer/handler.rs — `pointer_frame` rebuilds layout tuple every call (lines 45-65)
**File:** `src/pointer/handler.rs` lines 45-65

```rust
let dock_height = self.config.dock_height as i32;
let box_size = self.config.icon_size as i32;
let spacing = self.config.spacing as i32;
let total_items = apps_in_dock.len() as i32;
let dock_width = self.docks.first().map(|dock| dock.width as i32).unwrap_or(self.width.max(0));
let icon_content_width = if total_items > 0 {
    total_items * box_size + total_items.saturating_sub(1) * spacing
} else {
    0
};
let start_offset_x = if dock_width > icon_content_width {
    (dock_width - icon_content_width) / 2
} else {
    0
};
let layout = (dock_height, box_size, spacing, start_offset_x);
```

Every pointer event recalculates the layout tuple from scratch. The layout only changes when the dock width changes or when apps are added/removed.

**Impact:** Redundant arithmetic on every pointer event.

**Fix:** Cache the layout tuple. Invalidate when dock width or app count changes.

---

### 822. pointer/handler.rs — `pointer_frame` calls `get_running_by_app_cached` and `get_apps_in_dock_cached` (lines 42-43)
**File:** `src/pointer/handler.rs` lines 42-43

```rust
let running_by_app = self.get_running_by_app_cached();
let apps_in_dock = self.get_apps_in_dock_cached();
```

These methods return cloned HashMaps and Vecs. Every pointer event clones the entire running-by-app HashMap and apps-in-dock Vec.

**Impact:** Per-pointer-event heap allocation. With 60 pointer events per second, this is 60 HashMap clones and 60 Vec clones.

**Fix:** Pass references instead of cloning. Use `RefCell` or interior mutability.

---

### 823. state/dock.rs — `compute_apps_in_dock_uncached` clones pinned_apps (line 32)
**File:** `src/state/dock.rs` line 32

```rust
let mut apps_in_dock = self.pinned_apps.clone();
```

Every uncached computation clones `pinned_apps` (Vec<String>). With 10 pinned apps, this is 10 String clones.

**Impact:** Per-computation heap allocation.

**Fix:** Use `extend_from_slice` or iterate without cloning.

---

### 824. state/dock.rs — `compute_running_by_app_uncached` sorts windows by ObjectId (lines 52-63)
**File:** `src/state/dock.rs` lines 52-63

```rust
for windows in running_by_app.values_mut() {
    windows.sort_by_key(|id| {
        self.window_id_for_object(id)
            .map(|wid| {
                self.windows
                    .get(&wid)
                    .map(|w| w.order_index)
                    .unwrap_or(u64::MAX)
            })
            .unwrap_or(u64::MAX)
    });
}
```

For every app's window list, the function looks up each ObjectId in `self.windows` to get the `order_index`. This is O(n log n) per app, where n is the number of windows for that app.

**Impact:** O(n log n) per app per computation. Lookup overhead for each window.

**Fix:** Maintain sorted window lists. Cache order_index in the ObjectId mapping.

---

### 825. state/interaction.rs — `get_window_list_bounds` and `get_context_menu_bounds` duplicate logic (lines 46-135)
**File:** `src/state/interaction.rs` lines 46-135

Both methods follow the same pattern:
1. Get dock from `self.docks.first()`
2. Compute `dock_width` and `dock_height` from scale factor
3. Get target app ID
4. Calculate anchor X
5. Compute popup geometry

The only differences are the popup type and the item count source.

**Impact:** ~90 lines of duplicated logic. Bug fixes in one are not applied to the other.

**Fix:** Extract a `get_popup_bounds(popup_type: PopupType) -> (i32, i32, i32, i32)` method.

---

### 826. state/dock.rs — `sync_dock_surface_width` iterates all docks (lines 246-306)
**File:** `src/state/dock.rs` lines 246-306

Every call iterates all docks, checks `requested_width`, and updates surface size, widget bar position, and system bar position. For a single-monitor setup, this is 1 iteration. For multi-monitor, this is N iterations.

**Impact:** O(docks) per call. Widget bar and system bar positions are recalculated even when width hasn't changed.

**Fix:** Skip docks where `requested_width == desired_width` (already done). Early-return if no docks changed.

---

### 827. state/dock.rs — `sync_dashed_block_position` and `sync_date_time_block_position` duplicate logic (lines 313-406)
**File:** `src/state/dock.rs` lines 313-406

Both methods follow the same pattern:
1. Get config values (gap, margin)
2. Iterate all docks
3. Get output size
4. Calculate dock edge position
5. Set margin based on anchor

The only differences are the block type (dashed vs date-time) and the edge (left vs right).

**Impact:** ~90 lines of duplicated logic.

**Fix:** Extract a `sync_block_position(block_type: BlockType)` method.

---

### 828. resolvers/icon.rs — `search_icon_list_file` reads entire file into memory (lines 161-200)
**File:** `src/resolvers/icon.rs` lines 161-200

The icon list file is read line by line with `BufReader`. Each line is lowercased and compared with `contains`. The file format is one path per line. For a large icon list (10,000+ entries), this is 10,000 string allocations and comparisons.

**Impact:** O(n) string allocations per call.

**Fix:** Use a binary search on a sorted index. Cache the icon list in memory.

---

### 829. pointer/proximity.rs — `pointer_on_popup` iterates all docks (lines 83-126)
**File:** `src/pointer/proximity.rs` lines 83-126

```rust
for dock in &state.docks {
    let popup = match popup_type {
        PopupType::ContextMenu => dock.context_menu_popup.as_ref(),
        PopupType::WindowList => dock.window_list_popup.as_ref(),
        PopupType::Custom { .. } => None,
    };
```

For each dock, the function looks up the popup by type. For a single-monitor setup, this is 1 iteration. The `Custom` variant always returns `None`.

**Impact:** O(docks) per call. `Custom` popup type is unimplemented.

**Fix:** Look up the popup directly by surface ID. Implement `Custom` popup type.

---

### 830. types.rs — `HoverState` is unused (lines 399-408)
**File:** `src/types.rs` lines 399-408

```rust
pub struct HoverState {
    pub x: usize,
    pub app_id: Option<String>,
    pub is_visible: bool,
    pub last_leave_time: Option<std::time::Instant>,
}
```

`HoverState` is defined but the dock uses `interaction.hovered_icon` (Option<usize>) and `hover_state` in `AppState` directly. The `HoverState` struct appears to be a leftover from a refactoring.

**Impact:** Dead code — struct is defined but may not be used.

**Fix:** Remove if unused or verify usage.

---

## Summary of Issues by Severity

 #  | Severity | Issue | Impact |
----|----------|-------|--------|
 791 | **Critical** | terminal_graphics in lib.rs | Dead code, binary bloat |
 792 | **Critical** | SVG rasterization on main thread | Event loop blocking |
 793 | **Critical** | DockInstance 22 public fields | No encapsulation |
 794 | **Critical** | PopupState 30+ fields | Memory waste, dead fields |
 795 | **Critical** | sysinfo::System per Steam icon | Per-call allocation |
 796 | **High** | PopupSurface cached_content Vec | 2.4MB memory waste |
 797 | **High** | 10 Option<PopupSurface> fields | O(n) new popup cost |
 798 | **High** | 11 boolean flags for surface classification | O(docks×popups) per event |
 799 | **High** | is_special_surface tautology | Dead code |
 800 | **High** | Hardcoded drag threshold | Inconsistent across scales |
 801 | **High** | map_coordinates ignores scale factor | Ambiguous coordinate space |
 802 | **High** | Hardcoded layout in proximity check | Mismatch with config |
 803 | **High** | fade.rs applies alpha only | Confusing behavior |
 804 | **High** | hide.rs applies alpha to all channels | Inconsistent with fade.rs |
 805 | **High** | Duplicate fade animation logic | 350 lines duplicated |
 806 | **High** | HIDDEN_ALPHA is 0.2 not 0.0 | Dock never fully hides |
 807 | **High** | window_count_for_app O(n) scan | Per-hover allocation |
 808 | **High** | Hardcoded app ID rules | O(n) string ops |
 809 | **High** | Dead code in update_hover_and_hide_timers | Always returns false |
 810 | **High** | Hardcoded BOX_SIZE in hit testing | Mismatch with config |
 811 | **Medium** | DefaultHasher for cache keys | Cache invalidation on upgrade |
 812 | **Medium** | image::open silent failure | No error logging |
 813 | **Medium** | &mut FontManager in DockRenderState | Not Send/Sync |
 814 | **Medium** | WlSurface in InteractionState | Shallow clone |
 815 | **Medium** | ZwlrForeignToplevelHandleV1 in MenuItemType | Not Send |
 816 | **Medium** | find_icon_by_name parses all .desktop | O(n) file I/O |
 817 | **Medium** | find_icon_path WalkDir no depth limit | Slow resolution |
 818 | **Medium** | 10 directory searches per icon | 10× WalkDir |
 819 | **Medium** | Animation sort by digit extraction | Undefined order |
 820 | **Medium** | IconAnimation loops forever | No stop mechanism |
 821 | **Medium** | Layout tuple rebuilt per event | Redundant arithmetic |
 822 | **Medium** | HashMap clones per pointer event | Per-event allocation |
 823 | **Medium** | pinned_apps clone per computation | Per-computation allocation |
 824 | **Medium** | Window sort by ObjectId lookup | O(n log n) per app |
 825 | **Medium** | Duplicate popup bounds logic | 90 lines duplicated |
 826 | **Medium** | sync_dock_surface_width O(docks) | Redundant recalculations |
 827 | **Medium** | Duplicate block position logic | 90 lines duplicated |
 828 | **Medium** | search_icon_list_file O(n) strings | Per-call allocation |
 829 | **Medium** | pointer_on_popup O(docks) | Custom unimplemented |
 830 | **Medium** | HoverState potentially unused | Dead code |

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
 21 | gpu-rendering-review-21.md | 751-790 | 790 |
 **22** | **gpu-rendering-review-22.md** | **791-830** | **830** |

**Cumulative severity across all 22 rounds:**
- Critical: 105 (was 100 + 5 new)
- High: 244 (was 222 + 22 new)
- Medium: 347 (was 321 + 26 new)
- Low: 78 (no new low issues this round)
- **Total: 830 issues**

### Key Themes from Round 22

1. **DockInstance is a god struct**: 22 public fields including 10 `Option<PopupSurface>` fields, 3 `FadeAnimation` fields, and 2 block surface fields. Any module can mutate any field. Adding a new popup requires modifying 10+ files.

2. **PopupState is a god struct**: 30+ fields including debug-only fields, motion tracking counters, and timer fields. ~200 bytes per instance with many fields rarely used.

3. **Surface classification is O(docks × popups)**: 11 boolean flags, each set by iterating all docks and checking popup surfaces. Adding a new popup requires a new flag and a new `if` block.

4. **Duplicate fade animation logic**: `FadeAnimation` and `AutoHideState` implement nearly identical logic (~350 lines) with different `apply_alpha_to_canvas` behavior. One scales only alpha, the other scales all channels.

5. **Hardcoded values throughout**: `BOX_SIZE` (48), `SPACING` (8), `DOCK_HEIGHT` (60) are module-level constants used in hit testing, proximity checks, and popup anchor calculation. Config values are ignored in many places.

6. **Icon resolution is O(n) file I/O**: `get_icon_path` creates a `sysinfo::System` per Steam icon, runs `WalkDir` without depth limit on 10 directories, and parses all .desktop files for name-based lookup.

7. **Per-pointer-event allocations**: `pointer_frame` clones `running_by_app` HashMap and `apps_in_dock` Vec on every pointer event. At 60 events/second, this is 60 HashMap clones and 60 Vec clones.

8. **terminal_graphics is dead code**: Base64 encoding and ITU T.327 terminal graphics sequences in a Wayland application. The module pulls in dependencies and adds ~85 lines of unused code.

9. **Hardcoded app ID canonicalization**: AmneziaVPN, qBittorrent, WezTerm, Steam, and Bluetooth are hardcoded in `canonical_running_app_id`. Each call does substring matching and case-insensitive comparisons.

10. **Dead code in interaction.rs**: `update_hover_and_hide_timers` always returns `false`. The original functionality is commented out. Callers checking the return value get incorrect results.
