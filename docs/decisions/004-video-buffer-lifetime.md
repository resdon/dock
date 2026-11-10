# ADR-004: Video Buffer Lifetime

**Status**: Accepted
**Date**: 2024-04-05
**Context**: PipeWire video stream management

## Problem

PipeWire provides video frames through borrowed buffers with strict lifetime rules. Incorrect buffer handling causes:
- Use-after-free when buffers are accessed after being returned
- Memory leaks when buffers are not returned
- Deadlocks when buffer acquisition blocks the rendering thread
- Latency buildup when old frames are not discarded

## Decision

**Stream lifetime ≠ Frame lifetime.** A stream can exist for hours without being rendered. Only the active stream's frames are uploaded to GPU each frame.

### Rules

1. **PipeWire buffers must be returned/released promptly.** Never retain a borrowed buffer beyond its valid lifetime.
2. **Latest-frame policy.** When frames arrive faster than they are rendered, only the newest frame is kept. Obsolete frames are immediately discarded.
3. **Texture reuse.** When a stream's dimensions and format haven't changed, the existing GPU texture is reused — no destroy/recreate cycle.
4. **CPU upload first.** Initial implementation uses CPU buffer → wgpu texture upload. DMA-BUF GPU sharing (Path A) can be added later without API changes.
5. **Renderer does not own stream lifetimes.** VideoManager owns streams; renderer only consumes textures.

### Buffer Flow

```
PipeWire → acquire buffer → copy to FrameBuffer → release buffer
                                        ↓
                              upload to GPU texture
                                        ↓
                              render frame
```

The critical point: the PipeWire buffer is **released immediately** after copying. The FrameBuffer owns the pixel data independently.

## Consequences

**Positive**:
- No buffer lifetime issues with PipeWire
- Low latency through latest-frame policy
- Efficient GPU memory through texture reuse
- Clean separation between stream management and rendering

**Negative**:
- CPU copy overhead (mitigated by DMA-BUF path in future)
- Requires careful buffer management discipline
- FrameBuffer allocation for each frame

## Alternatives Considered

- **Zero-copy DMA-BUF**: More efficient but complex; deferred to Path A. See below for implementation notes.
- **Frame queue**: Queue multiple frames; rejected because it increases latency and memory usage.
- **Renderer-owned streams**: Rejected because it couples rendering to video lifecycle.

## DMA-BUF Implementation Notes (Path A)

When implementing DMA-BUF GPU sharing, follow the PipeWire DMA-BUF sharing procedure:

1. **Capability negotiation**: Both producer and consumer must negotiate `SPA_DATA_DmaBuf` support via `SPA_PARAM_BUFFERS_dataType`.
2. **Format modifier negotiation**: Announce supported DRM modifiers via `SPA_FORMAT_VIDEO_modifier` with `SPA_POD_PROP_FLAG_MANDATORY | SPA_POD_PROP_FLAG_DONT_FIXATE`.
3. **DMA-BUF mapping**: Most DMA-BUFs from discrete GPUs cannot be `mmap()`'d. Use GPU-side import (Vulkan `VK_KHR_external_memory_fd`) instead.
4. **Buffer release**: Even with DMA-BUF, the PipeWire buffer must be released after the frame is imported. The DMA-BUF fd remains valid until the GPU import is complete.
5. **Synchronization**: Use fences or semaphores to synchronize between PipeWire producer and wgpu consumer.
6. **Fallback**: If DMA-BUF negotiation fails, fall back to Path B (CPU upload) transparently.

Reference: [PipeWire DMA-BUF Sharing](https://docs.pipewire.org/devel/page_dma_buf.html)
