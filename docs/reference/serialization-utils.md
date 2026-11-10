# Serialization, Caching & Utilities Reference

Covers: **serde**, **serde_json**, **lru**, **memmap2**, **walkdir**, **dirs**, **urlencoding**, **os_pipe**, **log**

---

## 1. serde

**Version:** `1.0.229` (derive feature)

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/serde |
| Guide | https://serde.rs |
| GitHub Repository | https://github.com/serde-rs/serde |
| Data Format Guides | https://serde.rs/data-format-guide.html |

### Architecture in Dock

serde is the serialization/deserialization framework for Rust. Dock uses it for:
- Configuration file parsing and saving
- Data exchange between components
- State persistence across sessions

#### Key Concepts

- **Serialize**: Convert a Rust type to a data format (JSON, binary, etc.).
- **Deserialize**: Convert a data format back to a Rust type.
- **Derive Macros**: `#[derive(Serialize, Deserialize)]` auto-generates implementations.
- **Data Model**: Format-agnostic intermediate representation.

#### Usage in Dock

- Configuration structs derive `Serialize` and `Deserialize` for JSON persistence.
- D-Bus types use serde-compatible serialization via zvariant.
- Icon and texture cache keys may use serde for serialization.

#### Integration Notes

- serde works with any data format through serializer/deserializer traits.
- Use `#[serde(default)]` for optional config fields with defaults.
- Use `#[serde(skip)]` for fields that should not be serialized.
- Custom serialization logic via `serialize_with` and `deserialize_with`.

---

## 2. serde_json

**Version:** `1.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/serde_json |
| GitHub Repository | https://github.com/serde-rs/json |

### Architecture in Dock

serde_json provides JSON serialization and deserialization. Dock uses it for:
- Configuration file format (human-readable, editable)
- State persistence (dock layout, icon positions)
- Debug output and logging

#### Key Concepts

- **Value**: Dynamic JSON value (object, array, number, string, boolean, null).
- **Serializer**: Converts Rust types to JSON strings/bytes.
- **Deserializer**: Converts JSON strings/bytes to Rust types.

#### Integration Notes

- Use `serde_json::to_string()` for serialization to strings.
- Use `serde_json::from_str()` for deserialization from strings.
- Use `serde_json::to_vec()` and `from_slice()` for byte-level operations.
- JSON errors should be handled gracefully, especially for user-editable config files.

---

## 3. lru

**Version:** `0.12`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/lru |
| GitHub Repository | https://github.com/jeromefroe/lru |

### Architecture in Dock

lru provides a bounded LRU (Least Recently Used) cache. Dock uses it for:
- Icon cache with size limits to prevent unbounded memory growth
- Texture cache for frequently accessed GPU textures
- Font data cache

#### Key Concepts

- **Capacity**: Maximum number of entries in the cache.
- **Eviction**: When full, least recently used entries are removed.
- **Access order**: Each access updates the entry's recency.

#### Integration Notes

- Use `LruCache::new(capacity)` to create a bounded cache.
- Access via `get()` updates recency; `put()` adds or updates entries.
- For concurrent access, wrap in `parking_lot::Mutex` or use `dashmap`.
- Monitor cache hit rates to tune capacity.

---

## 4. memmap2

**Version:** `0.9`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/memmap2 |
| GitHub Repository | https://github.com/danburkert/memmap2 |

### Architecture in Dock

memmap2 provides memory-mapped file I/O. Dock uses it for:
- Efficient file access for large icon files
- Font file mapping for fast glyph access
- Zero-copy file reading where possible

#### Key Concepts

- **Mmap**: Memory-mapped region of a file.
- **MmapMut**: Mutable memory-mapped region.
- **Advice**: OS hints about access patterns (sequential, random, will-need).

#### Integration Notes

- Memory-mapped files avoid copying data into user-space buffers.
- Use for read-only access to large files (icons, fonts).
- Handle `io::Error` for file access failures.
- Memory mapping is lazy; actual page faults occur on first access.

---

## 5. walkdir

**Version:** `2.5.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/walkdir |
| GitHub Repository | https://github.com/BurntSushi/walkdir |

### Architecture in Dock

walkdir provides recursive directory traversal. Dock uses it for:
- Font discovery in system font directories
- Icon theme scanning
- Asset directory enumeration

#### Key Concepts

- **WalkDir**: Iterator over directory entries.
- **Filtering**: Skip directories, file types, or depth limits.
- **Ordering**: Depth-first traversal by default.

#### Integration Notes

- Use `.into_iter()` for owned iteration.
- Use `.skip_hidden(true)` to avoid hidden files.
- Combine with `dirs` crate for standard directory paths.
- Handle permission errors gracefully during traversal.

---

## 6. dirs

**Version:** `6.0.0`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/dirs |
| GitHub Repository | https://github.com/dirs-dev/dirs-rs |

### Architecture in Dock

dirs provides platform-specific directory paths. Dock uses it for:
- Configuration directory (`~/.config/dock`)
- Data directory (`~/.local/share/dock`)
- Cache directory (`~/.cache/dock`)

#### Key Concepts

| Function | Returns |
|----------|---------|
| `config_dir()` | XDG config directory |
| `data_dir()` | XDG data directory |
| `cache_dir()` | XDG cache directory |
| `home_dir()` | User home directory |

#### Integration Notes

- Follows XDG Base Directory Specification on Linux.
- Returns `None` if directory cannot be determined.
- Create subdirectories as needed; dirs does not create them.

---

## 7. urlencoding

**Version:** `2.1.3`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/urlencoding |
| GitHub Repository | https://github.com/Manishearth/rust-urlencoding |

### Architecture in Dock

urlencoding provides URL encoding and decoding. Dock uses it for:
- File path encoding in D-Bus messages
- URI handling for icon paths
- Data transfer over text-based protocols

#### Integration Notes

- Use `encode_all()` for full URL encoding.
- Use `decode()` for decoding, handling invalid sequences gracefully.

---

## 8. os_pipe

**Version:** `1.2.3`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/os_pipe |
| GitHub Repository | https://github.com/nickel-org/rust-os-pipe |

### Architecture in Dock

os_pipe provides OS pipe (FIFO) creation. Dock uses it for:
- Inter-process communication channels
- Data streaming between processes

#### Integration Notes

- Pipes are unidirectional; use a pair for bidirectional communication.
- Reading from a closed pipe returns EOF.
- Writing to a closed pipe returns an error.

---

## 9. log

**Version:** `0.4`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/log |
| GitHub Repository | https://github.com/rust-lang/log |

### Architecture in Dock

log provides a logging facade. Dock uses it in wgpu-api for:
- Debug output during rendering and physics
- Error reporting and diagnostics
- Performance monitoring

#### Key Concepts

| Level | Macro | Purpose |
|-------|-------|---------|
| Error | `error!()` | Critical failures |
| Warn | `warn!()` | Recoverable issues |
| Info | `info!()` | General information |
| Debug | `debug!()` | Debugging details |
| Trace | `trace!()` | Fine-grained tracing |

#### Integration Notes

- log is a facade; backend implementation (e.g., env_logger) is needed at runtime.
- Use `debug!()` and `trace!()` for development; `info!()` for runtime events.
- Log levels can be configured at runtime via environment variables.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| serde | `1.0.229` | Serialization framework (derive) |
| serde_json | `1.0` | JSON serialization |
| lru | `0.12` | Bounded LRU cache |
| memmap2 | `0.9` | Memory-mapped file I/O |
| walkdir | `2.5` | Recursive directory traversal |
| dirs | `6` | Platform-specific directories |
| urlencoding | `2.1` | URL encoding/decoding |
| os_pipe | `1.2` | OS pipe (FIFO) creation |
| log | `0.4` | Logging facade |

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Serialization error | Type not serializable | Implement serde derive macros |
| JSON parse error | Malformed config file | Validate and provide defaults |
| Cache overflow | No size limit | Use lru with bounded capacity |
| Mmap fails | File too large or permissions | Check file size and permissions |
| Dir not found | XDG not set | Fall back to home directory |

## Testing

- **Serialization changes**: Test round-trip serialization
- **Cache changes**: Verify eviction behavior under load
- **File I/O changes**: Test with various file sizes and permissions
