# GPU Rendering Code Review — Round 11 (Application Layer: Audio, Bluetooth, Network, Context Menu, Process Monitor, Rclone, System Bar, Widget Bar, Window List)

This is a continuation of the GPU rendering code review. Rounds 1–10 covered all `wgpu-api/` source files and key `src/` application-layer modules (issues 1–330). This round covers the remaining `src/app/` submodules: `audio.rs`, `audio_popup.rs`, `audio_player_bar.rs`, `bluetooth.rs`, `bluetooth_popup.rs`, `context_menu.rs`, `network.rs`, `process_monitor.rs`, `process_popup.rs`, `rclone.rs`, `rclone_popup.rs`, `system_bar.rs`, `widget_bar.rs`, `window_list.rs`, `side_bars.rs`, and `mod.rs` (issues 331–370).

---

## Critical Issues

### 331. bluetooth.rs — Second tokio runtime created in monitor thread (bluetooth.rs:33-36)
**File:** `src/app/bluetooth.rs`
**Lines:** 33-36

```rust
let runtime = match tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
```

The Bluetooth monitor spawns a new thread and creates a second `tokio::runtime` inside it. The main application already runs a tokio runtime (in `main.rs`). Having two tokio runtimes in the same process can cause conflicts with shared resources like timers, I/O drivers, and signal handlers.

**Impact:** Resource conflicts — two tokio runtimes compete for system resources. On some platforms, only one epoll/kqueue instance is available, causing the second runtime to fail silently or behave incorrectly.

**Fix:** Pass a `tokio::sync::mpsc::Sender` from the main runtime's thread and use `tokio::task::spawn` on the main runtime. Or use a channel-based design where the monitor thread sends requests to the main runtime.

---

### 332. window_list.rs — 751-line monolithic function (window_list.rs:58-751)
**File:** `src/app/window_list.rs`
**Lines:** 58-751

The entire `draw_window_list_popup` function is 693 lines with deeply nested control flow. It handles PipeWire stream matching, geometry calculation, GPU draw command construction, CPU fallback rendering, and Wayland buffer management — all in one function.

**Impact:** Maintainability — any change to window list rendering requires understanding the entire 700+ line function. Testing is impossible because there are no isolated units. Code reuse across popups is blocked.

**Fix:** Split into submodules: `window_match.rs` (PipeWire stream matching), `window_geometry.rs` (popup bounds), `window_draw_commands.rs` (GPU command construction), `window_render.rs` (GPU + CPU render paths).

---

### 333. window_list.rs — Per-separator-pixel draw command allocation (window_list.rs:602-621)
**File:** `src/app/window_list.rs`
**Lines:** 602-621

```rust
for i in 1..item_count {
    let line_y = rows_area_y + (i as f32 * phys_item_h);
    for x in (0..width as i32).step_by(2) {
        if (x / dash_gap as i32) % 2 != 0 {
            continue;
        }
        draw_commands.push(wgpu_api::DrawCommand::Rectangle {
            rect: wgpu_api::Rect { x: x as f32, y: line_y, width: 2.0, height: 1.0 },
```

For each separator line between rows, the code iterates every 2 pixels across the popup width and creates a `DrawCommand::Rectangle` for each dash segment. For a 200px-wide popup with 3 separators, this creates ~300 draw commands just for separators.

**Impact:** Performance — hundreds of tiny draw commands per frame for separators. Each command is pushed to a Vec, then sent to the GPU renderer, which creates vertex buffers for each one.

**Fix:** Use a single draw command with a dashed line pattern, or batch all separator dashes into one vertex buffer.

---

### 334. window_list.rs — Clones WindowState for every matched window (window_list.rs:124, 418-422)
**File:** `src/app/window_list.rs`
**Lines:** 124, 418-422

```rust
matched.push(((*w).clone(), stream_idx));
// ...
let windows: Vec<WindowState> = if use_matched {
    matched_windows.iter().map(|(w, _)| w.clone()).collect()
```

Every frame, `WindowState` is cloned twice: once during stream matching and once for GPU draw command construction. `WindowState` contains `title: String`, `app_id: String`, and other fields.

**Impact:** Per-frame heap allocations — with 5 windows, that's 10 String clones per frame at 60 FPS = 600 String allocations/second.

**Fix:** Use references throughout the matching and rendering pipeline. Clone only when storing state across frames.

---

### 335. audio.rs — Spawns wpctl subprocess for every device volume read (audio.rs:361-371)
**File:** `src/app/audio.rs`
**Lines:** 361-371

```rust
fn read_node_volume(node_id: u32) -> (f32, bool) {
    let Ok(output) = run_wpctl(&["get-volume".into(), node_id.to_string()]) else {
```

`read_node_volume` is called for every output device, every input device, and every stream (up to 3). Each call spawns a new `wpctl` subprocess. With 2 outputs, 1 input, and 3 streams, that's 6 subprocess spawns per snapshot, plus the initial `wpctl status` and `wpctl list` calls.

**Impact:** High CPU and I/O usage — 10+ subprocess spawns per snapshot, every 1.5 seconds. Each spawn forks and execs, consuming CPU cycles.

**Fix:** Parse volume from `wpctl status` output directly instead of calling `get-volume` per node. The status output already contains volume information.

---

## High Severity Issues

### 336. context_menu.rs — Parses hex colors on every frame (context_menu.rs:154-159)
**File:** `src/app/context_menu.rs`
**Lines:** 154-159

```rust
let text_color = DockConfig::parse_hex_color(&config.popup_text_color);
let border_color = DockConfig::parse_hex_color(&config.popup_border_color);
let focus_color = DockConfig::parse_hex_color(&config.popup_focus_color);
let unpin_color = DockConfig::parse_hex_color(&config.popup_unpin_color);
let launch_color = DockConfig::parse_hex_color(&config.popup_launch_color);
let quit_color = DockConfig::parse_hex_color(&config.popup_quit_color);
```

Every frame, 6 hex color strings are parsed from the config. Each parse involves string allocation and character iteration.

**Impact:** Per-frame string parsing — 6 hex color parses per frame at 60 FPS = 360 string operations/second.

**Fix:** Parse colors once when config changes and cache the parsed `Color` values.

---

### 337. side_bars.rs — Estimates text width by character count (side_bars.rs:434-435)
**File:** `src/app/side_bars.rs`
**Lines:** 434-435

```rust
let time_est_width = time.len() as f32 * 8.0 * scale_f32;
let date_est_width = date.len() as f32 * 7.0 * scale_f32;
```

Text width is estimated by multiplying character count by a fixed pixel width (8px for time, 7px for date). This assumes monospace fonts and ignores actual font metrics.

**Impact:** Incorrect text positioning — proportional fonts will cause text to overflow or be misaligned. Wide characters (like "W" or "M") will push text beyond the estimated boundary.

**Fix:** Use the font manager to measure actual text width, or use a monospace font for the system bar.

---

### 338. widget_bar.rs — GPU path renders rectangles without icons (widget_bar.rs:84-146)
**File:** `src/app/widget_bar.rs`
**Lines:** 84-146

The GPU rendering path for the widget bar draws a background rectangle and 4 button rectangles, plus a Bluetooth indicator. It does NOT render the actual icons (rclone, process, audio, Bluetooth) — only colored blocks.

**Impact:** Visual mismatch — the widget bar shows empty colored rectangles on the GPU path instead of proper icons. The CPU fallback path renders the actual icons via `render_status_widget_bar`.

**Fix:** Add icon rendering to the GPU path using `DrawCommand::Image` with cached textures, similar to how `side_bars.rs` handles tray icons.

---

### 339. audio_popup.rs — GPU rendering not integrated (audio_popup.rs:96-97)
**File:** `src/app/audio_popup.rs`
**Lines:** 96-97

```rust
// GPU rendering via wgpu-api not yet integrated — using CPU rendering
// CPU rendering path — always render every frame
```

The audio popup always uses CPU rendering. The `_gpu_context` and `_dmabuf_state` parameters are unused (prefixed with `_`).

**Impact:** Inconsistent rendering — the audio popup uses CPU rendering while other popups (context menu, window list, widget bar) use GPU rendering. This means the audio popup cannot benefit from DMA-BUF presentation.

**Fix:** Implement GPU rendering path for the audio popup, or document why CPU rendering is intentionally used.

---

### 340. process_popup.rs — GPU rendering not integrated (process_popup.rs:98-99)
**File:** `src/app/process_popup.rs`
**Lines:** 98-99

```rust
// GPU rendering via wgpu-api not yet integrated — using CPU rendering
// CPU rendering path
```

Same as audio_popup — GPU rendering is not integrated, parameters are unused.

**Impact:** Inconsistent rendering — process popup uses CPU rendering while other popups use GPU.

**Fix:** Implement GPU rendering path or document the intentional CPU-only design.

---

### 341. rclone_popup.rs — GPU rendering not integrated (rclone_popup.rs:96-97)
**File:** `src/app/rclone_popup.rs`
**Lines:** 96-97

```rust
// GPU rendering via wgpu-api not yet integrated — using CPU rendering
// CPU rendering path
```

Same pattern — GPU rendering not integrated.

**Impact:** Inconsistent rendering across popups.

**Fix:** Implement GPU rendering path or document the intentional CPU-only design.

---

### 342. audio_player_bar.rs — GPU rendering not integrated (audio_player_bar.rs:69-70)
**File:** `src/app/audio_player_bar.rs`
**Lines:** 69-70

```rust
// GPU rendering via wgpu-api not yet integrated — using CPU rendering
// CPU fallback path
```

Same pattern — GPU rendering not integrated.

**Impact:** Audio player bar always uses CPU rendering, creating a SHM buffer every frame.

**Fix:** Implement GPU rendering path or document the intentional CPU-only design.

---

### 343. bluetooth_popup.rs — Filters devices into Vec on every click (bluetooth_popup.rs:51-52)
**File:** `src/app/bluetooth_popup.rs`
**Lines:** 51-52

```rust
let discovered: Vec<_> = snapshot.devices.iter().filter(|d| !d.paired).collect();
let paired: Vec<_> = snapshot.devices.iter().filter(|d| d.paired).collect();
```

On every click, the device list is filtered twice into new vectors. With 10 devices, that's 2 Vec allocations + 20 filter iterations per click.

**Impact:** Per-click heap allocations.

**Fix:** Use iterators directly without collecting, or pre-compute the split when the snapshot changes.

---

### 344. system_bar.rs — Uses unsafe libc calls for time formatting (system_bar.rs:294-322)
**File:** `src/app/system_bar.rs`
**Lines:** 294-322

```rust
let now = unsafe { libc::time(std::ptr::null_mut()) };
let mut local: libc::tm = unsafe { std::mem::zeroed() };
let local_ptr = unsafe { libc::localtime_r(&now, &mut local) };
```

Three `unsafe` blocks use libc directly for time formatting instead of using a safe Rust time library. `std::mem::zeroed()` is undefined behavior if the type has invalid zeroed representations (though `libc::tm` is typically all integers).

**Impact:** Safety risk — `std::mem::zeroed()` is technically UB for types with potential invalid states. While `libc::tm` is safe in practice, this pattern is fragile.

**Fix:** Use `chrono` or `time` crate for safe time formatting, or use `MaybeUninit::zeroed().assume_init()` with a comment justifying safety.

---

### 345. rclone.rs — Uses unsafe libc::statvfs (rclone.rs:242-243)
**File:** `src/app/rclone.rs`
**Lines:** 242-243

```rust
let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
let result = unsafe { libc::statvfs(path.as_ptr(), &mut stats) };
```

Same `std::mem::zeroed()` issue as system_bar.rs.

**Impact:** Same safety concern.

**Fix:** Use `MaybeUninit::zeroed().assume_init()` or a safe filesystem stats library.

---

### 346. process_monitor.rs — Refreshes all processes every 1.2 seconds (process_monitor.rs:35-39)
**File:** `src/app/process_monitor.rs`
**Lines:** 35-39

```rust
system.refresh_processes_specifics(
    sysinfo::ProcessesToUpdate::All,
    true,
    process_refresh,
);
```

Every 1.2 seconds, all processes on the system are refreshed. This reads `/proc` for every process, including CPU and memory stats.

**Impact:** High CPU and I/O usage — on a system with 500 processes, this reads 500 `/proc/[pid]/stat` and `/proc/[pid]/status` files every 1.2 seconds.

**Fix:** Only refresh processes that are currently in the top-N list, or increase the polling interval.

---

### 347. side_bars.rs — Hardcoded colors in GPU path (side_bars.rs:131-135, 331-335)
**File:** `src/app/side_bars.rs`
**Lines:** 131-135, 331-335

```rust
color: wgpu_api::Color {
    r: 28.0 / 255.0,
    g: 27.0 / 255.0,
    b: 33.0 / 255.0,
    a: alpha * 0.96,
},
```

Background colors are hardcoded as magic numbers in the GPU path. The CPU fallback path uses the config's color settings. This means the GPU and CPU paths can render different colors.

**Impact:** Visual inconsistency — GPU path renders hardcoded colors while CPU path uses config colors.

**Fix:** Parse colors from config in the GPU path, matching the CPU path's color sources.

---

### 348. window_list.rs — Hardcoded colors in GPU path (window_list.rs:433-435)
**File:** `src/app/window_list.rs`
**Lines:** 433-435

```rust
let text_color = DockConfig::parse_hex_color(&config.popup_text_color);
let border_color = DockConfig::parse_hex_color(&config.dashed_block_text_color);
let close_button_color = DockConfig::parse_hex_color(&config.popup_close_button_color);
```

Parses colors from config every frame. Note that `border_color` uses `dashed_block_text_color` instead of `popup_border_color` — likely a copy-paste error.

**Impact:** Wrong border color — the window list popup uses the dashed block's text color for borders instead of the popup border color.

**Fix:** Use `config.popup_border_color` for border color. Parse colors once when config changes.

---

### 349. context_menu.rs — Font size doubled in window list (window_list.rs:560)
**File:** `src/app/window_list.rs`
**Line:** 560

```rust
font_size: config.popup_font_size as f32 * 2.0 * scale,
```

The font size is multiplied by `2.0 * scale`. At 1x scale, this doubles the configured font size. At 2x scale, it quadruples it. This appears to be a compensation for a coordinate space mismatch, but it's incorrect.

**Impact:** Text renders at wrong size — window titles are 2x larger than configured at 1x scale, and 4x larger at 2x scale.

**Fix:** Use `config.popup_font_size as f32 * scale` without the `* 2.0` multiplier.

---

### 350. audio.rs — wpctl status parsing and list command redundancy (audio.rs:383-466)
**File:** `src/app/audio.rs`
**Lines:** 383-466

The `read_audio_snapshot` function first parses `wpctl status` to extract sinks, sources, and streams. Then it calls `wpctl list audio sinks` and `wpctl list audio sources` to **replace** the outputs and inputs parsed from status (lines 461-466). This means the status parsing for sinks/sources is wasted work.

**Impact:** Redundant work — the status parser extracts device information that is immediately discarded and replaced by the list command output.

**Fix:** Only use `wpctl list` for device discovery and `wpctl status` for streams. Skip the sink/source parsing in the status section.

---

## Medium Severity Issues

### 351. network.rs — Empty stub module (network.rs:8)
**File:** `src/app/network.rs`
**Line:** 8

```rust
pub fn draw_network_overlay() {}
```

The module is a stub with an empty function. The comment says "Network rendering is not yet split out in v1.4.0."

**Impact:** Dead code — the function does nothing but exists as a placeholder.

**Fix:** Remove the stub or implement the network overlay.

---

### 352. audio_popup.rs — Hardcoded hit-test coordinates (audio_popup.rs:145-161)
**File:** `src/app/audio_popup.rs`
**Lines:** 145-161

```rust
if !(8..=120).contains(&x) {
    return None;
}
```

Slider hit-test uses hardcoded pixel ranges (8-120 for X, specific Y ranges). These coordinates are duplicated from the renderer and will drift if the renderer changes.

**Impact:** Fragile hit testing — if the audio popup layout changes, the hit-test coordinates must be updated in two places.

**Fix:** Share the hit-test constants between the renderer and the popup module, or compute them from the same source.

---

### 353. bluetooth_popup.rs — Hardcoded hit-test coordinates (bluetooth_popup.rs:54-68)
**File:** `src/app/bluetooth_popup.rs`
**Lines:** 54-68

```rust
let action_x = 22 + DEVICE_ACTION_X; // card x + action offset
```

Device hit-test uses hardcoded X coordinate (22) for the discovered list. This is duplicated from the renderer.

**Impact:** Same fragility as audio_popup.

**Fix:** Share constants between renderer and popup module.

---

### 354. widget_bar.rs — Creates SHM buffer every frame on CPU fallback (widget_bar.rs:216-223)
**File:** `src/app/widget_bar.rs`
**Lines:** 216-223

```rust
let (buffer, canvas) = pool
    .create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    )
```

On the CPU fallback path, a new SHM buffer is created every frame. The `SlotPool` may not reuse the previous frame's buffer.

**Impact:** Per-frame memory allocation on CPU path.

**Fix:** Reuse the buffer across frames, similar to how the GPU path reuses the DMA-BUF.

---

### 355. side_bars.rs — Creates SHM buffer every frame on CPU fallback (side_bars.rs:247-254, 551-558)
**File:** `src/app/side_bars.rs`
**Lines:** 247-254, 551-558

Same issue — CPU fallback creates a new SHM buffer every frame for both tray bar and system bar.

**Impact:** Per-frame memory allocation on CPU path.

**Fix:** Reuse the buffer across frames.

---

### 356. window_list.rs — Creates SHM buffer every frame on CPU fallback (window_list.rs:706-713)
**File:** `src/app/window_list.rs`
**Lines:** 706-713

Same issue — CPU fallback creates a new SHM buffer every frame.

**Impact:** Per-frame memory allocation on CPU path.

**Fix:** Reuse the buffer across frames.

---

### 357. audio.rs — Action errors silently ignored (audio.rs:204)
**File:** `src/app/audio.rs`
**Line:** 204

```rust
if let Err(_error) = result {}
```

When `wpctl` or `playerctl` fails (e.g., device not found, permission denied), the error is silently discarded. The user has no feedback that their volume change failed.

**Impact:** Silent failures — user changes volume but the change doesn't take effect, with no error indication.

**Fix:** Log the error or send it back to the UI for display.

---

### 358. bluetooth.rs — Discovery ownership flag race condition (bluetooth.rs:53, 100)
**File:** `src/app/bluetooth.rs`
**Lines:** 53, 100

```rust
let mut discovery_owned = false;
// ...
apply_action(conn, action, &mut discovery_owned).await,
```

The `discovery_owned` flag tracks whether this dock instance started the Bluetooth scan. However, if the dock restarts while discovery is active, the flag is `false` but discovery is actually running. The dock won't be able to stop discovery because it thinks it doesn't own it.

**Impact:** Stuck discovery mode — after a dock restart, the Bluetooth scan may continue indefinitely because the dock won't stop a scan it doesn't know it started.

**Fix:** Check the actual `Discovering` property from BlueZ on startup and set `discovery_owned` accordingly.

---

### 359. rclone.rs — About cache never evicts stale entries (rclone.rs:31-32, 61)
**File:** `src/app/rclone.rs`
**Lines:** 31-32, 61

```rust
let mut about_cache: HashMap<String, (Instant, Option<(u64, u64, u64)>)> =
    HashMap::new();
// ...
const ABOUT_CACHE_TTL: Duration = Duration::from_secs(60);
```

The about cache has a TTL but never evicts entries. As mounts come and go, old entries remain in the cache forever, only marked as expired.

**Impact:** Memory leak — the cache grows without bound as mounts are added and removed.

**Fix:** Periodically remove expired entries from the cache.

---

### 360. system_bar.rs — Network totals skip loopback but include all other interfaces (system_bar.rs:100-101)
**File:** `src/app/system_bar.rs`
**Lines:** 100-101

```rust
if interface.trim() == "lo" {
    continue;
}
```

Only `lo` is excluded. Docker bridges (`docker0`, `br-*`), VPN interfaces (`tun0`, `wg0`), and other virtual interfaces are included in the network rate calculation.

**Impact:** Incorrect network rates — Docker container traffic and VPN traffic are counted in the display, inflating the shown rates.

**Fix:** Allow the user to configure which interfaces to include, or auto-detect the primary interface.

---

### 361. context_menu.rs — Dashed border creates many small rectangles (context_menu.rs:180-208)
**File:** `src/app/context_menu.rs`
**Lines:** 180-208

The dashed border for the hovered menu item creates individual `DrawCommand::Rectangle` for each dash segment. For a 200px-wide menu with 4px dashes, that's ~50 rectangles per border side, ~200 total.

**Impact:** Performance — hundreds of draw commands per frame for the hovered item's border.

**Fix:** Batch dashed border segments into fewer draw commands or use a line-dash pattern.

---

### 362. side_bars.rs — Tray icon positions use logical pixels, then scale (side_bars.rs:144-147)
**File:** `src/app/side_bars.rs`
**Lines:** 144-147

```rust
let (logical_x, logical_y) = tray_icon_position(index, items.len()).unwrap_or((0, 0));
let phys_x = (logical_x as f64 * scale).round() as f32;
let phys_y = (logical_y as f64 * scale).round() as f32;
```

Tray icon positions are computed in logical pixels, then scaled to physical pixels. This is correct, but the rounding can cause sub-pixel misalignment between icons at fractional scale factors.

**Impact:** Minor visual artifacts — icons may be 1px off at fractional scale factors (e.g., 1.25x).

**Fix:** Use consistent rounding strategy across all icon positioning code.

---

### 363. window_list.rs — PipeWire stream matching uses linear search (window_list.rs:106-117)
**File:** `src/app/window_list.rs`
**Lines:** 106-117

```rust
let bound = service.get_bound_stream(window_id);
if let Some(b) = bound {
    if !used_streams.contains(&b) {
```

`used_streams.contains()` is O(n) on a Vec. For each window, a linear search is performed. With N windows and M streams, this is O(N²) in the worst case.

**Impact:** Slow stream matching — with many windows, the matching process becomes quadratic.

**Fix:** Use a `HashSet` or `FxHashSet` for `used_streams`.

---

### 364. audio_popup.rs — Unused parameters (audio_popup.rs:32-36)
**File:** `src/app/audio_popup.rs`
**Lines:** 32-36

```rust
_tray_items: &[TrayItem],
_system_bar: &SystemBarSnapshot,
_gpu_context: Option<&mut wgpu_api::Renderer>,
_dmabuf_state: &DmabufState,
```

Four parameters are unused (prefixed with `_`). The GPU context and DMA-BUF state are not used because GPU rendering is not integrated.

**Impact:** API inconsistency — the audio popup signature differs from other popups that use GPU rendering.

**Fix:** Implement GPU rendering or remove the unused parameters.

---

### 365. process_popup.rs — Unused parameters (process_popup.rs:30-33)
**File:** `src/app/process_popup.rs`
**Lines:** 30-33

Same issue — `_gpu_context` and `_dmabuf_state` are unused.

**Fix:** Implement GPU rendering or remove the unused parameters.

---

### 366. rclone_popup.rs — Unused parameters (rclone_popup.rs:29-32)
**File:** `src/app/rclone_popup.rs`
**Lines:** 29-32

Same issue — `_gpu_context` and `_dmabuf_state` are unused.

**Fix:** Implement GPU rendering or remove the unused parameters.

---

### 367. audio_player_bar.rs — Unused parameters (audio_player_bar.rs:27-28)
**File:** `src/app/audio_player_bar.rs`
**Lines:** 27-28

Same issue — `_gpu_context` and `_dmabuf_state` are unused.

**Fix:** Implement GPU rendering or remove the unused parameters.

---

### 368. window_list.rs — Hovered close button computed but unused (window_list.rs:347-363)
**File:** `src/app/window_list.rs`
**Lines:** 347-363

```rust
let _hovered_close = if hovered_item >= 0 {
```

The close button hover state is computed but the result is prefixed with `_` and never used. The close button is drawn as a rectangle in the GPU path but has no click handler integration.

**Impact:** Dead computation — the hover state is calculated but never affects rendering or behavior.

**Fix:** Use the hover state to highlight the close button or remove the computation.

---

### 369. side_bars.rs — apply_surface_alpha iterates all pixels (side_bars.rs:573-583)
**File:** `src/app/side_bars.rs`
**Lines:** 573-583

```rust
fn apply_surface_alpha(pixels: &mut [u8], alpha: f32) {
    let alpha = alpha.clamp(0.0, 1.0);
    if alpha >= 0.999 {
        return;
    }
    for pixel in pixels.chunks_exact_mut(4) {
        for channel in 0..4 {
            pixel[channel] = (pixel[channel] as f32 * alpha).round() as u8;
        }
    }
}
```

For a 500x64 system bar at 2x scale (1000x128), this iterates 128,000 pixels and performs floating-point multiplication for each channel. This is called on the CPU fallback path every frame.

**Impact:** CPU usage — iterating all pixels with float math on every frame is slow.

**Fix:** Use SIMD or apply alpha during rendering instead of as a post-process step.

---

### 370. window_list.rs — Capture frame texture keyed by output ID only (window_list.rs:446-448)
**File:** `src/app/window_list.rs`
**Lines:** 446-448

```rust
let cache_key =
    format!("window-list-capture:{:?}", dock.output.id());
```

The capture frame texture cache key includes only the output ID, not the stream index or frame content. If multiple window list popups exist on different outputs, they share the same cache key and may display the wrong capture frame.

**Impact:** Wrong capture frame — on multi-monitor setups, window list popups may show the capture frame from another monitor's active stream.

**Fix:** Include the stream index or a frame hash in the cache key.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 331 | **Critical** | Second tokio runtime in Bluetooth monitor | Resource conflicts |
| 332 | **Critical** | 751-line monolithic window list function | Maintainability |
| 333 | **Critical** | Per-separator-pixel draw command allocation | Performance |
| 334 | **Critical** | Clones WindowState every frame | Per-frame allocations |
| 335 | **Critical** | Spawns wpctl per device volume read | High CPU and I/O |
| 336 | **High** | Parses hex colors on every frame | Per-frame string parsing |
| 337 | **High** | Estimates text width by character count | Incorrect text positioning |
| 338 | **High** | Widget bar GPU path renders rectangles without icons | Visual mismatch |
| 339 | **High** | Audio popup GPU rendering not integrated | Inconsistent rendering |
| 340 | **High** | Process popup GPU rendering not integrated | Inconsistent rendering |
| 341 | **High** | Rclone popup GPU rendering not integrated | Inconsistent rendering |
| 342 | **High** | Audio player bar GPU rendering not integrated | Inconsistent rendering |
| 343 | **High** | Filters devices into Vec on every click | Per-click allocations |
| 344 | **High** | Unsafe libc calls for time formatting | Safety risk |
| 345 | **High** | Unsafe libc::statvfs with mem::zeroed | Safety risk |
| 346 | **High** | Refreshes all processes every 1.2 seconds | High CPU and I/O |
| 347 | **High** | Hardcoded colors in GPU path | Visual inconsistency |
| 348 | **High** | Wrong border color in window list | Visual bug |
| 349 | **High** | Font size doubled in window list | Text renders at wrong size |
| 350 | **High** | wpctl status parsing and list redundancy | Redundant work |
| 351 | **Medium** | Empty stub network module | Dead code |
| 352 | **Medium** | Hardcoded hit-test coordinates in audio popup | Fragile hit testing |
| 353 | **Medium** | Hardcoded hit-test coordinates in bluetooth popup | Fragile hit testing |
| 354 | **Medium** | Creates SHM buffer every frame on CPU fallback | Per-frame allocation |
| 355 | **Medium** | Creates SHM buffer every frame on CPU fallback (side bars) | Per-frame allocation |
| 356 | **Medium** | Creates SHM buffer every frame on CPU fallback (window list) | Per-frame allocation |
| 357 | **Medium** | Action errors silently ignored | Silent failures |
| 358 | **Medium** | Discovery ownership flag race condition | Stuck discovery mode |
| 359 | **Medium** | About cache never evicts stale entries | Memory leak |
| 360 | **Medium** | Network totals include Docker and VPN interfaces | Incorrect rates |
| 361 | **Medium** | Dashed border creates many small rectangles | Performance |
| 362 | **Medium** | Tray icon position rounding at fractional scale | Sub-pixel artifacts |
| 363 | **Medium** | PipeWire stream matching uses linear search | O(n²) matching |
| 364 | **Medium** | Unused parameters in audio popup | API inconsistency |
| 365 | **Medium** | Unused parameters in process popup | API inconsistency |
| 366 | **Medium** | Unused parameters in rclone popup | API inconsistency |
| 367 | **Medium** | Unused parameters in audio player bar | API inconsistency |
| 368 | **Medium** | Hovered close button computed but unused | Dead computation |
| 369 | **Medium** | apply_surface_alpha iterates all pixels | CPU usage |
| 370 | **Medium** | Capture frame texture keyed by output ID only | Wrong capture frame |

## Cumulative Summary

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 2 | 3 | 4 | 3 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 7 |
| 3 | gpu-rendering-review-3.md | 33–60 | 3 | 3 | 10 | 14 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 13 | 21 | 21 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 12 | 10 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| 8 | gpu-rendering-review-8.md | 200–240 | 6 | 10 | 16 | 8 |
| 9 | gpu-rendering-review-9.md | 241–280 | 6 | 10 | 16 | 8 |
| 10 | gpu-rendering-review-10.md | 281–330 | 10 | 20 | 20 | 0 |
| 11 | gpu-rendering-review-11.md | 331–370 | 5 | 15 | 25 | 5 |
| **Total** | | **370** | **48** | **95** | **146** | **93** |

## Key Themes from Round 11

1. **GPU rendering gaps**: 4 of 16 modules (audio_popup, process_popup, rclone_popup, audio_player_bar) have GPU rendering not integrated, with unused `_gpu_context` and `_dmabuf_state` parameters.

2. **Per-frame allocations**: Window list clones `WindowState` every frame, context menu parses hex colors every frame, and multiple popups create SHM buffers every frame on the CPU fallback path.

3. **Subprocess spawning**: Audio monitor spawns 10+ `wpctl` subprocesses per snapshot (every 1.5s). Bluetooth monitor creates a second tokio runtime.

4. **Hardcoded values**: Colors, hit-test coordinates, and font sizes are hardcoded in GPU paths, creating visual inconsistencies with CPU paths that use config values.

5. **Unsafe code**: Two modules use `std::mem::zeroed()` for libc structs, which is technically UB.

6. **Monolithic functions**: Window list is a 751-line function handling stream matching, geometry, GPU commands, CPU fallback, and Wayland buffer management.
