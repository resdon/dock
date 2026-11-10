# System Architecture Overview

## Big Picture

Dock is a **desktop application** that runs on Linux under Wayland. It has two main components:

1. **`wgpu-api`** — A GPU rendering library with physics, video, and Wayland integration
2. **`src/`** — The application layer that uses wgpu-api to build the dock

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                        │
│                                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
│  │  Dock    │  │ Widgets  │  │ Popups   │  │ System Bar     │ │
│  │  State   │  │  (Slint) │  │  (Wayland│  │  (audio, BT,  │ │
│  │          │  │          │  │  / visual)│  │  network)      │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────────┬───────┘ │
│       │              │              │                 │         │
│       ▼              ▼              ▼                 ▼         │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │              Rendering Pipeline (src/render/)              │ │
│  │  Composes all visual elements into draw commands           │ │
│  └────────────────────────────┬───────────────────────────────┘ │
│                               │                                 │
└───────────────────────────────┼─────────────────────────────────┘
                                │
┌───────────────────────────────▼─────────────────────────────────┐
│                        wgpu-api Library                         │
│                                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
│  │ Renderer │  │  Scene   │  │ Physics  │  │   Interaction  │ │
│  │ (GPU)    │  │  Graph   │  │  Engine  │  │   (input)      │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────────┬───────┘ │
│       │              │              │                 │         │
│       ▼              ▼              ▼                 ▼         │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
│  │   GPU    │  │  Layout  │  │  Video   │  │   Wayland      │ │
│  │ Backend  │  │  (dock)  │  │(PipeWire)│  │  (windows)     │ │
│  └──────────┘  └──────────┘  └──────────┘  └────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                                │
                                ▼
┌─────────────────────────────────────────────────────────────────┐
│                    System Services                              │
│                                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌────────────────┐ │
│  │  Vulkan  │  │ PipeWire │  │ Wayland  │  │   D-Bus        │ │
│  │  (GPU)   │  │ (video)  │  │(labwc)   │  │ (system tray)  │ │
│  └──────────┘  └──────────┘  └──────────┘  └────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## Data Flow

### Rendering Frame Cycle

1. **Input**: Wayland delivers pointer/keyboard/touch events
2. **Interaction**: Events are normalized and routed to the interaction system
3. **Physics**: Physics world steps at fixed 120 Hz, producing new body positions
4. **Scene Sync**: Physics results are applied to scene node transforms
5. **Layout**: Dock arrangement recalculates target positions
6. **Rendering**: Renderer traverses the scene graph, batches draw calls, submits to GPU
7. **Presentation**: GPU frame is presented to the Wayland surface

### Video Frame Cycle

1. **PipeWire**: Delivers new frame buffers from capture streams
2. **VideoManager**: Acquires the latest frame, converts format if needed
3. **GPU Upload**: Frame data is uploaded to a wgpu texture
4. **Rendering**: Video texture is rendered as part of the scene graph
5. **Buffer Release**: PipeWire buffer is returned for reuse

## Subsystem Dependencies

```
Application Layer
    └── wgpu-api
            ├── renderer → gpu, scene, texture, surface
            ├── physics  → (pure Rust, no wgpu dependency)
            ├── video    → texture, renderer
            ├── wayland  → geometry, viewport
            ├── interaction → physics, geometry
            ├── layout   → geometry
            └── animation → geometry, transform
```

## Key Design Principles

1. **Separation of Concerns**: Each subsystem has a single responsibility and clear boundaries.
2. **API Purity**: Public APIs hide implementation details (wgpu types, PipeWire internals).
3. **Coordinate Discipline**: Each function documents its coordinate space; conversions are explicit.
4. **Fixed Timestep Physics**: Physics runs at a constant rate independent of rendering.
5. **Resource Ownership**: Clear ownership chains for GPU resources, PipeWire buffers, and Wayland surfaces.
6. **Intent-Based Interaction**: High-level commands (drag, push, pull) instead of low-level physics manipulation.
