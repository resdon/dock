# ADR-003: Coordinate Systems

**Status**: Accepted
**Date**: 2024-03-10
**Context**: Multiple coordinate spaces across rendering, physics, and Wayland

## Problem

The system has at least 7 different coordinate spaces:

1. **Logical UI coordinates** — UI layout coordinates
2. **Physical framebuffer coordinates** — GPU pixel coordinates
3. **Physics world coordinates** — Simulation space
4. **Texture UV coordinates** — Normalized [0,1] range
5. **Local node coordinates** — Relative to a scene node's origin
6. **Parent-relative coordinates** — Relative to a node's parent
7. **Wayland surface coordinates** — Surface-relative, may differ due to scale factor

Mixing these causes subtle rendering and interaction bugs that are difficult to diagnose.

## Decision

**Mandatory coordinate space annotation** in all public APIs. Every function that takes or returns coordinates must document which coordinate space it uses.

### Annotation Format

```rust
/// Applies a force to a physics body.
///
/// # Coordinate Space
/// World-space coordinates (logical pixels).
///
/// # Units
/// Force is expressed in pixels per second squared.
///
/// # Invariants
/// - The body must exist.
/// - Static bodies ignore applied forces.
pub fn apply_force(&mut self, body: BodyId, force: Vector2D);
```

### Type-Level Distinction (Future)

Where conversions matter, use distinct types:

```rust
struct WorldPosition(Vec2);
struct LogicalPosition(Vec2);
struct PhysicalPosition(Vec2);
struct LocalPosition(Vec2);
```

### Current Convention

| Subsystem | Coordinate Space |
|-----------|-----------------|
| Physics | Logical pixels (world coordinates) |
| Scene graph | Logical pixels |
| Layout | Logical pixels |
| Renderer input | Logical pixels |
| Renderer output | Physical pixels |
| Wayland | Surface coordinates |
| Textures | UV [0,1] |

## Consequences

**Positive**:
- Prevents coordinate space mixing bugs
- Makes code self-documenting for AI agents and humans
- Enables future type-level enforcement
- Easier debugging when coordinate issues arise

**Negative**:
- More verbose documentation
- Requires discipline to maintain annotations
- Type-level distinction is future work

## Alternatives Considered

- **Single coordinate space**: Too restrictive; different subsystems genuinely need different spaces.
- **Runtime conversion tracking**: Too complex and error-prone; documentation is sufficient for now.
- **Immediate type-level enforcement**: Too disruptive for existing code; adopted as a future goal.
