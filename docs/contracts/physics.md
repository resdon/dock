# Physics Contract

## Purpose

This contract defines the boundaries and responsibilities of the physics engine. It specifies what physics owns, what it provides, and what it must not do.

## Ownership

### Physics Owns
- Body positions and velocities
- Body accelerations (reset after each integration step)
- Spring constraints and their parameters
- Force fields and their sources
- Collision detection and resolution
- Group memberships and relationships
- Constraint system state

### Physics Does NOT Own
- wgpu resources (textures, buffers, pipelines)
- Scene graph nodes
- Wayland surfaces
- PipeWire streams
- Visual appearance (squash, stretch, glow — handled by VisualEffects)

## Interface

### Provides
- `PhysicsWorld::new()` — Create a physics world
- `PhysicsWorld::add_body()` — Add a physics body
- `PhysicsWorld::remove_body()` — Remove a body
- `PhysicsWorld::step()` — Advance simulation by dt
- `InteractionApi::drag()` — Begin dragging a body
- `InteractionApi::push()` — Apply directional force
- `InteractionApi::pull()` — Apply attraction force
- `InteractionApi::insert()` — Insert body into layout
- `InteractionApi::cut()` — Split a group
- `InteractionApi::merge()` — Combine groups
- `FixedTimestepSimulator::step()` — Fixed timestep simulation
- `VisualEffects::compute()` — Derive visual transforms from physics state

### Consumes
- `InteractionCommand` — High-level interaction intent
- `Point`, `Vector2D`, `Rect` — Geometry types
- `NodeId` — Visual node identifier (bridge to scene graph)

## Invariants

1. **Physics owns body positions and velocities.** No external code modifies them directly.
2. **Physics never accesses wgpu resources.** It is pure Rust.
3. **Physics uses a fixed timestep.** Default 120 Hz, decoupled from rendering.
4. **Physics does not directly modify scene nodes.** Synchronization happens through an explicit interface.
5. **Scene synchronization happens through an explicit interface.** Physics produces MotionState; VisualEffects derives transforms.
6. **Input events are converted into interaction commands.** Physics does not receive raw input.
7. **Physics must not depend on Wayland.** No Wayland types in the physics module.
8. **Static bodies ignore applied forces.**
9. **Acceleration is reset after each integration step.** Forces don't accumulate across frames.

## Coordinate Space

- All physics operates in **logical pixel coordinates**.
- Physics world coordinates = logical UI coordinates.

## Units

- Position: Logical pixels
- Velocity: Pixels per second
- Acceleration: Pixels per second²
- Time: Seconds
- Mass: Arbitrary design units

## Dependencies

- **Depends on**: `geometry` (Point, Rect, Vector2D, Size, NodeId)
- **Must NOT depend on**: wgpu, Wayland, PipeWire, scene graph
