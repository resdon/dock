# Coding Conventions and Implementation Rules

## Code Style

### Indentation and Formatting

- **4 spaces** for indentation (no tabs).
- Run `cargo fmt` before committing.
- Maximum line length: **100 characters**.

### Naming Conventions

| Element | Convention | Example |
|---------|-----------|---------|
| Modules | `snake_case` | `physics_world`, `visual_effects` |
| Structs | `PascalCase` | `PhysicsWorld`, `MotionState` |
| Enums | `PascalCase` | `BodyType`, `InteractionCommand` |
| Functions | `snake_case` | `apply_force`, `step_physics` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_SUBSTEPS`, `DEFAULT_DT` |
| Type aliases | `PascalCase` | `WorldPosition`, `ForceVector` |
| Traits | `PascalCase` | `InteractionApi`, `Renderable` |
| Private fields | `snake_case` | `current_position`, `velocity` |

### Import Organization

```rust
// 1. Standard library
use std::collections::HashMap;
use std::sync::Arc;

// 2. External crates
use glam::Vec2;
use wgpu::Device;

// 3. Local crate modules
use crate::geometry::NodeId;
use crate::physics::body::PhysicsBody;
```

## Documentation Rules

### Public API Documentation

All public items **must** have Rust documentation comments (`///`):

```rust
/// Applies a force to a physics body.
///
/// # Coordinate Space
/// World-space coordinates (logical pixels).
///
/// # Units
/// Force is expressed in mass·pixels/second².
///
/// # Invariants
/// - The body must exist.
/// - Static bodies ignore applied forces.
///
/// # Threading
/// Must be called on the physics-world owner thread.
pub fn apply_force(&mut self, body: BodyId, force: Vec2) { ... }
```

### Required Documentation Sections

| Section | When Required |
|---------|---------------|
| `# Coordinate Space` | Always for functions with spatial parameters |
| `# Units` | When units are not obvious (seconds, pixels, radians) |
| `# Invariants` | When preconditions must hold |
| `# Ownership` | When resource lifecycle is non-trivial |
| `# Threading` | When thread affinity matters |
| `# Errors` | When functions can fail |

### Documentation Philosophy

- **Do** make important interfaces exceptionally well documented.
- **Do** make subsystem boundaries explicit.
- **Do** make architectural rules verifiable through tests and types.
- **Don't** add redundant comments that merely repeat the code.
- **Don't** try to make every source file extensively commented.

## Architectural Rules

### Non-Negotiable Rules

1. **Physics must not depend on wgpu.** The physics engine is pure Rust.
2. **Renderer must not calculate physical forces.** Rendering consumes transforms, not forces.
3. **PipeWire must not own renderer resources.** Video frames are uploaded via the renderer's API.
4. **Logical surfaces are distinct from Wayland surfaces.** Never conflate them.
5. **GPU resources are owned by the renderer.** No other subsystem creates wgpu resources directly.
6. **Physics uses a fixed timestep.** Never tie physics to the rendering frame rate.
7. **Rendering uses interpolated physics state.** Visual transforms are smooth between simulation steps.
8. **Cross-subsystem communication must use explicit interfaces.** No hidden dependencies.
9. **Coordinate spaces must never be mixed** without explicit conversion.
10. **PipeWire buffers must be released promptly.** Never retain beyond valid lifetime.

### Safe to Modify

- Adding new physics force types or constraints
- Adding new animation types or easing functions
- Adding new geometry primitives or path operations
- Adding new text styling options
- Adding new GPU pipeline types
- Adding new dock widget configurations
- Adding new event handlers
- Documentation and comments
- Test code

### Requires Caution

- Coordinate space conversions (viewport, transform)
- GPU resource lifetime management (texture, buffer)
- PipeWire buffer handling (acquire, release)
- Wayland protocol handlers (surface lifecycle)
- Scene graph traversal and mutation
- Physics world integration with rendering

## Error Handling

### Preferred Patterns

```rust
// Use Result for recoverable errors
pub fn apply_force(&mut self, body: BodyId, force: Vec2) -> Result<(), PhysicsError> {
    if !self.bodies.contains_key(&body) {
        return Err(PhysicsError::BodyNotFound(body));
    }
    // ...
    Ok(())
}

// Use Option for optional values
pub fn body_state(&self, body: BodyId) -> Option<&MotionState> {
    self.states.get(&body)
}

// Use expect() only for truly invariant conditions
let system = sysinfo::System::new_all().expect("Failed to create sysinfo System");
```

### Error Types

- Use custom error enums for domain-specific errors.
- Use `thiserror` for derive macros if the crate adds it.
- Never use `panic!` for recoverable conditions.

## Testing Rules

### Test Organization

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spring_force() {
        // Arrange
        let spring = Spring::new(100.0, 10.0, Vec2::ZERO, Vec2::new(10.0, 0.0));

        // Act
        let force = spring.compute_force();

        // Assert
        assert!((force.x + 100.0).abs() < 0.01);
    }
}
```

### Test Requirements

- All new public functions must have at least one test.
- Bug fixes must include a reproducer test.
- Physics tests should use deterministic seeds where applicable.
- Integration tests should verify cross-subsystem data flow.

## Change Management

### Before Implementation

1. Inspect relevant modules and their `AGENTS.md` files.
2. Read relevant architecture documentation in `docs/architecture/`.
3. Identify affected subsystem boundaries.
4. Propose a plan for nontrivial changes.
5. Check `docs/decisions/` for relevant ADRs.

### After Implementation

1. Update relevant documentation in `docs/architecture/`.
2. Update architectural diagrams in `docs/diagrams/` if data flow changes.
3. Add or update tests.
4. Run `cargo fmt`, `cargo check`, and relevant tests.
5. Report unresolved assumptions and limitations.
6. **Do not silently introduce new architectural dependencies.**
7. If a foundational decision changes, create a new ADR in `docs/decisions/`.

## Git Conventions

### Commit Messages

- Format: `<type>: <short description>`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`
- Examples:
  - `feat: add magnetic field to physics engine`
  - `fix: resolve borrow checker issue in solver`
  - `docs: update DMA-BUF architecture documentation`

### Co-Authorship

When committing changes, always add Junie as a co-author:

```bash
git commit --trailer "Co-authored-by: Junie <junie@jetbrains.com>"
```

## Reference

- **Rust engineering**: `docs/reference/rust-engineering.md`
- **System map**: `docs/architecture/SYSTEM_MAP.md`
- **ADRs**: `docs/decisions/`
