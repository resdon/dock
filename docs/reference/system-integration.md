# System Integration Reference

Covers: **zbus**, **zvariant**, **sysinfo**, **libc**, **serde**, **serde_json**, **lru**, **memmap2**, **walkdir**, **dirs**, **urlencoding**, **os_pipe**

---

## 1. zbus

**Version:** `6` (tokio feature)

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/zbus |
| Guide | https://docs.rs/zbus/latest/zbus/ |
| GitHub Repository | https://github.com/zbus-zbus/zbus |

### Architecture in Dock

zbus provides D-Bus client and server functionality. Dock uses it for:
- Communicating with the ScreenCast portal for screen capture
- System service integration (network, Bluetooth, power)
- Exposing Dock's own D-Bus interface for external control

#### Key Concepts

- **Connection**: A D-Bus connection (session or system bus).
- **Proxy**: A client-side proxy for a remote D-Bus service.
- **Service**: A D-Bus service exported by Dock.
- **Message**: A D-Bus message (method call, reply, signal, error).

#### Usage in Dock

- **ScreenCast portal**: Proxy to `org.freedesktop.portal.Desktop` for screen capture.
- **System services**: Proxies for network, Bluetooth, and power management.
- **Dock interface**: Exported D-Bus service for external control.

#### Integration Notes

- zbus uses tokio as the async runtime.
- D-Bus calls are async and should be awaited.
- Use proxies for client-side calls, services for server-side exports.

---

## 2. zvariant

**Version:** `4.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/zvariant |
| GitHub Repository | https://github.com/zbus-zbus/zvariant |

### Architecture in Dock

zvariant provides D-Bus variant serialization and deserialization. Dock uses it for:
- Serializing D-Bus method arguments and return values.
- Handling complex D-Bus types (arrays, dictionaries, variants).

#### Integration Notes

- zvariant is used internally by zbus.
- Custom types can implement `SerializeValue` and `DeserializeValue` for D-Bus compatibility.

---

## 3. sysinfo

**Version:** `0.33`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/sysinfo |
| GitHub Repository | https://github.com/GuillaumeGomez/sysinfo |

### Architecture in Dock

sysinfo provides system monitoring information. Dock uses it for:
- CPU usage monitoring
- Memory usage monitoring
- Process listing and management

#### Key Concepts

| Concept | Purpose |
|---------|---------|
| `System` | System-wide information (CPU, memory, processes) |
| `Processor` | CPU usage statistics |
| `Process` | Process information (PID, name, resource usage) |
| `Disk` | Disk usage statistics |

#### Integration Notes

- sysinfo updates are synchronous and may be slow.
- Use periodic updates rather than per-frame queries.
- Cache results to avoid frequent system calls.

---

## 4. libc

**Version:** `0.2.189`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/libc |
| GitHub Repository | https://github.com/rust-lang/libc |

### Architecture in Dock

libc provides low-level C FFI bindings. Dock uses it for:
- Direct system calls not covered by higher-level libraries.
- Memory-mapped file I/O.
- Platform-specific operations.

#### Integration Notes

- libc types are `#[repr(C)]` and require careful FFI handling.
- Use higher-level libraries whenever possible.
- Unsafe code using libc should be well-documented and minimal.

---

## 5. Linux and D-Bus References

These are essential references for understanding D-Bus protocol, Linux system integration, and kernel interfaces.

### D-Bus Specification

| Resource | URL |
|----------|-----|
| D-Bus Specification | https://dbus.freedesktop.org/doc/dbus-specification.html |
| D-Bus Message Protocol | https://dbus.freedesktop.org/doc/dbus-specification.html#message-protocol |
| D-Bus Types | https://dbus.freedesktop.org/doc/dbus-specification.html#message-protocol-types |
| D-Bus Standard Service Names | https://dbus.freedesktop.org/doc/dbus-specification.html#standard-names |

**Relevance to Dock:** D-Bus is the inter-process communication system used by Dock for:
- ScreenCast portal communication (screen capture permissions)
- System service integration (network, Bluetooth, power management)
- Exposing Dock's own interface for external control
- Understanding the D-Bus message protocol is essential for debugging zbus communication issues.

### D-Bus Tutorial

| Resource | URL |
|----------|-----|
| D-Bus Tutorial | https://dbus.freedesktop.org/doc/dbus-tutorial.html |
| D-Bus Introduction | https://dbus.freedesktop.org/docs/dbus/ |
| freedesktop.org D-Bus | https://www.freedesktop.org/wiki/Software/dbus |

**Relevance to Dock:** The tutorial covers D-Bus concepts, message types, and service registration. Useful for understanding how Dock's D-Bus services interact with the system.

### Linux Kernel Documentation

| Resource | URL |
|----------|-----|
| Kernel Documentation | https://www.kernel.org/doc/html/latest/ |
| DMA-BUF Framework | https://www.kernel.org/doc/html/latest/driver-api/dma-buf.html |
| DRM Subsystem | https://www.kernel.org/doc/html/latest/gpu/index.html |
| Memory Management | https://www.kernel.org/doc/html/latest/admin-guide/mm/index.html |

**Relevance to Dock:** Kernel documentation is relevant for:
- DMA-BUF sharing between PipeWire and wgpu (zero-copy buffer imports)
- DRM format modifiers and buffer layout
- Understanding memory management for GPU buffer allocation
- Debugging DMA-BUF export/import issues at the kernel level

### Linux man-pages

| Resource | URL |
|----------|-----|
| man-pages Project | https://man7.org/linux/man-pages/ |
| mmap(2) | https://man7.org/linux/man-pages/man2/mmap.2.html |
| ioctl(2) | https://man7.org/linux/man-pages/man2/ioctl.2.html |
| epoll(7) | https://man7.org/linux/man-pages/man7/epoll.7.html |
| signal(7) | https://man7.org/linux/man-pages/man7/signal.7.html |

**Relevance to Dock:** man-pages are the authoritative reference for Linux system calls used by Dock:
- `mmap` for memory-mapped file I/O (memmap2)
- `ioctl` for DRM and device control
- `epoll` for event-driven I/O (used by tokio)
- `signal` handling for graceful shutdown

---

## 6. Supporting Libraries

### serde (`1.0`, derive feature)
Serialization/deserialization framework. Used for configuration files and data exchange.

### serde_json (`1.0`)
JSON serialization. Used for configuration and state persistence.

### lru (`0.12`)
Bounded LRU cache. Used for icon and texture caching with size limits.

### memmap2 (`0.9`)
Memory-mapped file I/O. Used for efficient file access.

### walkdir (`2.5.0`)
Recursive directory traversal. Used for font and icon discovery.

### dirs (`6.0.0`)
Platform-specific directories. Used for config, data, and cache paths.

### urlencoding (`2.1.3`)
URL encoding/decoding. Used for file path handling.

### os_pipe (`1.2.3`)
OS pipe (FIFO) creation. Used for inter-process communication.

### log (`0.4`)
Logging facade. Used in wgpu-api for debug output.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| zbus | `6` | D-Bus client/server (tokio) |
| zvariant | merged | D-Bus serialization (merged into zbus as `zbus::wire`) |
| sysinfo | `0.40` | System monitoring |
| libc | `0.2` | Low-level FFI |
| serde | `1.0` | Serialization framework |
| serde_json | `1.0` | JSON serialization |
| lru | `0.12` | Bounded LRU cache |

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| D-Bus call fails | Service not available | Check service name and path |
| Serialization error | Type not serializable | Implement serde derive macros |
| System info slow | Frequent queries | Cache results, update periodically |
| FFI crash | Incorrect type layout | Verify `#[repr(C)]` and alignment |
| Cache overflow | No size limit | Use lru crate with bounded capacity |

## Testing

- **D-Bus changes**: Test with actual D-Bus services
- **System info changes**: Verify CPU/memory readings
- **Serialization changes**: Test round-trip serialization
