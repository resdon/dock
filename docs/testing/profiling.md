# WAYDOCK — Profiling Guide

This document describes how to profile CPU, RAM, and GPU usage for the Waydock application. Profiling is mandatory before and after any rendering change to verify the core engineering priorities are met.

## Core Engineering Priorities

1. **Lowest possible CPU usage** — Measure idle and active CPU
2. **Lowest possible system RAM usage** — Track resident and peak memory
3. **Highest possible responsiveness** — Measure input-to-presentation latency
4. **Graphics, animations, interactivity** — Verify 60+ FPS during animations
5. **Lowest possible VRAM usage** — Monitor GPU memory allocation

## CPU Profiling

### Linux perf

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

### Tracy

Inspect CPU timelines, GPU activity, thread synchronization, and frame timing.

```bash
# Build with Tracy support (add to Cargo.toml dev-dependencies)
# tracy-client = "0.16"

# Run with Tracy server
tracy_server  # Start Tracy capture server
cargo run     # Run the dock
# Tracy captures automatically; press Ctrl+C to stop
```

**What to look for:**
- Frame time distribution — should be <16.67ms for 60 FPS
- Thread contention — locks held across frame boundaries
- GPU-CPU synchronization points — blocking waits

## RAM Profiling

### Heaptrack

Find excessive allocations, memory leaks, and unexpected allocation patterns.

```bash
# Install: sudo apt install heaptrack
heaptrack -- cargo run &
# Interact with the dock for 30 seconds
heaptrack-print-trace /tmp/heaptrack.result > heaptrack_trace.txt
heaptrack_gui /tmp/heaptrack.result &
```

**What to look for:**
- Growing memory over time → memory leak
- Per-frame allocations → should be zero in steady state
- Large allocations in icon loading → should be cached, not repeated

### Valgrind Massif

Analyze peak heap usage and memory growth.

```bash
# Install: sudo apt install valgrind
valgrind --tool=massif --pages-as-heap=yes cargo run &
# Run for 30 seconds, then stop
ms_print massif.out.* > massif_report.txt
# Visualize
ms_print massif.out.* | gnuplot -e "set terminal svg; set output 'massif.svg'; plot '-' using 2 with lines"
```

**What to look for:**
- Peak memory usage — should be bounded
- Memory growth over time → leak indicator
- Allocation hotspots — which functions allocate most

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

| Tool | Installs | Purpose | Output |
|------|----------|---------|--------|
| `perf` | `sudo apt install linux-tools-generic` | CPU hotspots, flamegraphs | SVG flamegraph |
| `heaptrack` | `sudo apt install heaptrack` | Allocation tracking, leaks | GUI report |
| `valgrind --tool=massif` | `sudo apt install valgrind` | Peak heap analysis | Text report |
| `tracy` | Add `tracy-client` dep | CPU/GPU timeline, frame timing | Live capture |
| `MangoHud` | `sudo apt install mangohud` | FPS, GPU load overlay | On-screen HUD |
| `RenderDoc` | Download from renderdoc.org | Frame capture, draw calls | Frame analysis |

## Related guides

- [memory-profiling.md](memory-profiling.md) — practical heap/process memory workflow (Heaptrack, DHAT, `/proc/smaps_rollup`, Dock-specific investigation targets)
