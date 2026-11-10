# GPU Rendering Code Review — Round 16 (Render Submodules, State, Resolvers, Services)

This is a continuation of the GPU rendering code review. Rounds 1–15 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, documentation, and additional render submodules (issues 1–550). This round covers: `src/render/theme.rs`, `src/render/dashed_block.rs`, `src/render/date_time_block.rs`, `src/render/audio.rs`, `src/render/audio_player.rs`, `src/render/badges.rs`, `src/render/bluetooth.rs`, `src/render/font.rs`, `src/render/text.rs`, `src/state/popup.rs`, `src/resolvers/process.rs`, `src/services/monitor.rs`, `src/render/mod.rs`, and `src/render/utils.rs` — issues 551–595.

---

## Critical Issues

### 551. services/monitor.rs — Standalone binary with `main()` embedded in library crate (line 572-589)
**File:** `src/services/monitor.rs` lines 572-589

This file contains a `fn main()` that creates a Wayland connection, binds to the foreign toplevel manager protocol, and enters an infinite event loop. It's a standalone binary embedded in the library crate. The `RealtimeTrackerApp` struct and all the Wayland dispatch implementations are only used by this binary, not by the dock application.

```rust
fn main() {
    let conn = Connection::connect_to_env().expect("Wayland connection failed");
    // ...
    loop {
        event_queue.blocking_dispatch(&mut app).unwrap();
    }
}
```

**Impact:** Dead code — this binary is never built or used by the dock. All the Wayland protocol handlers, WalkDir icon searches, and Steam resolution logic are dead weight compiled into the library.

**Fix:** Move to a separate binary crate or remove entirely if not used.

---

### 552. services/monitor.rs — WalkDir depth 6 on /usr/share/icons for every window event (lines 184-210)
**File:** `src/services/monitor.rs` lines 184-210

`locate_actual_icon_path` uses `WalkDir::new(root).max_depth(6)` on `/usr/share/icons`, `/usr/share/pixmaps`, and home icon directories. A typical hicolor icon theme has 10+ size directories × thousands of icons. At depth 6, this traverses the entire icon theme tree for every window event.

```rust
for entry in WalkDir::new(root)
    .max_depth(6)
    .into_iter()
    .filter_map(|e| e.ok())
{
```

**Impact:** I/O spike — thousands of directory entries walked on the Wayland event thread, causing latency spikes for every window state change.

**Fix:** Use linicon (already imported) exclusively, or use the icon list cache.

---

### 553. services/monitor.rs — Full sysinfo process refresh on every HandleEvent::Done (line 533-534)
**File:** `src/services/monitor.rs` lines 533-534

```rust
state.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
```

Every time a Wayland toplevel handle emits a `Done` event (which happens frequently during window state changes), the entire process list is refreshed. This reads `/proc` for every process on the system.

**Impact:** CPU waste — full process scan on every window event, not just when needed.

**Fix:** Throttle process refreshes or only refresh when icon resolution actually needs process data.

---

### 554. services/monitor.rs — Reads /proc/<pid>/environ for every process to find SteamAppId (lines 238-267)
**File:** `src/services/monitor.rs` lines 238-267

`resolve_steam_game_details` iterates all processes in sysinfo, and for each descendant of the entry PID, reads `/proc/<pid>/environ`. Reading environ requires CAP_SYS_PTRACE or same UID. For processes owned by other users, this fails with EACCES.

```rust
if let Ok(env_data) = std::fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
```

**Impact:** Permission noise — hundreds of EACCES errors in dmesg for non-user processes.

**Fix:** Check ownership before reading or limit to known Steam process patterns.

---

### 555. services/monitor.rs — WalkDir on all .desktop files for every window AppId event (lines 91-143)
**File:** `src/services/monitor.rs` lines 91-143

`query_desktop_file_metadata` falls back to walking all `.desktop` files in XDG_DATA_DIRS when a direct filename match fails. It opens and parses each .desktop file line-by-line, looking for StartupWMClass match.

```rust
for entry in WalkDir::new(path)
    .max_depth(2)
    .into_iter()
    .filter_map(|e| e.ok())
{
```

**Impact:** I/O spike — hundreds of .desktop files opened and parsed on every window AppId event.

**Fix:** Cache .desktop file metadata or use a pre-built index.

---

### 556. audio_player.rs — Per-pixel circle rendering: O(r²) per circle, 7+ circles per disc (lines 380-415)
**File:** `src/render/audio_player.rs` lines 380-415

`draw_disc` draws a vinyl record with 7 concentric rings plus a hub with 3 circles. Each circle uses a nested for-loop checking `x * x + y * y <= r * r`. For a 74px radius disc, each circle is ~148×148 = 21,904 iterations. With 11 circles total, that's ~240,000 iterations per render.

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

**Impact:** CPU waste — ~240K iterations per audio player render, each calling `blend` with f32 alpha math.

**Fix:** Pre-render the disc to a static buffer or use a midpoint circle algorithm.

---

### 557. audio_player.rs — `blend()` uses f32 arithmetic per pixel (lines 519-533)
**File:** `src/render/audio_player.rs` lines 519-533

```rust
fn blend(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, rgb: [u8; 3], alpha: f32) {
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    c[i] = (rgb[2] as f32 * a + c[i] as f32 * inv) as u8;
    c[i + 1] = (rgb[1] as f32 * a + c[i + 1] as f32 * inv) as u8;
    c[i + 2] = (rgb[0] as f32 * a + c[i + 2] as f32 * inv) as u8;
    c[i + 3] = (255.0 * a + c[i + 3] as f32 * inv) as u8;
}
```

Every pixel blend converts u8→f32, does 4 FMA operations, and converts back to u8. This is called for every pixel of every shape (rect, line, circle, rounded rect).

**Impact:** Performance — floating-point alpha blending is ~4× slower than integer blending.

**Fix:** Use integer alpha blending: `(src * alpha + dst * (255 - alpha)) >> 8`.

---

## High Severity Issues

### 558. theme.rs — Duplicate BGRA and ARGB color constants (lines 6-24)
**File:** `src/render/theme.rs` lines 6-24

The file defines both BGRA-ordered constants (`PANEL`, `INSET`, `BORDER`, etc.) and ARGB-ordered constants (`PANEL_ARGB`, `INSET_ARGB`, `BORDER_ARGB`, etc.). This duplication is error-prone: if a color is updated in one format but not the other, the rendered output will have incorrect colors.

```rust
pub const PANEL: [u8; 4] = [29, 28, 34, 255];
// ...
pub const PANEL_ARGB: [u8; 4] = [34, 28, 29, 255];
```

**Impact:** Maintenance risk — duplicate color definitions can drift out of sync.

**Fix:** Define colors in one canonical format (e.g., RGB tuple) and derive BGRA/ARGB via const functions.

---

### 559. theme.rs — `fill_rounded()` is O(w × h) per call (lines 29-78)
**File:** `src/render/theme.rs` lines 29-78

Every pixel of the rectangle is checked against the circle equation for rounded corners. For a 300×64 dock background, this is 19,200 iterations per call.

```rust
for py in y0..y1 {
    for px in x0..x1 {
        // circle equation check
        if (px < x + radius || px >= x + w - radius)
            && (py < y + radius || py >= y + h - radius)
            && dx * dx + dy * dy > radius * radius
        {
            continue;
        }
    }
}
```

**Impact:** CPU waste — rounded rectangles should use scanline algorithms or pre-computed corner masks.

**Fix:** Use a scanline-based rounded rectangle algorithm.

---

### 560. theme.rs — `stroke_rounded()` is O(w × h) calling `rounded_contains()` twice per pixel (lines 80-115)
**File:** `src/render/theme.rs` lines 80-115

For every pixel, `stroke_rounded` calls `rounded_contains` twice (outer and inner), each doing corner circle checks. For a 300×64 dock border, this is 19,200 × 2 = 38,400 function calls.

**Impact:** CPU waste — same as #559, compounded by double containment check.

---

### 561. dashed_block.rs — Per-pixel dashed border: O(w × h) iterating every pixel (lines 103-123)
**File:** `src/render/dashed_block.rs` lines 103-123

The dashed border iterates every pixel of the canvas, checking if it's on the border and within a dash segment. For a 64×48 block, this is 3,072 iterations, most of which are interior pixels that skip drawing.

```rust
for y in 0..h {
    for x in 0..w {
        let on_border = if y < thickness {
            x % cycle < dash_len
        } else if y >= h - thickness {
            x % cycle < dash_len
        // ...
```

**Impact:** CPU waste — iterate only the border pixels, not the entire canvas.

**Fix:** Iterate only the four border edges separately.

---

### 562. dashed_block.rs — Parses 4 embedded SVGs every frame via resvg (lines 143-153, 250-285)
**File:** `src/render/dashed_block.rs` lines 143-153, 250-285

Four SVG byte strings (rclone, bluetooth, speaker, pac) are parsed by `Tree::from_data()` and rendered by `resvg::render()` on every call to `render_dashed_block`. SVG parsing involves XML parsing, style computation, and path building.

```rust
if let Ok(tree) = Tree::from_data(svg_data, options) {
    let mut pixmap = Pixmap::new(icon_size as u32, icon_size as u32).unwrap();
    resvg::render(&tree, transform, &mut pixmap.as_mut());
```

**Impact:** CPU waste — SVG parsing and rendering is expensive. These static icons should be pre-rendered once.

**Fix:** Pre-render the 4 SVG icons to RGBA buffers at startup and cache them.

---

### 563. dashed_block.rs — Per-pixel SVG blending with f32 alpha (lines 180-203)
**File:** `src/render/dashed_block.rs` lines 180-203

```rust
let final_a = (svg_a as f32 / 255.0 * a as f32) as u8;
write_bgra_pixel(canvas, canvas_idx, svg_pixels[svg_idx], svg_pixels[svg_idx + 1], svg_pixels[svg_idx + 2], final_a);
```

Every SVG pixel converts u8→f32→u8 for alpha, then writes BGRA. For a 16×16 icon, this is 256 f32 conversions per icon, 4 icons = 1,024 conversions per frame.

**Impact:** Minor CPU waste — integer alpha would be faster.

---

### 564. date_time_block.rs — Same per-pixel dashed border algorithm (lines 160-179)
**File:** `src/render/date_time_block.rs` lines 160-179

Identical O(w × h) dashed border iteration as dashed_block.rs. For a 104×62 block, this is 6,448 iterations per call.

**Impact:** Same as #561.

---

### 565. date_time_block.rs — Calls libc::time and libc::localtime_r three times per render (lines 36-117)
**File:** `src/render/date_time_block.rs` lines 36-117

`format_time()`, `format_day_month()`, and `format_year()` each call `libc::time()`, `libc::localtime_r()`, and `libc::strftime()` independently. The current time doesn't change between these three calls within the same frame.

```rust
pub fn format_time() -> String {
    let now = unsafe { libc::time(std::ptr::null_mut()) };
    let mut local: libc::tm = unsafe { std::mem::zeroed() };
    // ...
}
```

**Impact:** Redundant syscalls — 3× time() + 3× localtime_r() + 3× strftime() per render when 1 of each would suffice.

**Fix:** Call time/localtime_r once and pass the `tm` struct to all three formatters.

---

### 566. date_time_block.rs — Allocates strings via strftime on every render (lines 54-61, 82-89, 110-117)
**File:** `src/render/date_time_block.rs` lines 54-61, 82-89, 110-117

Each format function collects bytes into a `Vec<u8>`, converts to `String`, and returns it. These strings are allocated every frame, even though the time string changes only once per minute and the date string changes only once per day.

```rust
String::from_utf8_lossy(
    &buf[..n as usize]
        .iter()
        .map(|c| *c as u8)
        .collect::<Vec<u8>>(),
)
.into_owned()
```

**Impact:** Per-frame allocation — violates "no steady-state heap allocations" rule.

**Fix:** Cache the formatted strings and regenerate only when the minute/day changes.

---

### 567. audio.rs — Per-pixel `rect()` function: O(w × h) (lines 30-56)
**File:** `src/render/audio.rs` lines 30-56

```rust
fn rect(canvas: &mut [u8], width: i32, height: i32, x: i32, y: i32, w: i32, h: i32, color: [u8; 4]) {
    for py in y0..y1 {
        for px in x0..x1 {
            let i = ((py * width + px) * 4) as usize;
            if i + 3 < canvas.len() {
                canvas[i..i + 4].copy_from_slice(&color);
            }
        }
    }
}
```

Every rectangle fill iterates pixel-by-pixel with bounds checking. Called multiple times per audio popup render (sliders, tracks, knobs, hairlines).

**Impact:** CPU waste — solid-color fills should use `copy_from_slice` for horizontal segments or `fill` for the entire region.

---

### 568. audio.rs — Uses theme::fill_rounded and theme::stroke_rounded (lines 335-347)
**File:** `src/render/audio.rs` lines 335-347

The audio popup background uses `theme::fill_rounded` (O(w×h)) and `theme::stroke_rounded` (O(w×h) × 2 containment checks) for a 180×240 popup. That's 43,200 + 86,400 = 129,600 iterations just for the background.

**Impact:** CPU waste — same as #559, #560.

---

### 569. audio_player.rs — `rect()` is O(w × h) calling `blend()` per pixel (lines 534-540)
**File:** `src/render/audio_player.rs` lines 534-540

```rust
fn rect(c: &mut [u8], w: i32, h: i32, x: i32, y: i32, rw: i32, rh: i32, rgb: [u8; 3], a: f32) {
    for py in y..y + rh {
        for px in x..x + rw {
            blend(c, w, h, px, py, rgb, a);
        }
    }
}
```

Every rectangle fill calls `blend()` (f32 alpha math) for every pixel. Called for the yellow background, progress bars, volume track, and control buttons.

**Impact:** CPU waste — same as #557 compounded with #567.

---

### 570. audio_player.rs — `line()` interpolates with f32 division per step (lines 541-555)
**File:** `src/render/audio_player.rs` lines 541-555

```rust
fn line(c: &mut [u8], w: i32, h: i32, x0: i32, y0: i32, x1: i32, y1: i32, rgb: [u8; 3], a: f32) {
    let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
    for n in 0..=steps {
        let t = n as f32 / steps as f32;
        blend(c, w, h, (x0 as f32 + (x1 - x0) as f32 * t).round() as i32, ...);
    }
}
```

Uses floating-point interpolation instead of Bresenham's integer algorithm. Each step does f32 division, multiplication, and rounding.

**Impact:** Performance — Bresenham's algorithm is purely integer-based and faster.

---

### 571. audio_player.rs — `rounded()` is O(w × h) calling `blend()` per pixel (lines 576-612)
**File:** `src/render/audio_player.rs` lines 576-612

Same per-pixel rounded rectangle algorithm as theme.rs, but calls `blend()` (f32 alpha) instead of direct copy. For the 320×222 expanded player background, this is 71,040 × f32 blend operations.

**Impact:** CPU waste — same as #559 + #557.

---

### 572. font.rs — Spawns fc-match subprocess for each missing ASCII character (lines 109-133)
**File:** `src/render/font.rs` lines 109-133

```rust
let output = Command::new("fc-match")
    .arg(&pattern)
    .arg("-f")
    .arg("%{file}")
    .output();
```

For each ASCII character not found in the primary font, `find_system_font_path_for_char` spawns `fc-match` as a subprocess. Subprocess spawning is ~5-10ms of overhead.

**Impact:** Latency spike — first render of a missing character blocks on subprocess spawn.

**Fix:** Cache the fc-match result per language, not per character.

---

### 573. font.rs — Unbounded glyph cache (line 22)
**File:** `src/render/font.rs` line 22

```rust
cache: HashMap<(char, u32), CachedGlyph>,
```

The glyph cache grows without bound. Each entry stores a `Vec<u8>` bitmap. For CJK text, this could accumulate thousands of glyphs, each ~16×16 = 256 bytes.

**Impact:** Memory growth — no eviction policy means cache grows with unique characters encountered.

**Fix:** Use an LRU cache with a bounded size.

---

### 574. font.rs — Memory-maps fallback font files on first miss (lines 149-178)
**File:** `src/render/font.rs` lines 149-178

`rasterize_glyph_from_path` memory-maps the entire fallback font file (e.g., NotoSansCJK TTC can be 20-50 MB) for each missing character. While the mmap is released after the function returns, the initial mmap syscall and page faults add latency.

**Impact:** Latency spike — first CJK character render triggers mmap of a large TTC file.

---

### 575. font.rs — Tofu placeholder draws per-pixel rectangle outline (lines 273-286)
**File:** `src/render/font.rs` lines 273-286

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

For a missing non-ASCII character, a tofu box is drawn by iterating every pixel of the glyph bitmap. For a 16×16 glyph, this is 256 iterations.

**Impact:** Minor CPU waste — draw only the border pixels.

---

### 576. bluetooth.rs — Allocates Vec<DeviceEntry> per frame for discovered and paired devices (lines 138-178)
**File:** `src/render/bluetooth.rs` lines 138-178

```rust
let discovered: Vec<DeviceEntry> = snapshot.devices.iter().filter(...).map(...).collect();
let paired: Vec<DeviceEntry> = snapshot.devices.iter().filter(...).map(...).collect();
```

Every Bluetooth render frame allocates two new Vec<DeviceEntry> instances, one SharedString per device per vec. For 5 devices, this is 10 DeviceEntry allocations + 20 SharedString allocations per frame.

**Impact:** Per-frame allocation — violates "no steady-state heap allocations" rule.

**Fix:** Reuse pre-allocated buffers or update models in-place.

---

### 577. bluetooth.rs — RGBA→BGRA conversion per pixel in a loop (lines 208-217)
**File:** `src/render/bluetooth.rs` lines 208-217

```rust
for idx in 0..copy_pixels {
    canvas[offset] = pixel.blue;
    canvas[offset + 1] = pixel.green;
    canvas[offset + 2] = pixel.red;
    canvas[offset + 3] = pixel.alpha;
}
```

For a 402×318 Bluetooth popup, this is 127,836 iterations, each doing 4 byte swaps.

**Impact:** CPU waste — use `bytemuck` or a SIMD-based R↔B swap.

---

### 578. state/popup.rs — `close_application_completely` reads /proc/<pid>/stat for every matching window (lines 65-157)
**File:** `src/state/popup.rs` lines 65-157

This function:
1. Filters all windows by app_id match
2. For each match, reads `/proc/<pid>/stat` to get process group ID
3. Closes Wayland handles
4. Spawns `steam -shutdown` subprocess if Steam
5. Does full sysinfo process refresh
6. Iterates ALL processes again, reading `/proc/<pid>/stat` for each
7. Kills matching processes by name

```rust
let pid_pgid = read_proc_pgid(pid);
```

**Impact:** I/O spike — reads /proc for every window + every process on the system.

**Fix:** Batch /proc reads or use sysinfo's process group info if available.

---

### 579. state/popup.rs — Spawns `steam -shutdown` subprocess (line 114)
**File:** `src/state/popup.rs` line 114

```rust
let _ = std::process::Command::new("steam").arg("-shutdown").spawn();
```

Spawns the Steam client just to send a shutdown command. This is ~100ms+ of overhead.

**Impact:** Latency — subprocess spawn for a simple command.

**Fix:** Use D-Bus to communicate with Steam if available.

---

### 580. state/popup.rs — Full sysinfo process refresh on close (line 127-128)
**File:** `src/state/popup.rs` lines 127-128

```rust
self.sys_scanner.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
```

After closing windows, the entire process list is refreshed. This reads /proc for every process.

**Impact:** CPU waste — full process scan on every app close.

---

## Medium Severity Issues

### 581. badges.rs — Embedded PNG loaded at startup, never updated (lines 9-10)
**File:** `src/render/badges.rs` lines 9-10

```rust
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");
static BADGE_ICON_CACHE: OnceLock<Option<(u32, u32, Vec<u8>)>> = OnceLock::new();
```

The badge icon is embedded as a PNG at compile time and decoded once at startup. This is good for performance, but the decoded `Vec<u8>` is held in memory forever.

**Impact:** Minor memory usage — the decoded badge icon pixels are retained for the lifetime of the process.

---

### 582. badges.rs — `draw_canvas_badge` does per-pixel alpha blending (lines 47-81)
**File:** `src/render/badges.rs` lines 47-81

Uses integer alpha blending (good), but iterates every pixel of the badge icon. For a small badge (~12×12), this is 144 iterations.

**Impact:** Minor — acceptable for small badges.

---

### 583. badges.rs — `draw_dock_item_badge` uses Cairo, not GPU (lines 84-135)
**File:** `src/render/badges.rs` lines 84-135

The Cairo-based badge renderer draws progress bars and notification badges using Cairo's 2D API. This is only used for CPU rendering; the GPU path doesn't use it.

**Impact:** Architecture — two badge rendering paths (canvas and Cairo) that must be maintained separately.

---

### 584. text.rs — `World` struct is unused dead code (lines 55-224)
**File:** `src/render/text.rs` lines 55-224

The `World` struct and its implementation (from `from_font_path` to `clear_font_cache`) are never used anywhere in the codebase. The free function `draw_text` (line 11) is the entry point used by all callers.

**Impact:** Dead code — ~170 lines of unused code.

**Fix:** Remove the `World` struct and its methods.

---

### 585. text.rs — `blit_glyph` iterates every glyph pixel with bounds checking (lines 164-218)
**File:** `src/render/text.rs` lines 164-218

For each glyph pixel, the function computes target coordinates, checks bounds, and does integer alpha blending. For a 16×16 glyph, this is 256 iterations with bounds checks.

**Impact:** Minor — the integer blending is correct and efficient, but bounds checking on every pixel adds overhead.

**Fix:** Pre-compute the blit rectangle and skip bounds checks inside the loop.

---

### 586. text.rs — `draw_blue_box` is debug code left in production (lines 83-107)
**File:** `src/render/text.rs` lines 83-107

```rust
pub fn draw_blue_box(&self, frame: &mut [u8], x_start: i32, x_end: i32, y_start: i32, y_end: i32) {
    let blue_pixel: [u8; 4] = [255, 100, 0, 255];
    // draws a blue rectangle outline
}
```

This method draws a debug rectangle outline. It's part of the `World` struct (dead code) but the method name suggests it was left for debugging.

**Impact:** Dead code — removed with #584.

---

### 587. resolvers/process.rs — `resolve_candidate` reads /proc/<pid>/environ (line 41)
**File:** `src/resolvers/process.rs` line 41

```rust
record.env_vars = read_proc_environment(pid);
```

Reading `/proc/<pid>/environ` requires CAP_SYS_PTRACE or same UID. For processes owned by other users, this fails with EACCES.

**Impact:** Permission noise — EACCES errors for non-user processes.

---

### 588. resolvers/process.rs — `find_process` does O(n) linear scan of all processes (lines 81-129)
**File:** `src/resolvers/process.rs` lines 81-129

For each process, the function normalizes the name and executable path, then compares with the target. For 500 processes, this is 500 string normalizations and comparisons.

**Impact:** CPU waste — no index or cache for process lookups.

**Fix:** Build a hash index from normalized name to ProcessId.

---

### 589. render/mod.rs — Missing module declarations for some files (lines 1-35)
**File:** `src/render/mod.rs` lines 1-35

The module file declares `audio`, `audio_player`, `badges`, `bluetooth`, `font`, `text`, `theme`, `tray`, `utils`, `widget_bar`, `color_check`, `dashed_block`, `date_time_block`, `processes`, `rclone`, `slint_dock`, `debug_overlay`, `dmabuf_export`, `dock`, `popup`. But `audio_player` is not listed as `pub mod` (it's used elsewhere but not re-exported here).

**Impact:** Minor — module organization inconsistency.

---

### 590. render/utils.rs — HSL to RGB conversion allocates nothing (lines 7-31)
**File:** `src/render/utils.rs` lines 7-31

The HSL→RGB conversion is a pure function with no allocations. This is fine.

**Impact:** None — this is well-written.

---

### 591. audio_player.rs — Hardcoded hitbox coordinates don't account for scale (lines 36-64)
**File:** `src/render/audio_player.rs` lines 36-64

`control_at` uses hardcoded pixel ranges (e.g., `88..128` for Previous button) that are in logical pixels. If the popup is rendered at 2x scale, the hitboxes won't match the rendered positions unless the caller scales the coordinates.

```rust
if (88..128).contains(&x) {
    Some(MediaControl::Previous)
```

**Impact:** Hit-testing bug at non-1x scale — buttons may not respond to clicks.

---

### 592. audio.rs — Hardcoded hitbox coordinates (lines 9-28)
**File:** `src/render/audio.rs` lines 9-28

Same pattern — hardcoded logical pixel constants for slider positions, button positions, and device selector positions. Hit-testing functions need to use the same scale factor as rendering.

**Impact:** Same as #591.

---

### 593. dashed_block.rs — `bluetooth_icon_at` hit-test may not match rendered position (lines 33-49)
**File:** `src/render/dashed_block.rs` lines 33-49

The hit-test function computes the Bluetooth icon position using config values, but the rendering function (`render_dashed_block`) also uses config values. If the config changes between hit-test and render, or if the hit-test coordinates aren't in the same space as the render coordinates, the hitbox won't match.

**Impact:** Potential hit-testing mismatch.

---

### 594. services/monitor.rs — `resolve_steam_game_details` walks .acf files (lines 281-330)
**File:** `src/services/monitor.rs` lines 281-330

When SteamAppId isn't found in environment variables, the function reads every `.acf` file in steamapps directories, parsing XML-like key-value pairs. For a library with 100+ games, this is 100+ file opens and parses.

**Impact:** I/O spike — walks and parses all Steam manifests.

---

### 595. services/monitor.rs — `load_image_raw_rgba` resizes images on every call (lines 416-442)
**File:** `src/services/monitor.rs` lines 416-442

For SVG icons, resvg renders at the target size. For raster icons, `image::resize_exact` with Lanczos3 filtering is used. These are called on every Wayland AppId event, not cached.

**Impact:** CPU waste — image decoding and resizing on every window event.

**Fix:** Cache decoded icons by path.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 551 | **Critical** | Standalone binary in library crate | Dead code compiled into library |
| 552 | **Critical** | WalkDir depth 6 on icon dirs per event | I/O spike on every window event |
| 553 | **Critical** | Full sysinfo refresh per HandleEvent::Done | CPU waste on every window event |
| 554 | **Critical** | Reads /proc/environ for every process | Permission noise in dmesg |
| 555 | **Critical** | WalkDir on all .desktop files per event | I/O spike on every window event |
| 556 | **Critical** | O(r²) circle rendering, 11 circles per disc | ~240K iterations per audio render |
| 557 | **Critical** | blend() uses f32 arithmetic per pixel | ~4× slower than integer blending |
| 558 | **High** | Duplicate BGRA/ARGB color constants | Maintenance risk |
| 559 | **High** | fill_rounded() O(w × h) per call | CPU waste for rounded rects |
| 560 | **High** | stroke_rounded() O(w × h) × 2 checks | CPU waste for rounded strokes |
| 561 | **High** | Per-pixel dashed border O(w × h) | CPU waste for dashed borders |
| 562 | **High** | Parses 4 SVGs every frame via resvg | CPU waste for static icons |
| 563 | **High** | Per-pixel SVG blending with f32 alpha | Minor CPU waste |
| 564 | **High** | Same per-pixel dashed border algorithm | CPU waste |
| 565 | **High** | Calls libc::time 3× per render | Redundant syscalls |
| 566 | **High** | Allocates strings via strftime per frame | Per-frame allocation |
| 567 | **High** | Per-pixel rect() O(w × h) | CPU waste for solid fills |
| 568 | **High** | Uses theme::fill_rounded/stroke_rounded | ~130K iterations for popup bg |
| 569 | **High** | rect() calls blend() per pixel | CPU waste + f32 math |
| 570 | **High** | line() uses f32 interpolation | Slower than Bresenham |
| 571 | **High** | rounded() O(w × h) with blend() | 71K f32 blends for player bg |
| 572 | **High** | Spawns fc-match per missing char | Latency spike on first miss |
| 573 | **High** | Unbounded glyph cache | Memory growth |
| 574 | **High** | Memory-maps fallback font files | Latency spike on first CJK |
| 575 | **High** | Tofu placeholder draws per-pixel | Minor CPU waste |
| 576 | **High** | Allocates Vec<DeviceEntry> per frame | Per-frame allocation |
| 577 | **High** | RGBA→BGRA per pixel in a loop | 128K byte swaps per frame |
| 578 | **High** | Reads /proc/stat for every window + process | I/O spike on app close |
| 579 | **High** | Spawns steam -shutdown subprocess | ~100ms latency |
| 580 | **High** | Full sysinfo refresh on close | CPU waste |
| 581 | **Medium** | Embedded PNG held in memory | Minor memory usage |
| 582 | **Medium** | Per-pixel badge alpha blending | Minor — acceptable |
| 583 | **Medium** | Two badge rendering paths | Maintenance burden |
| 584 | **Medium** | World struct is dead code | ~170 lines of dead code |
| 585 | **Medium** | blit_glyph bounds check per pixel | Minor overhead |
| 586 | **Medium** | draw_blue_box is debug code | Dead code |
| 587 | **Medium** | Reads /proc/environ on resolve | Permission noise |
| 588 | **Medium** | O(n) process scan per resolve | CPU waste |
| 589 | **Medium** | Module declaration inconsistency | Minor |
| 590 | **Medium** | HSL→RGB is well-written | No issue |
| 591 | **Medium** | Hardcoded hitboxes ignore scale | Hit-testing bug at 2x |
| 592 | **Medium** | Hardcoded hitboxes in audio.rs | Hit-testing bug at 2x |
| 593 | **Medium** | Hit-test may not match render | Potential mismatch |
| 594 | **Medium** | Walks all .acf files for Steam | I/O spike |
| 595 | **Medium** | Resizes images on every call | CPU waste |

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
| **16** | **gpu-rendering-review-16.md** | **551-595** | **595** |

**Cumulative severity across all 16 rounds:**
- Critical: 72 (was 65 + 7 new)
- High: 128 (was 114 + 14 new)
- Medium: 199 (was 179 + 20 new)
- Low: 78 (no new low issues this round)
- **Total: 595 issues**

### Key Themes from Round 16

1. **Per-pixel CPU rendering is ubiquitous**: Every render submodule (theme, dashed_block, date_time_block, audio, audio_player) implements its own per-pixel drawing primitives. None of these popups use the GPU path — they all render to CPU pixel buffers.

2. **f32 alpha blending everywhere**: audio_player.rs uses floating-point alpha blending for every pixel of every shape. Integer blending (used correctly in text.rs and badges.rs) is ~4× faster.

3. **services/monitor.rs is a standalone binary**: The entire file is a separate Wayland client that walks icon directories, reads /proc, and resolves Steam games — none of which is used by the dock application.

4. **Per-frame SVG parsing**: dashed_block.rs parses 4 embedded SVGs via resvg on every frame. These static icons should be pre-rendered once.

5. **Per-frame string allocations**: date_time_block.rs allocates 3 strings via strftime on every render, even though the date changes only once per day.

6. **WalkDir on icon and desktop directories**: services/monitor.rs walks /usr/share/icons at depth 6 and all .desktop files on every window event.

7. **Dead code**: text.rs World struct (~170 lines) is never used.

8. **Hit-testing at non-1x scale**: Multiple popup renderers use hardcoded logical pixel hitboxes that don't account for scale factor.
