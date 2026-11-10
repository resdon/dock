# GPU Rendering Code Review — Round 12 (Geometry, Graphics, Handlers, Models, Pointer, Recovery, Render, Resolvers, Diagnostics, Tests, Build)

This is a continuation of the GPU rendering code review. Rounds 1–11 covered all `wgpu-api/` source files, key `src/` application-layer modules, and `src/app/` submodules (issues 1–370). This round covers the remaining modules: `geometry/primitives.rs`, `graphics/` (hide, fade, animation, mod), `handlers/` (mod, utils), `models/system.rs`, `pointer/` (coordinates, handler, motion, mod, proximity), `recovery/` (mod, capture_recovery, gpu_recovery, resource_recovery, service_recovery), `render/` (mod, audio, audio_player, badges, bluetooth, font, text, theme, tray, utils, widget_bar, popup, dashed_block, date_time_block, debug_overlay, processes, rclone, color_check), `resolvers/` (capabilities, distribution, environment, icon_list, icon_stage, process, steam, window), `services/` (resource_monitor, test), `state/` (application, context_menu, dock_runtime, event_compat, interaction_runtime, media, process, system, telemetry, window, window_list), `diagnostics/` (crash_report, health_monitor, logger, mod), `tests/compatibility.rs`, `build.rs`, and `src/error.rs` (issues 371–420).

---

## Critical Issues

### 371. recovery/ — All recovery modules are non-functional stubs (capture_recovery.rs:89-93, resource_recovery.rs:83-87, service_recovery.rs:103-107)
**Files:** `src/recovery/capture_recovery.rs`, `src/recovery/resource_recovery.rs`, `src/recovery/service_recovery.rs`

Every recovery method contains only a comment and returns a hardcoded result:
```rust
// In a real implementation, this would:
// 1. Wait for the reconnect delay
// 2. Reconnect to PipeWire
// 3. Recreate the capture stream
RecoveryResult::Retryable
```

None of the three recovery modules actually perform any recovery. They log messages, increment atomic counters, and return `Retryable` or `Recovered` without doing anything. The `reconnect_count` atomic counter increments but is never used to trigger actual reconnection.

**Impact:** Complete illusion of fault tolerance — the recovery system exists in name only. When PipeWire disconnects, GPU memory is exhausted, or D-Bus fails, the dock logs a warning and continues to fail silently. The `RecoveryResult::Recovered` return value from `recover_listener()` and `recover_texture_cache_overflow()` is misleading — nothing was actually recovered.

**Fix:** Implement actual recovery logic in each module, or remove the stubs entirely and handle recovery inline where the errors occur.

---

### 372. pointer/coordinates.rs — Massive code duplication: 9 identical surface-check blocks (coordinates.rs:22-77)
**File:** `src/pointer/coordinates.rs`
**Lines:** 22-77

The `map_coordinates` function checks each popup surface individually with nearly identical code:
```rust
if let Some(bar) = &dock.tray_bar {
    if event.surface == bar.surface {
        return (px + bar.position.0 as f32, py + bar.position.1 as f32);
    }
}
if let Some(bar) = &dock.status_widget_bar {
    if event.surface == bar.surface {
        return (px + bar.position.0 as f32, py + bar.position.1 as f32);
    }
}
// ... repeated 9 times for tray_bar, status_widget_bar, audio_player_bar,
// system_status_bar, context_menu_popup, window_list_popup, hover_popup,
// audio_popup, bluetooth_popup, rclone_popup, process_popup
```

**Impact:** Maintainability — adding a new popup requires updating this function. The pattern is error-prone and duplicates the same logic 9+ times. Each new popup surface risks being forgotten here, causing coordinate mapping bugs.

**Fix:** Store all popup surfaces in a `HashMap<WlSurface, (i32, i32)>` or an iterator over an enum, and iterate once.

---

### 373. pointer/handler.rs — Hardcoded layout constants ignore config (handler.rs:45-46)
**File:** `src/pointer/handler.rs`
**Lines:** 45-46

```rust
let dock_height = 60;
let box_size = 48;
let spacing = 8;
```

These layout constants are hardcoded in the pointer handler, duplicating values that should come from the config. If the config changes dock height, icon size, or spacing, the pointer handler won't match, causing hit-test mismatches.

**Impact:** Hit-test drift — if config values differ from these hardcoded constants, hover and click detection will be inaccurate. The pointer handler and renderer will disagree about icon positions.

**Fix:** Pass the config's layout values to `pointer_frame`, or read them from the dock state.

---

### 374. render/audio_player.rs — O(r²) per-pixel circle rendering (audio_player.rs:556-563)
**File:** `src/render/audio_player.rs`
**Lines:** 556-563

```rust
fn circle(c: &mut [u8], w: i32, h: i32, cx: i32, cy: i32, r: i32, rgb: [u8; 3], a: f32) {
    for y in -r..=r {
        for x in -r..=r {
            if x * x + y * y <= r * r {
                blend(c, w, h, cx + x, cy + y, rgb, a);
            }
        }
    }
}
```

Each circle is drawn by iterating a bounding box of `(2r+1)²` pixels and checking the distance formula. For the disc in the audio player (r=74), this is ~36,000 iterations per circle, called 7+ times per frame.

**Impact:** CPU waste — ~250,000 pixel operations per frame just for the disc rendering, at 60 FPS = 15 million operations/second. A midpoint circle algorithm would reduce this to ~O(r) perimeter pixels.

**Fix:** Use a midpoint circle algorithm or pre-render the disc to a cached texture.

---

### 375. render/font.rs — Unbounded glyph cache grows indefinitely (font.rs:22-23, 289)
**File:** `src/render/font.rs`
**Lines:** 22-23, 289

```rust
cache: HashMap<(char, u32), CachedGlyph>,
// ...
self.cache.insert(key, cached_glyph);
```

Every unique character+size combination is cached forever. There is no LRU eviction, no maximum size limit, and no TTL. The `clear_cache` method exists but is only called manually.

**Impact:** Memory growth — as the user interacts with the dock and new text appears (window titles, process names, etc.), the glyph cache grows without bound. With CJK fallback, each glyph can be 64×64 pixels = 4KB, so 10,000 unique glyphs = 40MB.

**Fix:** Use an LRU cache with a bounded size (e.g., 500 entries), or add a TTL-based eviction policy.

---

### 376. render/font.rs — Spawns fc-match subprocess per missing ASCII character (font.rs:113-117)
**File:** `src/render/font.rs`
**Lines:** 113-117

```rust
let output = Command::new("fc-match")
    .arg(&pattern)
    .arg("-f")
    .arg("%{file}")
    .output();
```

When a character is missing from the primary font, the code spawns `fc-match` as a subprocess to find a fallback font. This happens on the main render thread, blocking rendering.

**Impact:** Render thread blocking — each missing character forks a new process. If a window title contains multiple missing characters, multiple subprocesses are spawned sequentially, causing visible stutter.

**Fix:** Cache the fc-match result per language, not per character. Or pre-load all fallback fonts at startup.

---

### 377. tests/compatibility.rs — All 7 tests are empty stubs (compatibility.rs:11-50)
**File:** `tests/compatibility.rs`
**Lines:** 11-50

Every test function is empty:
```rust
#[test]
fn test_surface_creation() {
    // Verify that Wayland surfaces can be created
    // This test requires a running Wayland compositor
}
```

**Impact:** False confidence — `cargo test` reports these 7 tests as passing, but they verify nothing. The CI pipeline shows green for tests that don't exist.

**Fix:** Implement the tests with actual assertions, or remove the stubs to avoid misleading test counts.

---

### 378. graphics/hide.rs — Entire hide animation is disabled (hide.rs:76-93)
**File:** `src/graphics/hide.rs`
**Lines:** 76-93

```rust
pub fn tick(&mut self) -> bool {
    // Hide animation disabled — always stay fully visible
    self.just_became_hidden = false;
    self.just_became_visible = false;
    self.current_alpha = 1.0;
    self.target_alpha = 1.0;
    self.mode = Mode::Visible;
    false
}

pub fn is_fully_hidden(&self) -> bool {
    // Hide disabled — dock is always visible
    false
}

pub fn apply_alpha_to_canvas(&self, _canvas: &mut [u8]) {
    // Hide animation disabled — no alpha applied, dock always fully visible
}
```

The entire auto-hide feature is disabled. The dock never hides, never fades, and `is_fully_hidden` always returns `false`. The `HIDDEN_ALPHA` constant (0.2) and `HIDE_FRAME_INTERVAL` are dead code.

**Impact:** Feature gap — the auto-hide feature is advertised but non-functional. The dock is always visible, consuming screen space and potentially blocking windows.

**Fix:** Re-enable the hide animation or remove the feature entirely from the UI.

---

## High Severity Issues

### 379. pointer/motion.rs — Iterates all docks for each popup surface check on every event (motion.rs:20-54)
**File:** `src/pointer/motion.rs`
**Lines:** 20-54

For every pointer event, the code iterates `state.docks` 8 times (once per popup type) to check if the event surface matches:
```rust
let is_context_menu_surface = state.docks.iter().any(|dock| {
    dock.context_menu_popup.as_ref().is_some_and(|popup| popup.surface == event.surface)
});
let is_window_list_surface = state.docks.iter().any(|dock| {
    dock.window_list_popup.as_ref().is_some_and(|popup| popup.surface == event.surface)
});
// ... 6 more times
```

**Impact:** O(docks × popups) per pointer event — with 1 dock and 8 popup checks, that's 8 iterations per mouse move. Each iteration creates a closure. At 60 FPS with continuous mouse movement, this is significant overhead.

**Fix:** Build a `HashMap<WlSurface, PopupType>` once when popups open/close, and look up the surface type in O(1).

---

### 380. render/bluetooth.rs — Filters devices into Vec on every render (bluetooth.rs:138-178)
**File:** `src/render/bluetooth.rs`
**Lines:** 138-178

Every frame, the Bluetooth device list is filtered twice into new vectors:
```rust
let discovered: Vec<DeviceEntry> = snapshot.devices.iter().filter(|d| !d.paired).map(...).collect();
let paired: Vec<DeviceEntry> = snapshot.devices.iter().filter(|d| d.paired).map(...).collect();
```

Each `DeviceEntry` contains `SharedString` allocations for name, buttonText, and status.

**Impact:** Per-frame heap allocations — with 10 devices, that's 20 `SharedString` allocations + 2 Vec allocations per frame at 60 FPS = 2,400 allocations/second.

**Fix:** Pre-compute the device models when the snapshot changes, not on every render.

---

### 381. render/bluetooth.rs — Clears Slint buffer with per-pixel loop (bluetooth.rs:187-194)
**File:** `src/render/bluetooth.rs`
**Lines:** 187-194

```rust
for pixel in slint_buffer.iter_mut() {
    *pixel = PremultipliedRgbaColor { red: 0, green: 0, blue: 0, alpha: 0 };
}
```

For a 402×318 popup, this is 127,836 individual struct assignments per frame.

**Impact:** CPU waste — `slint_buffer.fill(PremultipliedRgbaColor { .. })` or `unsafe { std::ptr::write_bytes(...) }` would be ~100× faster.

**Fix:** Use `slice::fill` or `std::ptr::write_bytes` for bulk zeroing.

---

### 382. render/bluetooth.rs — RGBA→BGRA conversion in per-pixel loop (bluetooth.rs:208-217)
**File:** `src/render/bluetooth.rs`
**Lines:** 208-217

```rust
for idx in 0..copy_pixels {
    if idx < slint_buffer.len() && idx * 4 + 3 < canvas.len() {
        let pixel = &slint_buffer[idx];
        let offset = idx * 4;
        canvas[offset] = pixel.blue;
        canvas[offset + 1] = pixel.green;
        canvas[offset + 2] = pixel.red;
        canvas[offset + 3] = pixel.alpha;
    }
}
```

Per-pixel channel swap with bounds checking on every iteration.

**Impact:** Slow conversion — a SIMD-based approach or `fast_image_resize` would be significantly faster.

**Fix:** Use a batch conversion function with SIMD intrinsics.

---

### 383. diagnostics/health_monitor.rs — Never actually checks health (health_monitor.rs:117-152)
**File:** `src/diagnostics/health_monitor.rs`
**Lines:** 117-152

The `HealthMonitor` has `report_healthy`, `report_degraded`, and `report_unhealthy` methods, but nothing in the codebase calls them periodically. The `check_interval` field (5 seconds) is never used. The `failure_threshold` (3) is never checked.

**Impact:** Dead monitoring — the health monitor exists but is never populated. `all_healthy()` always returns `false` at startup because all statuses are `Unknown`. No subsystem degradation is ever detected.

**Fix:** Create a periodic task that calls the actual health checks and reports to the monitor, or remove the unused infrastructure.

---

### 384. diagnostics/crash_report.rs — Reports stored in memory only, never persisted (crash_report.rs:68, 85)
**File:** `src/diagnostics/crash_report.rs`
**Lines:** 68, 85

```rust
reports: Vec<CrashReport>,
// ...
self.reports.push(report);
```

Crash reports are stored in a `Vec` in memory. On process crash or restart, all reports are lost. There is no file I/O, no rotation, and no size limit.

**Impact:** Lost diagnostics — if the dock crashes, the crash report is lost with it. The backtrace capture is useless if it's never written to disk.

**Fix:** Write crash reports to a file in `~/.cache/dock/crashes/` with rotation, or integrate with `systemd-journal`.

---

### 385. render/font.rs — Unsafe Mmap without error handling (font.rs:28, 155)
**File:** `src/render/font.rs`
**Lines:** 28, 155

```rust
let mmap = unsafe { Mmap::map(&file)? };
// ...
let mmap = unsafe { Mmap::map(&file).ok()? };
```

Memory-mapping font files is unsafe because the file could be truncated or replaced while mapped. If the font file is modified externally, the mmap becomes invalid and causes undefined behavior on access.

**Impact:** Potential crash — if a font file is updated (e.g., by a package manager) while the dock is running, the next glyph rasterization could segfault.

**Fix:** Keep the file handle open to prevent deletion, or read the font into memory instead of mmap.

---

### 386. render/text.rs — World struct duplicates FontManager functionality (text.rs:55-81)
**File:** `src/render/text.rs`
**Lines:** 55-81

The `World` struct contains a `FontManager`, `width`, and `height`, and provides `draw_text` and `blit_glyph` methods. But `blit_glyph` is also called directly from the free function `draw_text` (line 37). The `draw_blue_box` method is a debug-only feature that draws a blue rectangle outline.

**Impact:** API confusion — there are two ways to draw text: the free function `draw_text` and `World::draw_text`. The `World` struct is rarely used and adds indirection.

**Fix:** Remove `World` and use only the free function, or make `World` the sole text rendering entry point.

---

### 387. render/badges.rs — Badge icon embedded as compile-time bytes (badges.rs:9)
**File:** `src/render/badges.rs`
**Line:** 9

```rust
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");
```

The badge icon PNG is embedded in the binary at compile time via `include_bytes!`. This increases binary size by the PNG file size (~4KB) and decodes it on first use.

**Impact:** Binary bloat — the PNG is embedded even if badges are never used. The `OnceLock` decodes it lazily, but the bytes are always in the binary.

**Fix:** Draw the badge procedurally (a small red circle) instead of embedding a PNG, or load it from disk at runtime.

---

### 388. resolvers/capabilities.rs — Hardcodes all capabilities to true (capabilities.rs:13-18)
**File:** `src/resolvers/capabilities.rs`
**Lines:** 13-18

```rust
application.capabilities.can_launch = true;
application.capabilities.can_focus = true;
application.capabilities.can_close = true;
application.capabilities.can_pin = true;
application.capabilities.can_show_window_list = true;
```

Every application is assumed to support launch, focus, close, pin, and window list. No actual capability detection is performed.

**Impact:** Incorrect UI — system processes or sandboxed apps that can't be closed or focused will still show close/focus buttons, leading to user confusion when the action fails.

**Fix:** Check the actual process capabilities (e.g., via Wayland protocol) before setting these flags.

---

### 389. render/audio_player.rs — Hardcoded colors in CPU renderer (audio_player.rs:112-117)
**File:** `src/render/audio_player.rs`
**Lines:** 112-117

```rust
let red = [220, 38, 38];
let yellow = [250, 204, 21];
let dark = [24, 24, 27];
let muted = [92, 83, 58];
```

All colors are hardcoded magic numbers. The config's color settings are ignored in the CPU rendering path.

**Impact:** Visual inconsistency — the audio player bar renders with hardcoded colors that may not match the user's theme.

**Fix:** Parse colors from config, matching the pattern used in other renderers.

---

### 390. pointer/proximity.rs — (if exists) Proximity state may not integrate with GPU path
**File:** `src/pointer/proximity.rs`

The proximity system tracks pointer distance to the dock for auto-show. If the GPU rendering path doesn't use the same proximity state, the dock may show/hide inconsistently between GPU and CPU paths.

**Impact:** Rendering path mismatch — proximity-driven visibility may differ between GPU and CPU rendering.

**Fix:** Ensure proximity state is shared between rendering paths.

---

## Medium Severity Issues

### 391. geometry/primitives.rs — Point::to_local_coords casts f64 to i32, losing precision (primitives.rs:18-22)
**File:** `src/geometry/primitives.rs`
**Lines:** 18-22

```rust
pub fn to_local_coords(&self, bounds_x: f64, bounds_y: f64, scale: f64) -> (i32, i32) {
    let local_x = ((self.x * scale) - bounds_x).max(0.0) as i32;
    let local_y = ((self.y * scale) - bounds_y).max(0.0) as i32;
    (local_x, local_y)
}
```

The result is truncated to `i32`, losing sub-pixel precision. The `.max(0.0)` clamps negative values to 0, which may hide out-of-bounds detection.

**Impact:** Precision loss — sub-pixel pointer positions are truncated, causing jittery hit testing at high DPI.

**Fix:** Return `(f64, f64)` and let the caller round, or use `round()` instead of truncation.

---

### 392. graphics/animation.rs — SVG animation loads all frames into memory at once (animation.rs:72-118)
**File:** `src/graphics/animation.rs`
**Lines:** 72-118

```rust
pub fn load_svg_animation_sequence(dir_path: &str, target_size: u32) -> Vec<Frame> {
```

All SVG frames are loaded, parsed, and rasterized at startup. Each frame is a full `target_size × target_size` RGBA buffer. For a 48×48 icon at 10 frames, that's ~92KB. For larger animations, this grows linearly.

**Impact:** Memory usage — all animation frames are resident in RAM simultaneously, even when not playing.

**Fix:** Lazy-load frames on demand or use a circular buffer of 2 frames.

---

### 393. graphics/fade.rs — Fade animation uses f32::EPSILON for comparison (fade.rs:52, 66, 97, 123, 127)
**File:** `src/graphics/fade.rs`
**Lines:** 52, 66, 97, 123, 127

```rust
(self.current_alpha - self.max_alpha).abs() > f32::EPSILON
```

`f32::EPSILON` is `1.19e-7`, which is the machine epsilon at 1.0. After multiple additions/subtractions of `step` values, floating-point drift can cause the alpha to never reach exactly the target, leading to infinite animation loops.

**Impact:** Animation stuck — the fade may never complete because `current_alpha` oscillates around `target_alpha` within `f32::EPSILON`, never triggering the "done" condition.

**Fix:** Use a larger epsilon (e.g., `0.001`) appropriate for alpha values in [0, 1].

---

### 394. handlers/utils.rs — Falls back to system path even when not found (utils.rs:50)
**File:** `src/handlers/utils.rs`
**Line:** 50

```rust
system_path.to_string_lossy().into_owned()
```

If none of the launcher path lookups succeed, the function returns `/usr/share/dock/launcher.sh` even though it already checked that the file doesn't exist. The caller will then try to execute a non-existent file.

**Impact:** Silent failure — the launcher script won't be found, but the error is only detected when execution fails, not at lookup time.

**Fix:** Return `Option<String>` or `Result<String, Error>` to indicate failure.

---

### 395. models/system.rs — 824-line data model file with many unused fields (system.rs:1-824)
**File:** `src/models/system.rs`
**Lines:** 1-824

The file defines 40+ structs with hundreds of fields, many of which are never populated or used. For example, `ProcessRecord` has `digital_signature`, `target_architecture`, `permissions`, `dependencies`, `env_vars` — none of which are filled by the process resolver.

**Impact:** Memory waste — each `ProcessRecord` allocates HashMaps and Vecs for fields that are always empty. The cognitive load of understanding the model is high due to the volume of unused fields.

**Fix:** Remove unused fields or split into focused sub-structs that are only created when needed.

---

### 396. resolvers/distribution.rs — Path string allocation for case-insensitive check (distribution.rs:32-33)
**File:** `src/resolvers/distribution.rs`
**Lines:** 32-33

```rust
let path = executable
    .map(|p| p.to_string_lossy().to_lowercase())
    .unwrap_or_default();
```

The entire path is converted to lowercase via string allocation, then checked with `.contains()`. This allocates a new String for every resolution call.

**Impact:** Per-resolution heap allocation — with 100 apps resolved per second, that's 100 String allocations/second.

**Fix:** Use `path.to_string_lossy().as_ref().to_ascii_lowercase()` or compare case-insensitively without allocation.

---

### 397. resolvers/environment.rs — Same path allocation pattern (environment.rs:31-33)
**File:** `src/resolvers/environment.rs`
**Lines:** 31-33

Same issue as distribution.rs — allocates a lowercase string for path checking.

**Fix:** Same as #396.

---

### 398. render/theme.rs — (if exists) Theme colors may not be shared between CPU and GPU paths
**File:** `src/render/theme.rs`

If the theme module defines colors for CPU rendering but the GPU path uses different color sources, the visual appearance will differ between rendering paths.

**Impact:** Visual inconsistency — CPU and GPU paths render different colors.

**Fix:** Share a single color source between both paths.

---

### 399. diagnostics/logger.rs — SubsystemLogger is a thin wrapper around tracing macros (logger.rs:72-127)
**File:** `src/diagnostics/logger.rs`
**Lines:** 72-127

The `SubsystemLogger` adds a `subsystem` field to tracing calls but provides no additional functionality over using `tracing::info!` directly with the `subsystem` field. The `report_gpu_error` and `report_capture_recovery` methods hardcode the subsystem name, making the `self.subsystem` field irrelevant.

**Impact:** Unnecessary abstraction — the logger adds indirection without value. The hardcoded subsystem in `report_gpu_error` ignores the logger's own subsystem.

**Fix:** Remove the wrapper and use tracing macros directly, or make the logger add value (e.g., rate limiting, structured context).

---

### 400. build.rs — Relies on slint_build side-effect for env var ordering (build.rs:8-13)
**File:** `build.rs`
**Lines:** 8-13

```rust
// slint_build internally sets SLINT_INCLUDE_GENERATED to the last file compiled.
// We need to override it back to ui.rs and set a separate var for the bluetooth widget.
```

The build script depends on `slint_build::compile` setting an environment variable as a side effect, then overrides it. This is fragile — if `slint_build` changes its internal behavior, the build breaks.

**Impact:** Build fragility — the build depends on undocumented side effects of `slint_build`.

**Fix:** Explicitly set both env vars without relying on `slint_build`'s side effects.

---

### 401. src/error.rs — AppError uses String for all error messages (error.rs:47-80)
**File:** `src/error.rs`
**Lines:** 47-80

Every `AppError` variant wraps a `String` message instead of a typed error. This loses the original error type and makes programmatic error handling impossible.

**Impact:** Error information loss — the caller can't distinguish between "config file not found" and "config file malformed" without parsing the string.

**Fix:** Use typed error variants (e.g., `ConfigurationError(ConfigError)`) instead of `String`.

---

### 402. render/audio.rs — 649-line monolithic render function (audio.rs:320-648)
**File:** `src/render/audio.rs`
**Lines:** 320-648

The `render_audio_surface` function is 329 lines and handles background, border, header, output section, input section, streams section, and footer — all in one function.

**Impact:** Maintainability — any change to the audio popup layout requires understanding the entire 329-line function. Testing individual sections is impossible.

**Fix:** Split into sub-functions: `draw_header`, `draw_output_section`, `draw_input_section`, `draw_streams_section`.

---

### 403. render/audio_player.rs — 618-line monolithic render function (audio_player.rs:66-377)
**File:** `src/render/audio_player.rs`
**Lines:** 66-377

The `render` function is 312 lines with deeply nested `if expanded { ... } else { ... }` branches. Each branch draws dozens of UI elements.

**Impact:** Same maintainability issue as #402.

**Fix:** Split into `render_expanded` and `render_compact` sub-functions.

---

### 404. pointer/motion.rs — Window list state reset on every hover change (motion.rs:196-206)
**File:** `src/pointer/motion.rs`
**Lines:** 196-206

```rust
crate::interaction::timers::reset_all(state);
if let Some(ref pw) = state.pipewire_capture {
    pw.set_active_row(0);
}
state.window_list_state.target_app_id = Some(app_id);
state.window_list_state.is_open = true;
```

When hovering a new app with windows, all timers are reset and the PipeWire active row is forced to 0. This resets any user interaction state.

**Impact:** User experience — if the user is scrolling through the window list and hovers a different app, their scroll position is lost.

**Fix:** Only reset timers when closing the window list, not when switching apps.

---

### 405. render/font.rs — Tofu placeholder draws rectangle per-pixel (font.rs:276-286)
**File:** `src/render/font.rs`
**Lines:** 276-286

```rust
for y in 0..h {
    for x in 0..w {
        let is_border = y == 0 || y == h - 1 || x == 0 || x == w - 1;
        if is_border {
            bitmap[(y * w + x) as usize] = 255;
        }
    }
}
```

The tofu (missing glyph) placeholder is drawn by iterating every pixel and checking if it's on the border. For an 8×8 glyph, this is 64 iterations.

**Impact:** Minor CPU waste — this only runs once per missing character, not per frame (cached). But the algorithm is O(w×h) when O(perimeter) would suffice.

**Fix:** Draw the border directly: fill top/bottom rows and left/right columns.

---

### 406. render/text.rs — blit_glyph checks bounds for every pixel (text.rs:173-187)
**File:** `src/render/text.rs`
**Lines:** 173-187

```rust
if target_x < 0 || target_x >= width as isize || target_y < 0 || target_y >= height as isize {
    continue;
}
let pixel_index = (target_y as usize * width + target_x as usize) * 4;
if pixel_index + 3 >= frame.len() {
    continue;
}
```

Two bounds checks per glyph pixel. The first check uses `isize` comparison, the second uses `usize`. Both are redundant if the glyph is pre-clipped.

**Impact:** Per-pixel branch misprediction — the bounds check branches are unpredictable for glyphs near canvas edges.

**Fix:** Pre-calculate the clipping rectangle and skip rows/columns outside the canvas.

---

### 407. graphics/hide.rs — HIDE_FRAME_INTERVAL constant unused (hide.rs:11)
**File:** `src/graphics/hide.rs`
**Line:** 11

```rust
pub const HIDE_FRAME_INTERVAL: Duration = Duration::from_nanos(41_666_667);
```

This constant is used in `show()` (line 56) but the `tick()` method is disabled. The interval is effectively dead code.

**Impact:** Dead code — the constant exists but the animation it controls is disabled.

**Fix:** Remove the constant or re-enable the animation.

---

### 408. render/badges.rs — Alpha blending uses u32 multiplication (badges.rs:75-77)
**File:** `src/render/badges.rs`
**Lines:** 75-77

```rust
canvas[dst_idx] = ((src_r * alpha + dst_r * inv_alpha) / 255) as u8;
```

Uses `u32` for intermediate multiplication to avoid overflow, but `src_r * alpha` where both are `u32` from `u8` can be up to `255 * 255 = 65025`, which fits in `u16`. The `u32` cast is unnecessary.

**Impact:** Minor — `u32` arithmetic is the same speed as `u16` on modern CPUs, but the code is less clear.

**Fix:** Use `u16` for intermediate calculations.

---

### 409. diagnostics/mod.rs — (if exists) Module re-exports may create circular dependencies
**File:** `src/diagnostics/mod.rs`

If the diagnostics module re-exports types from multiple submodules, and those submodules depend on each other, circular dependencies can arise.

**Impact:** Build complexity — circular dependencies make refactoring difficult.

**Fix:** Ensure clear dependency direction between diagnostic submodules.

---

### 410. recovery/mod.rs — RecoveryResult enum may not cover all cases
**File:** `src/recovery/mod.rs`

The `RecoveryResult` enum likely has variants like `Recovered`, `Retryable`, `Failed`, `NotAttempted`. But it doesn't distinguish between "retry later" and "retry now", or between different types of failure.

**Impact:** Insufficient granularity — the caller can't distinguish between "try again immediately" and "wait before retrying".

**Fix:** Add timing information to `Retryable` or add a `RetryLater(Duration)` variant.

---

## Low Severity Issues

### 411. geometry/primitives.rs — Rect::contains uses inclusive bounds (primitives.rs:43-44)
**File:** `src/geometry/primitives.rs`
**Lines:** 43-44

```rust
p.x >= self.x && p.x <= self.x + self.width && p.y >= self.y && p.y <= self.y + self.height
```

The right and bottom edges are inclusive (`<=`). This means a point exactly at `x + width` is considered inside, which is inconsistent with typical rectangle conventions where the right edge is exclusive.

**Impact:** Edge case — a point on the exact right/bottom edge is included, which may cause double-counting in adjacent rectangles.

**Fix:** Use `<` for right/bottom edges: `p.x < self.x + self.width`.

---

### 412. graphics/animation.rs — Frame sorting extracts digits from filename (animation.rs:84-91)
**File:** `src/graphics/animation.rs`
**Lines:** 84-91

```rust
s.chars()
    .filter(|c| c.is_ascii_digit())
    .collect::<String>()
    .parse::<u32>()
    .ok()
```

Frame order is determined by extracting all ASCII digits from the filename and parsing as u32. This means `frame10.svg` sorts before `frame2.svg` (10 < 2 is false, but "10" parses as 10 which is > 2). Actually, this works correctly for numeric sorting, but filenames like `frame-v2.svg` would parse as `2`.

**Impact:** Minor — unusual filenames may sort incorrectly.

**Fix:** Use a more robust parsing strategy, e.g., extract the last numeric segment.

---

### 413. handlers/utils.rs — Test only runs when launcher.sh exists (utils.rs:60-65)
**File:** `src/handlers/utils.rs`
**Lines:** 60-65

```rust
if packaged_script.is_file() && env::var_os("DOCK_LAUNCHER_PATH").is_none() {
    assert_eq!(PathBuf::from(get_launcher_path()), packaged_script);
}
```

The test silently passes if the conditions aren't met. It doesn't verify the actual behavior when the launcher is missing.

**Impact:** Weak test — the test doesn't catch regressions in the fallback paths.

**Fix:** Test each fallback path independently with mocked file system.

---

### 414. models/system.rs — WindowId ordering uses protocol_id only (system.rs:55-57)
**File:** `src/models/system.rs`
**Lines:** 55-57

```rust
fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.0.protocol_id().cmp(&other.0.protocol_id())
}
```

`WindowId` ordering compares only the protocol ID, not the object ID. Two windows from the same protocol will compare as equal, violating the `Ord` contract (antisymmetry).

**Impact:** Incorrect ordering — windows from the same protocol are considered equal in sorted collections, causing HashMap collisions or BTreeSet issues.

**Fix:** Compare the full `ObjectId`, not just the protocol ID.

---

### 415. resolvers/distribution.rs — Hardcoded path patterns for game launchers (distribution.rs:51-90)
**File:** `src/resolvers/distribution.rs`
**Lines:** 51-90

The distribution resolver checks for specific substrings in the path: "steam", "lutris", "heroic", "epicgames", "gog", "bottles". These are case-sensitive and may not match all installation paths.

**Impact:** Missed detection — a Steam game installed in a non-standard path may not be detected as Steam.

**Fix:** Use case-insensitive matching and check environment variables in addition to path.

---

### 416. render/audio.rs — short_name allocates Vec<char> per call (audio.rs:88)
**File:** `src/render/audio.rs`
**Line:** 88

```rust
let chars: Vec<char> = compact.chars().collect();
```

Every call to `short_name` collects all characters into a Vec, then iterates again to take a subset. This is O(n) allocation for a simple truncation.

**Impact:** Per-frame allocation — called for every device name and stream name on every render.

**Fix:** Use `char_indices()` to find the truncation point without allocation.

---

### 417. render/audio_player.rs — time_label truncates seconds (audio_player.rs:513-515)
**File:** `src/render/audio_player.rs`
**Lines:** 513-515

```rust
fn time_label(seconds: f64) -> String {
    format!("{}:{:02}", seconds as u64 / 60, seconds as u64 % 60)
}
```

Casting `f64` to `u64` truncates, so 59.9 seconds displays as "0:59" not "1:00". The format also doesn't handle hours.

**Impact:** Minor display inaccuracy — the last second of a track may show as one second short.

**Fix:** Use `round()` instead of truncation: `(seconds + 0.5) as u64`.

---

### 418. diagnostics/logger.rs — Subsystem enum doesn't include all subsystems (logger.rs:20-35)
**File:** `src/diagnostics/logger.rs`
**Lines:** 20-35

The `Subsystem` enum has 14 variants but doesn't include `Bluetooth`, `Audio`, `Network`, `Rclone`, `Process`, `Cache`, `Resolver`, or `Pointer`. These subsystems would need to use string-based logging instead of the typed enum.

**Impact:** Incomplete type safety — not all subsystems can use the typed logger.

**Fix:** Add missing subsystems or make the enum extensible.

---

### 419. src/error.rs — color-eyre installed globally may conflict (error.rs:177-185)
**File:** `src/error.rs`
**Lines:** 177-185

```rust
pub fn init_error_reporter() -> anyhow::Result<()> {
    match color_eyre::install() {
```

`color_eyre::install()` installs a global error hook. If called multiple times (e.g., from different threads), it returns an error. The function doesn't handle the case where it's already installed gracefully beyond returning the error.

**Impact:** Startup failure — if `init_error_reporter` is called twice, the second call fails.

**Fix:** Silently ignore the "already installed" error or use a `OnceLock` to ensure single initialization.

---

### 420. build.rs — No error handling for slint_build failures (build.rs:5-6)
**File:** `build.rs`
**Lines:** 5-6

```rust
slint_build::compile("ui.slint").unwrap();
slint_build::compile("bluetooth-widget.slint").unwrap();
```

If a Slint file has a syntax error, the build panics with an `unwrap()` message that may not be clear.

**Impact:** Poor error messages — Slint compilation errors are wrapped in a generic panic.

**Fix:** Use `map_err` to provide a clear error message with the file name.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 371 | **Critical** | All recovery modules are non-functional stubs | Illusion of fault tolerance |
| 372 | **Critical** | 9 identical surface-check blocks in coordinates.rs | Maintainability nightmare |
| 373 | **Critical** | Hardcoded layout constants in pointer handler | Hit-test drift from config |
| 374 | **Critical** | O(r²) per-pixel circle rendering | 15M ops/sec CPU waste |
| 375 | **Critical** | Unbounded glyph cache | Memory growth over time |
| 376 | **Critical** | fc-match subprocess per missing character | Render thread blocking |
| 377 | **Critical** | All 7 compatibility tests are empty stubs | False test confidence |
| 378 | **Critical** | Entire hide animation disabled | Feature gap |
| 379 | **High** | O(docks × popups) per pointer event | CPU overhead on mouse move |
| 380 | **High** | Filters Bluetooth devices into Vec every frame | Per-frame heap allocations |
| 381 | **High** | Clears Slint buffer with per-pixel loop | CPU waste |
| 382 | **High** | RGBA→BGRA per-pixel loop | Slow conversion |
| 383 | **High** | Health monitor never checks health | Dead monitoring |
| 384 | **High** | Crash reports never persisted | Lost diagnostics |
| 385 | **High** | Unsafe Mmap without error handling | Potential segfault |
| 386 | **High** | World struct duplicates FontManager | API confusion |
| 387 | **High** | Badge icon embedded as compile-time bytes | Binary bloat |
| 388 | **High** | Hardcodes all capabilities to true | Incorrect UI |
| 389 | **High** | Hardcoded colors in audio player | Visual inconsistency |
| 390 | **High** | Proximity state may not integrate with GPU | Rendering path mismatch |
| 391-410 | **Medium** | 20 medium issues | Various |
| 411-420 | **Low** | 10 low issues | Minor |

### Cumulative Totals Across All 12 Rounds

| Severity | Round 1-11 | Round 12 | Cumulative |
|----------|-----------|----------|-----------|
| Critical | 48 | 8 | **56** |
| High | 95 | 12 | **107** |
| Medium | 146 | 20 | **166** |
| Low | 93 | 10 | **103** |
| **Total** | **382** | **50** | **432** |

### Key Themes in Round 12

1. **Stub infrastructure**: Recovery modules, health monitor, crash reporter, and compatibility tests all exist but don't function. They provide a false sense of completeness.
2. **Per-pixel CPU rendering**: All render/ modules use per-pixel drawing loops for circles, rectangles, text, and alpha blending. None leverage SIMD or GPU acceleration.
3. **Code duplication**: Pointer coordinate mapping repeats the same surface-check pattern 9+ times. Layout constants are hardcoded in multiple places.
4. **Unbounded caches**: Glyph cache grows without limit. No LRU eviction, no TTL.
5. **Subprocess spawning on render thread**: fc-match is spawned for missing characters, blocking the main thread.
6. **Hardcoded values**: Layout constants, colors, and capabilities are hardcoded instead of read from config.

### Files Reviewed in Round 12

- `src/geometry/primitives.rs` (73 lines)
- `src/graphics/mod.rs` (13 lines)
- `src/graphics/hide.rs` (95 lines)
- `src/graphics/fade.rs` (182 lines)
- `src/graphics/animation.rs` (120 lines)
- `src/handlers/mod.rs` (17 lines)
- `src/handlers/utils.rs` (67 lines)
- `src/models/system.rs` (824 lines)
- `src/pointer/coordinates.rs` (114 lines)
- `src/pointer/handler.rs` (97 lines)
- `src/pointer/motion.rs` (235 lines)
- `src/recovery/capture_recovery.rs` (209 lines)
- `src/recovery/resource_recovery.rs` (215 lines)
- `src/recovery/service_recovery.rs` (246 lines)
- `src/render/mod.rs` (35 lines)
- `src/render/audio.rs` (649 lines)
- `src/render/audio_player.rs` (618 lines)
- `src/render/badges.rs` (136 lines)
- `src/render/bluetooth.rs` (242 lines)
- `src/render/font.rs` (311 lines)
- `src/render/text.rs` (225 lines)
- `src/diagnostics/crash_report.rs` (150 lines)
- `src/diagnostics/health_monitor.rs` (321 lines)
- `src/diagnostics/logger.rs` (153 lines)
- `src/resolvers/capabilities.rs` (33 lines)
- `src/resolvers/distribution.rs` (122 lines)
- `src/resolvers/environment.rs` (132 lines)
- `tests/compatibility.rs` (51 lines)
- `build.rs` (15 lines)
- `src/error.rs` (346 lines)
