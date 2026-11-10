# Wayland & Windowing Reference

Covers: **wayland-client**, **wayland-protocols**, **wayland-protocols-wlr**, **smithay-client-toolkit**, **raw-window-handle**

---

## 1. wayland-client

**Version:** `0.31.15`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/wayland-client |
| GitHub Repository | https://github.com/Smithay/wayland-rs |
| Wayland Protocol Documentation | https://wayland.freedesktop.org/docs/html |
| Wayland Core Protocol | https://wayland.freedesktop.org/wayland.xml |

### Architecture in Dock

wayland-client is the low-level Wayland protocol client library. It handles socket communication, event dispatching, and protocol message encoding/decoding.

#### Key Concepts

- **Display**: Connection to the Wayland compositor. One per application.
- **EventQueue**: Queue of incoming Wayland events. Events are dispatched by calling `dispatch_pending()` or `block()`.
- **Proxy**: Rust representation of a Wayland object (surface, shell_surface, output, etc.). Each proxy has a unique ID.
- **Global**: A compositor-provided object available to all clients (e.g., `wl_compositor`, `wl_shm`).
- **Registry**: Lists all available globals. Clients bind to globals to create proxies.
- **Roundtrip**: A request-send + event-receive cycle. Some operations require a roundtrip to take effect.

#### Event Loop Model

```
Wayland socket → EventQueue → dispatch_pending() → Event handlers
```

Dock uses smithay-client-toolkit's event loop, which wraps wayland-client's `EventQueue` with a more ergonomic API.

#### Critical Rules

1. **Single-threaded**: Wayland proxies are not thread-safe. All Wayland operations must occur on the same thread.
2. **Event ordering**: Events must be dispatched in order. Blocking on one event while another arrives can cause deadlocks.
3. **Roundtrip awareness**: Surface configuration changes require a roundtrip before they take effect.
4. **Proxy lifetime**: Proxies must be kept alive as long as the corresponding Wayland object is in use.

---

## 2. wayland-protocols

**Version:** `0.32.13`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/wayland-protocols |
| Protocol Definitions | https://gitlab.freedesktop.org/wayland/wayland-protocols |
| GitHub Repository | https://github.com/Smithay/wayland-protocols-rs |

### Architecture in Dock

wayland-protocols provides compiled protocol definitions for standard Wayland extensions. Dock uses these for:

- **Layer Shell** (`wlr_layer_shell_v1`): Positioning the dock at the screen edge.
- **Foreign Toplevels** (`wlr_foreign_toplevel_manager_v1`): Listing and managing application windows.
- **Relative Pointer** (`wp_relative_pointer_v1`): Raw pointer motion for input handling.
- **Pointer Constraints** (`wp_pointer_constraint_v1`): Pointer locking for specific interactions.
- **Fractional Scale** (`wp_fractional_scale_v1`): High-DPI rendering support.
- **Linux DMA-BUF** (`zwp_linux_dmabuf_v1`): Zero-copy buffer sharing.

#### Key Protocols Used

| Protocol | Feature | Dock Usage |
|----------|---------|-----------|
| `wlr_layer_shell_v1` | Layer shell | Dock positioning (bottom, top, left, right) |
| `wlr_foreign_toplevel_manager_v1` | Window listing | Application window management |
| `wp_relative_pointer_v1` | Raw pointer events | Input handling without acceleration |
| `wp_pointer_constraint_v1` | Pointer locking | Drag and drop interactions |
| `wp_fractional_scale_v1` | Fractional scaling | High-DPI support |
| `zwp_linux_dmabuf_v1` | DMA-BUF sharing | Zero-copy buffer export |
| `xdg_output_v1` | Output information | Multi-monitor support |

#### Integration Notes

- Protocol availability depends on the compositor. Always check for globals before binding.
- Layer shell is a Sway/WLR extension, not part of the Wayland core protocol.
- DMA-BUF protocol requires compositor support for zero-copy buffer sharing.

---

## 3. wayland-protocols-wlr

**Version:** `0.3`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/wayland-protocols-wlr |
| GitHub Repository | https://github.com/Smithay/wayland-protocols-wlr-rs |

### Architecture in Dock

wayland-protocols-wlr provides Sway/WLR-specific protocol extensions. These are not standard Wayland protocols but are widely supported by WLR-based compositors (Sway, Hyprland, River, etc.).

#### Key Protocols

- **Layer Shell**: The primary protocol for dock positioning. Allows anchoring to screen edges with margin and exclusive zone support.
- **Foreign Toplevels**: Window listing and management for dock integration.

#### Compositor Compatibility

| Compositor | Layer Shell | Foreign Toplevels | DMA-BUF |
|------------|-------------|-------------------|---------|
| Sway | ✓ | ✓ | ✓ |
| Hyprland | ✓ | ✓ | ✓ |
| River | ✓ | ✓ | ✓ |
| labwc | ✓ | ✗ | ✓ |
| Canton | ✓ | ✓ | ✓ |

**Note:** Dock requires a WLR-based compositor or one supporting layer shell for proper functionality.

---

## 4. smithay-client-toolkit

**Version:** `0.20.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/smithay-client-toolkit |
| GitHub Repository | https://github.com/Smithay/smithay-client-toolkit |
| Examples | https://github.com/Smithay/smithay-client-toolkit/tree/main/examples |

### Architecture in Dock

smithay-client-toolkit (sctk) provides a higher-level API over wayland-client. It handles:

- **Event loop**: Ergonomic event dispatch with callback-based handlers.
- **Global management**: Automatic global registration and binding.
- **Data device**: Clipboard and drag-and-drop support.
- **Output management**: Multi-monitor detection and tracking.
- **Window management**: Abstracted window creation and management.

#### Key Components

| Component | Purpose |
|-----------|---------|
| `EventLoop` | Main event loop with callback registration |
| `GlobalLister` | Discover available Wayland globals |
| `DataSource` | Clipboard data provider |
| `DataDevice` | Clipboard and drag-and-drop |
| `OutputHandler` | Multi-monitor output tracking |
| `Window` | Abstracted window creation (not used directly in Dock) |

#### Event Loop Integration

Dock uses sctk's event loop as the main application loop:

```
sctk EventLoop
  → Wayland event dispatch
  → Dock event handlers
  → Physics update
  → Scene graph update
  → Render frame
  → Present to Wayland
```

#### Integration Notes

- sctk's `Window` abstraction is not used directly; Dock creates surfaces manually for precise control.
- The event loop must run on the same thread as all Wayland operations.
- Global listing should happen early in the application lifecycle.

---

## 5. raw-window-handle

**Version:** `0.6`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/raw-window-handle |
| GitHub Repository | https://github.com/rust-windowing/raw-window-handle |
| Specification | https://github.com/rust-windowing/raw-window-handle/blob/main/Spec.md |

### Architecture in Dock

raw-window-handle provides a platform-agnostic window handle abstraction. It bridges Wayland surfaces to wgpu surface creation.

#### Key Concepts

- **WindowHandle**: Abstract representation of a window/surface.
- **DisplayHandle**: Abstract representation of a display/connection.
- **Platform tokens**: Platform-specific data (Wayland: `*mut wl_surface`, `*mut wl_display`).

#### Usage in Dock

```
Wayland surface (*mut wl_surface)
  → raw-window-handle (WindowHandle)
    → wgpu surface creation
      → GPU rendering target
```

#### Integration Notes

- The Wayland surface pointer must remain valid for the lifetime of the wgpu surface.
- Surface configuration changes (resize, scale factor) require wgpu surface recreation.
- raw-window-handle `0.6` uses the new `HasDisplayHandle`/`HasWindowHandle` trait system.

---

## Dependency Compatibility

| Library | Version | Compatible wayland-client | Notes |
|---------|---------|--------------------------|-------|
| wayland-protocols | `0.32.9` | `0.31` | Pinned to wayland-client 0.31 |
| wayland-protocols-wlr | `0.3.12` | `0.31` | Pinned to wayland-client 0.31 |
| smithay-client-toolkit | `0.20.0` | `0.31` | Pinned to wayland-client 0.31 |
| raw-window-handle | `0.6` | — | Platform-agnostic |

**Warning:** All Wayland libraries in this stack are from the Smithay ecosystem and must be kept version-compatible. Upgrading one typically requires upgrading all.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Surface creation fails | Wayland surface not configured | Ensure surface is attached and committed |
| Event loop deadlock | Blocking on event while holding lock | Use non-blocking dispatch where possible |
| Layer shell not available | Compositor doesn't support it | Check global registry before binding |
| Multi-monitor issues | Output scale factor not applied | Track output events and update scale |
| DMA-BUF export fails | Compositor doesn't support format | Fall back to SHM buffer |
| Pointer events lost | Relative pointer not bound | Check for `wp_relative_pointer_v1` global |

## Testing

- **Wayland changes**: Run under labwc, Sway, or Hyprland
- **Protocol changes**: Verify global availability before binding
- **Multi-monitor**: Test with different output configurations
- **DMA-BUF**: Verify compositor supports `zwp_linux_dmabuf_v1`
