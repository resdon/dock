# GPU Rendering Reference

Covers: **wgpu**, **Vulkan**, **wgpu-dmabuf**

**Note**: FemtoVG was removed from the project. The wgpu-api crate provides the custom GPU backend.

---

## 1. wgpu

**Version:** `30` (unified across the project)

### Official Documentation

| Resource | URL |
|----------|-----|
| API Documentation | https://docs.rs/wgpu |
| GitHub Repository | https://github.com/gfx-rs/wgpu |
| Examples | https://github.com/gfx-rs/wgpu/tree/v0.21.1/wgpu/examples |
| Learn wgpu Tutorial | https://sotrh.github.io/learn-wgpu |
| WebGPU Specification | https://www.w3.org/TR/webgpu/ |

### Architecture in Dock

wgpu is the GPU rendering backend, used exclusively by the root crate (`dock`). The `wgpu-api` library crate **does not depend on wgpu** — it provides an abstraction layer that hides all wgpu types behind its own API. This means application code describes **what** to render, not **how** to allocate GPU buffers.

**Critical:** wgpu types (`Device`, `Queue`, `Texture`, `Surface`) must never cross the `wgpu-api` crate boundary. All wgpu internals are confined to the root crate's `src/render/` and `src/app/` modules.

#### Key Concepts

- **Device**: The wgpu device owns all GPU resources. In Dock, the `Renderer` struct is the sole owner.
- **Queue**: Command submission interface. All render passes are encoded and submitted through the queue.
- **Surface**: Platform-specific presentation target. Created via `raw-window-handle` from Wayland surfaces.
- **Texture**: GPU-accessible image data. Video frames, icons, and render targets are all textures.
- **Buffer**: GPU-accessible linear memory. Used for uniform data, vertex data, and staging uploads.
- **Render Pipeline**: Fixed pipeline state (vertex shader, fragment shader, blend mode, depth test).
- **Compute Pipeline**: GPU compute shaders for parallel processing.

#### Resource Lifetime Rules

1. The `Renderer` struct owns the wgpu device and queue.
2. No other subsystem creates or destroys wgpu resources directly.
3. All GPU resource creation goes through the renderer's API.
4. Textures must be dropped before the device to avoid hangs.
5. Staging buffers should be reused, not recreated per frame.

#### Version Compatibility

| wgpu Version | Vulkan Backend | Notes |
|-------------|---------------|-------|
| `30.0.1` | Vulkan 1.3 | Unified version across the project |

**Resolved:** The previous wgpu version mismatch (root: 30, wgpu-api: 0.20) has been resolved. The `wgpu-api` crate no longer depends on wgpu at all — it never imported any wgpu types. All wgpu usage is confined to the root crate.

### Integration Points

- **wgpu-dmabuf**: Exports wgpu textures as DMA-BUFs for Wayland surface presentation. Depends on wgpu `30.0.1`.
- **PipeWire**: Video frames are uploaded to wgpu textures via staging buffers.
- **Scene Graph**: Scene nodes reference wgpu textures by handle, not by wgpu type.
- **Slint**: Uses `unstable-wgpu-30` feature for GPU rendering integration. Main dock uses Slint software renderer → CPU buffer → Wayland SHM (correct for headless Wayland).

---

## 2. Vulkan

**Version:** Vulkan 1.2 (wgpu-api), Vulkan 1.3 (root crate)

### Official Documentation

| Resource | URL |
|----------|-----|
| Vulkan Specification | https://registry.vulkan.org/vulkan/specs/1.3-extensions/html/vkspec.html |
| Vulkan Guide | https://vkguide.dev |
| Vulkan Samples | https://github.com/KhronosGroup/Vulkan-Samples |
| Vulkan Hardware Database | https://vulkan.gpuinfo.org |
| Vulkan Validation Layers | https://github.com/KhronosGroup/Vulkan-ValidationLayers |

### Architecture in Dock

Vulkan is the backend GPU API used by wgpu. Dock does not call Vulkan directly — all Vulkan operations go through wgpu's abstraction layer. However, understanding Vulkan is essential for:

- **DMA-BUF sharing**: Vulkan's `VK_KHR_external_memory_fd` extension is used for zero-copy buffer imports.
- **Synchronization**: Understanding fences, semaphores, and command buffers helps debug rendering issues.
- **Memory management**: Vulkan's memory allocation model affects texture upload strategies.
- **Multi-GPU**: Understanding device groups for systems with integrated + discrete GPUs.

#### Key Vulkan Concepts Relevant to Dock

| Concept | Relevance |
|---------|-----------|
| External memory import | DMA-BUF zero-copy from PipeWire |
| Image layouts | Texture format conversions (BGRx → RGBA) |
| Synchronization | Fence/semaphore coordination between PipeWire and wgpu |
| Memory heaps | VRAM vs system memory allocation |
| Swapchain | Wayland surface presentation |
| Format modifiers | DRM format negotiation for DMA-BUF |

#### DMA-BUF and Vulkan

For DMA-BUF GPU sharing (Path A), the Vulkan flow is:

1. PipeWire provides a DMA-BUF file descriptor.
2. Vulkan imports the fd via `vkImportMemoryFdKHR`.
3. A Vulkan image is created from the imported memory.
4. A fence/semaphore synchronizes access between producer and consumer.
5. The image is used as a wgpu texture source.

Reference: [PipeWire DMA-BUF Sharing](https://docs.pipewire.org/devel/page_dma_buf.html)

---

## 4. wgpu-dmabuf

**Version:** `0.1`

### Official Documentation

| Resource | URL |
|----------|-----|
| GitHub Repository | https://github.com/Emilgardis/wgpu-dmabuf |
| API Documentation | https://docs.rs/wgpu-dmabuf |

### Architecture in Dock

wgpu-dmabuf exports wgpu textures as DMA-BUFs for zero-copy Wayland surface presentation. This is the **export** direction (wgpu → Wayland), distinct from the **import** direction (PipeWire → wgpu).

#### Key Concepts

- **Export**: Convert a wgpu texture into DMA-BUF planes with file descriptors.
- **Planes**: Multi-plane formats (e.g., NV12) have separate fds per plane.
- **Format modifier**: DRM format modifier describing the memory layout.
- **Stride and offset**: Per-plane memory layout information.

#### Usage in Dock

File: `src/render/dmabuf_export.rs`

The export flow:
1. Create a wgpu texture with DMA-BUF-exportable format.
2. Render scene to the texture via wgpu-api renderer.
3. Submit GPU command buffer and wait for completion.
4. Export the texture via `wgpu_dmabuf::export()`.
5. Extract planes, fds, strides, offsets, and format modifier.
6. Create a `zwp_linux_buffer_params_v1` Wayland buffer.
7. Attach and commit to the Wayland surface.

#### Integration Notes

- The texture must be created with a format that supports DMA-BUF export (typically `B8G8R8A8Unorm`).
- GPU command buffer must be completed before export to ensure pixels are ready.
- File descriptors are transferred to the compositor and should not be reused.
- This is different from PipeWire DMA-BUF import, which uses Vulkan's external memory extension.

---

## Dependency Compatibility

| Library | Version | Compatible wgpu | Notes |
|---------|---------|----------------|-------|
| wgpu-dmabuf | `0.1` | `30.0.1` | Depends on wgpu 30.0.1 |
| wgpu-api | `0.1.0` | `30.0.1` | Custom GPU backend with wgpu 30 dependency |

**Resolved version mismatch:** All wgpu-dependent crates now use wgpu `30.0.1`. The `wgpu-api` library crate has no wgpu dependency — it provides a pure-Rust abstraction over GPU operations.

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| Texture upload stalls | Staging buffer not mapped | Ensure `queue.write_texture` uses a mapped buffer |
| DMA-BUF export fails | Texture format not exportable | Use `B8G8R8A8Unorm` with proper usage flags |
| Text blurry | DPI/scale factor mismatch | Set correct device pixel ratio in renderer |
| Vulkan OOM | Too many textures cached | Implement texture cache eviction policy |
| Surface creation fails | Wayland surface not ready | Ensure surface is configured before creating wgpu surface |

## Testing

- **Rendering changes**: Visual inspection + `cargo check`
- **DMA-BUF changes**: Run under Wayland compositor with DMA-BUF support
- **wgpu-api changes**: Render test scenes and compare output
- **GPU memory**: Monitor with `vulkaninfo` or `radeontop`
