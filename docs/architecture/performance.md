# Performance Targets and Optimization

## FPS Targets

| Component | Target | Measurement |
|-----------|--------|-------------|
| Dock rendering | 60 FPS minimum, 120+ FPS on capable hardware | Frame time ≤ 16.67 ms |
| Physics simulation | 120 Hz fixed timestep | Step time ≤ 8.33 ms |
| Video capture | Match source frame rate (typically 30-60 FPS) | Frame acquisition + upload ≤ 33 ms |
| Input response | < 16 ms from event to first frame | End-to-end latency |

### Frame Budget Breakdown (60 FPS target)

```
Total: 16.67 ms
├── Wayland event dispatch: 1-2 ms
├── Physics step: 1-2 ms
├── Scene graph traversal: 0.5-1 ms
├── GPU command encoding: 1-2 ms
├── GPU rendering: 4-8 ms (varies by complexity)
├── Video frame upload: 1-3 ms (if active)
└── Slint widget rendering: 1-2 ms
```

## Latency Budgets

### Input-to-Display Latency

Target: **< 33 ms** (two frames at 60 Hz)

```
Input event → Wayland dispatch → Interaction processing → Physics step → Render → Display
    0ms          ~1ms              ~1ms             ~8ms         ~5ms       ~1-2ms
```

### Physics Latency

- Physics runs at **fixed 120 Hz** (~8.33 ms per step).
- Input events are incorporated at simulation boundaries.
- Visual state is **interpolated** between physics steps for smooth rendering.
- Drag operations use spring-based pointer following, not direct teleportation.

## Caching Strategies

### Icon Cache

- **Problem**: Repeated icon lookups and SVG decoding are expensive.
- **Solution**: Cache resolved icon paths and decoded images.
- **Implementation**: `dashmap::DashMap` for concurrent access.
- **Eviction**: Clean up when icons are removed; consider LRU for bounded growth.

### Image Decode Cache

- **Problem**: SVG and PNG decoding is CPU-intensive.
- **Solution**: Cache decoded pixel data with `lru::LruCache`.
- **Size**: Bound cache to prevent unbounded memory growth.

### Font Cache

- **Problem**: Font discovery and glyph outline extraction is slow.
- **Solution**: `fontdb::Database` caches font metadata; `swash` caches glyph outlines.
- **Lifetime**: Fonts loaded once per session.

### GPU Resource Cache

- **Problem**: Pipeline and shader compilation is expensive.
- **Solution**: wgpu caches pipelines automatically; ensure consistent pipeline descriptors.
- **Strategy**: Pre-create pipelines at initialization, not per-frame.

## Optimization Techniques

### 1. Batch GPU Commands

- Group draw calls by pipeline state to minimize state changes.
- Use render bundles for repeated scene elements.
- Minimize texture bindings by atlas-ing small textures.

### 2. Physics Optimization

- Use spatial indexing (quadtree) for collision detection.
- Limit physics bodies to interactive elements only.
- Use fixed timestep to avoid variable simulation costs.

### 3. Image Resize Optimization

- Use `fast_image_resize` with SIMD for image scaling.
- Pre-compute resize parameters to avoid per-frame allocation.
- Cache resized images when multiple sizes are needed.

### 4. DMA-BUF Zero-Copy

- Use DMA-BUF sharing for Wayland surface presentation.
- Avoid CPU copies between wgpu textures and Wayland buffers.
- Negotiate format and modifier at surface creation.

### 5. Allocation Reduction

- Use `mimalloc` for faster allocation (benchmark before enabling).
- Pre-allocate vectors and arenas for per-frame data.
- Use `crossbeam` lock-free structures where applicable.

## Benchmarking with Criterion

The project now includes `criterion` for benchmarking:

```rust
// In wgpu-api/benches/
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_physics_step(c: &mut Criterion) {
    c.bench_function("physics_120hz_step", |b| {
        b.iter(|| {
            // Set up physics world and step
            let mut world = PhysicsWorld::new();
            // Add bodies...
            world.step(1.0 / 120.0);
        });
    });
}

criterion_group!(benches, bench_physics_step);
criterion_main!(benches);
```

Run with: `cargo bench`

### What to Benchmark

| Operation | Why |
|-----------|-----|
| Physics step | Ensure 120 Hz target is met |
| Icon resolution | Cache effectiveness |
| SVG decoding | Decode time and cache hit rate |
| Image resize | SIMD vs non-SIMD performance |
| Text layout | Shaping and rasterization time |
| Scene traversal | Graph depth and node count impact |

## Profiling with Tracing

The project now includes `tracing` for structured logging and performance tracing:

```rust
use tracing::{info, warn, error, trace, instrument, Span};
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

// Initialize once at startup
tracing_subscriber::registry()
    .with(EnvFilter::from_default_env())
    .with(tracing_subscriber::fmt::layer())
    .init();

// Instrument functions
#[instrument(skip(self))]
pub fn render_frame(&mut self) {
    trace!("Rendering frame");
    // ...
}

// Time specific blocks
let _span = tracing::info_span!("physics_step").entered();
world.step(dt);
```

Set `RUST_LOG=dock=trace,wgpu_api=debug` for detailed logging.

## Performance Invariants

1. **Frame rendering** must not exceed the frame budget (16.67 ms for 60 FPS).
2. **Physics step** must complete within 8.33 ms (120 Hz target).
3. **Input latency** must be < 33 ms end-to-end.
4. **Cache growth** must be bounded to prevent OOM.
5. **GPU resource creation** must not happen on the critical rendering path.
6. **PipeWire buffer release** must be prompt to avoid stream stalls.

## Reference

- **Memory management**: `docs/architecture/memory.md`
- **Threading**: `docs/architecture/threading.md`
- **Testing**: `docs/architecture/testing.md`
- **GPU rendering**: `docs/reference/gpu-rendering.md`
