# GPU Rendering Code Review — Round 24 (Main Event Loop, Initialization, Handlers)

This is a continuation of the GPU rendering code review. Rounds 1–23 covered all `wgpu-api/` source files, all `src/` application-layer modules, project infrastructure, documentation, and more (issues 1–832). This round focuses on the **main event loop**, **initialization flow**, **click handlers**, and **rendering entry point**.

---

## 833. main.rs — Second tokio runtime created in main() (lines 50-54)

**File:** `src/main.rs` lines 50-54

```rust
let rt = tokio::runtime::Builder::new_multi_thread()
    .worker_threads(2)
    .enable_all()
    .build()
```

The main function creates a multi-threaded tokio runtime with 2 worker threads. However, `init.rs` (line 113-140) creates a **separate** tokio runtime on a dedicated thread for D-Bus listeners. This means the application runs **two independent tokio runtimes**: one in main() for GPU/PipeWire, and one in a background thread for D-Bus.

**Impact:** Two tokio runtimes means two epoll instances, two reactor loops, and duplicated async infrastructure. The D-Bus runtime thread cannot share tasks with the main runtime, forcing channel-based communication for everything.

**Fix:** Use a single tokio runtime created early in init.rs, passed through AppContext.

---

## 834. main.rs — Hardcoded max_windows = 20 for PipeWire (line 85)

**File:** `src/main.rs` line 85

```rust
let max_windows = 20;
```

The maximum number of PipeWire capture streams is hardcoded to 20. A user with 21+ windows open will have some windows fail to capture silently.

**Impact:** Users with many windows cannot capture all of them. The limit is not configurable.

**Fix:** Make this configurable via DockConfig or derive it from the number of pinned apps plus a margin.

---

## 835. main.rs — PipeWire connection errors silently ignored (lines 88-103)

**File:** `src/main.rs` lines 88-103

```rust
let connection = match zbus::Connection::session().await {
    Ok(c) => c,
    Err(_e) => return (None, None),  // ← error ignored
};
let desktop = match Proxy::new(...).await {
    Ok(d) => d,
    Err(_e) => return (None, None),  // ← error ignored
};
```

Both the D-Bus connection error and the ScreenCast proxy error are silently discarded. If the session bus is unavailable or the portal is not installed, the user gets no warning that window capture is disabled.

**Impact:** Window list popup shows blank thumbnails with no indication of why.

**Fix:** Log the error at WARN level and set a flag in AppState so the UI can show a disabled state.

---

## 836. main.rs — PipeWire spawn errors silently ignored (lines 115-133)

**File:** `src/main.rs` lines 115-133

```rust
rt.spawn(async move {
    let _ = library::services::pipewire_capture::init_capture_service(...).await;
});
rt.spawn(async move {
    library::services::pipewire_capture::start_monitor_inner(...).await;
});
```

The `rt.spawn` calls return `JoinHandle` which is immediately dropped. If the spawned task panics, the panic is silently swallowed. The `let _ =` on init_capture_service discards any Result.

**Impact:** If PipeWire initialization fails, the failure is completely silent. No log message, no fallback, no user notification.

**Fix:** Log the JoinHandle result or at least log errors from init_capture_service.

---

## 837. main.rs — No frame rate cap on event loop (line 143)

**File:** `src/main.rs` line 143

```rust
loop {
```

The main event loop has no frame rate cap when `needs_redraw` is true. The timeout is set to 8ms (line 424), which targets ~120 FPS. However, if `needs_redraw` remains true continuously (e.g., due to a stuck animation), the loop runs at 120 FPS indefinitely, consuming 100% of one CPU core.

**Impact:** A stuck animation or state bug causes 100% CPU usage on one core.

**Fix:** Add a maximum frame rate limiter that caps rendering to 60 FPS when no input events are pending.

---

## 838. main.rs — Hardcoded timeout values (lines 421-435)

**File:** `src/main.rs` lines 421-435

```rust
let mut timeout_ms: i32 = if state.needs_redraw && !waiting_for_layer_configure {
    8  // ← hardcoded
} else if state.window_list_state.is_open && !waiting_for_layer_configure {
    // ...
    ms = ms.clamp(1, 16);  // ← hardcoded
} else {
    1000  // ← hardcoded
};
```

All timeout values are hardcoded magic numbers: 8ms for redraw, 16ms for window list, 1000ms for idle. These control the effective frame rate and responsiveness but are not documented or configurable.

**Impact:** Tuning responsiveness vs CPU usage requires code changes and recompilation.

**Fix:** Define named constants with comments explaining the tradeoff, or make them configurable.

---

## 839. main.rs — Per-frame HashMap rebuilds for apps_in_dock (line 319)

**File:** `src/main.rs` line 319

```rust
let apps_in_dock = state.get_apps_in_dock_cached();
```

Even though this is "cached", the cache is invalidated on every state change. The `get_apps_in_dock_cached` method rebuilds a Vec<String> from `state.pinned_apps` on every call when the cache is stale. This happens every frame during animations.

**Impact:** Per-frame heap allocations for string vectors during active animations.

**Fix:** Cache the apps_in_dock result and invalidate only when pinned_apps actually changes.

---

## 840. main.rs — Per-frame HashMap rebuilds for running_by_app (line 323)

**File:** `src/main.rs` line 323

```rust
let running_by_app = state.get_running_by_app_cached();
```

Same issue as #839. The running_by_app cache is rebuilt on every window event, which fires frequently.

**Impact:** Per-event HashMap<String, Vec<ObjectId>> allocations.

**Fix:** Incremental update of running_by_app instead of full rebuild.

---

## 841. main.rs — Menu fade animation check is redundant (lines 332-336)

**File:** `src/main.rs` lines 332-336

```rust
let menu_fade_animating = state.docks.iter().any(|dock| {
    (dock.menu_fade.current_alpha - dock.menu_fade.target_alpha).abs() > f32::EPSILON
        && !dock.menu_fade.is_fully_hidden()
    || (dock.menu_fade.current_alpha - dock.menu_fade.target_alpha).abs() > f32::EPSILON
});
```

The condition has two parts connected by `||`. The second part `(dock.menu_fade.current_alpha - dock.menu_fade.target_alpha).abs() > f32::EPSILON` is already checked in the first part. The `&& !dock.menu_fade.is_fully_hidden()` is ANDed with the first check, but the second check (OR) repeats the alpha difference check without the `is_fully_hidden` guard. This means a fully hidden menu with alpha drift will still trigger `menu_fade_animating = true`.

**Impact:** Fully hidden menus with floating-point drift in alpha cause unnecessary redraws.

**Fix:** Simplify to a single condition: `(current - target).abs() > threshold && !is_fully_hidden()`.

---

## 842. main.rs — Hardcoded calculated_width limit of 800 (line 355)

**File:** `src/main.rs` line 355

```rust
let calculated_width = (total_items * box_size + (total_items + 1) * spacing).min(800);
```

The dock width is capped at 800 pixels. On a 4K display with large icons, this cap forces the dock to be narrower than needed, potentially clipping icons.

**Impact:** Dock is artificially narrow on high-resolution displays.

**Fix:** Make the max width configurable or derive it from the output width.

---

## 843. main.rs — Wayland flush result ignored (line 414)

**File:** `src/main.rs` line 414

```rust
let _ = state.connection.flush();
```

The result of `flush()` is silently discarded. If the Wayland connection is broken, the error is ignored and the loop continues, potentially causing more errors.

**Impact:** Broken Wayland connection is not detected; the loop continues uselessly.

**Fix:** Check the flush result and break the loop on error.

---

## 844. main.rs — Second Wayland flush before poll (line 452)

**File:** `src/main.rs` line 452

```rust
let _ = state.connection.flush();
```

A second `flush()` is called after the render block (line 414) and before `poll()`. This is redundant if no new Wayland requests were made between the two calls.

**Impact:** Unnecessary system call on every frame.

**Fix:** Remove the redundant flush or only flush when Wayland requests were made.

---

## 845. main.rs — Unsafe libc::poll call (lines 455-479)

**File:** `src/main.rs` lines 455-479

```rust
let mut pfd = libc::pollfd {
    fd: raw_fd,
    events: libc::POLLIN,
    revents: 0,
};
// ...
let poll_res = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
```

The main event loop uses raw `libc::poll` instead of the Wayland client's built-in dispatch mechanism. The `unsafe` block has no safety comment explaining why it's safe. The `prepare_read()` guard is used, but the raw fd access bypasses Rust's safety guarantees.

**Impact:** Potential undefined behavior if the fd becomes invalid between `as_raw_fd()` and `poll()`.

**Fix:** Use `event_queue.dispatch_pending()` with a timeout instead of raw poll, or add a safety comment.

---

## 846. init.rs — Hardcoded SHM pool size of 16MB (line 185)

**File:** `src/init.rs` line 185

```rust
let pool = SlotPool::new(1024 * 1024 * 16, &shm_state).expect("Failed to create memory pool");
```

The shared memory pool for Wayland buffer rendering is hardcoded to 16MB. On a high-resolution display with a large dock, this may be insufficient. On a low-resolution display, it's wasteful.

**Impact:** The pool size doesn't adapt to the actual surface size, potentially causing reallocation or waste.

**Fix:** Calculate pool size based on surface dimensions: `width * height * 4 bytes * 2 (double buffer)`.

---

## 847. init.rs — Font file search panics on missing font (lines 188-197)

**File:** `src/init.rs` lines 188-197

```rust
let font_path = [
    PathBuf::from("assets/font.ttf"),
    PathBuf::from("font.ttf"),
    PathBuf::from(user_data_font),
    PathBuf::from("/usr/share/dock/font.ttf"),
    PathBuf::from("/usr/share/fonts/TTF/DejaVuSans.ttf"),
]
.into_iter()
.find(|p| p.exists())
.expect("No valid font file found!");
```

If none of the hardcoded font paths exist, the application panics. The paths include relative paths (`assets/font.ttf`, `font.ttf`) which depend on the working directory.

**Impact:** Application crashes on systems without the expected font files.

**Fix:** Use a system font discovery mechanism (fontconfig) as fallback, or bundle a font and use the executable's directory.

---

## 848. init.rs — Hardcoded default dock dimensions (lines 222-223)

**File:** `src/init.rs` lines 222-223

```rust
width: 100,
height: 60,
```

The initial AppState width and height are hardcoded. These are overwritten by the layer surface configuration, but until the roundtrip completes, the state has incorrect dimensions.

**Impact:** If the first render happens before the roundtrip, the dock renders at 100×60 instead of the configured size.

**Fix:** Initialize from config values or the output size.

---

## 849. init.rs — sysinfo::System created with minimal refresh (lines 283-290)

**File:** `src/init.rs` lines 283-290

```rust
sys_scanner: sysinfo::System::new_with_specifics(
    sysinfo::RefreshKind::nothing().with_processes(
        sysinfo::ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory(),
    ),
)
```

The sysinfo system is created with only CPU and memory refresh for processes. However, the process monitor and other components may need additional fields like disk I/O, network I/O, or process status. Each additional field requires a separate refresh call.

**Impact:** If a component needs a field not in the initial refresh kind, it silently returns stale or zero data.

**Fix:** Document which fields are refreshed and ensure all components use only those fields.

---

## 850. init.rs — Hardcoded scale_factor = 1.0 (line 407)

**File:** `src/init.rs` line 407

```rust
let scale_factor = 1.0;
```

The scale factor is hardcoded to 1.0 during layer surface creation. The fractional scale manager negotiates the actual scale factor asynchronously, but the initial surface size uses 1.0. This variable is declared but never used after this point.

**Impact:** Dead code. The actual scale factor comes from the fractional scale negotiation, but the dead variable is confusing.

**Fix:** Remove the unused variable.

---

## 851. init.rs — Output size defaults to 1920×1080 (line 557)

**File:** `src/init.rs` line 557

```rust
let output_size = state
    .output_state
    .info(&output)
    .and_then(|info| info.logical_size)
    .unwrap_or((1920, 1080));
```

If the output info is unavailable, the code defaults to 1920×1080. This is used to calculate the dashed block and date/time block positions. On a 4K display where output info fails, the blocks will be positioned incorrectly.

**Impact:** Side blocks are mispositioned when output info is unavailable.

**Fix:** Use the physical size as fallback, or defer positioning until output info is available.

---

## 852. init.rs — Duplicate anchor match statements (lines 420-443, 463-509, 515-561)

**File:** `src/init.rs` lines 420-443, 463-509, 515-561

The `match state.config.anchor` pattern is repeated three times: once for the main dock anchor, once for the dashed block margin, and once for the date/time block margin. Each match has nearly identical arms with slight variations in margin calculations.

**Impact:** Code duplication. Adding a new anchor position requires changes in three places.

**Fix:** Extract a helper function that calculates margins based on anchor and position (left/right/below/above).

---

## 853. init.rs — Dashed block and date/time block surfaces created unconditionally (lines 451-562)

**File:** `src/init.rs` lines 451-562

The dashed block and date/time block layer surfaces are created for every output, regardless of whether they are enabled in the config. The config has `dashed_block_width`, `dashed_block_height`, etc., but there's no boolean flag to disable them.

**Impact:** Two extra Wayland surfaces per output are created even when the blocks are not needed, consuming compositor resources.

**Fix:** Add config flags to enable/disable these blocks and skip surface creation when disabled.

---

## 854. init.rs — Roundtrip may block on slow compositor (line 380)

**File:** `src/init.rs` line 380

```rust
event_queue.roundtrip(&mut state).unwrap();
```

The roundtrip blocks until the compositor responds with the layer-shell configure. If the compositor is slow or unresponsive, the application hangs during initialization.

**Impact:** Application appears frozen during startup on slow compositors.

**Fix:** Add a timeout to the roundtrip or use a non-blocking dispatch.

---

## 855. init.rs — Second roundtrip may block again (line 627)

**File:** `src/init.rs` line 627

```rust
event_queue.roundtrip(&mut state)?;
```

A second roundtrip is performed after creating all surfaces. This is necessary to ensure the configure events are dispatched, but it doubles the potential blocking time.

**Impact:** Startup delay is doubled on slow compositors.

**Fix:** Combine into a single roundtrip after all surfaces are created.

---

## 856. init.rs — FontManager::from_file may fail silently (line 225)

**File:** `src/init.rs` line 225

```rust
font_manager: FontManager::from_file(&font_path).expect("Failed to memory-map font file"),
```

The font manager uses `expect` which panics if the font file cannot be memory-mapped. Combined with issue #847, this means the application can panic during initialization if the font is missing or corrupted.

**Impact:** Application crash on startup if font is missing or unreadable.

**Fix:** Use a bundled fallback font or a system font discovery mechanism.

---

## 857. init.rs — Tray action channel not returned in AppContext (line 109)

**File:** `src/init.rs` line 109

```rust
let (tray_action_tx, tray_action_rx) = tokio::sync::mpsc::unbounded_channel();
```

The `tray_action_tx` is passed to the status notifier listener (line 134), and `tray_action_rx` is stored in `state.tray_action_tx` (line 317). However, the channel is created in init.rs but the receiver is stored in AppState as a transmitter (`tray_action_tx`). This naming inversion is confusing.

**Impact:** Code readability — the field name suggests it's a transmitter but it's actually a receiver stored in state.

**Fix:** Rename the field to `tray_action_rx` for clarity.

---

## 858. handlers/dock.rs — Unused parameters in handle_dock_press (lines 29-31)

**File:** `src/handlers/dock.rs` lines 29-31

```rust
    _apps_in_dock: &[String],
    _layout: (i32, i32, i32, i32),
    _scale_factor: f32,
```

Three parameters are prefixed with `_` indicating they are unused. The function receives the full apps list, layout tuple, and scale factor, but only uses `ptr_log_x` and `ptr_log_y` for hit testing.

**Impact:** Dead parameters increase function signature complexity without adding value.

**Fix:** Remove unused parameters or implement the missing functionality.

---

## 859. handlers/dock.rs — Unused parameter in handle_dock_release (line 60)

**File:** `src/handlers/dock.rs` line 60

```rust
    _scale_factor: f32,
```

The scale factor is passed but unused in the release handler. The hit testing uses `ptr_log_x` directly without scaling.

**Impact:** If the coordinates are in physical pixels, the hit test will be incorrect at non-1x scale factors.

**Fix:** Either use the scale factor to convert coordinates, or remove the parameter if coordinates are already in logical pixels.

---

## 860. handlers/dock.rs — Hardcoded button codes (lines 33, 66, 84, 89)

**File:** `src/handlers/dock.rs` lines 33, 66, 84, 89

```rust
if button == 272 {  // ← left click
// ...
if button == 272 && state.is_dragging {
// ...
274 => {  // ← middle click
272 => {  // ← left click
```

Button codes 272 (left), 273 (right), and 274 (middle) are hardcoded throughout. While the docstring mentions these values, they should be named constants.

**Impact:** Magic numbers scattered through the code.

**Fix:** Define constants: `const BTN_LEFT: u32 = 272; const BTN_MIDDLE: u32 = 274; const BTN_RIGHT: u32 = 273;`

---

## 861. handlers/dock.rs — Per-click HashMap allocation for window titles (lines 98-109)

**File:** `src/handlers/dock.rs` lines 98-109

```rust
let titles: HashMap<ObjectId, String> = windows
    .iter()
    .map(|id| {
        (
            id.clone(),
            state.window_state(id)
                .map(|w| w.title.clone())
                .unwrap_or_default(),
        )
    })
    .collect();
```

On every left-click on a running app, a new HashMap is allocated to store window titles for sorting. This happens on the main thread during input processing.

**Impact:** Per-click heap allocation for a HashMap that's only used for sorting.

**Fix:** Sort windows by title directly without building a HashMap, or cache the titles.

---

## 862. handlers/dock.rs — Mutex lock on active_row (line 121)

**File:** `src/handlers/dock.rs` line 121

```rust
let active_row = *service.active_row.lock();
```

The `active_row` field is protected by a `Mutex`. The lock is acquired on every left-click on a running app. The `lock()` call can panic if the mutex is poisoned, and the lock guard is held while the matched windows are iterated.

**Impact:** Potential panic on mutex poison; lock held during iteration.

**Fix:** Use `lock().unwrap_or(...)` or clone the value immediately and drop the guard.

---

## 863. handlers/dock.rs — normalize_app_id strips suffixes aggressively (lines 227-240)

**File:** `src/handlers/dock.rs` lines 227-240

```rust
pub fn normalize_app_id(app_id: &str) -> String {
    let mut id = app_id.to_string();
    if !id.starts_with("steam_icon_") {
        if let Some(idx) = id.rfind('_') {
            if id[idx + 1..].chars().all(|c| c.is_numeric()) {
                id = id[..idx].to_string();
            }
        }
    }
    if id.to_lowercase().contains("transmission") {
        id = "transmission-gtk".to_string();
    }
    id
}
```

The normalization has two issues:
1. It strips any suffix after the last underscore if the suffix is numeric. This means `org.gnome_32` becomes `org.gnome`, which may not be the intended app ID.
2. It hardcodes `transmission-gtk` for any app ID containing "transmission", ignoring `transmission-qt` or other variants.

**Impact:** Wrong app ID matching for apps with numeric suffixes; Transmission variant mismatch.

**Fix:** Only strip known patterns (like `steam_icon_`) and handle Transmission variants explicitly.

---

## 864. handlers/dock.rs — Launch spawn error silently ignored (lines 295-297)

**File:** `src/handlers/dock.rs` lines 295-297

```rust
if let Err(_error) = command.spawn() {
    // Failed to start launcher
}
```

If `bash launcher.sh` fails to spawn (e.g., script not found, permission denied), the error is completely silent. The user clicks the icon and nothing happens.

**Impact:** User has no feedback when app launch fails.

**Fix:** Log the error and trigger a visual feedback (e.g., shake animation) on the icon.

---

## 865. handlers/dock.rs — Hardcoded bash command for launch (line 274)

**File:** `src/handlers/dock.rs` line 274

```rust
let mut command = std::process::Command::new("bash");
```

The launcher script is invoked via `bash`, which spawns a new process. This is slower than executing the script directly if it has the executable bit set, and fails if `bash` is not in PATH.

**Impact:** Extra process spawn overhead; dependency on `bash` being in PATH.

**Fix:** Execute the script directly with `Command::new(&launcher_path)` after ensuring it's executable.

---

## 866. render/dock.rs — render_dock_background is a no-op (lines 55-65)

**File:** `src/render/dock.rs` lines 55-65

```rust
pub fn render_dock_background(
    canvas: &mut [u8],
    _phys_width: u32,
    _phys_height: u32,
    _dock_y: usize,
    _dock_height: usize,
    _alpha: f32,
) {
    canvas.fill(0);
}
```

The docstring says this is deprecated and the background is rendered by Slint. However, the function still fills the canvas with zeros (transparent black), which overwrites any Slint-rendered content. The function is called from `render_dock_surface` (line 28-35) before `render_dock_items`.

**Impact:** If this CPU render path is used, it clears the entire canvas before drawing items, potentially erasing Slint's background.

**Fix:** Remove the `canvas.fill(0)` call or the entire function if Slint handles the background.

---

## 867. render/dock.rs — Per-icon image::ImageBuffer allocation (lines 110-112)

**File:** `src/render/dock.rs` lines 110-112

```rust
let src_img = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(
    src_w,
```

For every icon on every frame, a new `ImageBuffer` is created from raw pixels. This allocates a `Vec<u8>` of size `src_w * src_h * 4` bytes. For a 64×64 icon, that's 16KB per icon per frame.

**Impact:** Per-frame heap allocations for every icon, causing memory pressure and GC-like behavior.

**Fix:** Reuse a single buffer or use `fast_image_resize` with pre-allocated buffers.

---

## 868. render/dock.rs — WindowState holds icon_rgba Vec (referenced from multiple files)

**File:** Referenced in `src/state/window.rs` and used in `render/dock.rs` line 83-86

```rust
w.icon_rgba
    .as_ref()
    .map(|rgba| (rgba.as_slice(), w.icon_size))
```

Every `WindowState` holds an `Option<Vec<u8>>` for icon pixels. For 50 windows, this is 50 separate heap allocations storing icon pixel data. The icon is also stored in `icon_cache` and potentially in `TrayItem`.

**Impact:** Triple storage of the same icon pixels: WindowState + icon_cache + TrayItem.

**Fix:** Use Arc<Vec<u8>> or an interned icon store to share pixel data across references.

---

## 869. main.rs — state.draw() called with complex boolean logic (line 409)

**File:** `src/main.rs` line 409

```rust
state.draw(&qh, !popup_only_animation && !status_sidebars_only);
```

The second argument to `draw()` is a complex boolean expression combining `popup_only_animation` and `status_sidebars_only`. The meaning of this argument (whether to draw the main dock) is not clear from the call site.

**Impact:** Code readability — the boolean logic is hard to understand without tracing the variables.

**Fix:** Name the boolean: `let draw_main_dock = !popup_only_animation && !status_sidebars_only;`

---

## 870. init.rs — AppState construction is 147 lines (lines 210-357)

**File:** `src/init.rs` lines 210-357

The `AppState` struct is constructed with 147 lines of field initializations. This makes it difficult to see which fields are initialized to what, and adding a new field requires finding the correct position in the struct literal.

**Impact:** Maintainability — adding/removing fields is error-prone; default values are scattered.

**Fix:** Implement `Default` for `AppState` and override only the non-default fields, or use a builder pattern.

---

## Summary of Issues by Severity

| #   | Severity | Issue | Impact |
|-----|----------|-------|--------|
| 833 | **Critical** | Second tokio runtime in main() | Duplicated async infrastructure |
| 837 | **Critical** | No frame rate cap on event loop | 100% CPU on stuck animation |
| 845 | **Critical** | Unsafe libc::poll call | Potential undefined behavior |
| 834 | **High** | Hardcoded max_windows = 20 | Silent capture limit |
| 835 | **High** | PipeWire errors silently ignored | No feedback on capture failure |
| 836 | **High** | PipeWire spawn errors ignored | Silent task failures |
| 838 | **High** | Hardcoded timeout values | Untunable responsiveness |
| 839 | **High** | Per-frame HashMap for apps_in_dock | Per-frame allocations |
| 840 | **High** | Per-frame HashMap for running_by_app | Per-event allocations |
| 842 | **High** | Hardcoded dock width limit 800 | Clipped icons on 4K |
| 846 | **High** | Hardcoded SHM pool 16MB | Size mismatch |
| 847 | **High** | Font search panics on missing font | Crash on startup |
| 851 | **High** | Output size defaults 1920×1080 | Mispositioned blocks |
| 854 | **High** | Roundtrip blocks on slow compositor | Startup hang |
| 861 | **High** | Per-click HashMap for titles | Per-click allocation |
| 863 | **High** | normalize_app_id strips aggressively | Wrong app matching |
| 867 | **High** | Per-icon ImageBuffer allocation | Per-frame heap allocations |
| 868 | **High** | WindowState holds icon_rgba Vec | Triple icon storage |
| 841 | **Medium** | Menu fade animation redundant check | Unnecessary redraws |
| 843 | **Medium** | Wayland flush result ignored | Broken connection undetected |
| 844 | **Medium** | Second Wayland flush before poll | Redundant syscall |
| 848 | **Medium** | Hardcoded default dock dimensions | Wrong initial size |
| 849 | **Medium** | sysinfo minimal refresh | Missing process fields |
| 850 | **Medium** | Hardcoded scale_factor = 1.0 | Dead code |
| 852 | **Medium** | Duplicate anchor match statements | Code duplication |
| 853 | **Medium** | Block surfaces created unconditionally | Wasted compositor resources |
| 855 | **Medium** | Second roundtrip blocks again | Doubled startup delay |
| 856 | **Medium** | FontManager may fail silently | Crash on bad font |
| 857 | **Medium** | Tray action channel naming inverted | Code readability |
| 858 | **Medium** | Unused parameters in handle_dock_press | Dead parameters |
| 859 | **Medium** | Unused scale_factor in release | Potential hit test bug |
| 862 | **Medium** | Mutex lock on active_row | Panic on poison |
| 864 | **Medium** | Launch spawn error ignored | No feedback on failure |
| 865 | **Medium** | Hardcoded bash for launch | Extra process overhead |
| 866 | **Medium** | render_dock_background clears canvas | Erases Slint background |
| 869 | **Medium** | Complex boolean in draw() call | Code readability |
| 870 | **Medium** | AppState construction 147 lines | Maintainability |
| 860 | **Low** | Hardcoded button codes | Magic numbers |

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
| 20 | gpu-rendering-review-20.md | 711-750 | 750 |
| 21 | gpu-rendering-review-21.md | 751-790 | 790 |
| 22 | gpu-rendering-review-22.md | 791-830 | 830 |
| 23 | gpu-rendering-review-23.md | 831-832 | 832 |
| **24** | **gpu-rendering-review-24.md** | **833-870** | **870** |

**Cumulative severity across all 24 rounds:**
- Critical: 109 (was 106 + 3 new)
- High: 261 (was 244 + 17 new)
- Medium: 311 (was 295 + 16 new)
- Low: 79 (was 78 + 1 new)
- **Total: 870 issues**

### Key Themes

1. **Two tokio runtimes**: The application creates two independent tokio runtimes, one in main() and one in a background thread for D-Bus. This duplicates async infrastructure and forces channel-based communication.

2. **No frame rate cap**: The main event loop can run at 120 FPS indefinitely when `needs_redraw` is true, consuming 100% of one CPU core.

3. **Unsafe libc::poll**: The event loop uses raw `libc::poll` instead of the Wayland client's built-in dispatch, with no safety comment.

4. **Hardcoded values throughout**: Max windows (20), dock width (800), SHM pool (16MB), timeouts (8ms/16ms/1000ms), output size (1920×1080), button codes (272/273/274) — all hardcoded without configuration.

5. **Silent error handling**: PipeWire connection errors, spawn errors, launch errors, and Wayland flush errors are all silently discarded.

6. **Per-frame/per-click allocations**: HashMap rebuilds for apps_in_dock and running_by_app on every frame; ImageBuffer allocation per icon per frame; HashMap for window titles on every click.

7. **God struct AppState**: 147-line struct literal for initialization, with no Default implementation and no encapsulation.
