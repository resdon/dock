# ADR-001: Renderer Ownership

**Status**: Accepted
**Date**: 2024-01-15
**Context**: GPU resource management in a multi-subsystem rendering engine

## Problem

Multiple subsystems (physics, video, animation, scene graph) need to interact with GPU resources. Without clear ownership rules, resource leaks, double-frees, and use-after-free bugs are likely.

## Decision

The `Renderer` struct is the **sole owner** of the wgpu device and queue. No other subsystem creates or destroys wgpu resources directly.

### Rules

1. All GPU resource creation goes through the renderer's API (`renderer.create_texture()`, `renderer.create_surface()`, etc.)
2. Public API uses opaque handles (`TextureId`, `SurfaceId`), not wgpu types
3. GPU resources are cleaned up when the renderer is dropped
4. Subsystems request resources from the renderer; they do not create them directly

## Consequences

**Positive**:
- Single point of control for GPU resource lifecycle
- Easy to add resource tracking, debugging, and limits
- Prevents resource leaks from subsystem bugs
- Clear API boundary between "what to render" and "how to render"

**Negative**:
- Renderer becomes a central coordination point
- All subsystems must go through the renderer for GPU operations
- Slightly more indirection for simple operations

## Alternatives Considered

- **Shared ownership**: Each subsystem manages its own GPU resources. Rejected because it leads to inconsistent cleanup and makes resource tracking difficult.
- **Arena allocator**: GPU resources are allocated from a shared arena. Rejected because it doesn't solve the fundamental ownership question.
