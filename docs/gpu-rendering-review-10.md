# GPU Rendering Code Review — Round 10 (Application Layer: Events, Listeners, Resolvers, Cache, Handlers, Interaction, Graphics, Pointer, Recovery, Diagnostics, Errors, Geometry, Models, Services)

This is a continuation of the GPU rendering code review. Rounds 1–9 covered all `wgpu-api/` source files and key `src/` application-layer modules (issues 1–280). This round covers the remaining `src/` modules: `events/`, `listeners/`, `resolvers/`, `cache/`, `handlers/`, `interaction/`, `graphics/`, `pointer/`, `recovery/`, `diagnostics/`, `errors/`, `geometry/`, `models/`, and `services/` (issues 281–330).

---

## Critical Issues

### 281. listeners/wayland/core.rs — DmabufHandler::released destroys buffers from client side (core.rs:245-252)
**File:** `src/listeners/wayland/core.rs`
**Lines:** 245-252

```rust
fn released(
    &mut self,
    _conn: &Connection,
    _qh: &QueueHandle<Self>,
    buffer: &wayland_client::protocol::wl_buffer::WlBuffer,
) {
    buffer.destroy();
}
```

When the compositor releases a DMA-BUF buffer, the client calls `buffer.destroy()`. This is a Wayland protocol violation — the compositor signals that the client may release its reference to the underlying DMA-BUF fd, but the `WlBuffer` proxy itself should not be destroyed by the client in the `released` callback. The buffer proxy lifecycle is managed by the client, not triggered by the compositor's release event.

**Impact:** Protocol violation — destroying the buffer proxy in response to `released` can cause use-after-free if the compositor still references the buffer object. This can crash the compositor or the client.

**Fix:** In `released`, only release the DMA-BUF fd reference (e.g., drop the mapping). Do not call `buffer.destroy()`.

---

### 282. listeners/wayland/foreign_toplevel.rs — Closed windows not removed from window_handles (foreign_toplevel.rs:68-72)
**File:** `src/listeners/wayland/foreign_toplevel.rs`
**Lines:** 68-72

```rust
zwlr_foreign_toplevel_handle_v1::Event::Closed => {
    state.emit_event(DomainEvent::Window(WindowEvent::Closed {
        window_id: window,
    }));
}
```

When a window is closed, the event is emitted but the handle is never removed from `state.window_handles`. The handle accumulates indefinitely, even though the window no longer exists.

**Impact:** Memory leak — every closed window's handle remains in `window_handles` forever. With 1000 window open/close cycles, 1000 orphaned handles accumulate.

**Fix:** Remove the handle from `state.window_handles` when `Closed` is received.

---

### 283. events/utils.rs — parse_window_states uses native endianness (utils.rs:8-22)
**File:** `src/events/utils.rs`
**Lines:** 8-22

```rust
pub fn parse_window_states(state_bytes: &[u8]) -> (bool, bool) {
    let mut activated = false;
    let mut minimized = false;

    for chunk in state_bytes.chunks_exact(4) {
        let value = u32::from_ne_bytes(chunk.try_into().unwrap());
        match value {
            2 => activated = true,
            1 => minimized = true,
            _ => {}
        }
    }

    (activated, minimized)
}
```

The function uses `u32::from_ne_bytes` which assumes native endianness. On little-endian x86_64 this works, but the Wayland foreign toplevel protocol doesn't specify endianness for the `state` array — it's a raw byte array from the compositor. More importantly, `chunks_exact(4)` silently drops trailing bytes that don't form a complete 4-byte chunk, potentially missing the last state flag.

**Impact:** Incorrect window state parsing on big-endian architectures. Silent data loss if the state array length is not a multiple of 4.

**Fix:** Use `u32::from_le_bytes` explicitly (Wayland uses little-endian wire format). Handle trailing bytes gracefully.

---

### 284. resolvers/icon.rs — get_icon_path creates sysinfo::System per call (icon.rs:203-209)
**File:** `src/resolvers/icon.rs`
**Lines:** 203-209

```rust
pub fn get_icon_path(app_id: &str) -> Option<PathBuf> {
    if app_id.starts_with("steam_icon_") {
        let sys_scanner = sysinfo::System::new().expect("Failed to create sysinfo System");
        if let Some((_, _, icon_path)) = resolve_steam_game_details(app_id, "", &sys_scanner, None)
        {
            return Some(icon_path);
        }
    }
```

Every call to `get_icon_path` for a Steam icon creates a new `sysinfo::System` instance, which reads `/proc` to enumerate all processes. This is extremely expensive — it reads every `/proc/[pid]/` entry just to find a Steam game's icon.

**Impact:** High CPU and I/O usage — creating a sysinfo System on every Steam icon lookup dominates CPU usage. At 60 FPS with multiple Steam icons, this is ~60 full process scans per second.

**Fix:** Pass the existing `sysinfo::System` instance from the caller instead of creating a new one. Cache Steam game details.

---

### 285. resolvers/icon.rs — find_icon_path uses WalkDir recursively (icon.rs:149-159)
**File:** `src/resolvers/icon.rs`
**Lines:** 149-159

```rust
pub fn find_icon_path(root_dir: &str, target_name: &str) -> Option<PathBuf> {
    for entry in WalkDir::new(root_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(stem) = path.file_name().and_then(|n| n.to_str()) {
            if stem.eq_ignore_ascii_case(target_name) {
                return Some(path.to_path_buf());
            }
        }
    }
    None
}
```

`WalkDir` traverses the entire directory tree recursively with no depth limit. For `/usr/share/icons`, this can be hundreds of thousands of files across many themes and sizes.

**Impact:** Extremely slow icon lookup — traversing `/usr/share/icons` recursively can take seconds. This blocks the main thread during icon resolution.

**Fix:** Use `linicon` (already used in services/monitor.rs) or limit WalkDir to specific subdirectories. Cache results.

---

### 286. cache/mod.rs — refresh_cache_on_launch deletes entire cache (mod.rs:16-23)
**File:** `src/cache/mod.rs`
**Lines:** 16-23

```rust
pub fn refresh_cache_on_launch() {
    let cache_dir = get_cache_dir();

    if cache_dir.exists() {
        if let Err(_e) = fs::remove_dir_all(&cache_dir) {}
    }
    let _ = fs::create_dir_all(&cache_dir);
}
```

On every startup, the entire icon cache directory is deleted and recreated. This makes the cache completely useless — cached icons are never available because they are deleted before they can be loaded.

**Impact:** The icon cache provides zero benefit — every icon is re-downloaded and re-processed on every launch. Network bandwidth and CPU are wasted.

**Fix:** Remove `refresh_cache_on_launch` or implement version-based migration that only deletes outdated entries.

---

### 287. listeners/wayland/dnd.rs — Leave event clears all mime types (dnd.rs:128-132)
**File:** `src/listeners/wayland/dnd.rs`
**Lines:** 128-132

```rust
DndEvent::Leave => {
    state.dnd_protocol.current_offer = None;
    state.dnd_protocol.current_serial = None;
    state.dnd_protocol.mime_types.clear();
    state.emit_event(DomainEvent::DragDrop(DragDropEvent::Leave));
}
```

On `Leave`, all mime types for all offers are cleared, not just the current offer's mime types. If multiple drag offers exist simultaneously, their mime type information is lost.

**Impact:** Subsequent drag operations may fail to accept the correct mime type because the mime type registry was cleared.

**Fix:** Clear only the current offer's mime types: `state.dnd_protocol.mime_types.remove(&offer_id)`.

---

### 288. services/monitor.rs — Standalone binary in library crate (monitor.rs:572-589)
**File:** `src/services/monitor.rs`
**Lines:** 572-589

```rust
fn main() {
    let conn = Connection::connect_to_env().expect("Wayland connection failed");
    // ...
    loop {
        event_queue.blocking_dispatch(&mut app).unwrap();
    }
}
```

A `main()` function exists inside a library module. This code is dead — it can never be executed because the crate's `main.rs` is the entry point. The module also defines its own Wayland event loop, conflicting with the main application's event loop.

**Impact:** Dead code that confuses the build system. If accidentally compiled as a binary, it would create a second Wayland connection and conflict with the main app.

**Fix:** Remove the `main()` function or move it to a separate binary crate.

---

### 289. services/monitor.rs — WalkDir depth 6 for icon lookup (monitor.rs:188-209)
**File:** `src/services/monitor.rs`
**Lines:** 188-209

```rust
for root in search_roots {
    if !root.exists() {
        continue;
    }
    for entry in WalkDir::new(root)
        .max_depth(6)
        .into_iter()
        .filter_map(|e| e.ok())
    {
```

Icon lookup uses `WalkDir` with `max_depth(6)` to traverse `/usr/share/icons` recursively. This is extremely slow for large icon themes.

**Impact:** Slow icon resolution — traversing icon themes with depth 6 can take hundreds of milliseconds per icon.

**Fix:** Use `linicon` for icon lookup or limit the search to specific subdirectories.

---

### 290. services/monitor.rs — Per-Event full process refresh (monitor.rs:533-534)
**File:** `src/services/monitor.rs`
**Lines:** 533-534

```rust
HandleEvent::Done => {
    state
        .sys_scanner
        .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
```

Every `Done` event from the foreign toplevel protocol triggers a full refresh of all processes. The `Done` event is emitted after every batch of window state changes, which can be frequent.

**Impact:** High CPU usage — refreshing all processes on every `Done` event reads `/proc` for every process on the system multiple times per second.

**Fix:** Throttle process refreshes to a fixed interval (e.g., 1 second) instead of triggering on every event.

---

## High Severity Issues

### 291. listeners/dbus_unity.rs — Progress value interpretation incorrect (dbus_unity.rs:77-83)
**File:** `src/listeners/dbus_unity.rs`
**Lines:** 77-83

```rust
if let Some(val) = props.get("progress") {
    update.progress = match val {
        Variant::F64(v) => *v,
        Variant::I64(v) => *v as f64 / 100.0,
        _ => 0.0,
    };
}
```

The Unity launcher protocol specifies progress in the range [0.0, 1.0]. The code divides `I64` values by 100.0, assuming they are percentages. However, the protocol doesn't use percentages for integers — an `I64` progress value should be interpreted directly, not divided.

**Impact:** Progress bars show incorrect values when the Unity client sends integer progress. A progress of 50 (meaning 50%) becomes 0.5, which is correct, but a progress of 100 becomes 1.0, which is also correct. However, if the client sends a value in [0.0, 1.0] range as I64, it would be 0 or 1, and dividing by 100 would give 0.0 or 0.01.

**Fix:** Verify the Unity protocol specification for the exact format of integer progress values.

---

### 292. listeners/dbus_tray.rs — Hardcoded app-specific rules (dbus_tray.rs:394-413, 594-613)
**File:** `src/listeners/dbus_tray.rs`
**Lines:** 394-413, 594-613

The tray listener contains hardcoded rules for Steam and qBittorrent in multiple places: `is_sni_service_candidate`, `is_qbittorrent_or_steam_service`, `candidate_item_paths`, and icon resolution. Each new app with non-standard SNI behavior requires a code change.

**Impact:** Maintenance burden — every app with non-standard tray behavior requires a code change and rebuild. The `contains` checks are imprecise and may match unintended apps.

**Fix:** Use a configurable mapping file for app-specific tray rules.

---

### 293. listeners/dbus_tray.rs — Per-refresh HashMap rebuilds (dbus_tray.rs:302-377)
**File:** `src/listeners/dbus_tray.rs`
**Lines:** 302-377

Every 5 seconds, `refresh_all_items` builds a new `HashMap` from scratch, comparing it with the old one using `tray_items_differ`. This involves cloning all tray items and their icon pixel data.

**Impact:** Per-refresh heap allocations. With 20 tray items each having a 32×32 RGBA icon (4KB), that's ~80KB of pixel data compared on every refresh cycle.

**Fix:** Use incremental updates that only compare changed entries.

---

### 294. listeners/dbus_tray.rs — XML parsing without a parser (dbus_tray.rs:780-811)
**File:** `src/listeners/dbus_tray.rs`
**Lines:** 780-811

D-Bus introspection XML is parsed with manual string searching (`find("<node")`, `find("name")`). This is fragile and doesn't handle edge cases like attributes in different order, escaped quotes, or nested nodes.

**Impact:** Parsing errors on non-standard introspection XML. Malformed XML can cause the parser to skip valid nodes or crash.

**Fix:** Use a proper XML parser or a more robust regex-based approach.

---

### 295. handlers/dock.rs — Hardcoded button codes (dock.rs:33, 66, 83-84)
**File:** `src/handlers/dock.rs`
**Lines:** 33, 66, 83-84

```rust
if button == 272 {
```

Button codes are hardcoded as magic numbers (272=left, 273=right, 274=middle) instead of using named constants. While `interaction/click.rs` defines `BTN_LEFT`, `BTN_RIGHT`, `BTN_MIDDLE`, this file doesn't use them.

**Impact:** Readability — magic numbers are hard to understand without looking up the Wayland specification.

**Fix:** Use the named constants from `interaction/click.rs` or define them locally.

---

### 296. handlers/dock.rs — Bash spawn for every app launch (dock.rs:271-294)
**File:** `src/handlers/dock.rs`
**Lines:** 271-294

```rust
let mut command = std::process::Command::new("bash");
command.arg(&launcher_path).arg(&normalized);
```

Every app launch spawns a new `bash` process to run a launcher script. This adds ~10-50ms of overhead per launch and creates a new process just to execute another process.

**Impact:** Slow app launch — the bash intermediary adds latency to every launch. The launcher script could be integrated directly into the Rust code.

**Fix:** Execute the launcher directly in Rust or use `std::process::Command` to run the app binary directly.

---

### 297. handlers/dock.rs — get_windows_for_app clones Vec (dock.rs:204-222)
**File:** `src/handlers/dock.rs`
**Lines:** 204-222

```rust
pub fn get_windows_for_app(
    app_id: &str,
    running_by_app: &HashMap<String, Vec<ObjectId>>,
) -> Vec<ObjectId> {
```

Every call clones the `Vec<ObjectId>` for the app's windows. This is called on every click, every hover, and every scroll event.

**Impact:** Per-interaction heap allocation. With 5 windows per app, that's ~40 bytes allocated per click.

**Fix:** Return a slice `&[ObjectId]` instead of cloning.

---

### 298. interaction/click.rs — 1036 lines, single massive function (click.rs:31-1035)
**File:** `src/interaction/click.rs`

The entire click handling logic is in a single 1036-line function with deeply nested control flow. It iterates all docks multiple times per click event, builds HashMaps for sorting windows, and has complex popup hit testing logic.

**Impact:** Maintainability — a single change to click handling requires understanding the entire 1000+ line function. Testing is difficult because there are no isolated units.

**Fix:** Split into submodules: `dock_click.rs`, `popup_click.rs`, `tray_click.rs`, `menu_click.rs`.

---

### 299. interaction/click.rs — Hardcoded bluetooth widget path (click.rs:180-189)
**File:** `src/interaction/click.rs`
**Lines:** 180-189

```rust
if let Ok(child) = std::process::Command::new("sh")
    .args(&[
        "-c",
        "~/m.2/Rust/bluetooth-widget-complete/target/release/widget-test",
    ])
```

The Bluetooth widget path is hardcoded to a specific user's development directory. This will not work on any other system.

**Impact:** The Bluetooth toggle button crashes or does nothing on systems where the path doesn't exist.

**Fix:** Make the Bluetooth widget path configurable or discover it via `which` or a well-known path.

---

### 300. interaction/click.rs — Per-click HashMap builds for sorting (click.rs:98-115, 783-801)
**File:** `src/interaction/click.rs`
**Lines:** 98-115, 783-801

On every click, a `HashMap<ObjectId, String>` is built to sort windows by title. This is O(n) allocation and insertion, repeated for every click on a dock icon.

**Impact:** Per-click heap allocation. With 10 windows, that's 10 String lookups + 1 HashMap allocation per click.

**Fix:** Sort windows once and cache the sorted order, invalidating only when window titles change.

---

### 301. recovery/gpu_recovery.rs — All recovery methods are stubs (gpu_recovery.rs:91-127)
**File:** `src/recovery/gpu_recovery.rs`
**Lines:** 91-127

All three recovery methods (`recover_device_loss`, `recover_memory_pressure`, `recover_presentation`) return `Retryable` or `Recovered` without actually doing anything. The comments say "In a real implementation, this would..." but no real implementation exists.

**Impact:** False sense of security — the recovery system appears to work but does nothing. GPU errors are not actually recovered.

**Fix:** Implement the actual recovery logic or remove the stub code.

---

### 302. pointer/proximity.rs — Only checks first dock (proximity.rs:142-145)
**File:** `src/pointer/proximity.rs`
**Lines:** 142-145

```rust
let dock = match state.docks.first() {
    Some(dock) => dock,
    None => return false,
};
```

`pointer_between_icon_and_window` only checks the first dock, ignoring all other docks. On multi-monitor setups, the window list proximity detection only works for the first monitor.

**Impact:** Window list popup doesn't stay open when hovering between icon and popup on secondary monitors.

**Fix:** Check all docks or find the dock that contains the pointer position.

---

### 303. cache/persistence.rs — Hardcoded default pinned apps (persistence.rs:30-34)
**File:** `src/cache/persistence.rs`
**Lines:** 30-34

```rust
vec![
    "AmneziaVPN".into(),
    "steam".into(),
    "org.qbittorrent.qBittorrent".into(),
]
```

Every user gets three hardcoded pinned apps by default. These are specific to one user's setup and are irrelevant to most users.

**Impact:** Poor UX — new users see irrelevant apps in their dock. The apps may not even be installed.

**Fix:** Start with an empty pin list or use system-installed apps as defaults.

---

### 304. resolvers/pipeline.rs — Unbounded cache growth (pipeline.rs:132-133)
**File:** `src/resolvers/pipeline.rs`
**Lines:** 132-133

```rust
applications: HashMap<AppId, CachedApplication>,
processes: HashMap<ProcessId, CachedProcess>,
```

The resolver pipeline caches applications and processes but never evicts entries. As new apps and processes appear, the caches grow indefinitely.

**Impact:** Memory leak — the caches grow without bound over time. With 1000 unique app IDs and processes, the caches hold 1000 entries forever.

**Fix:** Implement LRU eviction or periodic cleanup of stale entries.

---

### 305. resolvers/context.rs — from_system clones all process data (context.rs:51-71)
**File:** `src/resolvers/context.rs`
**Lines:** 51-71

```rust
pub fn from_system(system: &System) -> Self {
    let processes = system
        .processes()
        .iter()
        .map(|(pid, process)| ProcessSnapshot {
            // ... clones name, executable, cwd, command_line, etc.
```

Every call clones all process data from sysinfo, including `command_line` which can be very long (e.g., Java processes with huge classpaths).

**Impact:** High memory usage — cloning all process data on every window event creates large temporary allocations.

**Fix:** Clone only the fields needed for resolution. Avoid cloning `command_line` unless necessary.

---

### 306. resolvers/context.rs — read_proc_environment fails for other users (context.rs:81-95)
**File:** `src/resolvers/context.rs`
**Lines:** 81-95

```rust
pub fn read_proc_environment(pid: ProcessId) -> HashMap<String, String> {
    let mut result = HashMap::new();
    let Ok(data) = fs::read(format!("/proc/{}/environ", pid.0)) else {
        return result;
    };
```

Reading `/proc/{pid}/environ` requires `CAP_SYS_PTRACE` for processes owned by other users. On most systems, this will fail silently for the majority of processes.

**Impact:** Environment-based resolution fails silently for most processes. The resolver falls back to less accurate methods.

**Fix:** Handle the permission error gracefully and log a warning. Don't rely on environment variables for resolution.

---

### 307. resolvers/desktop.rs — Hardcoded transmission rule (desktop.rs:101-103)
**File:** `src/resolvers/desktop.rs`
**Lines:** 101-103

```rust
if search_id.to_lowercase().contains("transmission") {
    search_id = "transmission-gtk".to_string();
}
```

The Transmission app has a hardcoded rule that forces `transmission-gtk` as the desktop file name. This breaks if the user has `transmission-qt` or another variant installed.

**Impact:** Wrong desktop file resolved for Transmission variants. The user's actual Transmission client may not be gtk.

**Fix:** Try multiple Transmission variants or use the actual app_id from the window.

---

### 308. resolvers/desktop.rs — Per-call directory iteration (desktop.rs:150-245)
**File:** `src/resolvers/desktop.rs`
**Lines:** 150-245

Both `find_desktop_file_by_name` and `find_desktop_file_by_exec` iterate all `.desktop` files in `/usr/share/applications` and `~/.local/share/applications` on every call, opening and parsing each file.

**Impact:** Slow resolution — with 500 desktop files, each call opens and parses 500 files. This blocks the main thread.

**Fix:** Cache the desktop file index or use a pre-built database.

---

### 309. geometry/popup.rs — Hardcoded popup dimensions (popup.rs:26-30)
**File:** `src/geometry/popup.rs`
**Lines:** 26-30

```rust
pub const BASE_ITEM_HEIGHT: i32 = 29;
pub const BASE_MENU_WIDTH: i32 = 144;
pub const BASE_WINDOW_LIST_WIDTH: i32 = 160;
pub const WINDOW_LIST_FRAME_HEIGHT: i32 = 240;
```

Popup dimensions are hardcoded constants instead of coming from the config. The comment says "Legacy constants — kept for backward compatibility. Use config values instead." but no config values are used.

**Impact:** Popup dimensions don't match the user's config. A user with larger font sizes will have clipped text in popups.

**Fix:** Read popup dimensions from `state.config` instead of using constants.

---

### 310. graphics/animation.rs — Pixmap::new unwrap panics (animation.rs:101)
**File:** `src/graphics/animation.rs`
**Line:** 101

```rust
let mut pixmap = Pixmap::new(target_size, target_size).unwrap();
```

If the pixmap allocation fails (e.g., out of memory), the application panics. This can happen with large target sizes or under memory pressure.

**Impact:** Application crash on OOM when loading animated icons.

**Fix:** Handle allocation failure gracefully with `?` or `ok()`.

---

## Medium Severity Issues

### 311. listeners/wayland/fractional_scale.rs — Scale cast to i32 (fractional_scale.rs:39)
**File:** `src/listeners/wayland/fractional_scale.rs`
**Line:** 39

```rust
scale: scale as i32,
```

The fractional scale protocol sends scale as an integer representing a percentage (e.g., 100 = 1.0x, 150 = 1.5x). The code casts it to `i32`, which is correct for storage, but the consumer must divide by 100 to get the actual scale factor. This is not documented.

**Impact:** Consumers of `FractionalScaleChanged` may use the raw value (150) instead of the actual scale (1.5), causing incorrect buffer sizes.

**Fix:** Document that the scale is a percentage, or convert it to a float at the source.

---

### 312. listeners/dbus_tray.rs — resolve_icon_by_name does many filesystem checks (dbus_tray.rs:913-994)
**File:** `src/listeners/dbus_tray.rs`
**Lines:** 913-994

For each tray icon, the resolver checks up to ~150 candidate paths across multiple icon themes, sizes, and extensions. Each check calls `candidate.exists()` which is a filesystem stat call.

**Impact:** Slow tray icon resolution — 150 stat calls per icon, per refresh cycle. With 10 tray icons refreshed every 5 seconds, that's 300 stat calls per second.

**Fix:** Cache the results or use `linicon` for efficient icon lookup.

---

### 313. handlers/dock.rs — Hardcoded transmission normalization (dock.rs:236-238)
**File:** `src/handlers/dock.rs`
**Lines:** 236-238

```rust
if id.to_lowercase().contains("transmission") {
    id = "transmission-gtk".to_string();
}
```

Same hardcoded Transmission rule as in desktop.rs. Duplicated logic across modules.

**Impact:** Inconsistent behavior if the rules diverge. Maintenance burden.

**Fix:** Centralize app ID normalization in a single module.

---

### 314. interaction/drag.rs — save_pinned_apps on every drag (drag.rs:77, 80)
**File:** `src/interaction/drag.rs`
**Lines:** 77, 80

```rust
crate::cache::persistence::save_pinned_apps(&state.pinned_apps);
```

`save_pinned_apps` is called on every drag release, even if the order didn't change (e.g., dropped in the same position). This writes to disk unnecessarily.

**Impact:** Unnecessary disk I/O on every drag. The file is written even when nothing changed.

**Fix:** Only save when the pinned apps list actually changed.

---

### 315. interaction/drag.rs — O(n) search for pinned apps (drag.rs:50-51)
**File:** `src/interaction/drag.rs`
**Lines:** 50-51

```rust
let old_idx_opt = state.pinned_apps.iter().position(|x| x == dragged_id);
let new_idx_opt = state.pinned_apps.iter().position(|x| x == target_app_id);
```

Both searches are O(n) linear scans of `pinned_apps`. With 20 pinned apps, that's 40 comparisons per drag.

**Impact:** Minor performance issue — O(n) search is fine for small lists but scales poorly.

**Fix:** Use a `HashSet` for O(1) lookup if the list grows large.

---

### 316. diagnostics/performance.rs — O(n) removal in end_frame (performance.rs:69)
**File:** `src/diagnostics/performance.rs`
**Line:** 69

```rust
self.frame_times.remove(0);
```

`Vec::remove(0)` shifts all elements, making it O(n). With `max_samples=60`, this is minor but unnecessary.

**Impact:** Minor performance overhead — O(n) shift on every frame after the window is full.

**Fix:** Use `VecDeque` for O(1) pop_front, or use a circular buffer.

---

### 317. diagnostics/performance.rs — FrameMetrics fields unused (performance.rs:22-31)
**File:** `src/diagnostics/performance.rs`
**Lines:** 22-31

```rust
pub struct FrameMetrics {
    pub frame_duration_ms: f64,
    pub render_time_ms: f64,
    pub physics_time_ms: f64,
    pub fps: f64,
}
```

`FrameMetrics` has `render_time_ms` and `physics_time_ms` fields, but `PerformanceMonitor` never tracks or populates them. The struct is never used.

**Impact:** Dead code — the struct suggests capabilities that don't exist.

**Fix:** Implement render/physics timing or remove the unused fields.

---

### 318. errors/mod.rs — DbusError recovery contradiction (errors/mod.rs:56-75)
**File:** `src/errors/mod.rs`
**Lines:** 56-75

`is_recoverable` returns `false` for `DbusError`, but `recovery_strategy` returns `Some("Retry D-Bus connection with backoff")` for it. The test confirms this contradiction.

**Impact:** The recovery strategy is defined but never used because `is_recoverable` returns `false`. The recovery system will never attempt to recover D-Bus errors.

**Fix:** Either make `DbusError` recoverable or remove its recovery strategy.

---

### 319. models.rs — ProcessSnapshot name collision (models.rs:264-268 vs context.rs:24-33)
**File:** `src/models.rs` line 264-268 vs `src/resolvers/context.rs` line 24-33

Two different `ProcessSnapshot` structs exist with different fields. `models::ProcessSnapshot` has `processes: Vec<ProcessEntry>` and `total_processes: usize`. `resolvers::context::ProcessSnapshot` has per-process fields like `pid`, `ppid`, `name`, `executable`, etc.

**Impact:** Confusion — the same name refers to different types in different modules. Code that imports the wrong one will compile but behave incorrectly.

**Fix:** Rename one of the types to avoid collision.

---

### 320. registry.rs — O(n) linear scans in helpers (registry.rs:20-28)
**File:** `src/registry.rs`
**Lines:** 20-28

```rust
fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.iter().any(|existing| existing == &item) {
        items.push(item);
    }
}

fn retain_value<T: PartialEq>(items: &mut Vec<T>, item: &T) {
    items.retain(|existing| existing != item);
}
```

Both helpers use O(n) linear scans. `push_unique` is called for every window-app association, and `retain_value` is called for every process detachment.

**Impact:** O(n²) behavior when many windows or processes are associated with the same app.

**Fix:** Use `HashSet` for O(1) lookups or sort the vectors for binary search.

---

### 321. cache/mod.rs — app_id used directly as filename (mod.rs:39)
**File:** `src/cache/mod.rs`
**Line:** 39

```rust
path.push(format!("{}.raw", app_id));
```

The app_id is used directly as a filename without sanitizing special characters. App IDs like `org.kde.file:///evil` or `app/with/slashes` will create invalid paths.

**Impact:** Path traversal — a malicious app_id could write outside the cache directory.

**Fix:** Sanitize the app_id by replacing `/` and `..` with safe characters.

---

### 322. cache/mod.rs — Uses .config instead of .cache (mod.rs:26-34)
**File:** `src/cache/mod.rs`
**Lines:** 26-34

```rust
fn get_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let mut path = PathBuf::from(home);
    path.push(".config");
    path.push("dock");
    path.push("cached_icons");
```

The icon cache is stored in `~/.config/dock/cached_icons` instead of `~/.cache/dock`. The XDG Base Directory Specification distinguishes between config (essential, backed up) and cache (recreatable, can be deleted).

**Impact:** Cache files are backed up unnecessarily. They may be included in config backups, wasting storage.

**Fix:** Use `~/.cache/dock` for the icon cache.

---

### 323. resolvers/context.rs — normalize_token strips useful characters (context.rs:97-103)
**File:** `src/resolvers/context.rs`
**Lines:** 97-103

```rust
pub fn normalize_token(value: &str) -> String {
    value
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect()
}
```

All non-alphanumeric characters are stripped, including hyphens, underscores, and dots. This loses useful information: `org.qbittorrent.qBittorrent` becomes `orgqbittorrentqbittorrent`, which is less distinctive.

**Impact:** Token collisions — different app IDs may normalize to the same token, causing incorrect matches.

**Fix:** Preserve hyphens and underscores in the normalized token.

---

### 324. listeners/dbus_tray.rs — Duplicate watcher implementations (dbus_tray.rs:26-132)
**File:** `src/listeners/dbus_tray.rs`
**Lines:** 26-132

`LocalStatusNotifierWatcher` and `LocalKdeStatusNotifierWatcher` are nearly identical — same fields, same methods, same logic. The only difference is the D-Bus interface name.

**Impact:** Code duplication — maintenance burden and risk of divergence.

**Fix:** Extract the common logic into a single generic implementation parameterized by interface name.

---

### 325. interaction/scroll.rs — Unused parameters (scroll.rs:22-25)
**File:** `src/interaction/scroll.rs`
**Lines:** 22-25

```rust
    _apps_in_dock: &[String],
    _running_by_app: &HashMap<String, Vec<ObjectId>>,
    _scale_factor: f32,
    _layout: (i32, i32, i32, i32),
```

Four parameters are unused (prefixed with `_`). The function signature suggests these might be needed for future features, but they add overhead to every call.

**Impact:** API surface pollution — unused parameters suggest incomplete implementation.

**Fix:** Remove unused parameters or implement the features that need them.

---

### 326. graphics/fade.rs — apply_alpha only modifies alpha channel (fade.rs:156-158)
**File:** `src/graphics/fade.rs`
**Lines:** 156-158

```rust
for chunk in canvas.chunks_exact_mut(4) {
    chunk[3] = ((chunk[3] as u16 * alpha + 127) / 255) as u8;
}
```

Only the alpha channel is modified. The R/G/B channels are unchanged. This is correct for premultiplied alpha rendering but may cause visual artifacts if the renderer expects straight alpha (where R/G/B should also be scaled).

**Impact:** Visual artifacts — if the renderer doesn't premultiply alpha, the color will be wrong at intermediate alpha values.

**Fix:** Document the expected alpha format or premultiply R/G/B as well.

---

### 327. pointer/proximity.rs — Hardcoded gap and padding (proximity.rs:173-178)
**File:** `src/pointer/proximity.rs`
**Lines:** 173-178

```rust
let gap_top = geom.y + geom.phys_height - (6.0 * dock_scale as f64).round() as i32;
let gap_bottom = (6.0 * dock_scale as f64).round() as i32;
// ...
let padding = (12.0 * dock_scale as f64).round() as i32;
```

The gap between the icon and the popup (6.0px) and the horizontal padding (12.0px) are hardcoded. These values affect the user experience but are not configurable.

**Impact:** The proximity detection area is fixed. Users with larger icons or different dock layouts may find the window list closes too easily or too late.

**Fix:** Make these values configurable or derive them from the icon size.

---

### 328. pointer/proximity.rs — Unused scale_factor parameter (proximity.rs:180)
**File:** `src/pointer/proximity.rs`
**Line:** 180

```rust
let _ = scale_factor;
```

The `scale_factor` parameter is explicitly ignored. The function uses `dock.scale_factor` instead.

**Impact:** API inconsistency — the parameter suggests the function uses the passed scale, but it doesn't.

**Fix:** Remove the unused parameter or use it consistently.

---

### 329. resolvers/desktop.rs — parse_desktop_entry ignores localized keys (desktop.rs:338-355)
**File:** `src/resolvers/desktop.rs`
**Lines:** 338-355

```rust
match key.trim() {
    "Name" => metadata.name = Some(value.to_string()),
    "GenericName" => metadata.generic_name = Some(value.to_string()),
    // ...
}
```

Desktop files support localized keys like `Name[en]`, `Name[fr]`, etc. The parser only handles the base `Name` key, ignoring localized variants.

**Impact:** Incorrect or missing names for desktop files that use only localized keys.

**Fix:** Fall back to localized keys when the base key is missing, respecting the user's locale.

---

### 330. services/monitor.rs — Duplicated Steam resolution (monitor.rs:214-414)
**File:** `src/services/monitor.rs`
**Lines:** 214-414

`resolve_steam_game_details` in `services/monitor.rs` is a duplicate of the function in `resolvers/steam.rs`. The implementations differ slightly, leading to inconsistent behavior.

**Impact:** Code duplication — maintenance burden and risk of divergence. The two implementations may produce different results for the same input.

**Fix:** Use the shared implementation from `resolvers/steam.rs`.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 281 | **Critical** | DmabufHandler::released destroys buffers | Protocol violation, potential crash |
| 282 | **Critical** | Closed windows not removed from window_handles | Memory leak |
| 283 | **Critical** | parse_window_states uses native endianness | Incorrect state on big-endian, silent data loss |
| 284 | **Critical** | get_icon_path creates sysinfo::System per call | High CPU/I/O usage |
| 285 | **Critical** | find_icon_path uses WalkDir recursively | Extremely slow icon lookup |
| 286 | **Critical** | refresh_cache_on_launch deletes entire cache | Cache provides zero benefit |
| 287 | **Critical** | DnD Leave clears all mime types | Subsequent drags may fail |
| 288 | **Critical** | Standalone binary in library crate | Dead code, build confusion |
| 289 | **Critical** | WalkDir depth 6 for icon lookup | Slow icon resolution |
| 290 | **Critical** | Per-Event full process refresh | High CPU usage |
| 291 | **High** | Progress value interpretation | Incorrect progress display |
| 292 | **High** | Hardcoded app-specific tray rules | Maintenance burden |
| 293 | **High** | Per-refresh HashMap rebuilds | Heap allocations |
| 294 | **High** | XML parsing without a parser | Fragile parsing |
| 295 | **High** | Hardcoded button codes | Readability |
| 296 | **High** | Bash spawn for every app launch | Slow app launch |
| 297 | **High** | get_windows_for_app clones Vec | Per-interaction allocation |
| 298 | **High** | 1036-line click handler | Maintainability |
| 299 | **High** | Hardcoded bluetooth widget path | Doesn't work on other systems |
| 300 | **High** | Per-click HashMap builds | Per-click allocation |
| 301 | **High** | GPU recovery methods are stubs | False sense of security |
| 302 | **High** | Only checks first dock | Multi-monitor broken |
| 303 | **High** | Hardcoded default pinned apps | Poor UX |
| 304 | **High** | Unbounded cache growth | Memory leak |
| 305 | **High** | from_system clones all process data | High memory usage |
| 306 | **High** | read_proc_environment fails for other users | Silent failure |
| 307 | **High** | Hardcoded transmission rule | Wrong desktop file |
| 308 | **High** | Per-call directory iteration | Slow resolution |
| 309 | **High** | Hardcoded popup dimensions | Doesn't match config |
| 310 | **High** | Pixmap::new unwrap panics | Application crash |
| 311 | **Medium** | Scale cast to i32 | Consumer confusion |
| 312 | **Medium** | resolve_icon_by_name filesystem checks | Slow tray resolution |
| 313 | **Medium** | Hardcoded transmission normalization | Duplicated logic |
| 314 | **Medium** | save_pinned_apps on every drag | Unnecessary disk I/O |
| 315 | **Medium** | O(n) search for pinned apps | Scales poorly |
| 316 | **Medium** | O(n) removal in end_frame | Minor overhead |
| 317 | **Medium** | FrameMetrics fields unused | Dead code |
| 318 | **Medium** | DbusError recovery contradiction | Recovery never attempted |
| 319 | **Medium** | ProcessSnapshot name collision | Confusion |
| 320 | **Medium** | O(n) linear scans in helpers | O(n²) behavior |
| 321 | **Medium** | app_id used directly as filename | Path traversal risk |
| 322 | **Medium** | Uses .config instead of .cache | Wrong XDG directory |
| 323 | **Medium** | normalize_token strips useful chars | Token collisions |
| 324 | **Medium** | Duplicate watcher implementations | Code duplication |
| 325 | **Medium** | Unused parameters | API pollution |
| 326 | **Medium** | apply_alpha only modifies alpha | Visual artifacts |
| 327 | **Medium** | Hardcoded gap and padding | Not configurable |
| 328 | **Medium** | Unused scale_factor parameter | API inconsistency |
| 329 | **Medium** | parse_desktop_entry ignores localized keys | Missing names |
| 330 | **Medium** | Duplicated Steam resolution | Code duplication |

---

## Cumulative Summary

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 1 | 2 | 2 | 7 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 7 |
| 3 | gpu-rendering-review-3.md | 33–60 | 5 | 5 | 10 | 10 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 9 | 11 | 11 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 11 | 11 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| 8 | gpu-rendering-review-8.md | 200–240 | 6 | 10 | 16 | 8 |
| 9 | gpu-rendering-review-9.md | 241–280 | 6 | 10 | 16 | 8 |
| 10 | gpu-rendering-review-10.md | 281–330 | 10 | 20 | 20 | 0 |
| **Total** | | **330** | **44** | **87** | **108** | **78** |
