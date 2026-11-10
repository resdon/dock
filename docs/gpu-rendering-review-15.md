# GPU Rendering Code Review — Round 15 (Render Submodules, Errors, Resolvers, Services)

This is a continuation of the GPU rendering code review. Rounds 1–14 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, and documentation (issues 1–505). This round covers the remaining unexamined modules: `src/render/` submodules (color_check, debug_overlay, popup, processes, rclone, tray, utils, widget_bar), `src/errors/` (all 6 submodules + mod), `src/resolvers/` (icon_list, icon_stage, steam, window), and `src/services/resource_monitor.rs` — issues 506–550.

---

## Critical Issues

### 506. tray.rs — Per-pixel bilinear interpolation loop for every tray icon, every frame (lines 670-724, 829-878)
**File:** `src/render/tray.rs` lines 670-724, 829-878

Both `render_tray_into_canvas` and `render_tray_icons_into_canvas` perform per-pixel bilinear interpolation on the CPU for every tray icon, every frame. For a 24×24 icon at 2x scale (48×48 = 2,304 pixels), each pixel requires 16 memory reads (4 corners × 4 channels) and 12 floating-point multiplications. With 4 tray icons, this is ~9,216 pixels × 16 reads = ~147,456 memory operations per frame, per tray render call.

```rust
for y in 0..icon_size {
    for x in 0..icon_size {
        // 16 memory reads, 12 FMA operations per pixel
        let mut sample = [0.0f32; 4];
        for channel in 0..4 {
            let top = icon.rgba[idx00 + channel] as f32 * (1.0 - tx) + icon.rgba[idx10 + channel] as f32 * tx;
            let bottom = icon.rgba[idx01 + channel] as f32 * (1.0 - tx) + icon.rgba[idx11 + channel] as f32 * tx;
            sample[channel] = top * (1.0 - ty) + bottom * ty;
        }
    }
}
```

**Impact:** CPU waste — this is exactly the kind of per-pixel CPU work that GPU rendering should handle. The GPU path should use textured quads with hardware bilinear filtering instead.

**Fix:** Pre-scale tray icons to the target size once and cache the result, or render tray icons via the GPU path with hardware texture sampling.

---

### 507. popup.rs — Per-pixel nearest-neighbor scaling for capture frames (lines 202-231)
**File:** `src/render/popup.rs` lines 202-231

`draw_capture_frame` scales PipeWire capture frames using a nested for-loop with nearest-neighbor sampling. For a 200×150 thumbnail, this is 30,000 iterations per frame, each doing floating-point division and multiplication.

```rust
for fy in 0..frame_h {
    for fx in 0..frame_w {
        let sx = (fx as f32 * x_step) as i32;
        let sy = (fy as f32 * y_step) as i32;
    }
}
```

**Impact:** CPU waste — capture frame scaling should use `fast_image_resize` or GPU texture sampling.

**Fix:** Use `fast_image_resize` for CPU path or render via GPU textured quad.

---

### 508. steam.rs — WalkDir with depth 6 on icon theme directories (lines 242-255)
**File:** `src/resolvers/steam.rs` lines 242-255

`find_steam_icon` uses `WalkDir::new(&theme_dir).max_depth(6)` to search hicolor icon directories recursively. A typical hicolor directory has 10+ size subdirectories (16x16, 22x22, 24x24, 32x32, 48x48, 64x64, 128x128, 256x256, 512x512, scalable) each containing thousands of icons. At depth 6, this traverses the entire icon theme tree.

```rust
for entry in WalkDir::new(&theme_dir)
    .max_depth(6)
    .into_iter()
    .filter_map(|e| e.ok())
{
```

**Impact:** I/O spike — this walks thousands of directories and files on the render thread (or whatever thread calls it), causing latency spikes.

**Fix:** Use the icon list cache (`icon_list.rs`) instead of WalkDir, or limit the search to specific size directories.

---

### 509. icon_list.rs — Parallel file search opens the file 4 times (lines 176-268)
**File:** `src/resolvers/icon_list.rs` lines 176-268

`search_icon_list_file` spawns 4 worker threads, each opening the same file independently (`File::open(&list_path_clone)`). For a large icon list file (e.g., 1 MB), this means 4× the I/O overhead. Additionally, each thread seeks to its chunk position and reads line-by-line, which is inefficient for a simple string search.

```rust
let handle = thread::spawn(move || {
    let file = match File::open(&list_path_clone) {
        Ok(f) => f,
        Err(_) => return,
    };
```

**Impact:** I/O waste — 4 concurrent file opens and seeks on the same file. For SSDs this is minor, but for HDDs or network mounts, this is significantly slower than a single sequential scan.

**Fix:** Use `memmap2` to memory-map the file once and search the mapped region in parallel, or use a single sequential scan for files < 1 MB.

---

### 510. debug_overlay.rs — Debug stats compute VRAM via sysfs read on every sample (lines 60-86)
**File:** `src/render/debug_overlay.rs` lines 60-86

`read_vram` reads `/sys/class/drm/card0/device/memory_info_vram_total` via `std::fs::read_to_string` on every call. While throttled to once per second, this is still a filesystem read on the render thread.

```rust
if let Ok(total_bytes) = std::fs::read_to_string(total_path) {
```

**Impact:** Minor I/O on render thread — the sysfs read is fast but unnecessary on the critical path.

**Fix:** Cache the VRAM paths and read them in a background thread.

---

## High Severity Issues

### 511. tray.rs — `draw_rounded_panel` is O(width × height) per panel (lines 321-378)
**File:** `src/render/tray.rs` lines 321-378

Every pixel of the panel is checked against the circle equation for rounded corners. For a 76×72 panel, this is 5,472 iterations per call. With multiple panels (tray, keyboard badge, clock panel, network panel), this is ~20,000+ iterations per tray render.

```rust
for py in 0..panel_height {
    for px in 0..panel_width {
        if dx > 0 && dy > 0 && dx * dx + dy * dy > radius * radius {
            continue;
        }
    }
}
```

**Impact:** CPU waste — rounded rectangles should be drawn with scanline algorithms or pre-computed corner masks.

**Fix:** Use a scanline-based rounded rectangle algorithm or pre-compute corner masks.

---

### 512. widget_bar.rs — Same O(width × height) rounded panel algorithm (lines 127-165)
**File:** `src/render/widget_bar.rs` lines 127-165

Identical per-pixel rounded panel algorithm as tray.rs. Called 5 times per widget bar render (1 panel + 4 buttons).

**Impact:** Same as #511.

---

### 513. popup.rs — `render_window_list_surface` does per-pixel row background fill (lines 364-373)
**File:** `src/render/popup.rs` lines 364-373

When a close button is hovered, the row background is filled pixel-by-pixel:

```rust
for ry in row_y..row_y + row_h {
    for rx in row_x..row_x + row_w {
        canvas[idx..idx + 4].copy_from_slice(&color);
    }
}
```

**Impact:** CPU waste — `copy_from_slice` in a loop is slower than `fill` or `std::slice::fill`.

**Fix:** Use `canvas[range].fill()` or `std::ptr::write_bytes` for solid-color fills.

---

### 514. processes.rs — Clones entire process list on every render (line 314)
**File:** `src/render/processes.rs` line 314

```rust
let mut processes = snapshot.processes.clone();
```

The entire `Vec<ProcessInfo>` is cloned on every render frame, then sorted. For 100 processes, this is ~100 × sizeof(ProcessInfo) bytes allocated per frame.

**Impact:** Per-frame allocation — violates the "no steady-state heap allocations" rule.

**Fix:** Sort a slice in-place or use a pre-sorted snapshot.

---

### 515. resource_monitor.rs — Spawns nvidia-smi subprocess every 2 seconds (lines 56-109)
**File:** `src/services/resource_monitor.rs` lines 56-109

`query_vram_mb` spawns `nvidia-smi pmon -c 1 -d MEM -q -p <pid>` as a subprocess every 2 seconds. If that fails, it falls back to another `nvidia-smi` call. Each subprocess spawn is ~5-10ms of overhead.

```rust
let output = Command::new("nvidia-smi")
    .args(&["pmon", "-c", "1", "-d", "MEM", "-q", "-p", &pid.to_string()])
    .output()
    .ok()?;
```

**Impact:** CPU waste — subprocess spawning every 2 seconds is unnecessary overhead for a debug feature.

**Fix:** Read VRAM from `/sys/class/drm/` sysfs (as done in debug_overlay.rs) or parse `/proc/driver/nvidia/gpus/*/memory/info`.

---

### 516. errors/mod.rs — Documents miette integration but miette may not be a dependency (line 11)
**File:** `src/errors/mod.rs` line 11

```rust
//! - Errors integrate with `miette` for human-readable diagnostics.
```

Per Round 14 issue #480, miette and color-eyre are documented but may not be in Cargo.toml. The error modules define `Recoverable` trait and recovery strategies, but none of the error types implement `miette::Diagnostic`.

**Impact:** Misleading documentation — the module claims miette integration that doesn't exist.

**Fix:** Remove the miette reference or add the dependency.

---

### 517. errors/mod.rs — `Recoverable` for `AppError` has a gap: DbusError returns recovery_strategy but is NOT recoverable (lines 55-75)
**File:** `src/errors/mod.rs` lines 55-75

```rust
fn is_recoverable(&self) -> bool {
    matches!(self, AppError::ServiceError(_) | AppError::ListenerError(_) | ...)
    // AppError::DbusError is NOT matched here
}

fn recovery_strategy(&self) -> Option<&'static str> {
    match self {
        AppError::DbusError(_) => Some("Retry D-Bus connection with backoff"),
        // ...
    }
}
```

`DbusError` returns `false` for `is_recoverable()` but provides a recovery strategy. This is contradictory — if there's a recovery strategy, the error should be recoverable.

**Impact:** Logic bug — the recovery system won't auto-recover D-Bus errors even though a strategy exists.

**Fix:** Add `AppError::DbusError(_)` to the `is_recoverable` match arm.

---

### 518. icon_list.rs — Recursive directory scan without symlink protection (lines 97-113)
**File:** `src/resolvers/icon_list.rs` lines 97-113

`scan_directory_to_writer` recurses into every subdirectory without checking for symlinks. A malicious symlink in `/usr/share/icons` pointing to `/` would cause the scanner to walk the entire filesystem.

```rust
if path.is_dir() {
    scan_directory_to_writer(&path, writer);
}
```

**Impact:** Security risk — symlink following could cause excessive I/O or expose sensitive paths in the icon list cache.

**Fix:** Use `path.symlink_metadata()` to check for symlinks and skip them, or use `WalkDir` with `follow_links(false)`.

---

### 519. tray.rs — `blend_pixel` uses f32 arithmetic for every pixel (lines 122-133)
**File:** `src/render/tray.rs` lines 122-133

```rust
fn blend_pixel(canvas: &mut [u8], index: usize, color: [u8; 3], alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    canvas[index] = (color[2] as f32 * a + canvas[index] as f32 * inv).round() as u8;
```

Every pixel blend converts u8→f32, does 2 FMA operations, rounds, and converts back to u8. For thousands of pixels, this is significantly slower than integer alpha blending.

**Impact:** Performance — floating-point alpha blending is ~4× slower than integer blending on most CPUs.

**Fix:** Use integer alpha blending: `(src * alpha + dst * (255 - alpha)) >> 8`.

---

### 520. widget_bar.rs — Bresenham line drawing for simple icons (lines 92-125)
**File:** `src/render/widget_bar.rs` lines 92-125

The widget bar draws icons (rclone, process, audio, Bluetooth) using Bresenham's line algorithm, pixel by pixel. For a 28×28 button, each icon uses ~10-20 line segments, each calling `blend` (which does f32 arithmetic). This is called every frame.

**Impact:** CPU waste — these static icons should be pre-rendered to a buffer once and blitted.

**Fix:** Pre-render the widget bar icons to a static buffer and copy it each frame.

---

### 521. popup.rs — Dashed border draws pixel-by-pixel with bounds checking (lines 44-98)
**File:** `src/render/popup.rs` lines 44-98

`draw_border` iterates every pixel of the border, checking bounds and writing BGRA values. For a 200×200 popup, the border is ~800 pixels, each with a bounds check and a 4-byte copy.

**Impact:** Minor CPU waste — the bounds check is redundant for well-formed coordinates.

**Fix:** Remove redundant bounds checks or use `copy_from_slice` for horizontal/vertical segments.

---

### 522. processes.rs — Hardcoded popup dimensions (lines 10-22)
**File:** `src/render/processes.rs` lines 10-22

```rust
pub const PROCESS_POPUP_WIDTH: i32 = 300;
pub const PROCESS_POPUP_HEIGHT: i32 = 280;
pub const HEADER_MINIMIZE_X: i32 = 272;
```

All hitboxes, positions, and dimensions are hardcoded logical pixels. At 2x scale, these need to be multiplied by scale, but the hit-testing functions (`minimize_button_at`, `page_delta_at`, `sort_at`) use unscaled coordinates.

**Impact:** Hit-testing bug at non-1x scale — the hitboxes won't match the rendered positions.

**Fix:** Ensure hit-testing uses the same scale factor as rendering.

---

### 523. rclone.rs — Same hardcoded hitbox issue (lines 10-20)
**File:** `src/render/rclone.rs` lines 10-20

Same pattern as processes.rs — hardcoded logical pixel hitboxes that don't account for scale factor in hit-testing.

---

### 524. steam.rs — Reads /proc/<pid>/environ for every process (lines 76-84, 117-128)
**File:** `src/resolvers/steam.rs` lines 76-84, 117-128

`resolve_appid_from_process` and `resolve_appid_from_gamescope` read `/proc/<pid>/environ` for every process in the system. Reading environ requires CAP_SYS_PTRACE or the same UID. For processes owned by other users, this will fail with EACCES, generating noise in dmesg.

```rust
if let Ok(env_data) = fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
```

**Impact:** Permission noise — EACCES errors for every non-user process. On systems with many processes, this is hundreds of failed reads.

**Fix:** Check ownership before reading or use `sysinfo`'s environment access if available.

---

### 525. resource_monitor.rs — `System::new_all()` loads all processes into memory (line 120)
**File:** `src/services/resource_monitor.rs` line 120

```rust
let mut sys = System::new_all().expect("Failed to create sysinfo System");
```

`new_all()` loads information about every process on the system. The resource monitor only needs the dock process's own stats. Loading all processes is ~10-50ms of I/O and several MB of RAM.

**Impact:** Unnecessary overhead — the monitor only tracks one process but loads all of them.

**Fix:** Use `System::new()` and refresh only the current process.

---

## Medium Severity Issues

### 526. color_check.rs — Module exists but GPU path doesn't use it
**File:** `src/render/color_check.rs`

The BGRA pixel helpers (`write_bgra_pixel`, `write_bgra_color`) are designed for CPU rendering. The GPU path in `renderer.rs` does its own RGBA→BGRA conversion in `upload_texture_from_pixels`. The color_check module is only used by CPU renderers.

**Impact:** Code duplication — BGRA conversion logic exists in multiple places.

---

### 527. tray.rs — `draw_missing_tray_icon` draws 8 dots with per-pixel loops (lines 393-453)
**File:** `src/render/tray.rs` lines 393-453

A missing tray icon is rendered as 8 small dots, each drawn with a nested for-loop checking `ox * ox + oy * oy > dot_r * dot_r`. For a 2px radius dot, this is 25 iterations × 8 dots = 200 iterations.

**Impact:** Minor CPU waste — these dots should be pre-rendered.

---

### 528. tray.rs — `draw_status_arrow` draws arrows pixel-by-pixel (lines 135-172)
**File:** `src/render/tray.rs` lines 135-172

Network download/upload arrows are drawn as vector pixels, not font glyphs. This avoids font loading but requires per-pixel blending for every arrow, every frame.

**Impact:** Minor CPU waste — pre-render the arrows.

---

### 529. widget_bar.rs — Bluetooth symbol drawn with 6 Bresenham lines (lines 408-472)
**File:** `src/render/widget_bar.rs` lines 408-472

The Bluetooth rune is drawn with 6 line segments using Bresenham's algorithm. Each line calls `blend` (f32 arithmetic) for every pixel.

**Impact:** Same as #520 — pre-render static icons.

---

### 530. popup.rs — `CaptureFrameData` holds Vec<u8> per frame (lines 160-167)
**File:** `src/render/popup.rs` lines 160-167

```rust
pub struct CaptureFrameData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub original_width: u32,
    pub original_height: u32,
}
```

Each capture frame allocates a `Vec<u8>` for the pixel data. For a 1920×1080 frame at RGBA, this is 8.3 MB. If the capture service updates this every frame, it's a per-frame allocation.

**Impact:** Per-frame allocation — should use a pre-allocated buffer.

---

### 531. icon_list.rs — Shard files left on disk if merge fails (lines 83-92)
**File:** `src/resolvers/icon_list.rs` lines 83-92

If `File::create(&list_path)` fails during merge, the shard files (`icon_list.txt.shard.0`, etc.) are left on disk. They're only cleaned up after successful merge.

**Impact:** Disk clutter — orphaned shard files accumulate on failed index runs.

**Fix:** Clean up shards in a `drop` handler or on merge failure.

---

### 532. errors/gpu.rs — `GpuAppError` variants are never constructed in the codebase
**File:** `src/errors/gpu.rs`

None of the `GpuAppError` variants (`DeviceLostDuringRendering`, `MemoryPressure`, `FrameTimeout`, etc.) are constructed anywhere in the codebase. The GPU error handling falls back to `log::error!` or `tracing::error!` instead of using these typed errors.

**Impact:** Dead code — the error types exist but are never used.

---

### 533. errors/memory.rs — Same: variants never constructed
**File:** `src/errors/memory.rs`

Same as #532 — `MemoryAppError` variants are defined but never constructed.

---

### 534. errors/pipewire.rs — Same: variants never constructed
**File:** `src/errors/pipewire.rs`

Same as #532.

---

### 535. errors/system.rs — Same: variants never constructed
**File:** `src/errors/system.rs`

Same as #532.

---

### 536. errors/ui.rs — Same: variants never constructed
**File:** `src/errors/ui.rs`

Same as #532.

---

### 537. errors/wayland.rs — Same: variants never constructed
**File:** `src/errors/wayland.rs`

Same as #532.

---

### 538. icon_stage.rs — `IconResolver` is `Copy` but calls `get_icon_path` which does I/O
**File:** `src/resolvers/icon_stage.rs`

`IconResolver` is `Copy` and stateless, but `resolve` calls `get_icon_path` which reads the filesystem. The `Copy` trait suggests this is a lightweight operation, but it's actually I/O-bound.

**Impact:** Misleading API — `Copy` implies cheap cloning, but the method does filesystem I/O.

---

### 539. window.rs — `WindowResolver::resolve` hardcodes capabilities (lines 57-65)
**File:** `src/resolvers/window.rs` lines 57-65

```rust
window.capabilities = WindowCapabilities {
    can_activate: true,
    can_minimize: true,
    can_close: true,
    can_move: true,
    can_raise: true,
    can_pin: true,
    can_move_to_output: !hints.outputs.is_empty(),
};
```

All capabilities are hardcoded to `true` except `can_move_to_output`. Not all windows support all operations — some may be uncloseable or unmovable.

**Impact:** Incorrect capabilities — the dock may offer actions that fail silently.

---

### 540. resource_monitor.rs — FPS counter is average since start, not current FPS (lines 37-48)
**File:** `src/services/resource_monitor.rs` lines 37-48

```rust
pub fn get_fps() -> f64 {
    let count = FRAME_COUNT.load(Ordering::Relaxed);
    let start = match START_TIME.get() { ... };
    let elapsed = start.elapsed().as_secs_f64();
    count as f64 / elapsed
}
```

This returns the average FPS since the first frame, not the current instantaneous FPS. After 1 hour of running at 60 FPS, a brief drop to 10 FPS won't be reflected because the average is dominated by the earlier frames.

**Impact:** Misleading metric — the FPS shown in debug output is not useful for detecting performance regressions.

**Fix:** Use a sliding window (e.g., last 60 seconds) for FPS calculation.

---

### 541. tray.rs — `measure_text` sums glyph advance widths without kerning (lines 381-391)
**File:** `src/render/tray.rs` lines 381-391

```rust
fn measure_text(font_manager: &mut FontManager, text: &str, size: f32) -> i32 {
    text.chars().map(|character| {
        font_manager.get_glyph(character, size).metrics.advance_width
    }).sum::<f32>().round() as i32
}
```

Text width is estimated by summing individual glyph advance widths without kerning pairs. This overestimates or underestimates the actual rendered width.

**Impact:** Minor layout inaccuracy — text may be slightly wider or narrower than measured.

---

### 542. popup.rs — Font size doubled for window list popup (line 412)
**File:** `src/render/popup.rs` line 412

```rust
let font_size = (font_size_cfg * 2.0 * scale_factor) as i32;
```

The font size is multiplied by 2.0 before scale_factor. This appears to compensate for logical-to-physical pixel conversion, but it's already handled by `scale_factor`. At 1x scale, the font is 2× the configured size.

**Impact:** Visual inconsistency — popup text is larger than intended.

---

### 543. processes.rs — `format_bytes` uses f64 for byte conversion (lines 84-97)
**File:** `src/render/processes.rs` lines 84-97

```rust
fn format_bytes(bytes: u64) -> String {
    let mut value = bytes as f64;
```

`u64` to `f64` conversion loses precision for values > 2^53. For memory values in the TB range, this is fine, but the conversion is unnecessary — integer division would work.

**Impact:** Minor precision loss for very large values.

---

### 544. rclone.rs — `format_bytes` has 7 units vs processes.rs 5 units (line 86 vs line 87)
**File:** `src/render/rclone.rs` line 86 vs `src/render/processes.rs` line 87

```rust
// rclone.rs
const UNITS: [&str; 7] = ["B", "K", "M", "G", "T", "P", "E"];
// processes.rs
let units = ["B", "K", "M", "G", "T"];
```

The two `format_bytes` functions have different unit arrays. This is code duplication with inconsistent behavior.

**Impact:** Inconsistent formatting — the same byte value may display differently in different popups.

---

### 545. widget_bar.rs — `blend` function sets alpha to 255 when source alpha is 1.0 (line 71)
**File:** `src/render/widget_bar.rs` line 71

```rust
canvas[i + 3] = (255.0 * a + canvas[i + 3] as f32 * inv).round() as u8;
```

When `alpha` is 1.0, the destination alpha becomes 255 regardless of the original. This is correct for opaque drawing but incorrect for semi-transparent overlays on a transparent background.

**Impact:** Correct for current usage but fragile if the widget bar is rendered on a non-transparent background.

---

### 546. tray.rs — `render_tray_into_canvas` has unused `debug` parameter with empty blocks (lines 479, 494, 531, 584, 555, 779)
**File:** `src/render/tray.rs` lines 479, 494, 531, 584, 555, 779

```rust
if debug {}
```

Multiple empty `if debug {}` blocks are scattered throughout the function. These are dead code placeholders for future debug logging.

**Impact:** Dead code — these blocks do nothing and add noise.

---

### 547. icon_list.rs — `search_icon_list_file` prefers .png over .svg (lines 151-152)
**File:** `src/resolvers/icon_list.rs` lines 151-152

```rust
if line_lower.ends_with(".png") {
    return Some(path);
}
```

PNG is preferred over SVG for exact matches. SVG icons scale better and are generally preferred for modern icon themes.

**Impact:** Suboptimal icon quality — raster icons are preferred over vector icons.

---

### 548. steam.rs — `find_steam_icon` checks librarycache before hicolor (lines 207-258)
**File:** `src/resolvers/steam.rs` lines 207-258

Steam library cache icons are checked first, then hicolor theme. The library cache contains pre-rendered icons at specific sizes, while hicolor may have SVG sources. The library cache icons may be lower quality.

**Impact:** Suboptimal icon quality — cached icons may be pixelated at high DPI.

---

### 549. resource_monitor.rs — `query_vram_mb` parses nvidia-smi output with fragile string matching (lines 65-91)
**File:** `src/services/resource_monitor.rs` lines 65-91

The nvidia-smi output parser splits on whitespace and tries to parse any token as a number. This is fragile — if the output format changes, the parser returns incorrect values.

```rust
for part in &parts {
    if let Ok(val) = part.trim_end_matches('M').trim_end_matches("iB").parse::<u64>() {
        if val > 0 {
            return Some(val);
        }
    }
}
```

**Impact:** Fragile parsing — returns the first positive number found, which may not be VRAM usage.

---

### 550. errors/mod.rs — `AppError` recovery for `DbusError` is inconsistent (lines 97-103)
**File:** `src/errors/mod.rs` lines 97-103

```rust
#[test]
fn test_recoverable_dbus_error() {
    let err = AppError::DbusError("connection refused".into());
    assert!(!err.is_recoverable());  // <-- NOT recoverable
    assert_eq!(
        err.recovery_strategy(),
        Some("Retry D-Bus connection with backoff")  // <-- BUT has a strategy
    );
}
```

The test explicitly asserts that D-Bus errors are NOT recoverable but have a recovery strategy. This is the same contradiction as #517, confirmed by the test.

**Impact:** The recovery system will never auto-recover D-Bus errors.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 506 | **Critical** | Per-pixel bilinear interpolation for tray icons | CPU waste on every frame |
| 507 | **Critical** | Per-pixel nearest-neighbor scaling for capture frames | CPU waste on every frame |
| 508 | **Critical** | WalkDir depth 6 on icon theme directories | I/O spike on render thread |
| 509 | **Critical** | Parallel file search opens file 4 times | I/O waste |
| 510 | **Critical** | sysfs read on render thread for VRAM | Minor I/O on critical path |
| 511 | **High** | O(w×h) rounded panel algorithm | CPU waste per panel |
| 512 | **High** | Same O(w×h) in widget_bar | CPU waste per panel |
| 513 | **High** | Per-pixel row background fill | CPU waste |
| 514 | **High** | Clones process list every render | Per-frame allocation |
| 515 | **High** | nvidia-smi subprocess every 2 seconds | CPU waste |
| 516 | **High** | Documents miette integration that doesn't exist | Misleading docs |
| 517 | **High** | DbusError has strategy but not recoverable | Logic bug |
| 518 | **High** | Recursive scan without symlink protection | Security risk |
| 519 | **High** | f32 alpha blending instead of integer | Performance |
| 520 | **High** | Bresenham line drawing for static icons | CPU waste |
| 521 | **High** | Dashed border pixel-by-pixel with bounds check | Minor CPU waste |
| 522 | **High** | Hardcoded hitboxes don't account for scale | Hit-testing bug |
| 523 | **High** | Same hardcoded hitbox issue in rclone | Hit-testing bug |
| 524 | **High** | Reads /proc/pid/environ for all processes | Permission noise |
| 525 | **High** | System::new_all() loads all processes | Unnecessary overhead |
| 526-550 | **Medium** | Various issues | See above |

### Cumulative Totals (Rounds 1–15)

| Severity | Count |
|----------|-------|
| Critical | 70 |
| High | 99 |
| Medium | 121 |
| Low | 78 |
| **Total** | **368** |

Wait, let me recalculate. Previous total was 505 issues (75 Critical, 152 High, 211 Medium, 61 Low). This round adds 45 issues (5 Critical, 20 High, 20 Medium).

| Severity | Previous | This Round | New Total |
|----------|----------|------------|-----------|
| Critical | 75 | 5 | **80** |
| High | 152 | 20 | **172** |
| Medium | 211 | 20 | **231** |
| Low | 61 | 0 | **61** |
| **Total** | **505** | **45** | **550** |

### Key Themes

1. **Per-pixel CPU rendering**: Every CPU renderer (tray, popup, processes, rclone, widget_bar) uses nested for-loops for pixel operations. None of these popups use the GPU path.
2. **Static icons drawn dynamically**: Widget bar icons (Bluetooth, audio, rclone, process) are drawn with Bresenham line algorithms every frame instead of being pre-rendered.
3. **Error types defined but unused**: All 6 error submodules define rich error types with recovery strategies, but none are actually constructed in the codebase.
4. **I/O on render thread**: WalkDir, /proc reads, and sysfs reads happen on threads that may be the render thread.
5. **Code duplication**: `format_bytes`, `short`, `rect`, `label` are duplicated across processes.rs and rclone.rs.
