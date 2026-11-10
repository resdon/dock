# ADR 005: WAYDOCK Core Engineering Priorities

**Status**: Accepted  
**Date**: 2026-11-10  
**Scope**: All subsystems (renderer, physics, Wayland, PipeWire, UI, system)  
**Replaces**: No prior ADR; this is the foundational priority document.

---

## Decision

WAYDOCK engineering priorities are mandatory and must guide all architectural and implementation decisions. These priorities are ordered by importance:

1. **Lowest possible CPU usage**
2. **Lowest possible system RAM usage**
3. **Highest possible responsiveness**
4. **Graphics, animations, interactivity and Slint integration**
5. **Lowest possible VRAM usage**

## Reason

The dock runs continuously on the user's system. Excessive CPU usage drains battery, generates heat, and competes with other applications. RAM usage must be minimal because the dock is always resident. Responsiveness is critical for user perception — laggy docks feel broken. Visual quality is important but must never compromise the higher priorities.

## Engineering Requirements

### Event-Driven Architecture

- Use an event-driven Wayland event loop.
- Never continuously poll or render while idle.
- Target zero unnecessary idle rendering.
- The event loop must sleep until Wayland input, a timer event, or an animation frame is due.

### Memory Efficiency

- Avoid steady-state heap allocations during rendering.
- Use compact persistent data structures.
- Use lazy allocation for optional effects.
- Use shared texture atlases.
- Reuse GPU buffers and resources.
- Bound all caches and resource pools.

### GPU Efficiency

- Avoid unnecessary render passes and intermediate textures.
- Never use GPU readback for ordinary UI state.
- Avoid blocking GPU synchronization.
- Use GPU-assisted animations where they reduce CPU work.

### Physics Strategy

- **Prefer analytical spring animations over general-purpose physics engines.**
- A damped spring (`F = -k(x - x_target) - c·v`) is sufficient for dock icon behavior.
- Do not use Rapier2D, nalgebra, or any general-purpose physics library.
- Custom spring solver should be lightweight, allocation-free, and operate on fixed arrays.

### Performance Targets

- Support at least 60 FPS during normal animations, adapting to display refresh.
- Preserve responsive pointer and keyboard interaction.
- Measure CPU, RAM, VRAM and presentation latency independently.

### Extensibility

- Keep optional visual effects independently configurable.
- Visual quality must remain extensible, but expensive effects must never be mandatory.

## Consequences

### Removed Dependencies

- **Rapier2D**: Removed. Dock icons do not need rigid-body collision, joints, or CCD.
- **nalgebra**: Removed. Only needed by Rapier2D.
- General-purpose physics features (joints, impulse resolution, island management) are removed.

### Simplified Physics

The physics engine retains only what the dock needs:
- Damped springs for icon positioning
- Force fields for proximity effects (attraction, repulsion)
- Fixed-timestep simulation (120 Hz)
- Visual effects derived from physics state (squash, stretch, glow)
- Interaction API for semantic operations (drag, push, pull, insert)

### Optimization Decision Policy

When two implementations provide comparable functionality, prefer the implementation with:
1. Lower idle CPU consumption.
2. Lower persistent RAM consumption.
3. Lower interaction latency.
4. Lower VRAM consumption.
5. Lower complexity and maintenance cost.

Do not introduce GPU compute, additional threads, render targets, caching, or background services without demonstrating their benefits.

### Architecture

- **Rust** language
- **Wayland** and **layer-shell** for window management
- **Vulkan** through **wgpu** for GPU rendering
- **Custom modular renderer** (wgpu-api, FemtoVG removed)
- **Independent Slint compatibility adapter** for widgets
- **Event-driven animation scheduler**
- **Lazy modular effects graph**
- Support bottom, top, left, and right dock orientations

## Alternatives Considered

1. **Keep Rapier2D for future-proofing**: Rejected. The dock's physics needs are well-defined and simple. Adding a general-purpose physics engine increases CPU, RAM, and VRAM usage without benefit.

2. **Use FemtoVG as the final renderer**: Rejected and removed. FemtoVG has been eliminated from the codebase. The custom wgpu-api renderer is the sole GPU backend.

3. **Continuous 60 FPS rendering loop**: Rejected. The dock should render only when state changes, animations are active, or input events occur. Idle rendering wastes CPU and battery.

## Integration Notes

- The wgpu-api crate is the custom modular renderer. FemtoVG has been removed.
- The physics engine in wgpu-api will be simplified to analytical springs only.
- The event loop in `src/main.rs` will be optimized to sleep longer when idle.
- Slint will remain for complex widgets (bluetooth, system tray) but not for the main dock rendering.
- All changes must be incremental, with the workspace compiling after each milestone.

## References

- `wgpu-api/src/physics/` — Physics engine modules
- `wgpu-api/Cargo.toml` — Dependencies (Rapier2D, nalgebra, FemtoVG removed)
- `src/main.rs` — Event loop
- `src/render/dock.rs` — CPU pixel rendering removed, Slint software renderer remains for main dock
- `docs/architecture/physics.md` — Physics architecture documentation
