# Interaction Contract

## Purpose

This contract defines the boundaries and responsibilities of the interaction system. It specifies how input events flow from raw hardware events to physics actions.

## Ownership

### Interaction Owns
- Input event normalization
- Hit testing and picking
- Drag state tracking
- Gesture recognition
- Command queue and dispatch
- Keyboard shortcut mapping

### Interaction Does NOT Own
- Physics simulation (delegated to PhysicsWorld)
- Visual rendering (delegated to Renderer)
- Window management (delegated to Wayland)
- Application state (delegated to app layer)

## Interface

### Provides
- `InteractionManager::queue()` — Queue an interaction command
- `InteractionManager::process()` — Dispatch queued commands to PhysicsWorld
- `InteractionApi::drag()` — Begin spring-based dragging
- `InteractionApi::push()` — Apply directional force
- `InteractionApi::pull()` — Apply attraction force
- `InteractionApi::hit_test()` — Find body at pointer position

### Consumes
- Raw Wayland input events (pointer, keyboard, touch)
- `InteractionCommand` — High-level interaction intent
- `PhysicsWorld` — Target for command execution

## Invariants

1. **Input events are converted to interaction commands.** Application code should not directly manipulate physics.
2. **Commands are dispatched through InteractionManager.** Direct physics manipulation bypasses the command system.
3. **Drag uses spring-based pointer following.** The dragged object does not teleport to the pointer.
4. **Force vs structural operations are distinct.** Insert, cut, merge change relationships, not just forces.
5. **Interaction does not run physics simulation.** It sends commands; PhysicsWorld executes them.

## Coordinate Space

- Input events: Logical pixels (UI coordinates)
- Physics operations: Logical pixels (world coordinates)
- Physical positions: Device pixels (for high-DPI accuracy)

## Dependencies

- **Depends on**: `physics` (InteractionApi, PhysicsWorld), `geometry` (Point, Rect)
- **Depended on by**: Application layer, Wayland input handlers
