//! DMA-BUF export utilities.
//!
//! Exports shared memory buffers for GPU-accelerated rendering.
//! Provides a bounded pool of DMABUF textures to avoid per-frame GPU allocations.

/// DMA-BUF export pipeline for wgpu → Wayland surfaces.
///
/// Converts a wgpu::Texture into a Wayland wl_buffer
/// via the wp_linux_dmabuf_v1 protocol, so the compositor can display
/// GPU-rendered frames without CPU round-trips.
///
/// Pipeline:
/// 1. Acquire a dmabuf-exportable wgpu texture from the pool
/// 2. Render scene to that texture
/// 3. Submit GPU command buffer
/// 4. Create a Wayland wl_buffer from the dmabuf planes
/// 5. Attach wl_buffer to the Wayland surface and commit
/// 6. On wl_buffer.release, return the slot to the pool
use std::collections::HashMap;
use std::os::fd::AsFd;

use smithay_client_toolkit::dmabuf::DmabufState;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface};
use wayland_client::QueueHandle;

use crate::state::AppState;

/// Re-export types from wgpu-api backend for DMABUF operations.
use wgpu_api::backend::{DmabufExportInfo, DmabufPlane};

/// A dmabuf-exportable render target.
///
/// Holds both the wgpu::Texture and the
/// corresponding Wayland WlBuffer (for surface attachment).
#[derive(Clone)]
pub struct DmabufRenderTarget {
    pub texture: wgpu::Texture,
    pub buffer: WlBuffer,
    pub width: u32,
    pub height: u32,
}

/// FIX (v1.5.5): Bounded DMABUF texture pool with release tracking and global
/// memory budget.
///
/// Instead of allocating a new GPU texture and Wayland buffer on every render,
/// this pool maintains a bounded set of pre-allocated texture slots per dimension.
/// When the compositor sends `wl_buffer.release`, the slot becomes available again.
///
/// Pool size is capped at 2 slots per dimension (double-buffering), which is
/// sufficient for the compositor to always have one buffer displayed while the
/// other is being rendered to. This eliminates the per-frame Vulkan allocation
/// (~1-2ms) and Wayland protocol round-trip for buffer creation.
///
/// **FIX (v1.5.5)**: Global memory budget prevents unbounded accumulation of
/// different popup dimensions. When total pooled memory exceeds the budget,
/// the least-recently-used dimension is evicted.
///
/// Architecture:
/// - Each slot holds a `(texture, export_info)` pair from `wgpu_dmabuf::export()`.
/// - On acquire, the pool returns the texture for rendering and the export_info
///   for creating a new WlBuffer via `params.create_immed()`.
/// - The created WlBuffer is tracked by its ID. When `released` fires, the slot
///   is marked available.
/// - Since `create_immed()` creates a new WlBuffer proxy each time, the old
///   buffer is destroyed on release, and a new one is created from the same FDs.
/// - LRU tracking evicts stale dimensions when the global budget is exceeded.
///
/// The pool uses `RefCell` for interior mutability to avoid borrow conflicts
/// with the renderer's mutable borrow of the backend.
pub struct DmabufPool {
    /// Pooled slots keyed by (width, height). Each dimension has up to 2 slots.
    slots: std::cell::RefCell<HashMap<(u32, u32), DimensionEntry>>,
    /// FIX (v1.5.5): Global memory budget in bytes (default 32 MB).
    /// When total pooled memory exceeds this, the LRU dimension is evicted.
    max_memory_bytes: u64,
    /// FIX (v1.5.5): Current total memory used by all pooled slots.
    current_memory_bytes: std::cell::Cell<u64>,
    /// FIX (v1.5.5): Monotonically increasing access counter for LRU tracking.
    access_counter: std::cell::Cell<u64>,
    /// FIX (v1.5.5): Total bytes allocated by fallback (unpooled) textures.
    /// Tracked separately so the memory budget is a true hard limit.
    fallback_memory_bytes: std::cell::Cell<u64>,
    /// FIX (v1.5.5): Number of active fallback (unpooled) textures.
    fallback_count: std::cell::Cell<u64>,
}

/// A reusable DMABUF slot in the pool.
struct DmabufSlot {
    /// The wgpu texture for rendering.
    texture: wgpu::Texture,
    /// Export info with FDs for creating WlBuffers.
    export_info: DmabufExportInfo,
    /// Whether the compositor has released this buffer and it's safe to reuse.
    available: bool,
    /// The last WlBuffer ID created from this slot's export info.
    /// Used by `on_buffer_release` to identify which slot to mark available.
    last_buffer_id: Option<wayland_client::backend::ObjectId>,
    /// FIX (v1.5.5): Memory used by this slot (width * height * 4 bytes/pixel).
    memory_bytes: u64,
}

/// FIX (v1.5.5): Dimension entry with LRU tracking for eviction.
struct DimensionEntry {
    slots: Vec<DmabufSlot>,
    /// Monotonically increasing access counter for LRU eviction.
    last_access: u64,
}

impl DmabufPool {
    /// Create a new DMABUF pool with a 32 MB global memory budget.
    pub fn new() -> Self {
        Self {
            slots: std::cell::RefCell::new(HashMap::new()),
            // FIX (v1.5.5): 32 MB global budget prevents unbounded accumulation
            // of different popup dimensions. Each slot is width*height*4 bytes.
            max_memory_bytes: 32 * 1024 * 1024,
            current_memory_bytes: std::cell::Cell::new(0),
            access_counter: std::cell::Cell::new(0),
            fallback_memory_bytes: std::cell::Cell::new(0),
            fallback_count: std::cell::Cell::new(0),
        }
    }

    /// FIX (v1.5.5): Called from the `wl_buffer.release` event handler.
    /// Marks the released buffer's slot as available for reuse.
    ///
    /// Without this, buffers would never be reused because the pool wouldn't
    /// know when the compositor is done reading them.
    pub fn on_buffer_release(&self, buffer_id: wayland_client::backend::ObjectId) {
        let mut slots = self.slots.borrow_mut();
        for (_key, entry) in slots.iter_mut() {
            for slot in entry.slots.iter_mut() {
                if slot.last_buffer_id.as_ref() == Some(&buffer_id) {
                    slot.available = true;
                    slot.last_buffer_id = None;
                    return;
                }
            }
        }
    }

    /// FIX (v1.5.5): Set the buffer ID for the most recently acquired slot.
    ///
    /// Called after creating a WlBuffer from a pooled slot's export info.
    /// This enables the pool to track which slot corresponds to which buffer,
    /// so that `on_buffer_release` can mark the correct slot as available.
    pub fn set_buffer_id(&self, buffer_id: wayland_client::backend::ObjectId) {
        let mut slots = self.slots.borrow_mut();
        for (_key, entry) in slots.iter_mut() {
            entry.last_access = self.access_counter.get();
            self.access_counter.set(self.access_counter.get() + 1);
            for slot in entry.slots.iter_mut() {
                if slot.last_buffer_id.is_none() && !slot.available {
                    slot.last_buffer_id = Some(buffer_id);
                    return;
                }
            }
        }
    }

    /// FIX (v1.5.5): Evict the least-recently-used dimension until total memory
    /// is under the global budget. Called before creating a new slot.
    fn evict_lru(&self, needed_bytes: u64) {
        let mut slots = self.slots.borrow_mut();
        while self.current_memory_bytes.get() + needed_bytes > self.max_memory_bytes
            && !slots.is_empty()
        {
            // Find the dimension with the lowest last_access (LRU)
            let lru_key = slots
                .iter()
                .min_by_key(|(_, entry)| entry.last_access)
                .map(|((w, h), _)| (*w, *h));
            if let Some(key) = lru_key {
                if let Some(entry) = slots.remove(&key) {
                    let freed: u64 = entry.slots.iter().map(|s| s.memory_bytes).sum();
                    self.current_memory_bytes.set(self.current_memory_bytes.get() - freed);
                }
            } else {
                break;
            }
        }
        drop(slots);
    }

    /// Acquire a render target from the pool for the given dimensions.
    ///
    /// Returns an available slot if one exists, or creates a new one
    /// (up to the pool size limit of 2 per dimension).
    /// If all slots are in-use by the compositor, falls back to creating
    /// a fresh texture (not pooled) to avoid blocking the render thread.
    ///
    /// FIX (v1.5.5): If the global memory budget is exceeded, the LRU dimension
    /// is evicted before creating a new slot.
    ///
    /// # Returns
    /// A tuple of (texture, export_info) where the texture is for rendering
    /// and the export_info contains the FDs for creating a WlBuffer.
    pub fn acquire_texture(
        &self,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        qh: &QueueHandle<AppState>,
        dmabuf_state: &DmabufState,
        width: u32,
        height: u32,
    ) -> Result<(wgpu::Texture, DmabufExportInfo), Box<dyn std::error::Error>> {
        let key = (width, height);
        let slot_memory = (width as u64) * (height as u64) * 4;

        // Update access counter
        {
            let mut slots = self.slots.borrow_mut();
            if let Some(entry) = slots.get_mut(&key) {
                entry.last_access = self.access_counter.get();
                self.access_counter.set(self.access_counter.get() + 1);
            }
        }

        // Try to find an available slot
        {
            let slots = self.slots.borrow();
            if let Some(entry) = slots.get(&key) {
                for slot in entry.slots.iter() {
                    if slot.available {
                        return Ok((slot.texture.clone(), slot.export_info.clone()));
                    }
                }
            }
        }

        // FIX (v1.5.5): Evict LRU dimensions if needed to make room
        self.evict_lru(slot_memory * 2); // 2 slots per dimension

        // No available slot — create a new one if under the limit
        let mut slots = self.slots.borrow_mut();
        let entry = slots.entry(key).or_insert_with(|| DimensionEntry {
            slots: Vec::new(),
            last_access: self.access_counter.get(),
        });
        entry.last_access = self.access_counter.get();
        self.access_counter.set(self.access_counter.get() + 1);

        // Pool size limit: 2 slots per dimension (double-buffering)
        if entry.slots.len() < 2 {
            let target = create_dmabuf_slot(adapter, device, qh, dmabuf_state, width, height, slot_memory)?;
            let texture = target.texture.clone();
            let info = target.export_info.clone();
            self.current_memory_bytes.set(self.current_memory_bytes.get() + slot_memory);
            entry.slots.push(target);
            return Ok((texture, info));
        }

        // All slots in-use — fall back to creating a new texture
        // This avoids blocking the render thread
        let (texture, info) = create_fresh_dmabuf_slot(adapter, device, width, height)?;
        // FIX (v1.5.5): Track fallback texture memory so the budget is a true hard limit.
        self.fallback_memory_bytes.set(
            self.fallback_memory_bytes.get().saturating_add(slot_memory),
        );
        self.fallback_count.set(self.fallback_count.get().saturating_add(1));
        if self.fallback_count.get().is_multiple_of(10) {
            eprintln!(
                "[DMABUF] Pool: {} fallback textures active ({} MB pooled + {} MB fallback)",
                self.fallback_count.get(),
                self.current_memory_bytes.get() / (1024 * 1024),
                self.fallback_memory_bytes.get() / (1024 * 1024),
            );
        }
        Ok((texture, info))
    }

    /// Clear the pool, dropping all cached textures and buffers.
    /// Call this when the GPU context is being destroyed.
    pub fn clear(&self) {
        self.slots.borrow_mut().clear();
        self.current_memory_bytes.set(0);
        self.fallback_memory_bytes.set(0);
        self.fallback_count.set(0);
    }

    /// FIX (v1.5.5): Return pool statistics for monitoring.
    ///
    /// Returns `(pooled_bytes, pooled_slot_count, fallback_bytes, fallback_count)`.
    pub fn stats(&self) -> (u64, u64, u64, u64) {
        let pooled_bytes = self.current_memory_bytes.get();
        let pooled_slots = self.slots.borrow().values().map(|e| e.slots.len() as u64).sum();
        let fallback_bytes = self.fallback_memory_bytes.get();
        let fallback_count = self.fallback_count.get();
        (pooled_bytes, pooled_slots, fallback_bytes, fallback_count)
    }
}

/// Internal helper: create a single dmabuf slot (texture + export info).
fn create_dmabuf_slot(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    _qh: &QueueHandle<AppState>,
    _dmabuf_state: &DmabufState,
    width: u32,
    height: u32,
    memory_bytes: u64,
) -> Result<DmabufSlot, Box<dyn std::error::Error>> {
    let fourcc = wgpu_dmabuf::format::Fourcc::Argb8888;
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let modifiers = wgpu_dmabuf::modifiers::shareable(adapter, fourcc, usage)
        .map_err(|e| format!("Failed to get shareable modifiers: {}", e))?;
    if modifiers.is_empty() {
        return Err("No shareable dmabuf modifiers available".into());
    }
    let modifier_values: Vec<u64> = modifiers.iter().map(|m| m.value).collect();

    let desc = wgpu_dmabuf::ExportDescriptor {
        label: Some("Dock Dmabuf Render Target"),
        width,
        height,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage,
        modifiers: &modifier_values,
    };

    let (texture, dmabuf) = wgpu_dmabuf::export(device, &desc)
        .map_err(|e| format!("Failed to export dmabuf texture: {}", e))?;

    let info = DmabufExportInfo {
        width,
        height,
        modifier: dmabuf.modifier,
        fourcc: dmabuf.fourcc,
        planes: dmabuf
            .planes
            .iter()
            .map(|p| -> Result<DmabufPlane, String> {
                Ok(DmabufPlane {
                    fd: p.fd.try_clone().map_err(|e| format!("Failed to clone dmabuf fd: {}", e))?,
                    offset: p.offset,
                    stride: p.stride,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(DmabufSlot {
        texture,
        export_info: info,
        available: false,
        last_buffer_id: None,
        memory_bytes,
    })
}

/// Create a fresh DMABUF texture + export info without pooling (fallback path).
/// Called when all pool slots are in-use by the compositor.
fn create_fresh_dmabuf_slot(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> Result<(wgpu::Texture, DmabufExportInfo), Box<dyn std::error::Error>> {
    let fourcc = wgpu_dmabuf::format::Fourcc::Argb8888;
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let modifiers = wgpu_dmabuf::modifiers::shareable(adapter, fourcc, usage)
        .map_err(|e| format!("Failed to get shareable modifiers: {}", e))?;
    if modifiers.is_empty() {
        return Err("No shareable dmabuf modifiers available".into());
    }
    let modifier_values: Vec<u64> = modifiers.iter().map(|m| m.value).collect();

    let desc = wgpu_dmabuf::ExportDescriptor {
        label: Some("Dock Dmabuf Render Target (unpooled)"),
        width,
        height,
        format: wgpu::TextureFormat::Bgra8UnormSrgb,
        usage,
        modifiers: &modifier_values,
    };

    let (texture, dmabuf) = wgpu_dmabuf::export(device, &desc)
        .map_err(|e| format!("Failed to export dmabuf texture: {}", e))?;

    let info = DmabufExportInfo {
        width,
        height,
        modifier: dmabuf.modifier,
        fourcc: dmabuf.fourcc,
        planes: dmabuf
            .planes
            .iter()
            .map(|p| -> Result<DmabufPlane, String> {
                Ok(DmabufPlane {
                    fd: p.fd.try_clone().map_err(|e| format!("Failed to clone dmabuf fd: {}", e))?,
                    offset: p.offset,
                    stride: p.stride,
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok((texture, info))
}

/// Create a Wayland WlBuffer from DMABUF export info.
///
/// Uses the FDs from a pooled (or fresh) export to create a new buffer
/// for surface attachment. Each call creates a new WlBuffer proxy,
/// but reuses the same underlying FDs.
pub fn create_buffer_from_export(
    dmabuf_state: &DmabufState,
    qh: &QueueHandle<AppState>,
    export_info: &DmabufExportInfo,
) -> Result<WlBuffer, Box<dyn std::error::Error>> {
    let params = dmabuf_state
        .create_params(qh)
        .map_err(|e| format!("Failed to create dmabuf params: {}", e))?;

    for (i, plane) in export_info.planes.iter().enumerate() {
        params.add(
            plane.fd.as_fd(),
            i as u32,
            plane.offset,
            plane.stride,
            export_info.modifier,
        );
    }

    let (buffer, _params) = params.create_immed(
        export_info.width as i32,
        export_info.height as i32,
        export_info.fourcc as u32,
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags::empty(),
        qh,
    );

    Ok(buffer)
}

/// Export a wgpu texture to a Wayland dmabuf buffer.
///
/// Creates a new dmabuf-exportable texture and returns both the texture
/// (for rendering) and the wl_buffer (for Wayland surface attachment).
pub fn export_dmabuf_buffer(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    qh: &QueueHandle<AppState>,
    dmabuf_state: &DmabufState,
    width: u32,
    height: u32,
) -> Result<DmabufRenderTarget, Box<dyn std::error::Error>> {
    let memory_bytes = (width as u64) * (height as u64) * 4;
    let slot = create_dmabuf_slot(adapter, device, qh, dmabuf_state, width, height, memory_bytes)?;
    let buffer = create_buffer_from_export(dmabuf_state, qh, &slot.export_info)?;
    Ok(DmabufRenderTarget {
        texture: slot.texture,
        buffer,
        width,
        height,
    })
}

/// Attach a dmabuf buffer to a Wayland surface and commit.
pub fn attach_and_commit(surface: &WlSurface, buffer: &WlBuffer) {
    surface.attach(Some(buffer), 0, 0);
    surface.commit();
}
