# Dock — Profiling Guide

This document describes how to profile CPU, RAM, and GPU usage for the Dock application. Profiling is mandatory before and after any rendering change to verify the core engineering priorities are met.

## Core Engineering Priorities

1. **Lowest possible CPU usage** — Measure idle and active CPU
2. **Lowest possible system RAM usage** — Track resident and peak memory
3. **Highest possible responsiveness** — Measure input-to-presentation latency
4. **Graphics, animations, interactivity** — Verify 60+ FPS during animations
5. **Lowest possible VRAM usage** — Monitor GPU memory allocation

## Build Profile

The project includes a `[profile.profiling]` build profile (inherits `release` with debug symbols) for Samply and Tracy:

```bash
cargo build --profile profiling
./target/profiling/dock
```

## Profiling Toolkit

### Tracy — CPU/GPU Timeline Profiler

**Primary profiler** for CPU timelines, frame timing, thread activity, and instrumented allocations.

**Installation:**
```bash
# Download from https://github.com/wolfpld/tracy/releases
# Or build from source:
git clone https://github.com/wolfpld/tracy.git
cd tracy && mkdir build && cd build && cmake .. && make -j
```

**Build & Run:**
```bash
# Build with Tracy zones (instrumented in src/main.rs)
cargo build --profile profiling --features tracy

# Start Tracy server, then run the dock
./tracy/build/tracy_server &
./target/profiling/dock
```

**Zone instrumentation** (in `src/main.rs`):
- `Wayland Events` — Wayland socket dispatch
- `Domain Events` — background channel processing
- `Animation Update` — fade, bounce, icon animation clocks
- `Render` — frame scheduler decision
- `Draw` — actual GPU render pass
- `Poll` — libc::poll wait for Wayland events

**What to look for:**
- Frame time distribution — should be <13ms for 75 FPS
- Thread contention — locks held across frame boundaries
- GPU-CPU synchronization points — blocking waits
- Zone durations — which loop phase consumes the most CPU

### Samply — CPU Flamegraph Profiler

**CPU investigation** tool for identifying CPU-intensive functions and unexplained busy threads.

**Installation:**
```bash
cargo install --locked samply
```

**Build & Run:**
```bash
cargo build --profile profiling
samply record ./target/profiling/dock
```

**Output:** Interactive flamegraph showing which functions consume CPU time, with per-thread inspection.

**What to look for:**
- Hot functions in the render loop → per-frame allocations
- Time spent in wgpu/Vulkan → GPU command submission overhead
- Time spent in cosmic-text → text shaping cost
- Time spent in PipeWire → frame conversion overhead

### wgpu-profiler — GPU Pass Timing

**GPU investigation** tool for measuring individual GPU rendering passes using timestamp queries.

**Build:**
```bash
# Enables TIMESTAMP_QUERY wgpu feature for GPU timestamp support
cargo build --profile profiling --features wgpu-api/profiler
```

**Note:** The `wgpu-profiler` crate is available as a dependency but requires manual integration due to API complexity with wgpu 30. The `profiler` feature enables `TIMESTAMP_QUERY` on the wgpu device, which is the prerequisite for GPU timestamp queries. For now, use Tracy's GPU profiling or RenderDoc for GPU timing.

**What to measure:**
- Is the GPU doing unnecessary work while the dock is idle?
- Which rendering pass is expensive?
- Does fading consume measurable GPU time?
- Are shadows or text rendering unexpectedly expensive?

### nvtop — Hardware GPU Monitor

**Hardware monitoring** for GPU utilization, VRAM, temperature, and power.

**Installation:**
```bash
# NVIDIA
sudo pacman -S nvtop  # Arch
sudo apt install nvtop  # Debian/Ubuntu

# AMD
sudo pacman -S nvtop

# Intel
sudo pacman -S intel-gpu-top
```

**Run:**
```bash
nvtop  # Real-time GPU monitoring
# Or for NVIDIA-specific:
nvidia-smi dmon  # Detailed GPU monitoring
```

**What to look for:**
- GPU utilization during idle → should be near 0%
- VRAM usage → should be bounded by texture cache limits
- Per-process GPU memory → dock's share of total VRAM

### DHAT — Heap Allocation Profiler

**Memory investigation** tool for finding allocation patterns, peak heap usage, and allocation frequency.

**Build & Run:**
```bash
# Build with DHAT global allocator
cargo build --profile profiling --features dhat-heap
./target/profiling/dock

# On exit, generates dhat-heap.json
# View in DHAT web viewer: https://nnethercote.github.io/dh_view/dh_view.html
# Upload dhat-heap.json to the viewer
```

**What to look for:**
- Allocation frequency — which functions allocate most often
- Peak heap usage — total memory at peak
- Allocation sizes — large allocations in icon loading or frame capture
- "Leaked" memory — long-lived state vs actual leaks

### Valgrind Massif — Heap Growth Over Time

**Memory investigation** tool for analyzing peak heap usage and memory growth over the application's lifetime.

**Run:**
```bash
# Install: sudo apt install valgrind / sudo pacman -S valgrind
valgrind --tool=massif --pages-as-heap=yes ./target/profiling/dock &
# Run for 30 seconds, then stop
ms_print massif.out.* > massif_report.txt
# Visualize
ms_print massif.out.* | gnuplot -e "set terminal svg; set output 'massif.svg'; plot '-' using 2 with lines"
```

**What to look for:**
- Peak memory usage — should be bounded
- Memory growth over time → leak indicator
- Allocation hotspots — which functions allocate most

### Linux perf — CPU Hotspots

Find CPU hotspots, unnecessary event processing, excessive wakeups, and expensive functions.

```bash
# Record CPU profile for 30 seconds
sudo perf record -g -F 99 -- sleep 30 &
# Run the dock interactively during recording
sudo perf record -g -p $(pgrep dock) -- sleep 30

# Generate flamegraph
sudo perf script | stackcollapse-perf.pl | flamegraph-perf.pl > cpu_flamegraph.svg

# Top functions by CPU time
sudo perf report --sort symbol

# Check for excessive wakeups
sudo perf stat -e context-switches,cpu-migrations,page-faults -p $(pgrep dock) sleep 10
```

**What to look for:**
- High CPU in idle state → event loop polling too frequently
- Excessive context switches → too many threads or frequent synchronization
- Hot functions in rendering path → per-frame allocations or redundant calculations

### Heaptrack — Allocation Tracking

Find excessive allocations, memory leaks, and unexpected allocation patterns.

```bash
# Install: sudo apt install heaptrack
heaptrack -- ./target/profiling/dock &
# Interact with the dock for 30 seconds
heaptrack-print-trace /tmp/heaptrack.result > heaptrack_trace.txt
heaptrack_gui /tmp/heaptrack.result &
```

**What to look for:**
- Growing memory over time → memory leak
- Per-frame allocations → should be zero in steady state
- Large allocations in icon loading → should be cached, not repeated

## Measurement Matrix

What to measure for each component:

| Component | Metrics | Tool |
|-----------|---------|------|
| Main event loop | CPU time, wakeups, iterations/sec | Tracy, perf |
| Animation system | CPU time, frame rate, invalidations | Tracy |
| Render engine | CPU time, GPU time, frames rendered | Tracy, wgpu-profiler |
| Font manager | Allocations, cache misses, memory | DHAT, heaptrack |
| Icon manager | Allocations, image cache size, loading time | DHAT, heaptrack |
| Window list | CPU time, memory, update frequency | Tracy, heaptrack |
| Context menus | Rendering time, allocation count | Tracy, DHAT |
| Wayland listeners | Events/sec, processing time | Tracy |
| GPU resources | Buffer sizes, texture sizes, estimated VRAM | nvtop, RenderDoc |
| PipeWire capture | Frame copies, conversion time, memory | Tracy, heaptrack |

## Recommended Workflow

1. **Start with Tracy** — gives a unified view of CPU activity, GPU timing, thread synchronization, and manually instrumented memory events
2. **Add wgpu-profiler** — for per-render-pass timings (when API is compatible)
3. **Use Samply** — when you need deeper CPU investigation with flamegraphs
4. **Use nvtop** — for real-time GPU/VRAM monitoring alongside other profilers
5. **Use DHAT/Massif** — for memory investigation when heap profiling is needed

## Tool Comparison

| Tool | Type | CPU | GPU | Memory | Real-time | Setup |
|------|------|-----|-----|--------|-----------|-------|
| Tracy | Timeline | ✓ | ✓ | ✓ | ✓ | Build with `--features tracy` |
| Samply | Flamegraph | ✓ | — | — | — | `cargo install samply` |
| wgpu-profiler | GPU timing | — | ✓ | — | — | Build with `--features wgpu-api/profiler` |
| nvtop | Hardware monitor | — | ✓ | ✓ | ✓ | `sudo pacman -S nvtop` |
| DHAT | Heap profiler | — | — | ✓ | — | Build with `--features dhat-heap` |
| Massif | Heap growth | — | — | ✓ | — | `sudo apt install valgrind` |
| perf | CPU profiler | ✓ | — | — | — | `sudo apt install linux-tools` |
| heaptrack | Allocation tracker | — | — | ✓ | — | `sudo apt install heaptrack` |

## Performance Test Scenarios

Run each scenario and measure the metrics below.

### 1. Idle Test (5 minutes)

```bash
# Start dock, do nothing for 5 minutes
cargo run &
sleep 300
# Measure CPU and RAM during idle
ps -o %cpu,rss -p $(pgrep dock)
```

**Expected:**
- CPU: Near 0% (occasional spikes from system events)
- RAM: Stable, no growth
- Rendering: Zero frames rendered while idle

### 2. Pointer Hover Test

```bash
# Move pointer repeatedly over dock icons for 30 seconds
# Use xdotool or wtype for automation
xdotool mousemove 100 700 sleep 0.1 mousemove 200 700 sleep 0.1
```

**Expected:**
- CPU: <5% average
- Frame rendering: Only on hover state change, not continuous
- RAM: No growth

### 3. Animation Test

Open and close popups repeatedly, trigger icon animations.

**Expected:**
- FPS: ≥60 during animations
- CPU: <20% during active animation
- RAM: Stable

### 4. Image Load/Unload Test

Load and unload icons (e.g., by changing pinned apps).

**Expected:**
- CPU: Spike during load, then settle
- RAM: Bounded growth with texture cache limits
- VRAM: Textures cached, not reloaded

### 5. Orientation Switch Test

Switch between bottom, top, left, and right dock orientations.

**Expected:**
- No memory leak across switches
- Smooth transition animation

## Performance Report Template

After profiling, fill in this template:

| Metric | Target | Measured | Status |
|--------|--------|----------|--------|
| Idle CPU usage | Near 0% | | |
| Idle RAM | Bounded | | |
| Peak RAM | Measured under stress | | |
| Animation FPS | ≥60 | | |
| Input latency | <50ms | | |
| VRAM usage | Bounded | | |
| Per-frame allocations | 0 | | |
| Memory leaks | None | | |

## Tools Summary

| Tool | Install | Purpose | Output |
|------|---------|---------|--------|
| Tracy | `--features tracy` | CPU/GPU timeline, frame timing | Live capture |
| Samply | `cargo install samply` | CPU flamegraphs | Interactive SVG |
| wgpu-profiler | `--features wgpu-api/profiler` | GPU pass timing | Timestamp queries |
| nvtop | `sudo pacman -S nvtop` | GPU/VRAM monitoring | Real-time TUI |
| DHAT | `--features dhat-heap` | Heap allocation patterns | JSON report |
| Massif | `sudo apt install valgrind` | Peak heap analysis | Text report |
| `perf` | `sudo apt install linux-tools` | CPU hotspots, flamegraphs | SVG flamegraph |
| `heaptrack` | `sudo apt install heaptrack` | Allocation tracking, leaks | GUI report |
| `RenderDoc` | Download from renderdoc.org | Frame capture, draw calls | Frame analysis |

## Related guides

- [memory-profiling.md](memory-profiling.md) — practical heap/process memory workflow (Heaptrack, DHAT, `/proc/smaps_rollup`, Dock-specific investigation targets)
