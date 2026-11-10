# Renderer Contract

## Purpose

This contract defines the boundaries and responsibilities of the renderer subsystem. It specifies what the renderer owns, what it provides, and what it must not do.

## Ownership

### Renderer Owns
- The wgpu device and queue
- All GPU textures (`TextureId` → `TextureView` mapping)
- All GPU surfaces (`SurfaceId` → `Surface` mapping)
- All render pipelines and their caches
- All GPU buffers (uniform, instance, vertex)
- Command encoder lifecycle

### Renderer Does NOT Own
- Scene graph nodes
- Physics bodies
- PipeWire streams
- Wayland surfaces (created by Wayland module)
- Application state

## Interface

### Provides
- `Renderer::new()` — Async initialization
- `Renderer::create_surface()` — Create a render target
- `Renderer::create_texture()` — Create a GPU texture
- `Renderer::with_frame()` — Record and submit a frame
- `Renderer::upload_texture()` — Update texture data
- `Renderer::remove_texture()` — Destroy a texture
- `Renderer::stats()` — Query renderer statistics

### Consumes
- `Command` — Draw commands (fill, stroke, text, image)
- `Scene` — Scene graph for traversal
- `Viewport` — Coordinate transformation info

## Invariants

1. **Renderer never calculates physical forces.** Physics is handled by the physics engine.
2. **Renderer consumes interpolated transforms.** It does not run physics simulation.
3. **GPU resources are owned by the renderer.** No other subsystem creates wgpu resources directly.
4. **Texture uploads must not happen unnecessarily.** Reuse existing textures when possible.
5. **Rendering must support multiple targets.** Multiple surfaces can be rendered independently.
6. **Renderer must not own PipeWire stream lifetimes.** VideoManager owns streams.

## Error Handling

- `RenderError` is returned for GPU failures
- Device loss is detected and reported
- Surface configuration errors are handled gracefully

## Threading

- Renderer is typically used on a single thread
- GPU submission is asynchronous
- Frame callbacks are synchronous within `with_frame`
