# Async & Concurrency Reference

Covers: **tokio**, **futures-util**, **parking_lot**, **dashmap**, **once_cell**, **lazy_static**

---

## 1. tokio

**Version:** `1.54` (full features)

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/tokio |
| Guide | https://tokio.rs/tokio/tutorial |
| GitHub Repository | https://github.com/tokio-rs/tokio |

### Architecture in Dock

tokio is the async runtime for Dock. It provides the event loop for asynchronous operations like D-Bus communication, PipeWire stream management, and file I/O.

#### Key Concepts

- **Runtime**: The async executor. Dock uses a multi-threaded runtime.
- **Task**: An async unit of work, scheduled by the runtime.
- **Channel**: Async message passing between tasks (`mpsc`, `oneshot`).
- **Sync**: Synchronization primitives (`Mutex`, `RwLock`, `Semaphore`).
- **Timer**: Async timers and timeouts.

#### Usage in Dock

tokio is used for:
- D-Bus client and server communication via `zbus`
- PipeWire stream management and buffer callbacks
- File I/O for icon loading and configuration
- System monitoring (CPU, memory, processes)
- Background services and periodic tasks

#### Integration Notes

- The tokio runtime runs on a separate thread from the main Wayland event loop.
- Use `tokio::task::spawn_blocking()` for CPU-bound work to avoid blocking the async runtime.
- Channels are used to communicate between async tasks and the main thread.
- PipeWire operations may block; use `spawn_blocking()` for PipeWire callbacks.

---

## 2. futures-util

**Version:** `0.3`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/futures-util |
| GitHub Repository | https://github.com/rust-lang/futures-rs |

### Architecture in Dock

futures-util provides combinators and utilities for working with async futures. Dock uses it for:
- Chaining async operations
- Stream processing and transformation
- Future polling and execution

#### Integration Notes

- futures-util is part of the `futures` ecosystem, which tokio is built on.
- Use for composing async operations without blocking.

---

## 3. parking_lot

**Version:** `0.12`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/parking_lot |
| GitHub Repository | https://github.com/Amanieu/parking_lot |

### Architecture in Dock

parking_lot provides improved synchronization primitives. Dock uses it for:
- Timeout-based Mutex to prevent PipeWire threads from freezing.
- RwLock for read-heavy shared state.
- Condvar for thread coordination.

#### Key Concepts

| Primitive | Purpose |
|-----------|---------|
| `Mutex` | Mutual exclusion with timeout support |
| `RwLock` | Read-write lock for shared state |
| `Condvar` | Condition variable for thread signaling |
| `Semaphore` | Limit concurrent access |

#### Integration Notes

- parking_lot's `Mutex` is faster than `std::sync::Mutex`.
- Use `try_lock_for()` to avoid indefinite blocking.
- PipeWire threads may hold locks; use timeouts to prevent deadlocks.

---

## 4. dashmap

**Version:** `6.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/dashmap |
| GitHub Repository | https://github.com/xacrl/dashmap |

### Architecture in Dock

dashmap provides a concurrent hash map. Dock uses it for:
- Icon cache (concurrent reads and writes)
- Texture cache (shared across threads)
- Stream state management

#### Integration Notes

- dashmap supports concurrent reads and writes without locking the entire map.
- Use for caches that need high read throughput.
- Entries can be removed based on custom criteria (e.g., LRU eviction).

---

## 5. once_cell & lazy_static

**Versions:** once_cell `1.21` (lazy_static removed)

### Official Documentation

| Resource | URL |
|----------|-----|
| once_cell | https://docs.rs/once_cell |
| lazy_static | https://docs.rs/lazy_static |

### Architecture in Dock

once_cell and lazy_static provide lazy static initialization. Dock uses them for:
- Static caching maps
- Singleton resources (font database, icon theme)
- Configuration values

#### Integration Notes

- once_cell is preferred over lazy_static for new code (more flexible).
- Use `OnceCell` for single-assignment statics.
- Use `SyncOnceCell` for thread-safe single-assignment.

---

## 6. Rust Async and Concurrency References

These are essential references for understanding Rust's async model, concurrency primitives, and unsafe code patterns.

### The Rust Async Book

| Resource | URL |
|----------|-----|
| The Rust Async Book | https://rust-lang.github.io/async-book |
| GitHub Repository | https://github.com/rust-lang/async-book |

**Relevance to Dock:** Comprehensive guide to Rust async/await, covering futures, executors, streams, and async I/O. Essential for understanding how tokio, futures-util, and async D-Bus/PipeWire operations work together.

### The Rustonomicon

| Resource | URL |
|----------|-----|
| The Rustonomicon | https://doc.rust-lang.org/nomicon |
| Raw Pointers | https://doc.rust-lang.org/nomicon/raw-pointers.html |
| Undefined Behavior | https://doc.rust-lang.org/nomicon/undefined-behavior.html |
| FFI Omnibus | https://doc.rust-lang.org/nomicon/ffi.html |

**Relevance to Dock:** Reference for unsafe Rust code patterns. Dock uses unsafe code for:
- FFI with PipeWire, Wayland, and libc
- GPU buffer management with bytemuck
- Memory-mapped file I/O with memmap2
- Direct memory access in video frame handling

### Rust Concurrency Documentation

| Resource | URL |
|----------|-----|
| Rust Concurrency (The Book Ch. 16) | https://doc.rust-lang.org/book/ch16-00-concurrency.html |
| std::sync Module | https://doc.rust-lang.org/std/sync |
| std::thread Module | https://doc.rust-lang.org/std/thread |
| Fearless Concurrency Guide | https://doc.rust-lang.org/book/ch16-00-concurrency.html |

**Relevance to Dock:** Dock uses multiple threads for:
- Main thread: Wayland event loop and rendering
- Async runtime: D-Bus, PipeWire, file I/O
- Background services: system monitoring, icon loading
- Understanding Rust's ownership model for thread safety is essential for correct synchronization.

---

## Dependency Compatibility

| Library | Version | Notes |
|---------|---------|-------|
| tokio | `1.54` | Async runtime (full features) |
| futures-util | `0.3` | Future combinators |
| parking_lot | `0.12` | Sync primitives |
| dashmap | `6.1` | Concurrent hash map |
| once_cell | `1.21` | Lazy static init (replaces lazy_static) |

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Deadlock | Lock held across await point | Use `tokio::sync::Mutex` for async code |
| PipeWire freeze | Blocking call on async thread | Use `spawn_blocking()` |
| Cache miss | Concurrent write invalidation | Use dashmap for concurrent access |
| Static init race | Multiple threads initializing | Use once_cell for thread-safe init |

## Testing

- **Async changes**: Run with `tokio` runtime, check for deadlocks
- **Concurrency changes**: Stress test with multiple threads
- **Cache changes**: Verify concurrent read/write behavior
