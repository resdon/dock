# Renderer Subsystem — Local Rules

## What This Subsystem Does

Manages the wgpu device, queue, pipelines, and caches. It is the **single entry point** for creating surfaces, textures, and rendering frames. All wgpu internals are hidden from application code.

## Files

| File | Purpose |
|------|---------|
| `renderer.rs` | Renderer struct — GPU device, surfaces, textures |
| `gpu.rs` | Low-level GPU backend — pipelines, buffers, caches |
| `render.rs` | Rendering pipeline — draw commands, frame state |
| `command.rs` | Draw command recording |
| `surface.rs` | Surface management |
| `texture.rs` | Texture management |
| `batching.rs` | Draw call batching |
| `shaders.rs` | WGSL shader sources |

## Rules

1. **Renderer owns the wgpu device and queue.** No other subsystem creates wgpu resources directly.
2. **Public API hides wgpu types.** Use opaque handles (`TextureId`, `SurfaceId`), not `TextureView`, `RenderPipeline`, etc.
3. **Never calculate physical forces.** Physics is handled by the physics engine.
4. **Consume interpolated transforms.** Do not run physics simulation.
5. **Texture uploads must not happen unnecessarily.** Reuse existing textures when possible.
6. **Support multiple render targets.** Multiple surfaces can be rendered independently.
7. **Do not own PipeWire stream lifetimes.** VideoManager owns streams; renderer only consumes textures.

## Coordinate Spaces

- **Input**: Logical pixels (UI coordinates)
- **Output**: Physical pixels (framebuffer coordinates)
- **Conversion**: Viewport handles logical→physical transformation

## Testing

Run `cargo check` from `wgpu-api/`. Rendering changes require visual inspection.

## Safe to Add

- New pipeline types
- New texture formats
- New render states (blend modes, clip modes)
- Optimized batch sorting
- New GPU cache strategies
