# ADR-002: Physics Fixed Timestep

**Status**: Accepted
**Date**: 2024-02-20
**Context**: Physics simulation for UI animation

## Problem

Tying physics calculations directly to rendering frame rate causes:
- Inconsistent behavior at different frame rates
- Instability when frame rates drop (large dt values)
- Non-reproducible simulations
- "Spiral of death" when physics can't keep up

## Decision

Use a **fixed simulation timestep** (default 120 Hz), decoupled from rendering frame rate. The physics system accumulates time and runs fixed-size steps, then interpolates the visual state between simulation steps.

### Architecture

```
Application event loop
    ↓
Input events / pointer movement
    ↓
Physics accumulator
    ↓
Fixed physics steps (120 Hz when active)
    ↓
Interpolate physical state
    ↓
wgpu rendering
```

### Parameters

| Parameter | Default | Description |
|-----------|---------|-------------|
| `fixed_dt` | 1/120 | Fixed simulation timestep in seconds |
| `max_substeps` | 8 | Maximum catch-up steps (prevents spiral of death) |
| `velocity_iterations` | 8 | Solver velocity iterations |
| `position_iterations` | 4 | Solver position iterations |

## Consequences

**Positive**:
- Consistent physics behavior regardless of frame rate
- Stable simulation even under load
- Reproducible results for debugging
- Smooth visual interpolation between steps

**Negative**:
- Slightly more complex than variable timestep
- Input latency of up to one physics frame (~8ms at 120 Hz)
- Requires interpolation logic for smooth rendering

## Alternatives Considered

- **Variable timestep**: Simple but unstable; rejected.
- **Fixed timestep tied to rendering**: Simpler but causes frame rate dependency; rejected.
- **60 Hz fixed timestep**: Lower CPU usage but less smooth for UI; 120 Hz chosen for responsiveness.
