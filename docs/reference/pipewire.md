# PipeWire & Screen Capture Reference

Covers: **pipewire**, **pipewire-sys**, **libspa-sys**

---

## 1. pipewire (Rust Bindings)

**Version:** `0.10.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| Rust Bindings Documentation | https://docs.rs/pipewire |
| PipeWire API Documentation | https://docs.pipewire.org |
| PipeWire Design Documentation | https://pipewire.org/docs/ |
| PipeWire Rust GitHub | https://github.com/mvlabat/pipewire-rs |
| PipeWire DMA-BUF Sharing | https://docs.pipewire.org/devel/page_dma_buf.html |

### Architecture in Dock

The `pipewire` crate provides Rust bindings for the PipeWire C API. Dock uses it for screen/window capture via the ScreenCast portal protocol.

#### Key Concepts

- **Context**: The top-level PipeWire connection. One per application.
- **Core**: The PipeWire server connection. Handles object creation and event dispatch.
- **Stream**: A data producer or consumer. For screen capture, Dock creates a capture stream.
- **Node**: A processing element in the PipeWire graph. Streams connect to nodes.
- **Port**: Input or output connection point on a node.
- **Props**: Properties dictionary used for negotiation and configuration.

#### Screen Capture Flow

```
1. Create PipeWire context
2. Connect to PipeWire server
3. Create capture stream with ScreenCast props
4. Negotiate format (resolution, format, framerate)
5. Receive frames via buffer callbacks
6. Process frames (format conversion, GPU upload)
7. Release buffers promptly
```

#### Buffer Handling

PipeWire provides buffers in two forms:

| Buffer Type | Access Method | Use Case |
|-------------|---------------|----------|
| Shared memory (SHM) | `mmap()` accessible | CPU processing, format conversion |
| DMA-BUF | GPU import via fd | Zero-copy GPU upload (Path A) |

**Critical Rule:** Buffers must be released promptly. Never retain a borrowed buffer beyond its valid lifetime.

#### DMA-BUF Integration

For zero-copy GPU sharing (Path A), follow the [PipeWire DMA-BUF sharing procedure](https://docs.pipewire.org/devel/page_dma_buf.html):

1. **Capability negotiation**: Both producer and consumer must negotiate `SPA_DATA_DmaBuf` support.
2. **Format modifier negotiation**: Announce supported DRM modifiers via `SPA_FORMAT_VIDEO_modifier`.
3. **DMA-BUF mapping**: Most DMA-BUFs from discrete GPUs cannot be `mmap()`'d. Use GPU-side import.
4. **Buffer release**: Even with DMA-BUF, the PipeWire buffer must be released after the frame is imported.
5. **Synchronization**: Use fences or semaphores to synchronize between producer and consumer.
6. **Fallback**: If DMA-BUF negotiation fails, fall back to CPU upload (Path B).

Current implementation uses **Path B (CPU upload)** for all frames.

---

## 2. pipewire-sys

**Version:** `0.10.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/pipewire-sys |
| PipeWire C API Reference | https://docs.pipewire.org/devel/group__api__pw.html |

### Architecture in Dock

pipewire-sys provides low-level FFI bindings to the PipeWire C library. Dock typically uses the higher-level `pipewire` crate, but pipewire-sys may be needed for:

- Accessing unreleased PipeWire features not yet in the Rust bindings.
- Direct buffer manipulation for DMA-BUF handling.
- Custom event loop integration.

#### Integration Notes

- pipewire-sys types are `#[repr(C)]` and should not be used directly in Rust code without proper FFI safety.
- Memory management follows C conventions — manual allocation and deallocation.
- Use the `pipewire` crate's safe wrappers whenever possible.

---

## 3. libspa-sys (Simple Plugin API)

**Version:** `0.10.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/libspa-sys |
| SPA Documentation | https://docs.pipewire.org/devel/group__spa.html |
| SPA API Reference | https://docs.pipewire.org/devel/group__spa__pod.html |

### Architecture in Dock

SPA (Simple Plugin API) is the underlying framework for PipeWire's buffer and format negotiation. Dock uses SPA types for:

- **Format negotiation**: `SPA_FORMAT_VIDEO` for video frame formats.
- **Buffer types**: `SPA_DATA_MemFd` (SHM) and `SPA_DATA_DmaBuf` (DMA-BUF).
- **Property dictionaries**: `SPA_POD` (Plain Old Data) for parameter passing.
- **Buffer metadata**: Frame timestamps, dimensions, and stride information.

#### Key SPA Types

| Type | Purpose |
|------|---------|
| `SPA_POD` | Generic data type for properties |
| `SPA_FORMAT_VIDEO` | Video format description |
| `SPA_DATA_MemFd` | Shared memory buffer |
| `SPA_DATA_DmaBuf` | DMA-BUF buffer |
| `SPA_BUFFER_META` | Buffer metadata (timestamps, strides) |

#### Format Negotiation

Video format negotiation uses SPA's POD system:

1. Client announces supported formats via `SPA_PARAM_Format`.
2. Server fixates on a single format from the announced list.
3. Buffer type (SHM vs DMA-BUF) is negotiated via `SPA_PARAM_BUFFERS_dataType`.
4. Format modifiers (for DMA-BUF) are negotiated via `SPA_FORMAT_VIDEO_modifier`.

#### DMA-BUF Format Modifiers

For DMA-BUF support, the client announces supported DRM format modifiers:

```
SPA_FORMAT_VIDEO_modifier → SPA_CHOICE_Enum of DRM modifiers
```

The server selects a modifier from the announced list. If no modifier is announced, the server may fall back to modifier-less buffers.

---

## 4. Linux Screen Capture

### Official Documentation

| Resource | URL |
|----------|-----|
| XDG Desktop Portal Documentation | https://github.com/flatpak/xdg-desktop-portal |
| ScreenCast Portal | https://flatpak.github.io/xdg-desktop-portal/portals/screenscast.html |
| PipeWire Video Tutorial | https://docs.pipewire.org/en/latest/ |

### Architecture in Dock

Screen capture on Linux uses the XDG Desktop Portal's ScreenCast protocol, which routes through PipeWire. The flow is:

```
1. D-Bus call to ScreenCast portal
2. User selects screen/window/region to capture
3. Portal returns PipeWire node IDs
4. Dock connects to PipeWire nodes as capture streams
5. Frames flow via PipeWire buffers
```

#### D-Bus Integration

The ScreenCast portal is accessed via D-Bus:

- **Service**: `org.freedesktop.portal.Desktop`
- **Interface**: `org.freedesktop.portal.ScreenCast`
- **Method**: `Open()` to start capture, `Close()` to stop

Dock uses `zbus` for D-Bus communication with the portal.

#### Capture Types

| Type | Description |
|------|-------------|
| Screen | Full screen capture |
| Window | Single window capture |
| Virtual | Virtual monitor capture |

#### Permissions

Screen capture requires user authorization via the portal's selection dialog. The dialog is managed by the desktop environment (GNOME, KDE, Sway, etc.).

---

## Dependency Compatibility

| Library | Version | Compatible PipeWire | Notes |
|---------|---------|---------------------|-------|
| pipewire | `0.10` | 0.3.x - 1.0.x | Rust bindings |
| pipewire-sys | `0.10` | 0.3.x - 1.0.x | FFI bindings |
| libspa-sys | `0.10` | 0.3.x - 1.0.x | SPA FFI bindings |

**Warning:** All three libraries are from the same ecosystem and should be kept at matching versions.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Capture fails | Portal not available | Check for `org.freedesktop.portal.ScreenCast` |
| Black frames | Format mismatch | Verify negotiated format matches buffer format |
| Buffer access denied | DMA-BUF not mappable | Use GPU-side import, not `mmap()` |
| Frame drops | Buffer not released | Release buffers promptly after processing |
| Stream disconnects | PipeWire server restart | Reconnect and recreate streams |
| Permission denied | User denied capture | Re-prompt via portal's selection dialog |

## Testing

- **PipeWire changes**: Run with PipeWire stream active
- **DMA-BUF changes**: Verify format modifier negotiation
- **Buffer handling**: Monitor buffer release timing
- **Portal changes**: Test with different desktop environments
