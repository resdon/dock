# wgpu-api — Library Rules

## What This Crate Does

Hardware-accelerated 2D rendering API built on wgpu (Vulkan). The public API is **independent of wgpu types** — application code describes **what** to render, not **how** to allocate GPU buffers.

## Subsystems

| Subsystem | AGENTS.md Location |
|-----------|-------------------|
| Physics | `src/physics/AGENTS.md` |
| Renderer | `src/renderer/AGENTS.md` |
| Video | `src/video/AGENTS.md` |
| Wayland | `src/wayland/AGENTS.md` |

## Global Rules

1. **Public API hides wgpu types.** Never expose `TextureView`, `RenderPipeline`, `BindGroup`, etc. in public interfaces.
2. **Coordinate spaces must be documented.** Every public function that takes or returns coordinates must state which coordinate space it uses.
3. **Physics is pure Rust.** No wgpu, Wayland, or PipeWire imports in the physics module.
4. **Renderer owns GPU resources.** No other subsystem creates wgpu resources directly.
5. **PipeWire buffers are released promptly.** Never retain borrowed buffers beyond their valid lifetime.
6. **Use the annotation convention.** See `AGENTS.md` at project root for the standard format.

## Testing

```bash
cd wgpu-api
cargo check          # Compilation
cargo test           # Full test suite
cargo test physics   # Physics tests only
```

## Dependencies

- `wgpu` — GPU rendering
- `glam` — Math (vectors, matrices, quaternions)
- `rapier2d` — Optional rigid-body physics
- `cosmic-text` — Text shaping
- `fontdb` — Font database
- `gif` — GIF decoding
