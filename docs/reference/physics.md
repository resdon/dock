# Physics & Math Reference

Covers: **rapier2d**, **nalgebra**, **glam**

---

## 1. rapier2d

**Version:** `0.36.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/rapier2d |
| GitHub Repository | https://github.com/dimforge/rapier |
| Rapier Book | https://rapier.rs/docs |
| Examples | https://github.com/dimforge/rapier/tree/main/examples |

### Architecture in Dock

rapier2d provides rigid-body physics simulation. Dock uses it as the **System B** (rigid-body) backend in the three-system physics architecture.

#### Three-System Architecture

| System | Implementation | Purpose |
|--------|---------------|---------|
| A (Springs) | Custom Rust | Dock icon arrangement, smooth dragging, magnetic snapping |
| B (Collision) | Custom Rust | AABB/circle collision detection, simple impulse resolution |
| C (Fields) | Custom Rust | Attraction, repulsion, group cohesion/separation |

#### Key Concepts

- **RigidBody**: A physical object with mass, position, velocity, and collision shape.
- **Collider**: A collision shape attached to a rigid body.
- **Joint**: A constraint between two rigid bodies (fixed, prismatic, revolute, etc.).
- **Force**: A continuous influence on a body's motion.
- **Impulse**: An instantaneous change in a body's velocity.
- **Integration**: Advancing the simulation by one timestep.

#### Body Types

| Type | Behavior |
|------|----------|
| Fixed | Immovable, infinite mass |
| Dynamic | Moves under forces and collisions |
| Kinematic | Moves by setting position/velocity, not forces |

#### Usage in Dock

rapier2d is used for:
- Collision detection between dock icons
- Physical pushing and bouncing effects
- Joint-based constraints between related objects
- Spatial queries (ray casting, point containment)

#### Integration Notes

- rapier2d uses `nalgebra` for linear algebra, not `glam`. Coordinate conversions are needed.
- The physics engine operates in **logical pixel coordinates**.
- rapier2d is independent of wgpu — no GPU dependencies.
- Use the `RapierBackend` wrapper in `wgpu-api/src/physics/rapier.rs` for Dock-specific integration.

#### Pixel-to-Meter Conversion

rapier2d expects meter-scale coordinates. Dock uses a conversion factor:

```
1 meter = 100 logical pixels
```

This scaling ensures stable simulation behavior at UI-scale distances.

---

## 2. nalgebra

**Version:** `0.35`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/nalgebra |
| GitHub Repository | https://github.com/dimforge/nalgebra |
| Book | https://nalgebra.org |

### Architecture in Dock

nalgebra is the linear algebra backend for rapier2d. Dock does not use nalgebra directly — all vector/matrix operations go through `glam`. However, understanding nalgebra is needed for rapier2d integration.

#### Key Types

| Type | Purpose |
|------|---------|
| `Vector2<f32>` | 2D vector |
| `Vector3<f32>` | 3D vector |
| `Matrix2<f32>` | 2x2 matrix |
| `Matrix3<f32>` | 3x3 matrix |
| `Isometry2<f32>` | 2D transformation (translation + rotation) |

#### Integration Notes

- nalgebra types are not directly compatible with glam types.
- Use conversion functions when passing data between rapier2d (nalgebra) and the rest of Dock (glam).
- The `RapierBackend` wrapper handles nalgebra↔glam conversions internally.

---



## 3. glam

**Version:** `0.29`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/glam |
| GitHub Repository | https://github.com/bitshifter/glam-rs |

### Architecture in Dock

glam is the primary math library for Dock. All vector, matrix, and quaternion operations use glam.

#### Key Types

| Type | Purpose |
|------|---------|
| `Vec2` | 2D vector (position, velocity, force) |
| `Vec3` | 3D vector |
| `Vec4` | 4D vector |
| `Mat2` | 2x2 matrix |
| `Mat3` | 3x3 matrix |
| `Mat4` | 4x4 matrix |
| `Quat` | Quaternion (rotation) |
| `Affine2` | 2D affine transformation |

#### Usage in Dock

glam is used throughout the codebase for:
- Physics calculations (positions, velocities, forces)
- Transform matrices (translation, rotation, scaling)
- Geometry operations (intersections, projections)
- Animation interpolation

#### Integration Notes

- glam is zero-cost and SIMD-optimized.
- All Dock physics uses glam, not nalgebra.
- When integrating with rapier2d, convert between glam and nalgebra types.

---

## 4. bytemuck

**Version:** `1` (latest compatible)

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/bytemuck |
| GitHub Repository | https://github.com/jhpratt/bytemuck |

### Architecture in Dock

bytemuck provides zero-cost byte-level type casting. Dock uses it for:
- Converting between Rust types and GPU buffer layouts
- Ensuring memory safety when casting pixel data
- Safe reinterpret casts for uniform buffer data

#### Key Concepts

- **Pod**: Plain Old Data — types that can be safely zero-initialized and have no invalid bit patterns.
- **Zeroable**: Types that can be safely created from zeroed memory.
- **NoUninit**: Types with no uninitialized state.
- **TransparentWrapper**: A type whose memory layout is identical to its wrapped type.

#### Usage in Dock

bytemuck is used for:
- Casting `Vec2`/`Mat4` to byte slices for GPU uniform buffers
- Converting pixel formats (RGBA8 → byte arrays) for texture uploads
- Ensuring GPU buffer safety through compile-time type checks

#### Integration Notes

- Use `#[derive(Pod, Zeroable)]` on custom types that need GPU buffer access.
- Prefer `bytemuck::cast_slice()` over raw pointer casts for safety.
- All types passed to wgpu buffers must implement `bytemuck::Pod`.
- bytemuck catches layout mismatches at compile time, preventing GPU memory corruption.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| rapier2d | **Removed** | Per ADR 005, analytical springs are sufficient |
| nalgebra | **Removed** | Only needed by Rapier2D |
| glam | `0.29` | Primary math library for Dock |
| bytemuck | `1` | Zero-cost byte casting for GPU buffers |

**Note:** Rapier2D and nalgebra were removed per ADR 005. glam is the sole math library. bytemuck works with glam types for GPU buffer safety.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Collision not detected | Collider not attached | Ensure collider is linked to rigid body |
| Physics unstable | Timestep too large | Use fixed timestep (120 Hz) |
| Vector mismatch | glam vs nalgebra | Use conversion functions |
| Joint breaks | Too much force | Increase joint strength or add limits |
| Body sleeping | No forces applied | Wake body or reduce sleep threshold |
| GPU buffer mismatch | Type not Pod | Derive `Pod, Zeroable` on custom types |
| Byte cast panic | Alignment mismatch | Check `#[repr(C)]` and alignment |

## Testing

- **Physics changes**: `cargo test physics` in wgpu-api
- **Rapier changes**: Visual inspection + unit tests
- **Math changes**: Verify vector/matrix operations
- **bytemuck changes**: Compile-time type checks catch most issues
