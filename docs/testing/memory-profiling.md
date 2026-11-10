# WAYDOCK — Memory & Heap Profiling Guide

This guide complements [`profiling.md`](profiling.md) with practical heap-allocation and process-memory workflows for the Dock on Linux (Arch/Wayland). Use it to find allocation hotspots, retained memory, and real RAM footprint (including graphics/shared-memory mappings).

> **Important:** High RAM usage does not necessarily mean excessive Rust heap allocations. It can also come from GPU textures/buffers, Wayland SHM buffers, DMA-BUF/mapped resources, PipeWire frame buffers, fonts/glyph atlases, shared libraries, and compositor-side surfaces. Always correlate heap profilers with process memory accounting.

## 1. Heap profiling — identify allocation hotspots (Heaptrack)

**Best starting point.** Heaptrack records allocations and associates them with call stacks.

### Install (Arch)

```bash
sudo pacman -S heaptrack
```

### Build with debug symbols

```bash
RUSTFLAGS="-C debuginfo=2" cargo build --release
```

### Run

```bash
heaptrack ./target/release/dock
```

Exercise the dock while profiling:
- Open/close window list and context menus
- Open/close popups (audio, bluetooth, rclone, process, tray menus)
- Trigger hover/animations, drag icons, scroll
- Start/stop capture/preview features if applicable
- Leave idle to observe steady-state behavior
- Exit cleanly to inspect leaked/outstanding allocations

Inspect results:

```bash
heaptrack_gui heaptrack.*.zst
```

### Metrics to review

| Metric | What it tells you |
|---|---|
| Peak heap memory | Maximum allocated heap memory |
| Temporary allocations | Memory allocated and subsequently freed |
| Leaked/outstanding allocations | Allocations still live at exit (or retained) |
| Allocation count | Number of allocation operations |
| Allocation hotspots | Functions/call stacks responsible for most allocations/bytes |

**Dock-specific suspects:** icon/image decode caches, thumbnail buffers, window/tray/process registries, popup render buffers, string-heavy metadata/desktop entries, per-frame formatting/cache keys, animation state maps, PipeWire frame copies (`to_vec()` paths), text/font shaping caches.

> Heaptrack traces heap allocations; it may not account for all GPU memory or shared-memory mappings. Correlate with `/proc/<pid>/smaps_rollup` and GPU tooling.

## 2. Process memory monitoring — actual RAM while running

Linux exposes detailed memory accounting via `/proc`.

```bash
PID=$(pgrep -n -x dock)
cat /proc/$PID/smaps_rollup
```

### Useful fields

| Field | Meaning |
|---|---|
| `Rss` | Physical RAM currently resident |
| `Pss` | Proportional share of RAM (best for processes with shared pages/mappings) |
| `Private_Dirty` | Private modified memory |
| `Anonymous` | Anonymous memory (includes much of heap/stack allocations) |
| `Pss_Shmem` | Proportional shared-memory usage |
| `Swap` | Memory swapped out |

`Pss` is especially useful for this dock because rendering may involve Wayland SHM buffers (main dock/Slint software path), shared libraries, and mapped resources.

### Monitor continuously

```bash
watch -n 1 "cat /proc/$PID/smaps_rollup"
# or
pmap -x $PID
```

`pmap -x` helps distinguish anonymous allocations, shared memory, libraries, file-backed mappings, and graphics-related mappings.

## 3. In-process allocation profiling with DHAT (Rust)

Use DHAT when you need Rust-side allocation call stacks and lifetime/usage patterns beyond Heaptrack.

### `Cargo.toml`

```toml
[dependencies]
dhat = { version = "0.3", optional = true }

[features]
dhat-heap = ["dep:dhat"]
```

### Global allocator and profiler

```rust
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // existing initialization...
}
```

### Run and inspect

```bash
cargo run --release --features dhat-heap
# generates dhat-heap.json; inspect with DHAT viewer
```

**Notes:** DHAT adds overhead (do not use for performance benchmarks). Prefer release builds with debug info for useful stacks.

## 4. Measure individual Rust data structures

Useful for validating cache/registry sizes against profiler results.

```rust
use std::mem::size_of;

fn vec_memory<T>(name: &str, items: &Vec<T>) {
    let bytes = items.capacity() * size_of::<T>();
    println!(
        "[MEMORY] {}: {} items, capacity {}, ~{:.2} KB",
        name,
        items.len(),
        items.capacity(),
        bytes as f64 / 1024.0
    );
}

fn string_memory(name: &str, value: &String) {
    println!(
        "[MEMORY] {}: len={} capacity={} bytes",
        name,
        value.len(),
        value.capacity()
    );
}
```

**Caveats:** This measures backing storage/capacity, not necessarily owned heap behind elements (e.g. `Vec<String>` string buffers), hashmap bucket overhead, or allocator overhead.

## 5. Dock-specific areas to investigate first

| Component | What to investigate |
|---|---|
| Icon indexer/cache | Cached/decoded icons, duplicates, eviction, path/key strings |
| Window registry/list | Stale window models/handles, retained metadata/titles, preview data |
| Thumbnail/capture | Image/frame buffers, dimensions, copies, retention after popup close |
| Glyph/font/text | Font DB/faces, glyph caches/atlases, shaping buffers, fallback chains |
| Wayland rendering (SHM/wgpu) | SHM pool/buffer reuse, resized buffers, surface lifetimes, wgpu textures/bind groups/registries |
| Tray (SNI/DBus) | Icons/pixmaps, menu models, retained items after removal |
| Process/rclone/audio monitors | Snapshot histories, accumulated lists, polling allocations |
| Animation/interaction state | Per-icon/frame allocations, global maps (mutex-protected), temporary collections |
| PipeWire capture (`src/services/pipewire_capture.rs`) | Per-frame `Vec` copies, format/state retention, thread lifecycle/teardown |

## 6. Recommended workflow

1. **Baseline:** record RSS/PSS after startup and after idle settle.
2. **Heap profile:** run Heaptrack to find largest allocation hotspots.
3. **Exercise features individually:** window list, menus, tray, thumbnails/capture, animations, drag/scroll.
4. **Check post-close retention:** after closing popups/windows/features, verify memory returns toward baseline (watch for leaks/retained caches).
5. **Subsystem counters:** add debug-only measurements for caches/registries/buffers and compare against Heaptrack/PSS.
6. **Correlate:** heap allocations vs anonymous/shared/file-backed mappings vs GPU memory.

### Example debug-only report (conceptual)

```text
[MEMORY REPORT]
Application:
  RSS: ...
  PSS: ...

Subsystem estimates/measured buffers:
  Icon cache: ...
  Window registry: ...
  Thumbnail/capture buffers: ...
  Glyph/text caches: ...
  Tray state: ...
  Wayland SHM/wgpu resources: ...
```

Treat subsystem numbers as estimates or tracked buffer sizes, not automatically attributable portions of RSS.

## References
- See also: [`profiling.md`](profiling.md), [`TESTING_PROTOCOL.md`](TESTING_PROTOCOL.md)
- Linux procfs memory fields: kernel `proc` documentation (`/proc/[pid]/smaps_rollup`)
- Heaptrack: https://github.com/KDE/heaptrack
- DHAT: https://docs.rs/dhat
