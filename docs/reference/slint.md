# Slint UI & Widget System Reference

Covers: **slint**, **slint-build**

---

## 1. slint

**Version:** `1.18.1` (software renderer)

### Official Documentation

| Resource | URL |
|----------|-----|
| Official Slint Documentation | https://slint.dev/docs |
| Rust API Reference | https://docs.rs/slint |
| Slint Language Reference | https://slint.dev/docs/grammar |
| Software Renderer | https://slint.dev/docs/widgets/common/widget-gallery |
| Slint GitHub | https://github.com/slint-ui/slint |

### Architecture in Dock

Slint provides the UI widget system for dock icons and components. Dock uses the **software renderer** backend, which renders to pixel buffers that are then uploaded to GPU textures.

#### Key Concepts

- **Component**: A Slint UI component defined in `.slint` files. Compiled to Rust code.
- **Widget**: A UI element (button, label, image, etc.) within a component.
- **Property**: A component's state variable, accessible from Rust.
- **Signal**: An event emitted by a widget (click, hover, etc.).
- **Renderer**: The rendering backend. Dock uses the software renderer for custom GPU integration.
- **Partial rendering**: Render only changed regions for efficiency.

#### Slint in Dock

Dock uses Slint for:
- Dock icon widgets (`.slint` definitions compiled to Rust)
- System bar components
- Popup panels
- Bluetooth widget (`bluetooth-widget.slint`)
- Main UI definition (`ui.slint`)

#### Rendering Architecture

```
Slint component
  → Software renderer (pixel buffer)
    → GPU texture upload
      → wgpu rendering
        → Wayland surface presentation
```

The software renderer produces RGBA pixel data, which is uploaded to a wgpu texture for GPU compositing. This allows Dock to integrate Slint widgets into its custom rendering pipeline.

#### Custom Platform Integration

Dock implements a custom Slint platform to integrate with Wayland:

- **Event handling**: Wayland input events are translated to Slint events.
- **Rendering**: Slint renders to an offscreen buffer, which is uploaded as a texture.
- **Timer**: Slint's animation timer is integrated with the main event loop.

#### Integration Notes

- Slint's software renderer is CPU-bound. Minimize the number of Slint-rendered elements.
- Partial rendering can reduce CPU usage by only rendering changed regions.
- Slint properties and signals are accessed via generated Rust code.
- The `.slint` files are compiled at build time by `slint-build`.

---

## 2. slint-build

**Version:** `1.18.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| Build Documentation | https://slint.dev/docs/rust/tutorial/getting-started |
| API Reference | https://docs.rs/slint-build |

### Architecture in Dock

slint-build is a build-time compiler that converts `.slint` files to Rust code. It runs as a Cargo build dependency.

#### Build Process

```
.slint files
  → slint-build (compile_partial! macro)
    → Generated Rust code
      → Cargo compilation
        → Dock binary
```

#### Usage in Dock

- `ui.slint` → Main dock UI definition
- `bluetooth-widget.slint` → Bluetooth widget component
- Additional `.slint` files for specific components

#### Integration Notes

- Changes to `.slint` files require a rebuild.
- The `compile_partial!` macro allows incremental compilation of individual components.
- Generated code includes property getters/setters and signal connections.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| slint | `1.18.1` | Software renderer backend |
| slint-build | `1.18.1` | Build-time compiler |

**Note:** slint and slint-build must be at the same version.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Slint render slow | Too many widgets | Use partial rendering, reduce widget count |
| Property not updating | Signal not connected | Check signal/property bindings in `.slint` |
| Build fails | slint-build version mismatch | Ensure slint and slint-build match |
| Text not rendering | Font not found | Configure font paths in Slint |
| Image not loading | Format not supported | Use PNG/JPEG formats supported by Slint |

## Testing

- **Slint changes**: Visual inspection of rendered widgets
- **Performance**: Monitor CPU usage of software renderer
- **Build changes**: Verify `.slint` files compile correctly
