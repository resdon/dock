# Video Subsystem — Local Rules

## What This Subsystem Does

Manages PipeWire video streams, frame acquisition, and GPU texture uploads. Designed for ~20 independent streams where only one is rendered at a time.

## Files

| File | Purpose |
|------|---------|
| `video.rs` | VideoManager, VideoStream, frame acquisition, GPU upload |

## Rules

1. **PipeWire buffers must be returned/released promptly.** Never retain a borrowed buffer beyond its valid lifetime.
2. **Latest-frame policy.** When frames arrive faster than they are rendered, only the newest frame is kept.
3. **Texture reuse.** When dimensions/format haven't changed, reuse the existing GPU texture.
4. **Renderer does not own stream lifetimes.** VideoManager owns streams; renderer only consumes textures.
5. **Stream lifetime ≠ frame lifetime.** A stream can exist for hours without being rendered.
6. **CPU upload first.** Interface is designed so DMA-BUF (Path A) can be added later.
7. **DMA-BUF capability negotiation.** When implementing Path A, negotiate `SPA_DATA_DmaBuf` support and DRM format modifiers before any frames flow.
8. **DMA-BUF mapping warning.** Most DMA-BUFs from discrete GPUs cannot be `mmap()`'d. Use GPU-side import (Vulkan `VK_KHR_external_memory_fd`).
9. **DMA-BUF fallback.** If DMA-BUF negotiation fails, fall back to Path B (CPU upload) transparently.
10. **DMA-BUF synchronization.** Use fences or semaphores to synchronize between PipeWire producer and wgpu consumer.

## Buffer Flow

```
PipeWire → acquire buffer → copy to FrameBuffer → release buffer
                                        ↓
                              upload to GPU texture
```

The PipeWire buffer is **released immediately** after copying. FrameBuffer owns the pixel data independently.

## Coordinate Spaces

- **Video frames**: Native pixel coordinates (width × height of the stream)
- **Rendering**: Logical pixels (scaled to fit the video node's bounds)

## Testing

Run with PipeWire stream active. Verify frame acquisition and rendering.

## Safe to Add

- New video format conversions
- New fit modes
- New stream statistics
- DMA-BUF support (Path A) — follow the PipeWire DMA-BUF sharing procedure

## DMA-BUF Reference

- PipeWire DMA-BUF Sharing: https://docs.pipewire.org/devel/page_dma_buf.html
- `FrameBufferSource::DmaBuf` — enum variant for DMA-BUF path (stub)
- `FrameBuffer::dma_buf()` — constructor for DMA-BUF frames (stub)
- `src/render/dmabuf_export.rs` — DMA-BUF **export** for wgpu → Wayland (different direction)
