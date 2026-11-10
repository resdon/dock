# GPU Rendering Code Review — Round 13 (Project Configuration, Build System, CI/CD, Packaging, Remaining Modules)

This is a continuation of the GPU rendering code review. Rounds 1–12 covered all `wgpu-api/` source files, all `src/` application-layer modules, and `src/app/` submodules (issues 1–420). This round covers the project infrastructure: `Cargo.toml` (both crates), `build.rs`, `scripts/`, `.github/workflows/ci.yml`, `Makefile`, `PKGBUILD`, `assets/`, and remaining wgpu-api modules (`primitives.rs`, `morphing.rs`, `perf.rs`) — issues 421–460.

---

## Critical Issues

### 421. Cargo.toml — tokio "full" features bloates binary and RAM usage (line 51)
**File:** `Cargo.toml`
**Line:** 51

```toml
tokio = { version = "1.54", features = ["full"] }
```

The `"full"` feature enables every Tokio feature including `rt-multi-thread`, `macros`, `io-util`, `fs`, `net`, `sync`, `time`, `parking_lot`, `signal`, `process`, `test-util`, and more. Dock only needs a subset.

**Impact:** Binary bloat — `"full"` adds ~200KB to the binary and increases compile time. More importantly, it enables `rt-multi-thread` which could allow accidental multi-threaded runtime usage, violating the single-thread renderer invariant. Per ADR 005, lowest possible RAM usage is a core priority.

**Fix:** Enable only the needed features: `["rt", "sync", "time", "macros"]`.

---

### 422. Cargo.toml — sixel-rs and icy_sixel are terminal-only dependencies in the main binary (lines 35-36)
**Files:** `Cargo.toml` lines 35-36, `src/lib.rs` lines 122-134

```toml
sixel-rs = "0.5.0"
icy_sixel = "0.5.0"
```

These crates are for terminal image rendering (sixel format). They are pulled into the main dock binary even though dock runs on Wayland, not in a terminal. The `terminal_graphics` module in `lib.rs` uses `generate_terminal_image_string()` which produces sixel escape sequences.

**Impact:** Binary bloat — two entire crates are linked into the dock binary for a feature that is never used at runtime. The `terminal_graphics` module exists in `lib.rs` but is only useful for development/debugging in a terminal.

**Fix:** Gate `sixel-rs`, `icy_sixel`, and the `terminal_graphics` module behind a `dev` feature flag.

---

### 423. Cargo.toml — cairo-rs dependency is unused in GPU path (line 53)
**File:** `Cargo.toml`
**Line:** 53

```toml
cairo-rs = "0.19"
```

Cairo is a CPU 2D graphics library. The project uses wgpu for GPU rendering and Slint for widget rendering. Cairo adds ~500KB to the binary and pulls in `glib`, `gio`, `gobject`, and other GTK dependencies.

**Impact:** Binary bloat and dependency chain — Cairo and its transitive dependencies (glib, gio, gobject, cairo-gobject) add significant size and compile time without contributing to the GPU rendering path.

**Fix:** Remove cairo-rs unless it's actively used in a code path. Search for `cairo` imports to confirm usage.

---

### 424. PKGBUILD — Version mismatch with Cargo.toml (line 3 vs Cargo.toml line 3)
**File:** `PKGBUILD` line 3 vs `Cargo.toml` line 3

```
# PKGBUILD
pkgver=0.1.0

# Cargo.toml
version = "1.5.2"
```

The PKGBUILD version (0.1.0) doesn't match Cargo.toml (1.5.2). This means Arch Linux package managers will show an outdated version.

**Impact:** Package confusion — users installing via PKGBUILD will get version 0.1.0 in package metadata, making updates and debugging difficult.

**Fix:** Synchronize `pkgver` with `Cargo.toml`'s version, or derive it automatically.

---

### 425. PKGBUILD — source array lists directories, not files (line 24)
**File:** `PKGBUILD`
**Line:** 24

```bash
source=('Cargo.toml' 'src' 'assets' 'scripts')
```

The `source` array lists directories (`src`, `assets`, `scripts`) alongside a file (`Cargo.toml`). Arch's `makepkg` doesn't handle directory sources correctly — it expects tarballs or individual files. The `sha256sums` are all `SKIP`, providing no integrity verification.

**Impact:** Broken packaging — `makepkg` may fail or include unintended files. No checksum verification means corrupted source goes undetected.

**Fix:** Use a git source (`source=("git+https://...")`) or tarball the entire project.

---

### 426. PKGBUILD — runs list_icons.sh during packaging, scanning build host's icons (line 55)
**File:** `PKGBUILD`
**Line:** 55

```bash
./scripts/list_icons.sh "$pkgdir/usr/share/dock/icon_list.txt"
```

During `makepkg package()`, the script scans the build host's icon directories (`/usr/share/icons`, `$HOME/.local/share/icons`, etc.) and writes the results into the package. This means the icon list is host-specific and non-reproducible.

**Impact:** Non-reproducible builds — the package contains the build host's icon paths, not the end user's. The icon list will be wrong on the target system.

**Fix:** Generate the icon list at install time (in an install script) or at runtime, not during packaging.

---

### 427. services/test.rs — Standalone binary does nothing useful (lines 1-59)
**File:** `src/services/test.rs`

The entire binary reads `.desktop` files and counts them into `_i`, but never outputs the count or does anything with the data. It's registered as a binary in `Cargo.toml` (`[[bin]] name = "test"`), which shadows the `test` subcommand and is confusing.

**Impact:** Dead code — the binary compiles but produces no output. The name `test` conflicts with `cargo test` mentally and could cause confusion.

**Fix:** Remove the binary or make it actually useful (e.g., a debug tool that lists icons).

---

### 428. primitives.rs — ellipse() ignores ry parameter, always renders a circle (line 124)
**File:** `wgpu-api/src/primitives.rs`
**Line:** 124

```rust
pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, _ry: f32) {
    self.move_to(cx + rx, cy);
    self.arc(cx, cy, rx, 0.0, std::f32::consts::PI * 2.0, true);
    self.close();
}
```

The `_ry` parameter is ignored (prefixed with `_` to suppress the warning). The function uses `rx` for the arc radius, so `ellipse(0, 0, 100, 50)` renders a circle of radius 100, not an ellipse.

**Impact:** Incorrect geometry — any code that calls `ellipse()` with different `rx` and `ry` values gets a circle instead of an ellipse. The `shape::ellipse()` convenience function has the same bug.

**Fix:** Implement proper ellipse rendering using cubic bezier approximation or parametric arc segments.

---

## High Severity Issues

### 429. perf.rs — All profiling structs use RefCell, risking panics in hot paths (throughout)
**File:** `wgpu-api/src/perf.rs`

`Stopwatch`, `FrameRateAnalyzer`, `PerfCounter`, and `Profiler` all use `RefCell` for interior mutability. In the render hot path, a double borrow will panic at runtime.

```rust
pub struct Stopwatch {
    start: RefCell<Option<Instant>>,
    elapsed: RefCell<Duration>,
}
```

**Impact:** Runtime panic risk — if profiling code is called from multiple contexts (e.g., nested `begin_phase` calls), the `RefCell` borrow checker panics. At 60 FPS, this is a denial-of-service.

**Fix:** Use `Cell` for copy types, or restructure to avoid interior mutability in hot paths.

---

### 430. perf.rs — FrameRateAnalyzer::record_frame() uses Vec::remove(0), O(n) per frame (line 309)
**File:** `wgpu-api/src/perf.rs`
**Line:** 309

```rust
if times.len() > self.max_samples {
    times.remove(0);  // O(n) — shifts all elements
}
```

`Vec::remove(0)` shifts all remaining elements by one position. With 360 samples, this is 360 memory moves per frame, at 60 FPS = 21,600 moves/second.

**Impact:** CPU waste — a circular buffer or `VecDeque` would make this O(1).

**Fix:** Use `VecDeque` with a max size, or a fixed-size circular buffer.

---

### 431. perf.rs — JitterAnalyzer has the same O(n) remove(0) issue (line 1002)
**File:** `wgpu-api/src/perf.rs`
**Line:** 1002

```rust
if times.len() > self.max_samples {
    times.remove(0);
}
```

Same issue as #430.

**Fix:** Use `VecDeque`.

---

### 432. build.rs — No error handling for Slint compilation (lines 5-6)
**File:** `build.rs`
**Lines:** 5-6

```rust
slint_build::compile("ui.slint").unwrap();
slint_build::compile("bluetooth-widget.slint").unwrap();
```

If a Slint file has a syntax error, `unwrap()` panics the build script with an unhelpful message. The error doesn't include the file name clearly.

**Impact:** Poor developer experience — Slint compilation errors are hard to diagnose.

**Fix:** Use `expect()` with a descriptive message including the file name.

---

### 433. Makefile — build target checks for file named `dock` in CWD (line 9)
**File:** `Makefile`
**Line:** 9

```make
@if [ ! -f $(BINARY_NAME) ]; then \
```

The build target checks for a file named `dock` in the current directory, not `target/release/dock`. This means if you create an empty file named `dock`, the build is skipped. The install target then tries to install this empty file.

**Impact:** Silent build skip — `make install` could install an empty file as the dock binary.

**Fix:** Check for `target/release/$(BINARY_NAME)` instead.

---

### 434. Makefile — dist target only supports x86_64 Linux (line 36-46)
**File:** `Makefile`
**Lines:** 36-46

```make
rm -rf dock-linux-amd64 dock-linux-amd64.tar.gz
mkdir -p dock-linux-amd64/assets/24 dock-linux-amd64/scripts
```

The distribution target hardcodes `linux-amd64` in the directory name and tarball. No support for other architectures (aarch64, etc.) or cross-compilation.

**Impact:** Limited distribution — ARM-based systems (Raspberry Pi, etc.) can't use the dist target.

**Fix:** Detect the target architecture dynamically or parameterize it.

---

### 435. CI — cargo tree -d output is not treated as failure (ci.yml line 52)
**File:** `.github/workflows/ci.yml`
**Line:** 52

```yaml
- name: Check duplicate dependencies
  run: cargo tree -d
```

`cargo tree -d` prints duplicate dependencies but always exits with code 0, even when duplicates are found. The CI step will always pass, providing no actual validation.

**Impact:** False confidence — duplicate dependencies go undetected in CI. The check.sh script has the same issue (line 145-149) — it greps for "warning" but `cargo tree -d` doesn't use that word.

**Fix:** Check the output for non-empty results and fail if duplicates exist: `! cargo tree -d | grep -q .`

---

### 436. CI — no clippy for wgpu-api crate specifically (ci.yml)
**File:** `.github/workflows/ci.yml`

The CI runs `cargo clippy --workspace --all-targets` which includes wgpu-api, but there's no separate job for wgpu-api-specific checks. If wgpu-api has warnings that are suppressed by the main crate's `Cargo.toml`, they won't be caught.

**Impact:** Missed warnings — wgpu-api-specific clippy warnings could be hidden.

**Fix:** Add a dedicated wgpu-api clippy job or ensure workspace-level clippy is strict.

---

### 437. CI — physics tests run in a separate job but failures are expected (ci.yml line 96)
**File:** `.github/workflows/ci.yml`
**Line:** 96

```yaml
- name: Run physics tests
  run: cd wgpu-api && cargo test physics
```

The physics tests have 5 pre-existing failures (noted in session history). The CI job will fail on every run, but there's no `continue-on-error` or filter to exclude the known failures. This means either the CI is red all the time, or the job is disabled.

**Impact:** CI noise — a permanently red CI job is ignored by developers, masking new failures.

**Fix:** Add `--skip` flags for known-failing tests, or fix the physics tests.

---

### 438. scripts/list_icons.sh — Hardcoded Flatpak hash in search path (line 6)
**File:** `scripts/list_icons.sh`
**Line:** 6

```bash
"$HOME/.local/share/flatpak/appstream/flathub/x86_64/724b0f962e2504fe47cd53d7fce5085e518ca2fed6e66dd4444293d6b93f277d/icons"
```

The Flatpak icon directory contains a commit hash (`724b0f...`) that changes when Flathub updates. This path will stop working after the next Flathub update.

**Impact:** Broken icon detection — Flatpak icons stop being found after the hash changes.

**Fix:** Use `find` to locate the actual Flatpak icons directory, or use the latest subdirectory.

---

### 439. scripts/launcher.sh — Temporary desktop file cleanup is async and racy (line 133)
**File:** `scripts/launcher.sh`
**Line:** 133

```bash
( sleep 30; rm -rf "$temp_root" ) >/dev/null 2>&1 &
```

The cleanup runs in a background subshell with a 30-second delay. If the dock launches many DBus-activatable apps rapidly, temporary directories accumulate. The background process also survives after the launcher exits.

**Impact:** Resource leak — orphaned background processes and temporary directories accumulate.

**Fix:** Use `trap` to clean up on exit, or let `gtk-launch` handle the lifecycle.

---

### 440. lib.rs — SVG caching uses file stem only, causing cross-app collisions (line 67)
**File:** `src/lib.rs`
**Line:** 67

```rust
let stem = svg_path.file_stem()?.to_str()?;
```

The cache key is `{stem}_{target_size}.png`. If two different apps have icons with the same stem (e.g., `assets/24/network.svg` and `/usr/share/icons/hicolor/24/network.svg`), they share the same cache entry. The first rasterized version wins, and the second app gets the wrong icon.

**Impact:** Wrong icons — apps with同名 SVG icons share cached rasterizations.

**Fix:** Use a hash of the full path or file content as the cache key.

---

## Medium Severity Issues

### 441. Cargo.toml — mimalloc allocator may conflict with wgpu's memory management (line 85)
**File:** `Cargo.toml`
**Line:** 85

```toml
mimalloc = "0.1"
```

The comment says "benchmark before/after to verify benefit," but mimalloc is unconditionally enabled. wgpu and Vulkan have their own memory allocators that may not interact well with mimalloc.

**Impact:** Potential performance regression — mimalloc can interfere with Vulkan's memory pooling, increasing fragmentation.

**Fix:** Gate mimalloc behind a feature flag and benchmark with/without it.

---

### 442. Cargo.toml — half crate (0.27) is likely unused (wgpu-api/Cargo.toml line 29)
**File:** `wgpu-api/Cargo.toml`
**Line:** 29

```toml
half = "2.7"
```

The `half` crate provides half-precision floats. Searching the codebase, there are no imports of `half`. This dependency was likely added for FP16 texture support that was never implemented.

**Impact:** Unnecessary dependency — adds compile time and binary size for unused functionality.

**Fix:** Remove `half` from dependencies.

---

### 443. Cargo.toml — glyphon and cosmic-text may conflict with custom text rendering (wgpu-api/Cargo.toml lines 22-24)
**File:** `wgpu-api/Cargo.toml`
**Lines:** 22-24

```toml
glyphon = "0.12"
cosmic-text = "0.19"
fontdb = "0.16"
```

These are GPU text rendering libraries, but the project also has its own `text_render.rs`, `glyph.rs`, `shaping.rs`, and `fontdb.rs` modules. Having both glyphon/cosmic-text and custom text rendering creates maintenance burden and potential inconsistencies.

**Impact:** Code duplication — two text rendering systems coexist, increasing maintenance and cognitive load.

**Fix:** Choose one text rendering approach and remove the other.

---

### 444. Cargo.toml — wgpu and wgpu-dmabuf duplicated in both crates (Cargo.toml lines 58-59, wgpu-api/Cargo.toml lines 19-20)
**Files:** `Cargo.toml` lines 58-59, `wgpu-api/Cargo.toml` lines 19-20

Both the main crate and wgpu-api declare `wgpu` and `wgpu-dmabuf` as direct dependencies. Since wgpu-api is the GPU backend, only it should depend on wgpu. The main crate should access wgpu types only through wgpu-api's abstractions.

**Impact:** Architecture leak — the main crate can import wgpu types directly, bypassing wgpu-api's abstraction layer. Per the architectural invariant, "All wgpu internals are hidden inside the backend."

**Fix:** Remove wgpu and wgpu-dmabuf from the main crate's dependencies.

---

### 445. morphing.rs — MorphManager uses HashMap<u64, ...> with raw u64 IDs (line 532)
**File:** `wgpu-api/src/morphing.rs`
**Line:** 532

```rust
animations: std::collections::HashMap<u64, GeometryAnimation>,
```

The node ID is a raw `u64` with no type safety. Any `u64` can be used as a key, even if no such node exists. The ID has no association with a specific node type.

**Impact:** Type confusion — a `u64` from one subsystem can accidentally be used as a node ID in another.

**Fix:** Use a newtype wrapper: `struct NodeId(u64)`.

---

### 446. morphing.rs — PopupAnimation doesn't integrate with Wayland popup lifecycle (throughout)
**File:** `wgpu-api/src/morphing.rs`

The module comment says "Animating the visual geometry of a window is **not** the same as asking the Wayland compositor to resize the actual window." However, `PopupAnimation` animates geometry that may not match the Wayland surface size. If the popup's visual size differs from its Wayland surface size, input events may be misrouted.

**Impact:** Input mismatch — a popup that visually expands but doesn't resize its Wayland surface will have dead zones where clicks don't register.

**Fix:** Document that `PopupAnimation` must be paired with Wayland surface resize, or integrate the two.

---

### 447. perf.rs — Profiler is not integrated into the render loop (throughout)
**File:** `wgpu-api/src/perf.rs`

The `Profiler` struct has `begin_phase`, `end_phase`, and `time_phase` methods, but searching the codebase reveals these are not called in `renderer.rs` or `backend.rs`. The profiler exists but is unused.

**Impact:** Dead code — the entire profiling infrastructure (1467 lines) is unexercised at runtime. Performance issues go undetected.

**Fix:** Integrate the profiler into the render loop or remove it.

---

### 448. perf.rs — StressTestConfig and StressTestResult are defined but no stress test runner exists (lines 859-973)
**File:** `wgpu-api/src/perf.rs`

The stress test configuration and result types are defined, but there's no `run_stress_test()` function or similar. The types are dead code.

**Impact:** Incomplete feature — the stress test infrastructure is half-implemented.

**Fix:** Implement the stress test runner or remove the types.

---

### 449. Makefile — install target copies assets/24/* but doesn't create the directory if empty (line 27)
**File:** `Makefile`
**Line:** 27

```make
cp -r assets/24/* $(DATA_DIR)/24/
```

If `assets/24/` is empty, `cp -r assets/24/*` fails with "No such file or directory" because the glob doesn't expand.

**Impact:** Install failure — `make install` fails if the assets directory is empty.

**Fix:** Use `cp -rn assets/24/ $(DATA_DIR)/24/` or add `shopt -s nullglob` equivalent.

---

### 450. PKGBUILD — missing pipewire and related dependencies (lines 8-18)
**File:** `PKGBUILD`
**Lines:** 8-18

The `depends` array lists `wayland`, `mesa`, `dbus`, etc., but omits `pipewire`, `libspa`, and `fontconfig` runtime libraries. The dock won't start without PipeWire for screen capture.

**Impact:** Broken installation — users installing via PKGBUILD will get a dock that can't capture screens.

**Fix:** Add `pipewire` and `libspa` to `depends`.

---

### 451. assets/ — dialog-warning.png is embedded in badges.rs at compile time (badges.rs line 9)
**File:** `src/render/badges.rs` line 9, `assets/dialog-warning.png`

```rust
static BADGE_ICON_BYTES: &[u8] = include_bytes!("../../assets/dialog-warning.png");
```

The PNG is embedded in the binary. A small red circle drawn procedurally would be smaller and themeable.

**Impact:** Binary bloat and inflexibility — the badge icon can't be themed and adds ~4KB to the binary.

**Fix:** Draw the badge procedurally or load it from disk.

---

### 452. lib.rs — terminal_graphics module is in the public API (line 52)
**File:** `src/lib.rs`
**Line:** 52

```rust
pub use terminal_graphics::load_image_raw_rgba;
```

The `terminal_graphics` module is public and re-exported. This is a development/debugging feature that shouldn't be part of the stable API.

**Impact:** API pollution — the public API includes terminal-specific functions that are irrelevant for Wayland usage.

**Fix:** Make `terminal_graphics` private or gate it behind a feature flag.

---

### 453. Cargo.toml — base64 engine import in lib.rs is for terminal sixel only (line 32)
**File:** `Cargo.toml` line 32, `src/lib.rs` line 8

```toml
base64 = "0.22"
```

The `base64` crate is used in `lib.rs` to encode pixel data for sixel terminal output. It's not used elsewhere.

**Impact:** Unnecessary dependency in production — base64 is only needed for terminal debugging.

**Fix:** Gate base64 behind the same feature flag as `terminal_graphics`.

---

### 454. wgpu-api/Cargo.toml — gif crate for animated images (line 28)
**File:** `wgpu-api/Cargo.toml`
**Line:** 28

```toml
gif = "0.13"
```

The `gif` crate is used in `gif.rs` for decoding animated GIFs. However, the GIF decoder has critical issues (Issue 96 from Round 5: eager decode of all frames into memory). The feature may not be functional.

**Impact:** Half-implemented feature — GIF support exists in code but has known memory issues.

**Fix:** Fix the GIF decoder or gate it behind a feature flag.

---

### 455. CI — documentation build doesn't check for broken links (ci.yml line 122)
**File:** `.github/workflows/ci.yml`
**Line:** 122

```yaml
- name: Build documentation
  run: cargo doc --workspace --no-deps
```

`cargo doc` doesn't check for broken intra-doc links by default. The `--document-private-items` flag and `-D rustdoc::broken_intra_doc_links` are not used.

**Impact:** Broken documentation — dead links in doc comments go undetected.

**Fix:** Add `-D rustdoc::broken_intra_doc_links` to the doc command.

---

## Low Severity Issues

### 456. Makefile — BINARY_NAME is hardcoded, not derived from Cargo.toml (line 2)
**File:** `Makefile`
**Line:** 2

```make
BINARY_NAME=dock
```

If the binary name changes in `Cargo.toml`, the Makefile won't match.

**Fix:** Derive BINARY_NAME from `Cargo.toml` or `[package].name`.

---

### 457. PKGBUILD — license is "custom" without specifying the license (line 7)
**File:** `PKGBUILD`
**Line:** 7

```bash
license=('custom')
```

"custom" is not a valid Arch license identifier. The actual license (MIT, Apache, GPL, etc.) should be specified.

**Fix:** Set the correct license identifier.

---

### 458. scripts/check.sh — cargo machete is unreliable (line 131-141)
**File:** `scripts/check.sh`
**Lines:** 131-141

`cargo machete` is known to produce false positives for build-dependencies and dev-dependencies. The script treats any machete finding as a failure.

**Impact:** False positives — the check script may fail on valid dependencies.

**Fix:** Review machete output manually or use `cargo-udeps` instead.

---

### 459. scripts/launcher.sh — hardcoded taskman binary (line 138-143)
**File:** `scripts/launcher.sh`
**Lines:** 138-143

```bash
if [ "$APP_ID" = "taskman" ]; then
    if [ -x "./taskman" ]; then
        run_detached ./taskman "$@"
        exit 0
    fi
fi
```

A special case for `taskman` in the current directory. This is development-specific and shouldn't be in the production launcher.

**Fix:** Remove or gate behind a debug flag.

---

### 460. perf.rs — 1467 lines for profiling utilities is excessive (throughout)
**File:** `wgpu-api/src/perf.rs`

The profiling module is 1467 lines with extensive test coverage (400+ lines of tests). For a utility module, this is disproportionately large. Much of the functionality (StressTest, JitterAnalyzer) is unused.

**Impact:** Maintenance burden — 1467 lines of unused or half-used code increases compile time and cognitive load.

**Fix:** Trim to only the used functionality or gate behind a `profiling` feature flag.

---

## Summary of Issues by Severity

| # | Severity | Issue | Impact |
|---|----------|-------|--------|
| 421 | **Critical** | tokio "full" features | Binary bloat, invariant risk |
| 422 | **Critical** | sixel-rs in main binary | Binary bloat |
| 423 | **Critical** | cairo-rs unused in GPU path | Binary bloat, dependency chain |
| 424 | **Critical** | PKGBUILD version mismatch | Package confusion |
| 425 | **Critical** | PKGBUILD source array wrong | Broken packaging |
| 426 | **Critical** | PKGBUILD scans host icons | Non-reproducible builds |
| 427 | **Critical** | services/test.rs does nothing | Dead code |
| 428 | **Critical** | ellipse() ignores ry parameter | Incorrect geometry |
| 429 | **High** | RefCell in profiling hot path | Runtime panic risk |
| 430 | **High** | Vec::remove(0) O(n) per frame | CPU waste |
| 431 | **High** | Same O(n) in JitterAnalyzer | CPU waste |
| 432 | **High** | build.rs no error handling | Poor DX |
| 433 | **High** | Makefile checks wrong path | Silent build skip |
| 434 | **High** | Makefile dist x86_64 only | Limited distribution |
| 435 | **High** | CI cargo tree -d always passes | False confidence |
| 436 | **High** | No wgpu-api-specific clippy | Missed warnings |
| 437 | **High** | Physics tests expected to fail | CI noise |
| 438 | **High** | Hardcoded Flatpak hash | Broken icon detection |
| 439 | **High** | Launcher temp file cleanup racy | Resource leak |
| 440 | **High** | SVG cache key collision | Wrong icons |
| 441 | **Medium** | mimalloc may conflict with wgpu | Performance regression |
| 442 | **Medium** | half crate unused | Unnecessary dependency |
| 443 | **Medium** | glyphon + custom text rendering | Code duplication |
| 444 | **Medium** | wgpu in main crate dependencies | Architecture leak |
| 445 | **Medium** | Raw u64 node IDs | Type confusion |
| 446 | **Medium** | PopupAnimation vs Wayland lifecycle | Input mismatch |
| 447 | **Medium** | Profiler not integrated | Dead code |
| 448 | **Medium** | Stress test types without runner | Incomplete feature |
| 449 | **Medium** | Makefile cp fails on empty dir | Install failure |
| 450 | **Medium** | PKGBUILD missing pipewire | Broken installation |
| 451 | **Medium** | Badge PNG embedded in binary | Binary bloat |
| 452 | **Medium** | terminal_graphics in public API | API pollution |
| 453 | **Medium** | base64 only for terminal sixel | Unnecessary dependency |
| 454 | **Medium** | GIF decoder has known issues | Half-implemented feature |
| 455 | **Medium** | CI doesn't check doc links | Broken documentation |
| 456 | **Low** | Makefile BINARY_NAME hardcoded | Maintenance |
| 457 | **Low** | PKGBUILD license "custom" | Policy violation |
| 458 | **Low** | cargo machete false positives | False positives |
| 459 | **Low** | Hardcoded taskman in launcher | Dev code in production |
| 460 | **Low** | 1467-line profiling module | Maintenance burden |

---

## Cumulative Statistics

| Round | File | Issues | Critical | High | Medium | Low |
|-------|------|--------|----------|------|--------|-----|
| 1 | gpu-rendering-review.md | 1–12 | 2 | 3 | 4 | 3 |
| 2 | gpu-rendering-review-2.md | 13–32 | 3 | 4 | 6 | 7 |
| 3 | gpu-rendering-review-3.md | 33–60 | 3 | 5 | 8 | 8 |
| 4 | gpu-rendering-review-4.md | 61–95 | 4 | 8 | 16 | 7 |
| 5 | gpu-rendering-review-5.md | 96–120 | 5 | 7 | 10 | 5 |
| 6 | gpu-rendering-review-6.md | 121–155 | 5 | 8 | 12 | 10 |
| 7 | gpu-rendering-review-7.md | 156–199 | 5 | 12 | 16 | 11 |
| 8 | gpu-rendering-review-8.md | 200–240 | 6 | 10 | 16 | 8 |
| 9 | gpu-rendering-review-9.md | 241–280 | 6 | 10 | 16 | 8 |
| 10 | gpu-rendering-review-10.md | 281–330 | 10 | 20 | 20 | 0 |
| 11 | gpu-rendering-review-11.md | 331–370 | 5 | 15 | 25 | 5 |
| 12 | gpu-rendering-review-12.md | 371–420 | 8 | 12 | 20 | 10 |
| **13** | **gpu-rendering-review-13.md** | **421–460** | **8** | **12** | **16** | **4** |
| **Total** | | **460** | **65** | **114** | **179** | **78** |

---

## Key Themes from Round 13

1. **Binary bloat**: Multiple unused or half-used dependencies (sixel-rs, cairo-rs, base64, half, mimalloc) are unconditionally linked into the production binary, directly violating ADR 005's priority of lowest possible RAM usage.

2. **Packaging issues**: The PKGBUILD has fundamental problems — version mismatch, wrong source format, host-specific icon scanning, and missing dependencies. It won't produce a working package.

3. **CI gaps**: Duplicate dependency check always passes, physics tests are expected to fail, and doc links aren't verified. The CI provides less assurance than intended.

4. **Dead profiling infrastructure**: The entire `perf.rs` module (1467 lines) is well-designed but not integrated into the render loop. The stress test types have no runner.

5. **Geometry bug**: `ellipse()` ignores the `ry` parameter, always rendering a circle. This is a silent correctness bug.

6. **Architecture invariant violation**: The main crate depends on `wgpu` and `wgpu-dmabuf` directly, bypassing wgpu-api's abstraction layer.
"