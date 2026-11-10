# Physics Subsystem — Local Rules

## What This Subsystem Does

Provides natural UI animation through three physical systems: springs (A), rigid bodies (B), and force fields (C). It is **pure Rust** with no wgpu, Wayland, or PipeWire dependencies.

## Files

| File | Purpose |
|------|---------|
| `mod.rs` | Module declarations and re-exports |
| `body.rs` | PhysicsBody, PhysicsObject, PhysicalProperties |
| `world.rs` | PhysicsWorld — orchestrates all three systems |
| `springs.rs` | Spring constraints, chains, radial systems |
| `forces.rs` | Force generators (spring, drag, repulsion, attraction, wind, push) |
| `collision.rs` | AABB/circle collision detection and resolution |
| `solver.rs` | Impulse-based contact resolution |
| `constraints.rs` | 9 constraint types + ConstraintSystem |
| `fields.rs` | Force fields (magnetic, cohesion, separation, radial, directional) |
| `groups.rs` | Body groups with cohesion and separation |
| `spatial_index.rs` | Spatial hash grid for broad-phase collision |
| `interaction.rs` | High-level InteractionApi trait |
| `commands.rs` | InteractionCommand enum and InteractionManager |
| `dock.rs` | Physical dock layout model (1D ordered layout) |
| `visual_effects.rs` | MotionState → VisualTransform bridge |
| `timestep.rs` | Fixed timestep simulation with interpolation |
| `rapier.rs` | Optional Rapier2D backend integration |

## Rules

1. **No wgpu imports.** This subsystem must compile without wgpu.
2. **No Wayland imports.** Physics is independent of windowing.
3. **No PipeWire imports.** Physics does not handle video.
4. **All coordinates are logical pixels.** Never use physical pixels or meters.
5. **Fixed timestep is mandatory.** Never use variable dt for integration.
6. **Use the annotation convention.** Every public function must document coordinate space, units, and invariants.
7. **Physics never modifies scene nodes directly.** Synchronization is through MotionState/VisualEffects.

## Testing

Run `cargo test physics` from `wgpu-api/`. Target: 334+ passing tests.

## Safe to Add

- New force types in `forces.rs`
- New constraint types in `constraints.rs`
- New field types in `fields.rs`
- New InteractionApi methods in `interaction.rs`
- New InteractionCommand variants in `commands.rs`
- New visual effects in `visual_effects.rs`
