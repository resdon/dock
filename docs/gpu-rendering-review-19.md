# GPU Rendering Code Review — Round 19 (Error Handling, Geometry, Models, Cache, Resolvers)

This is a continuation of the GPU rendering code review. Rounds 1–18 covered all `wgpu-api/` source files, all `src/` application-layer modules, `src/app/` submodules, project infrastructure, documentation, render submodules, state modules, services, physics interaction, groups, input handling, popup rendering, and tray rendering (issues 1–670). This round covers: `src/error.rs`, `src/errors/` (gpu, memory, pipewire, system, ui, wayland, mod), `src/geometry/` (mod, primitives, popup), `src/models.rs`, `src/models/system.rs`, `src/cache/` (mod, persistence, icon_indexer), and `src/resolvers/` (icon_list, icon_stage, steam, window, capabilities, distribution, environment) — issues 671–710.

---

## Critical Issues

### 671. cache/mod.rs — `refresh_cache_on_launch` deletes entire icon cache on every startup (lines 16-23)
**File:** `src/cache/mod.rs` lines 16-23

On every launch, the dock deletes `~/.config/dock/cached_icons/` and recreates it. This means all cached icons are lost on every restart, forcing a full re-download or re-resolution of every icon. The function name `refresh_cache_on_launch` suggests intentional behavior, but it defeats the purpose of caching entirely.

```rust
pub fn refresh_cache_on_launch() {
    let cache_dir = get_cache_dir();
    if cache_dir.exists() {
        if let Err(_e) = fs::remove_dir_all(&cache_dir) {}
    }
    let _ = fs::create_dir_all(&cache_dir);
}
```

**Impact:** I/O waste — every launch forces full icon re-resolution, negating the cache's purpose. The icon indexer (icon_indexer.rs) rebuilds the icon list, but the actual icon pixel cache is destroyed.

**Fix:** Remove this function or make it conditional (e.g., only clear on version mismatch).

---

### 672. resolvers/steam.rs — Iterates ALL system processes to find Steam AppId (lines 55-102)
**File:** `src/resolvers/steam.rs` lines 55-102

`resolve_appid_from_process` iterates every process in sysinfo, walking the parent chain for each one, then reads `/proc/<pid>/environ` and `/proc/<pid>/cmdline` for descendants of the entry PID. On a system with 300 processes, this is 300 parent-chain walks plus potentially dozens of `/proc` reads.

```rust
for (c_pid, c_proc) in sys_scanner.processes() {
    let mut current = c_proc.parent();
    // ... walks parent chain
    if is_descendant {
        if let Ok(env_data) = fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
```

**Impact:** CPU and I/O spike — full process tree traversal on every Steam resolution attempt.

**Fix:** Only check the entry PID and its direct children, not all processes.

---

### 673. resolvers/steam.rs — `resolve_appid_from_gamescope` reads `/proc/<pid>/environ` for ALL processes (lines 104-132)
**File:** `src/resolvers/steam.rs` lines 104-132

When the target app or window title contains "gamescope", the function iterates ALL processes and reads `/proc/<pid>/environ` for each one. This is extremely broad — any process name containing "gamescope" triggers a full `/proc` scan.

```rust
for c_pid in sys_scanner.processes().keys() {
    if let Ok(env_data) = fs::read_to_string(format!("/proc/{}/environ", c_pid.as_u32())) {
```

**Impact:** Permission noise — hundreds of EACCES errors for non-user processes. I/O spike on the event thread.

**Fix:** Only check the gamescope process itself, not all processes.

---

### 674. resolvers/steam.rs — WalkDir depth 6 on hicolor icon theme (lines 242-256)
**File:** `src/resolvers/steam.rs` lines 242-256

When looking for a Steam icon in the hicolor theme, `WalkDir::new(&theme_dir).max_depth(6)` traverses the entire icon theme tree. A typical hicolor theme has 10+ size directories × thousands of icons.

```rust
for entry in WalkDir::new(&theme_dir)
    .max_depth(6)
    .into_iter()
    .filter_map(|e| e.ok())
```

**Impact:** I/O spike — thousands of directory entries walked for a single icon lookup.

**Fix:** Use the icon list cache or linicon library instead of WalkDir.

---

### 675. resolvers/icon_list.rs — Duplicate of cache/icon_indexer.rs (entire file)
**File:** `src/resolvers/icon_list.rs`

This file contains `get_icon_list_path()` and `spawn_startup_indexer()` which are nearly identical to the functions in `src/cache/icon_indexer.rs`. The only differences are:
- icon_list.rs uses 4 workers vs icon_indexer.rs uses 2 workers
- icon_indexer.rs sets nice priority via `libc::setpriority`, icon_list.rs does not
- icon_indexer.rs checks cache age before rebuilding, icon_list.rs always rebuilds
- icon_indexer.rs uses atomic rename, icon_list.rs writes directly

Both modules export the same function names, creating confusion about which one is actually used.

**Impact:** Maintenance risk — two implementations of the same logic diverging over time. One of them lacks priority lowering and cache age checking.

**Fix:** Consolidate into a single implementation in cache/icon_indexer.rs.

---

## High Severity Issues

### 676. models/system.rs — `ProcessRecord` stores `env_vars: HashMap<String, String>` (line 625)
**File:** `src/models/system.rs` line 625

Every process record stores the full environment as a HashMap. For a process with 50 env vars averaging 50 bytes each, this is ~2.5 KB per process. For 300 processes, that's ~750 KB of rarely-used environment data in memory.

**Impact:** Memory waste — environment data is stored for all processes but rarely accessed.

**Fix:** Lazy-load environment data only when needed.

---

### 677. models/system.rs — `WindowRecord` has redundant boolean fields (lines 684-688)
**File:** `src/models/system.rs` lines 684-688

```rust
pub is_activated: bool,
pub is_minimized: bool,
pub is_foreground: bool,
pub is_background: bool,
pub is_locked: bool,
```

These booleans duplicate information already in `status: WindowStatus` (Active, Minimized, Background, Hidden, Unknown). `is_foreground` and `is_background` are derivable from `is_activated` and `is_minimized`.

**Impact:** Memory waste and consistency risk — 5 booleans that can drift out of sync with the enum status.

**Fix:** Derive these from `WindowStatus` via accessor methods.

---

### 678. models/system.rs — `ApplicationStatus` has redundant boolean fields (lines 443-453)
**File:** `src/models/system.rs` lines 443-453

```rust
pub running: bool,
pub foreground: bool,
pub background: bool,
pub hidden: bool,
pub locked: bool,
pub suspended: bool,
pub paused: bool,
pub stuck: bool,
pub attention: bool,
```

`foreground` and `background` are mutually exclusive and derivable. `suspended`, `paused`, and `stuck` are rarely used. This struct has 9 booleans where 3-4 would suffice.

**Impact:** Cognitive complexity and memory waste.

---

### 679. models/system.rs — `ProcessStatus` has 10 boolean fields (lines 471-482)
**File:** `src/models/system.rs` lines 471-482

Similar to ApplicationStatus, `ProcessStatus` has 10 independent booleans: running, foreground, background, sandboxed, locked, suspended, paused, stuck, responding, exited. Many are derivable or mutually exclusive.

**Impact:** Same as #678.

---

### 680. models.rs — `TrayItem` has 15 fields (lines 41-61)
**File:** `src/models.rs` lines 41-61

`TrayItem` stores key, service_name, object_path, interface_name, id, title, category, status, icon_name, icon_theme_path, icon (Option<TrayIcon>), tooltip, item_is_menu, and menu_path. The `icon` field holds a `TrayIcon` with `rgba: Vec<u8>`, duplicating icon pixel data per tray item.

**Impact:** Memory waste — each tray icon holds its own pixel buffer.

---

### 681. models.rs — `MediaWindowLink` has 15 fields (lines 326-346)
**File:** `src/models.rs` lines 326-346

`MediaWindowLink` stores handle, uri, window_rect, content_rect, video_rect, title, is_playing, muted, volume, position_ms, duration_ms, and source. Most of these are `Option<T>`, adding overhead. This struct is embedded in `WindowRecord`, meaning every window can carry this overhead.

**Impact:** Memory overhead per window — 15 fields × size of Option for every window.

---

### 682. geometry/primitives.rs — `to_local_coords` casts f64 to i32, losing precision (lines 18-22)
**File:** `src/geometry/primitives.rs` lines 18-22

```rust
pub fn to_local_coords(&self, bounds_x: f64, bounds_y: f64, scale: f64) -> (i32, i32) {
    let local_x = ((self.x * scale) - bounds_x).max(0.0) as i32;
    let local_y = ((self.y * scale) - bounds_y).max(0.0) as i32;
    (local_x, local_y)
}
```

The result is truncated to i32, losing sub-pixel precision. For fractional scale factors (e.g., 1.25), this can cause off-by-one errors in hit testing.

**Impact:** Hit testing inaccuracy — sub-pixel positions are truncated, causing edge-case misses.

---

### 683. geometry/popup.rs — Hardcoded popup dimensions (lines 26-30)
**File:** `src/geometry/popup.rs` lines 26-30

```rust
pub const BASE_ITEM_HEIGHT: i32 = 29;
pub const BASE_MENU_WIDTH: i32 = 144;
pub const BASE_WINDOW_LIST_WIDTH: i32 = 160;
pub const WINDOW_LIST_FRAME_HEIGHT: i32 = 240;
```

Popup dimensions are hardcoded constants. The comment says "Legacy constants — kept for backward compatibility. Use config values instead." but no config values exist.

**Impact:** Inflexibility — popup sizes cannot be customized without code changes.

---

### 684. cache/persistence.rs — Hardcoded default pinned apps (lines 30-35)
**File:** `src/cache/persistence.rs` lines 30-35

```rust
vec![
    "AmneziaVPN".into(),
    "steam".into(),
    "org.qbittorrent.qBittorrent".into(),
]
```

When the pins.json file doesn't exist, the dock defaults to 3 specific applications. These are developer-specific defaults, not appropriate for a general-purpose dock.

**Impact:** Poor user experience — new users see irrelevant pinned apps.

**Fix:** Use an empty default or read from a system-wide config.

---

### 685. cache/persistence.rs — `save_pinned_apps` silently ignores errors (lines 38-46)
**File:** `src/cache/persistence.rs` lines 38-46

Both `fs::create_dir_all` and `fs::write` results are silently ignored with `let _ =`. If the save fails, the user's pin list changes are lost with no warning.

```rust
let _ = fs::create_dir_all(dir);
if let Ok(data) = serde_json::to_string(pinned) {
    let _ = fs::write(path, data);
} else {
}
```

The empty `else` block is also dead code.

**Impact:** Silent data loss — pin list changes may be silently discarded.

---

### 686. resolvers/icon_list.rs — `search_icon_list_file` spawns 4 threads per query (lines 166-269)
**File:** `src/resolvers/icon_list.rs` lines 166-269

Every icon lookup spawns up to 4 threads, each opening the same file independently and seeking to a different offset. For a dock with 10 icons, this is 40 thread spawns and 40 file opens per resolution cycle.

```rust
let num_workers = 4.min((file_size / (16 * 1024)).max(1) as usize);
```

**Impact:** Thread overhead — spawning threads for a text search is disproportionate. A sequential scan or binary search would be faster for small files.

**Fix:** Use a single-threaded search or a pre-built index (HashMap).

---

### 687. resolvers/icon_list.rs — `search_icon_list_file` uses `to_lowercase()` per line (lines 144, 230)
**File:** `src/resolvers/icon_list.rs` lines 144, 230

Every line of the icon list is lowercased for comparison. For a 100,000-line icon list, this is 100,000 String allocations per search.

**Impact:** Per-search heap allocation — violates "no steady-state heap allocations" rule.

**Fix:** Store the icon list in lowercase or use case-insensitive comparison without allocation.

---

### 688. error.rs — `AppError` variants store `String` instead of specific error types (lines 42-81)
**File:** `src/error.rs` lines 42-81

Most `AppError` variants wrap `String` messages, losing the original error type and context. For example, `ServiceError(String)` could have been `ServiceError(std::io::Error)` or a specific service error type.

```rust
#[error("Service error: {0}")]
ServiceError(String),
```

**Impact:** Error information loss — callers cannot distinguish between different causes of the same error variant.

---

### 689. errors/mod.rs — `Recoverable` trait for `AppError` doesn't cover all variants (lines 55-76)
**File:** `src/errors/mod.rs` lines 55-76

The `is_recoverable` implementation for `AppError` only covers ServiceError, ListenerError, IconError, and ProcessError. WgpuApi, InitFailed, ConfigurationError, StateError, EventError, ResolverError, DbusError, and SystemMonitorError all return `false`. However, `recovery_strategy` provides a strategy for DbusError even though `is_recoverable` returns `false`.

**Impact:** Inconsistency — DbusError has a recovery strategy but is marked non-recoverable.

---

### 690. errors/gpu.rs — `GpuAppError::is_recoverable` marks FrameTimeout as non-recoverable (lines 48-55)
**File:** `src/errors/gpu.rs` lines 48-55

A frame timeout (GPU hang) is marked as non-recoverable, but the recovery strategy says "Reset GPU command encoder and retry frame". This contradicts the `is_recoverable` result.

**Impact:** Logic error — a recoverable error is marked non-recoverable.

---

## Medium Severity Issues

### 691. cache/icon_indexer.rs — Uses `unsafe { libc::setpriority }` (lines 23-25, 83-85)
**File:** `src/cache/icon_indexer.rs` lines 23-25, 83-85

```rust
unsafe {
    libc::setpriority(libc::PRIO_PROCESS, 0, 10);
}
```

The `unsafe` block calls `libc::setpriority` without checking the return value. On failure, the priority change is silently ignored. The PID 0 means "calling process", which is correct for a thread.

**Impact:** Silent failure — priority change may not take effect.

---

### 692. cache/icon_indexer.rs — `_start` variable unused (line 28)
**File:** `src/cache/icon_indexer.rs` line 28

```rust
let _start = Instant::now();
```

The timer is started but never used. Dead code placeholder for profiling.

---

### 693. cache/mod.rs — `save_cached_icon` ignores write errors (lines 41-50)
**File:** `src/cache/mod.rs` lines 41-50

All `write_all` calls use `let _ =`, silently ignoring errors. If the disk is full or permissions are wrong, the cache write fails silently.

**Impact:** Silent cache corruption — partial writes leave incomplete files.

---

### 694. cache/mod.rs — `load_cached_icon` reads entire file into memory (line 85)
**File:** `src/cache/mod.rs` line 85

```rust
file.read_to_end(&mut rgba).ok()?;
```

The entire icon file is read into a Vec. For a 512×512 RGBA icon, this is 1 MB. This is fine for small icons but could be large for high-resolution icons.

**Impact:** Memory usage — large icons consume proportional RAM.

---

### 695. models/system.rs — `ApplicationRecord` has 17 fields (lines 723-739)
**File:** `src/models/system.rs` lines 723-739

`ApplicationRecord` stores id, identity, binary, platform, runtime, packaging, distribution, installation, launch, status, priority, processes, windows, capabilities, resolution_confidence, and resolution_trace. Many of these are nested structs with their own fields.

**Impact:** Cognitive complexity — understanding an application's state requires navigating 17 nested fields.

---

### 696. models/system.rs — `ResolutionTrace::add` uses `contains()` on Vec (line 295)
**File:** `src/models/system.rs` line 295

```rust
if !self.evidence.contains(&evidence) {
    self.evidence.push(evidence);
}
```

`contains()` on Vec is O(n). With 13 possible `ResolutionEvidence` variants, this is at most 13 comparisons, but it's still a linear scan.

**Impact:** Minor — use a HashSet or bitset for O(1) lookup.

---

### 697. resolvers/capabilities.rs — `can_open_settings` uses substring match (lines 33-36)
**File:** `src/resolvers/capabilities.rs` lines 33-36

```rust
application.capabilities.can_open_settings = actions.iter().any(|a| {
    let n = a.to_lowercase();
    n.contains("settings") || n.contains("preferences")
});
```

Allocates a lowercase String for each desktop action. The substring match is broad — "settings" could match "advanced-settings" or "no-settings".

**Impact:** Per-resolution allocation and broad matching.

---

### 698. resolvers/distribution.rs — Path matching uses `contains()` (lines 51-90)
**File:** `src/resolvers/distribution.rs` lines 51-90

Distribution detection uses `path.contains("steam")`, `path.contains("lutris")`, etc. The path is lowercased once, but `contains` is a broad substring match. A path like `/usr/bin/steamcleaner` would match "steam".

**Impact:** False positive — paths containing the keyword but not belonging to the distribution will be misidentified.

---

### 699. resolvers/environment.rs — `packaging.kind` defaults to SystemPackage (line 94)
**File:** `src/resolvers/environment.rs` line 94

If none of the packaging checks match (Flatpak, Snap, AppImage), the default is `PackagingKind::SystemPackage`. However, the `else` branch also sets `packaging.kind = PackagingKind::Unknown` in the Default impl. The code overrides this to SystemPackage, which may be incorrect for source-built or manually installed binaries.

**Impact:** Misclassification — source-built binaries are classified as system packages.

---

### 700. resolvers/icon_stage.rs — `resolve` clones application identity (line 18)
**File:** `src/resolvers/icon_stage.rs` line 18

```rust
let id = application.identity.identifier.clone();
```

The identifier String is cloned before use. This is a per-application allocation.

---

### 701. errors/wayland.rs — `EventQueueBlocked` is non-recoverable (lines 44-49)
**File:** `src/errors/wayland.rs` lines 44-49

A blocked event queue is marked non-recoverable, but the recovery strategy says "Check for blocking operations in event handlers". This is a recoverable condition — the blocking operation can be identified and fixed.

**Impact:** Inconsistency — like #690, a recoverable error is marked non-recoverable.

---

### 702. errors/pipewire.rs — `StreamUnderrun` recovery says "reduce frame rate" (line 69)
**File:** `src/errors/pipewire.rs` line 69

```rust
PipeWireAppError::StreamUnderrun => "Reduce frame rate or increase buffer size",
```

The dock uses zero idle rendering (per ADR 005). Reducing frame rate is not a valid recovery strategy for an event-driven dock.

**Impact:** Misleading recovery strategy — the dock doesn't have a fixed frame rate to reduce.

---

### 703. geometry/popup.rs — `PopupType::Custom` takes raw dimensions (line 9)
**File:** `src/geometry/popup.rs` line 9

```rust
Custom { width: i32, height: i32 },
```

Custom popup dimensions are in logical pixels, but the function doesn't document this. The `compute_bounds` function treats them differently from ContextMenu and WindowList — no item height or extra height is added.

**Impact:** API ambiguity — callers need to know the coordinate space.

---

### 704. models.rs — `TrayActionKind::Scroll` uses `&'static str` for orientation (line 85)
**File:** `src/models.rs` line 85

```rust
Scroll { delta: i32, orientation: &'static str },
```

The orientation is a string literal ("horizontal" or "vertical"). This should be an enum for type safety.

**Impact:** Type safety — any string can be passed, not just valid orientations.

---

### 705. cache/icon_indexer.rs — Recursive `scan_directory_to_writer` can stack overflow (lines 131-147)
**File:** `src/cache/icon_indexer.rs` lines 131-147

The recursive directory scan has no depth limit. A deeply nested directory structure (e.g., symlinks forming a cycle) could cause a stack overflow.

**Impact:** Crash risk — infinite recursion on cyclic symlinks.

---

### 706. resolvers/steam.rs — `find_appid_by_manifest` parses .acf files with string splitting (lines 134-187)
**File:** `src/resolvers/steam.rs` lines 134-187

Steam's .acf files are key-value pairs in a custom format. The parsing uses `split('"').nth(3)` which is fragile — it assumes exactly 3 quote-delimited segments before the value.

**Impact:** Parsing fragility — malformed .acf files will produce incorrect results.

---

### 707. resolvers/window.rs — `order_index` uses protocol_id as u64 (line 41)
**File:** `src/resolvers/window.rs` line 41

```rust
window.order_index = id.0.protocol_id() as u64;
```

The Wayland protocol ID is used as the window's order index. Protocol IDs are assigned sequentially by the compositor and don't necessarily reflect window creation order or z-order.

**Impact:** Incorrect ordering — windows may be sorted by protocol ID instead of actual z-order.

---

### 708. errors/memory.rs — `LeakDetected` is non-recoverable (lines 43-48)
**File:** `src/errors/memory.rs` lines 43-48

A detected memory leak is marked non-recoverable. However, the recovery strategy says "Investigate and fix the memory leak source", which implies the error is actionable.

**Impact:** Inconsistency — like #690 and #701.

---

### 709. models/system.rs — `OutputRecord` has `scale_factor: f64` defaulting to 1.0 (line 795)
**File:** `src/models/system.rs` line 795

```rust
scale_factor: 1.0,
```

The default scale factor is hardcoded to 1.0. On a high-DPI display, this will cause incorrect sizing until the scale factor is updated.

**Impact:** Visual glitch — surfaces may be sized incorrectly on first render.

---

### 710. resolvers/icon_list.rs — Shard files not cleaned up on panic (lines 60-92)
**File:** `src/resolvers/icon_list.rs` lines 60-92

If a worker thread panics during shard writing, the shard file is left on disk. The main thread doesn't check for worker failures before merging.

**Impact:** Disk clutter — orphaned shard files accumulate.

---

## Summary of Issues by Severity

 # | Severity | Issue | Impact |
---|----------|-------|--------|
 671 | **Critical** | Cache deleted on every launch | Defeats purpose of caching |
 672 | **Critical** | Steam resolver iterates all processes | CPU and I/O spike |
 673 | **Critical** | Gamescope resolver reads /proc for all PIDs | Permission noise, I/O spike |
 674 | **Critical** | WalkDir depth 6 on hicolor theme | I/O spike per icon lookup |
 675 | **Critical** | Duplicate icon indexer code | Maintenance risk |
 676 | **High** | ProcessRecord stores full env HashMap | 750 KB memory waste |
 677 | **High** | WindowRecord redundant booleans | Memory waste, consistency risk |
 678 | **High** | ApplicationStatus 9 boolean fields | Cognitive complexity |
 679 | **High** | ProcessStatus 10 boolean fields | Cognitive complexity |
 680 | **High** | TrayItem holds icon pixel buffer | Memory per tray item |
 681 | **High** | MediaWindowLink 15 Option fields | Per-window overhead |
 682 | **High** | to_local_coords truncates f64 to i32 | Hit testing inaccuracy |
 683 | **High** | Hardcoded popup dimensions | Inflexibility |
 684 | **High** | Hardcoded default pinned apps | Poor UX for new users |
 685 | **High** | save_pinned_apps ignores errors | Silent data loss |
 686 | **High** | 4 threads per icon query | Thread overhead |
 687 | **High** | to_lowercase() per line | Per-search heap allocation |
 688 | **Medium** | AppError stores String, not types | Error information loss |
 689 | **Medium** | Recoverable trait inconsistent | DbusError has strategy but not recoverable |
 690 | **Medium** | FrameTimeout non-recoverable | Contradicts recovery strategy |
 691 | **Medium** | unsafe libc::setpriority | Silent failure |
 692 | **Medium** | _start variable unused | Dead code |
 693 | **Medium** | save_cached_icon ignores errors | Silent cache corruption |
 694 | **Medium** | load_cached_icon reads entire file | Memory for large icons |
 695 | **Medium** | ApplicationRecord 17 fields | Cognitive complexity |
 696 | **Medium** | ResolutionTrace::add O(n) lookup | Minor — use HashSet |
 697 | **Medium** | can_open_settings substring match | Per-resolution allocation |
 698 | **Medium** | Distribution path contains() match | False positive risk |
 699 | **Medium** | packaging defaults to SystemPackage | Misclassification |
 700 | **Medium** | Icon resolver clones identifier | Per-app allocation |
 701 | **Medium** | EventQueueBlocked non-recoverable | Contradicts recovery strategy |
 702 | **Medium** | StreamUnderrun recovery invalid | Misleading strategy |
 703 | **Medium** | PopupType::Custom coordinate space | API ambiguity |
 704 | **Medium** | TrayActionKind::Scroll uses &str | Type safety |
 705 | **Medium** | Recursive scan can stack overflow | Crash risk on symlinks |
 706 | **Medium** | .acf parsing with string splitting | Fragile parsing |
 707 | **Medium** | order_index from protocol_id | Incorrect window ordering |
 708 | **Medium** | LeakDetected non-recoverable | Contradicts recovery strategy |
 709 | **Medium** | scale_factor defaults to 1.0 | Visual glitch on HiDPI |
 710 | **Medium** | Shard files not cleaned on panic | Disk clutter |

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
 **19** | **gpu-rendering-review-19.md** | **671-710** | **710** |

**Cumulative severity across all 19 rounds:**
- Critical: 90 (was 85 + 5 new)
- High: 176 (was 160 + 16 new)
- Medium: 265 (was 239 + 26 new)
- Low: 78 (no new low issues this round)
- **Total: 710 issues**

### Key Themes from Round 19

1. **Cache is counterproductive**: `refresh_cache_on_launch` deletes the entire icon cache on every startup, making the cache useless. The icon indexer rebuilds the list, but the pixel cache is destroyed.

2. **Steam resolver is O(n) over all processes**: Both `resolve_appid_from_process` and `resolve_appid_from_gamescope` iterate every process on the system, reading `/proc/<pid>/environ` for each. This generates hundreds of EACCES errors and I/O spikes.

3. **Duplicate icon indexer code**: `resolvers/icon_list.rs` and `cache/icon_indexer.rs` both implement the same icon indexing logic with different parameters. One lacks priority lowering and cache age checking.

4. **Error recovery inconsistencies**: Multiple error types (FrameTimeout, EventQueueBlocked, LeakDetected, DbusError) have recovery strategies but are marked non-recoverable, or vice versa.

5. **Redundant boolean fields**: WindowRecord, ApplicationStatus, and ProcessStatus all store 8-10 boolean flags where 3-4 would suffice. These booleans can drift out of sync with enum status fields.

6. **Per-query thread spawning**: `search_icon_list_file` spawns 4 threads per icon lookup, each opening the file independently. For 10 icons, this is 40 thread spawns.

7. **Silent error ignoring**: `save_pinned_apps`, `save_cached_icon`, and `libc::setpriority` all silently ignore errors, leading to silent data loss or misconfiguration.

8. **Model bloat**: ApplicationRecord has 17 fields, ProcessRecord has 22 fields, WindowRecord has 20 fields, and MediaWindowLink has 15 fields. These are heavy structs cloned frequently.
