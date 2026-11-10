# GPU Rendering Code Review — Round 9 (Application Layer: State, Main Loop, Init, DMA-BUF, Config, Types, PipeWire)

This is a continuation of the GPU rendering code review. Rounds 1–8 covered all `wgpu-api/` source files and key `src/app/` modules (issues 1–240). This round covers the remaining `src/` application-layer modules: `state/mod.rs`, `state/dock.rs`, `state/animation.rs`, `main.rs`, `init.rs`, `types.rs`, `config.rs`, `render/dmabuf_export.rs`, `render/slint_dock.rs`, `render/dock.rs`, and `services/pipewire_capture.rs` (issues 241–290).

---

## Critical Issues

### 241. init.rs — Multiple independent tokio runtimes (init.rs:111-117, 121-130)
**File:** `src/init.rs`
**Lines:** 111-117, 121-130

```rust
thread::spawn(move || {
    if let Ok(runtime) = tokio::runtime::Runtime::new() {
        runtime.block_on(async move {
            let _ = crate::listeners::start_unity_dbus_listener(badge_tx).await;
        });
    }
});
```

Each background listener (Unity badges, system tray, etc.) spawns a new thread with its own `tokio::runtime::Runtime::new()`. This creates at least 2 independent tokio runtimes in addition to the one in `main.rs` (line 50-54). Each runtime has its own reactor, timer wheel, and I/O poller.

**Impact:** Resource waste — each tokio runtime allocates its own epoll instance, timer wheel, and background threads. With 2+ runtimes, the application uses 2× the I/O polling resources. The `main.rs` runtime (line 50) uses `worker_threads(2)`, while the init.rs runtimes use the default (1 worker each).

**Fix:** Share a single tokio runtime across all background listeners. Pass the runtime handle or use a single `tokio::spawn` from the main runtime.

---

### 242. init.rs — Scale factor hardcoded to 1.0 (init.rs:393)
**File:** `src/init.rs`
**Line:** 393

```rust
let scale_factor = 1.0;
```

The scale factor is hardcoded to `1.0` during layer surface creation. This variable is declared but never used in the surrounding code — the actual scale factor comes from `state.scale_factor` which is set by the fractional scale listener. However, if this variable is ever used for buffer size calculations, it would produce incorrect results on HiDPI displays.

**Impact:** Dead code with a dangerous default. If used, all buffer calculations would be wrong at non-1.0 scale factors.

**Fix:** Remove the unused variable or use `state.scale_factor` if needed.

---

### 243. main.rs — Main loop has no frame rate cap when idle rendering (main.rs:405-420)
**File:** `src/main.rs`
**Lines:** 405-420

```rust
let mut timeout_ms: i32 = 16;

let waiting_for_layer_configure =
    state.docks.is_empty() || state.docks.iter().any(|dock| !dock.configured);
if state.needs_redraw && !waiting_for_layer_configure {
    timeout_ms = 0;
} else if state.window_list_state.is_open && !waiting_for_layer_configure {
    timeout_ms = window_list_refresh_interval
        .saturating_sub(last_window_list_refresh.elapsed())
        .as_millis()
        .min(i32::MAX as u128) as i32;
    timeout_ms = timeout_ms.max(1);
} else {
    // When idle (no redraw needed), sleep longer to reduce CPU usage
    timeout_ms = 1000; // 1 second sleep when idle
}
```

When `needs_redraw` is true and the layer is configured, `timeout_ms` is set to `0`. This means `poll()` returns immediately, and the loop iterates as fast as possible. If `needs_redraw` is set by a background channel (e.g., audio snapshot every 1.5s), the loop will spin at full CPU until the next poll timeout.

However, the more critical issue is that `timeout_ms = 0` means no sleep between frames. If `needs_redraw` is continuously true (e.g., due to a stuck animation or a bug), the loop consumes 100% of one CPU core.

**Impact:** CPU usage spike — the main loop can spin at 100% CPU when `needs_redraw` is continuously true. This violates Core Engineering Priority #1 (lowest possible CPU usage).

**Fix:** Enforce a minimum timeout (e.g., 8ms for ~120 FPS max) even when `needs_redraw` is true.

---

### 244. main.rs — get_apps_in_dock and get_running_by_app called every frame (main.rs:318-322)
**File:** `src/main.rs`
**Lines:** 318-322

```rust
let apps_in_dock = state.get_apps_in_dock();
if state.sync_dock_surface_width(apps_in_dock.len()) {
    status_sidebars_only = false;
}
let running_by_app = state.get_running_by_app();
```

Every iteration of the main loop, `get_apps_in_dock()` clones `pinned_apps` and iterates all windows, calling `canonical_running_app_id()` for each. Then `get_running_by_app()` builds a new HashMap from scratch. These are O(n) operations that allocate new Vecs and HashMaps every frame.

**Impact:** Per-frame heap allocations. With 50 windows, this is ~50 String clones + 1 Vec allocation + 1 HashMap allocation per frame. At 60 FPS, that's ~3,000 HashMap builds per second.

**Fix:** Cache these results and invalidate only when windows or pinned_apps change.

---

### 245. state/mod.rs — sync_live_state_from_registry is a full rebuild (state/mod.rs:622-746)
**File:** `src/state/mod.rs`
**Lines:** 622-746

```rust
pub fn sync_live_state_from_registry(&mut self) {
    self.applications.clear();
    let mut old_windows = std::mem::take(&mut self.windows);
    self.processes.clear();
    self.outputs.clear();
    // ... rebuilds everything from scratch
}
```

Every call to `sync_live_state_from_registry` clears and rebuilds all applications, windows, processes, and outputs. This is called from `sync_system_registry` which is called from `init.rs:367` once at startup. However, if called during runtime (e.g., after a process scan), it destroys and recreates all state.

**Impact:** Full rebuild of all live state on every call. All HashMap entries are dropped and re-inserted, causing cache misses and allocation pressure.

**Fix:** Implement incremental updates that only add/remove/modify changed entries.

---

### 246. state/dock.rs — canonical_running_app_id has hardcoded app-specific rules (state/dock.rs:29-88)
**File:** `src/state/dock.rs`
**Lines:** 29-88

```rust
let lowered = normalized.to_ascii_lowercase();
let requested_pin = if lowered.contains("amnezia") {
    Some("AmneziaVPN")
} else if lowered.contains("qbittorrent") {
    Some("org.qbittorrent.qBittorrent")
} else {
    None
};
```

The canonicalization logic contains hardcoded rules for specific apps (AmneziaVPN, qBittorrent, WezTerm, Steam, Bluetooth). Each new app with a mismatched app_id requires a code change. The `contains` check is also imprecise — any app with "wezterm" in its ID will match, even if it's not WezTerm.

**Impact:** Maintenance burden — every app with a non-standard app_id requires a code change. The `contains` checks are imprecise and may match unintended apps.

**Fix:** Use a configurable mapping file (e.g., `~/.config/dock/app_id_map.json`) instead of hardcoded rules.

---

## High Severity Issues

### 247. render/dmabuf_export.rs — DmabufPool never evicts entries (dmabuf_export.rs:41-90)
**File:** `src/render/dmabuf_export.rs`
**Lines:** 41-90

```rust
pub struct DmabufPool {
    targets: std::cell::RefCell<HashMap<(u32, u32), DmabufRenderTarget>>,
}
```

The pool caches render targets by `(width, height)` but never removes entries. If the dock resizes multiple times (e.g., as apps are pinned/unpinned), each size creates a new entry. The `clear()` method exists but is only called when the GPU context is destroyed.

**Impact:** VRAM leak — each resize creates a new wgpu::Texture that is never freed until GPU destruction. With 10 resizes, 10 orphaned textures accumulate.

**Fix:** Evict entries when they're no longer needed, or limit the pool size to the current dock dimensions.

---

### 248. render/dmabuf_export.rs — Duplicate DMA-BUF creation code (dmabuf_export.rs:93-151, 157-218)
**File:** `src/render/dmabuf_export.rs`
**Lines:** 93-151, 157-218

The `create_dmabuf_target` function (line 93) and `export_dmabuf_buffer` function (line 157) are nearly identical — both create a dmabuf-exportable texture with the same format, usage, and modifier logic. The only difference is the function name and visibility.

**Impact:** Code duplication — maintenance burden and risk of divergence. If one is fixed, the other may be missed.

**Fix:** Extract the common logic into a single helper function.

---

### 249. state/mod.rs — AppState is a god struct with ~200 fields (state/mod.rs:110-259)
**File:** `src/state/mod.rs`
**Lines:** 110-259

The `AppState` struct contains approximately 200 public fields spanning Wayland protocol state, GPU state, UI state, interaction state, background channels, and configuration. All fields are `pub`, meaning any module can modify any field without going through a controlled interface.

**Impact:** No encapsulation — any module can modify any state field, making it impossible to reason about invariants. Race conditions are possible if multiple threads access AppState (though it's primarily single-threaded). The struct is also expensive to clone or move.

**Fix:** Group related fields into sub-structs (e.g., `WaylandState`, `GPUState`, `UIState`) with controlled accessors. Make fields `pub(crate)` or private with getter/setter methods.

---

### 250. state/mod.rs — Surface ID lookups iterate all docks (state/mod.rs:329-443)
**File:** `src/state/mod.rs`
**Lines:** 329-443

Methods like `is_status_tray_bar_surface`, `is_bottom_system_status_surface`, `is_status_widget_bar_surface`, `is_audio_player_surface`, and `audio_player_expanded_on_surface` all iterate `self.docks` to find which dock a `WlSurface` belongs to. With N docks, each lookup is O(N).

**Impact:** O(n) lookup for surface identity. With 2-4 docks (multi-monitor), each click handler iterates all docks multiple times.

**Fix:** Use a `HashMap<ObjectId, SurfaceRole>` for O(1) lookups, or cache the dock index by surface ID.

---

### 251. main.rs — Animations disabled with hardcoded false values (main.rs:324-335)
**File:** `src/main.rs`
**Lines:** 324-335

```rust
// Hide/show animations disabled
// state.update_animations(&qh);

// Animation flags all false — animations disabled
let _hover_anim_active = false;
let _menu_anim_active = false;
let _window_list_anim_active = false;
// state.finalize_popup_closures(); // disabled

let _hide_animating = false;
let _menu_fade_animating = false;
let _window_list_fade_animating = false;
```

All animation-related code is disabled with hardcoded `false` values. The variables are prefixed with `_` to suppress unused warnings. The `update_animations` and `finalize_popup_closures` methods are commented out.

**Impact:** All dock animations (fade in/out, hover, menu, window list) are completely disabled. The dock appears/disappears instantly without transition.

**Fix:** Re-enable the animation system or remove the dead code.

---

### 252. main.rs — Popup animation always disabled (main.rs:393)
**File:** `src/main.rs`
**Line:** 393

```rust
let popup_only_animation = false;
```

The comment says "All animations disabled — popup_only_animation always false." This means even when only a popup is animating, the entire dock is re-rendered instead of just the popup subsurface.

**Impact:** Inefficient rendering — the entire dock is re-rendered when only a popup needs updating. This wastes GPU/CPU cycles.

**Fix:** Re-enable popup-only animation mode.

---

### 253. init.rs — sysinfo System created with full process refresh (init.rs:273-277)
**File:** `src/init.rs`
**Lines:** 273-277

```rust
sys_scanner: sysinfo::System::new_with_specifics(
    sysinfo::RefreshKind::nothing()
        .with_processes(sysinfo::ProcessRefreshKind::everything()),
)
```

The `sysinfo::System` is created with `ProcessRefreshKind::everything()`, which means every call to `refresh_processes()` will read `/proc` for every process on the system. This is expensive on systems with many processes.

**Impact:** High CPU usage during process scanning — reading all `/proc/[pid]/` entries is I/O intensive. If called every frame, this would dominate CPU usage.

**Fix:** Use `ProcessRefreshKind::nothing().with_cpu().with_memory()` to refresh only the needed fields. Throttle the refresh rate.

---

### 254. state/dock.rs — window_count_for_app calls get_running_by_app every time (state/dock.rs:158-160)
**File:** `src/state/dock.rs`
**Lines:** 158-160

```rust
pub fn window_count_for_app(&self, app_id: &str) -> usize {
    self.get_running_by_app().get(app_id).map_or(0, Vec::len)
}
```

Every call to `window_count_for_app` rebuilds the entire `running_by_app` HashMap from scratch, even though it only needs the count for one app.

**Impact:** O(n) HashMap rebuild for a single count query. Called for each icon during rendering, this is O(n × m) where n is window count and m is icon count.

**Fix:** Cache the running_by_app map or implement a direct count method.

---

### 255. render/slint_dock.rs — Slint icon_cache never evicted (slint_dock.rs:53)
**File:** `src/render/slint_dock.rs`
**Line:** 53

```rust
pub icon_cache: std::collections::HashMap<String, Image>,
```

The Slint renderer's icon cache stores `Image` objects keyed by app_id. Images are Slint-wrapped pixel buffers. When an app is unpinned or its icon changes, the old image remains in the cache.

**Impact:** Memory leak — old icon images accumulate in the Slint cache. Each 48×48 RGBA icon is ~9KB. With 100 apps over time, that's ~900KB of orphaned images.

**Fix:** Evict entries when apps are unpinned or when the cache exceeds a size limit.

---

### 256. services/pipewire_capture.rs — 2511 lines, single massive module (pipewire_capture.rs)
**File:** `src/services/pipewire_capture.rs`

The entire PipeWire capture service is in a single 2511-line file. It handles XDG Desktop Portal D-Bus communication, PipeWire stream management, frame conversion, window-to-stream matching, and monitor tasks — all in one module.

**Impact:** Maintainability — a single change to any part of the capture pipeline requires understanding the entire 2500-line file. Testing is difficult because there are no isolated units.

**Fix:** Split into submodules: `portal.rs` (D-Bus), `stream.rs` (PipeWire streams), `frame.rs` (conversion), `monitor.rs` (background task), `matching.rs` (window-to-stream).

---

## Medium Severity Issues

### 257. state/dock.rs — get_app_id_at_location uses hardcoded constants (state/dock.rs:162-196)
**File:** `src/state/dock.rs`
**Lines:** 162-196

```rust
let app_row_height = DOCK_HEIGHT;
// ...
let total_icons_width =
    total_items as f64 * BOX_SIZE + total_items.saturating_sub(1) as f64 * SPACING;
```

The hit test uses module-level constants (`BOX_SIZE = 48.0`, `SPACING = 8.0`, `DOCK_HEIGHT = 60.0`) instead of reading from `self.config`. If the user changes the icon size or spacing in the config, the hit test will use the wrong dimensions.

**Impact:** Hit testing fails when config differs from defaults. A user with 64px icons will have incorrect hit areas.

**Fix:** Read `BOX_SIZE`, `SPACING`, and `DOCK_HEIGHT` from `self.config` instead of using module constants.

---

### 258. state/dock.rs — sync_dock_surface_width doesn't check all docks consistently (state/dock.rs:227-287)
**File:** `src/state/dock.rs`
**Lines:** 227-287

```rust
for dock in &mut self.docks {
    if dock.requested_width == desired_width {
        continue;
    }
    // ...
}
```

The method iterates all docks and resizes them to `desired_width`. However, `desired_width` is calculated once for all docks, assuming all outputs have the same app count. On multi-monitor setups with different pinned apps per output, this would incorrectly resize all docks to the same width.

**Impact:** On multi-monitor setups, all docks are resized to the same width, even if they have different app counts.

**Fix:** Calculate `desired_width` per-dock based on each dock's app count.

---

### 259. main.rs — Hardcoded layout constants in main loop (main.rs:345-347)
**File:** `src/main.rs`
**Lines:** 345-347

```rust
let dock_height = 60;
let box_size = 48;
let spacing = 8;
```

The main loop uses hardcoded layout constants instead of reading from `state.config`. These are passed to `library::interaction::update()` for proximity detection.

**Impact:** Interaction proximity detection uses hardcoded values that don't match the user's config. A user with 64px icons will have incorrect hover detection.

**Fix:** Read these values from `state.config` instead of hardcoding them.

---

### 260. state/mod.rs — IconLoadResult animation field unused (state/mod.rs:102)
**File:** `src/state/mod.rs`
**Line:** 102

```rust
pub animation: Option<IconAnimation>,
```

The `IconLoadResult` struct has an `animation` field for animated icon frames. However, in `main.rs:301-310`, the animation update loop only processes `state.animations`, not the `IconLoadResult.animation` field. The background icon loader never populates this field.

**Impact:** Dead field — animated icons from the loader are never used. The `state.animations` HashMap is populated elsewhere, but the loader's animation field is ignored.

**Fix:** Either use the `IconLoadResult.animation` field or remove it.

---

### 261. state/mod.rs — load_generic_fallback_bytes creates a solid gray icon (state/mod.rs:576-587)
**File:** `src/state/mod.rs`
**Lines:** 576-587

```rust
pub(crate) fn load_generic_fallback_bytes() -> Option<(Vec<u8>, u32)> {
    let size = 48;
    let total_bytes = (size * size * 4) as usize;
    let mut rgba = vec![0u8; total_bytes];

    // Efficiently fill buffer using fixed 4-byte RGBA chunks (128, 128, 128, 180)
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[128, 128, 128, 180]);
    }

    Some((rgba, size))
}
```

The fallback icon is a solid gray rectangle (128, 128, 128, 180). This is visually indistinguishable from a missing icon or a loading state. The alpha is 180/255 ≈ 70%, so it's semi-transparent.

**Impact:** Poor UX — a solid gray square provides no visual feedback about which app it represents. A better fallback would show the app's initial letter or a generic app icon.

**Fix:** Render the app's initial letter or use a themed fallback icon.

---

### 262. init.rs — Font search uses expect() (init.rs:188)
**File:** `src/init.rs`
**Line:** 188

```rust
.find(|p| p.exists())
.expect("No valid font file found!");
```

If no font file is found, the application panics. The search paths include `assets/font.ttf`, `font.ttf`, `~/.local/share/dock/font.ttf`, `/usr/share/dock/font.ttf`, and `/usr/share/fonts/TTF/DejaVuSans.ttf`. On a minimal system without DejaVu, this will crash.

**Impact:** Application crash on systems without the expected font files.

**Fix:** Fall back to a built-in font or use a system font discovery mechanism.

---

### 263. init.rs — Layer surface created with Layer::Top (init.rs:376)
**File:** `src/init.rs`
**Line:** 376

```rust
let layer_surface = state.layer_shell.create_layer_surface(
    &qh,
    raw_surface,
    Layer::Top,
    Some("dock_panel"),
    Some(&output),
);
```

The dock uses `Layer::Top`, which places it above normal windows but below overlay layers. Some compositors may place other Top-layer surfaces (like notification popups) above the dock. The comment says "Overlay mode" but `Layer::Overlay` would be more appropriate for a dock that should always be visible.

**Impact:** The dock may be obscured by other Top-layer surfaces (notifications, etc.).

**Fix:** Use `Layer::Overlay` for the dock to ensure it's always visible above normal windows.

---

### 264. init.rs — Exclusive zone set to 0 (init.rs:403)
**File:** `src/init.rs`
**Line:** 403

```rust
layer_surface.set_exclusive_zone(0);
```

The exclusive zone is 0, meaning the compositor won't reserve any work area for the dock. Windows will extend under the dock, and the dock will overlap them. The comment says "Do not change this on show/hide transitions."

**Impact:** Windows extend under the dock, potentially making content invisible. The dock overlaps window content.

**Fix:** Set `exclusive_zone` to the dock height so the compositor reserves space for the dock.

---

### 265. config.rs — 1521 lines, massive configuration struct (config.rs)
**File:** `src/config.rs`

The configuration struct spans 1521 lines with hundreds of fields covering dock layout, animations, popups, badges, time display, tray, and more. All fields are serialized/deserialized with serde.

**Impact:** Maintainability — adding a new config option requires understanding the entire 1500-line file. Migration between versions is difficult with no versioning strategy.

**Fix:** Split into sub-structs (e.g., `DockConfig`, `AnimationConfig`, `PopupConfig`) with a versioned top-level struct.

---

### 266. types.rs — DockRenderState borrows mutable FontManager (types.rs:103)
**File:** `src/types.rs`
**Line:** 103

```rust
pub font_manager: &'a mut FontManager,
```

`DockRenderState` contains a mutable reference to `FontManager`. This means only one `DockRenderState` can exist at a time, and the font manager cannot be used concurrently. If parallel rendering is ever attempted, this will cause a borrow conflict.

**Impact:** Prevents parallel rendering — the mutable borrow on FontManager means rendering cannot be parallelized.

**Fix:** Use `&'a FontManager` (immutable) or `Arc<FontManager>` if the font manager is thread-safe.

---

### 267. state/animation.rs — AnimationState doesn't track duration (state/animation.rs:18-26)
**File:** `src/state/animation.rs`
**Lines:** 18-26

```rust
pub struct AnimationState {
    pub active: bool,
    pub dirty: bool,
    pub started_at: Option<Instant>,
    pub last_frame: Option<Instant>,
    pub frame_count: u64,
    pub target_fps: u32,
    pub kind: Option<AnimationKind>,
}
```

The animation state tracks `frame_count` and `target_fps`, but has no `duration` field. The animation runs indefinitely until `finish()` is called. There's no automatic stop based on elapsed time.

**Impact:** Animations run forever unless explicitly stopped. If the stop condition is missed, the animation consumes CPU indefinitely.

**Fix:** Add a `duration: Duration` field and auto-finish when elapsed time exceeds it.

---

### 268. main.rs — Animation update loop clones strings (main.rs:301-310)
**File:** `src/main.rs`
**Lines:** 301-310

```rust
for (app_id, anim) in state.animations.iter_mut() {
    if anim.update() {
        if let Some(frame) = anim.current_frame() {
            state
                .icon_cache
                .insert(app_id.clone(), (frame.rgba.clone(), frame.width));
            animated_apps_changed = true;
        }
    }
}
```

For each animated frame, the app_id is cloned and the entire RGBA frame is cloned into the icon cache. This is a full Vec<u8> clone per frame per animation.

**Impact:** Per-frame heap allocation for animated icon frames. A 48×48 RGBA frame is ~9KB. At 60 FPS with 2 animations, that's ~1MB/s of allocations.

**Fix:** Use `Arc<Vec<u8>>` for frame data to avoid cloning, or swap the frame in place.

---

### 269. state/dock.rs — get_apps_in_dock clones pinned_apps (state/dock.rs:95)
**File:** `src/state/dock.rs`
**Line:** 95

```rust
let mut apps_in_dock = self.pinned_apps.clone();
```

Every call to `get_apps_in_dock` clones the entire `pinned_apps` Vec. With 20 pinned apps, this is 20 String clones per call.

**Impact:** Per-frame String cloning. At 60 FPS, that's 1,200 String clones per second.

**Fix:** Use `self.pinned_apps.as_slice()` and extend it with unpinned running apps, avoiding the initial clone.

---

### 270. render/slint_dock.rs — State hash computation iterates all pins (slint_dock.rs:103-109)
**File:** `src/render/slint_dock.rs`
**Lines:** 103-109

The `compute_state_hash` method iterates all pins to compute a hash. This is called every frame to determine if re-rendering is needed. The hash includes pin app_ids, running status, focus state, window counts, badges, hover, and drag state.

**Impact:** O(n) hash computation per frame. With 20 pins, this is 20 iterations per frame.

**Fix:** Incrementally update the hash when state changes, rather than recomputing from scratch.

---

## Low Severity Issues

### 271. state/mod.rs — Unused variable warnings suppressed (state/mod.rs:287, 293-297)
**File:** `src/state/mod.rs`
**Lines:** 287, 293-297

```rust
let _item_count = snapshot.items.len();
// ...
let _icon_count = self
    .tray_items
    .iter()
    .filter(|item| item.icon.is_some())
    .count();
```

These variables are computed but unused. They were likely added for debugging and left in the code.

**Impact:** Wasted CPU cycles computing unused values.

**Fix:** Remove the unused variables.

---

### 272. main.rs — GPU error silently ignored (main.rs:65-68)
**File:** `src/main.rs`
**Lines:** 65-68

```rust
Err(_e) => {
    state.gpu_available = false;
}
```

If GPU initialization fails, the error is silently ignored. No log message is emitted, so the user has no way to know why GPU rendering is unavailable.

**Impact:** Silent fallback — the user won't know GPU rendering failed. Debugging GPU issues is difficult without error context.

**Fix:** Log the error with `tracing::error!` or `eprintln!`.

---

### 273. init.rs — App log uses Instant::now().elapsed() (init.rs:98)
**File:** `src/init.rs`
**Line:** 98

```rust
writeln!(
    app_logger,
    "[{}] INFO: Dock application starting up.",
    Instant::now().elapsed().as_secs()
)
```

`Instant::now().elapsed()` returns the time since process start, not a wall-clock timestamp. The log entry `[0] INFO: Dock application starting up.` is meaningless for debugging — it doesn't tell you when the event occurred.

**Impact:** Useless timestamps in logs — all entries start from 0, making it impossible to correlate with system logs.

**Fix:** Use `chrono::Local::now()` or `std::time::SystemTime::now()` for wall-clock timestamps.

---

### 274. state/dock.rs — sync_dashed_block_position and sync_date_time_block_position duplicate logic (state/dock.rs:294-387)
**File:** `src/state/dock.rs`
**Lines:** 294-387

Both methods follow the same pattern: iterate docks, get output size, calculate position based on anchor, set margins, and commit. The only differences are the block type (dashed vs date/time) and the gap/width config values.

**Impact:** Code duplication — if the anchor logic changes, both methods must be updated.

**Fix:** Extract a generic `sync_block_position` method that takes the block type and config values as parameters.

---

### 275. types.rs — PinItem uses usize for size (types.rs:66)
**File:** `src/types.rs`
**Line:** 66

```rust
pub size: usize,
```

The icon size is `usize` (platform-dependent, 64-bit on Linux), but it's used for pixel dimensions. The config uses `u32` for icon size. This type mismatch requires casting everywhere.

**Impact:** Type mismatch — `usize` vs `u32` requires casts between config and runtime types.

**Fix:** Use `u32` for pixel dimensions consistently.

---

### 276. config.rs — Background alpha uses f32 (config.rs:81)
**File:** `src/config.rs`
**Line:** 81

```rust
pub background_alpha: f32,
```

The alpha is `f32` with range 0.0–1.0. However, in `dock.rs` (GPU path), the alpha was previously divided by 100, suggesting the config value might be 0–100 in some versions. The serde default is `default_background_alpha` which returns `0.8`.

**Impact:** Ambiguous range — is the config value 0.0–1.0 or 0–100? Different parts of the code may interpret it differently.

**Fix:** Document the range explicitly and validate the config value on load.

---

### 277. state/mod.rs — DmabufState created before GPU check (init.rs:333)
**File:** `src/init.rs`
**Line:** 333

```rust
dmabuf_state: smithay_client_toolkit::dmabuf::DmabufState::new(&globals, &qh),
```

The `DmabufState` is created during AppState initialization, before the GPU is checked in `main.rs`. If DMA-BUF is not supported by the compositor, the state is still created but never used.

**Impact:** Minor — the DmabufState binding is harmless but unnecessary if GPU rendering is unavailable.

**Fix:** Create DmabufState lazily after GPU availability is confirmed.

---

### 278. main.rs — Poll FD access is unsafe (main.rs:436-449)
**File:** `src/main.rs`
**Lines:** 436-449

```rust
if let Some(guard) = state.connection.prepare_read() {
    let raw_fd = std::os::unix::io::AsRawFd::as_raw_fd(&guard.connection_fd());
    let mut pfd = libc::pollfd {
        fd: raw_fd,
        events: libc::POLLIN,
        revents: 0,
    };

    let poll_res = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
```

The `libc::poll` call is `unsafe` because it takes a raw file descriptor. If the guard is dropped before `poll` returns, the FD could be invalid. However, the guard is held for the duration of the poll, so this is safe in practice.

**Impact:** Low — the unsafe block is correct but could be replaced with a safe wrapper.

**Fix:** Use `wayland_client`'s built-in event queue dispatch instead of manual poll, or wrap the unsafe block in a safe helper.

---

### 279. init.rs — Tray action tx cloned before use (init.rs:204)
**File:** `src/init.rs`
**Line:** 204

```rust
tray_action_tx: tray_action_tx.clone(),
```

The `tray_action_tx` is cloned before being stored in AppState. The original is moved into the background thread in `init.rs:124-125`. This means both the AppState and the background thread have a sender, but only the background thread has the receiver.

**Impact:** Confusing ownership — the clone suggests both sides can send, but the receiver is only in one place.

**Fix:** Document the ownership clearly or restructure to avoid the clone.

---

### 280. services/pipewire_capture.rs — StreamFrameData uses Arc<Mutex<Option<...>>> (pipewire_capture.rs:98-108)
**File:** `src/services/pipewire_capture.rs`
**Lines:** 98-108

```rust
pub struct StreamFrameData {
    pub frame: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat, Vec<u8>)>>>,
    pub format_info: Arc<Mutex<Option<(u32, u32, pw::spa::param::video::VideoFormat)>>>,
    pub ready: Arc<AtomicBool>,
```

Each field is wrapped in `Arc<Mutex<>>`, meaning every access requires a lock. The `frame` field contains a `Vec<u8>` that is locked, replaced, and unlocked on every new frame. This is a lock contention hotspot.

**Impact:** Lock contention — every frame acquisition and release requires a mutex lock. With multiple windows, this compounds.

**Fix:** Use a lock-free ring buffer or `ArcSwap` for the frame data.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 241 | **Critical** | Multiple independent tokio runtimes | Resource waste, 2× I/O polling |
| 242 | **Critical** | Scale factor hardcoded to 1.0 | Wrong buffer sizes on HiDPI |
| 243 | **Critical** | Main loop no frame rate cap | 100% CPU spin |
| 244 | **Critical** | get_apps_in_dock called every frame | Per-frame heap allocations |
| 245 | **Critical** | sync_live_state_from_registry full rebuild | Cache misses, allocation pressure |
| 246 | **Critical** | Hardcoded app-specific canonicalization | Maintenance burden, imprecise matches |
| 247 | **High** | DmabufPool never evicts entries | VRAM leak on resize |
| 248 | **High** | Duplicate DMA-BUF creation code | Code duplication, divergence risk |
| 249 | **High** | AppState god struct with ~200 fields | No encapsulation, invariant violations |
| 250 | **High** | Surface ID lookups iterate all docks | O(n) per click |
| 251 | **High** | Animations disabled with hardcoded false | No dock animations |
| 252 | **High** | Popup animation always disabled | Inefficient full-dock re-render |
| 253 | **High** | sysinfo full process refresh | High CPU during process scan |
| 254 | **High** | window_count_for_app rebuilds HashMap | O(n×m) per render |
| 255 | **High** | Slint icon_cache never evicted | Memory leak |
| 256 | **High** | 2511-line single module | Maintainability nightmare |
| 257 | **Medium** | Hit test uses hardcoded constants | Fails with non-default config |
| 258 | **Medium** | sync_dock_surface_width same width for all | Wrong on multi-monitor |
| 259 | **Medium** | Hardcoded layout in main loop | Wrong hover detection |
| 260 | **Medium** | IconLoadResult.animation unused | Dead field |
| 261 | **Medium** | Fallback icon is solid gray | Poor UX |
| 262 | **Medium** | Font search uses expect() | Crash on minimal systems |
| 263 | **Medium** | Layer::Top instead of Overlay | Dock may be obscured |
| 264 | **Medium** | Exclusive zone = 0 | Windows extend under dock |
| 265 | **Medium** | 1521-line config file | Maintainability |
| 266 | **Medium** | DockRenderState borrows mut FontManager | Prevents parallel rendering |
| 267 | **Medium** | AnimationState no duration | Animations run forever |
| 268 | **Medium** | Animation loop clones strings | Per-frame heap allocations |
| 269 | **Medium** | get_apps_in_dock clones pinned_apps | Per-frame String cloning |
| 270 | **Medium** | State hash recomputed every frame | O(n) per frame |
| 271 | **Low** | Unused variable warnings | Wasted CPU |
| 272 | **Low** | GPU error silently ignored | Silent fallback |
| 273 | **Low** | App log uses process-relative time | Useless timestamps |
| 274 | **Low** | Duplicate block position logic | Code duplication |
| 275 | **Low** | PinItem size uses usize | Type mismatch |
| 276 | **Low** | Background alpha range ambiguous | Config interpretation |
| 277 | **Low** | DmabufState created before GPU check | Unnecessary binding |
| 278 | **Low** | Poll FD access is unsafe | Could use safe wrapper |
| 279 | **Low** | Tray action tx cloned | Confusing ownership |
| 280 | **Low** | StreamFrameData Arc<Mutex<Option>> | Lock contention |

---

## Cumulative Summary

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 1 | 2 | 2 | 3 |
| 2 | gpu-rendering-review-2.md | 13–32 | 2 | 4 | 4 | 3 |
| 3 | gpu-rendering-review-3.md | 33–60 | 3 | 5 | 6 | 4 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 6 | 7 | 5 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 8 | 6 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| 8 | gpu-rendering-review-8.md | 200–240 | 6 | 10 | 16 | 8 |
| 9 | gpu-rendering-review-9.md | 241–280 | 6 | 10 | 16 | 8 |
| **Total** | | **280** | **37** | **66** | **109** | **53** |
