//! DMA-BUF export utilities.
//!
//! Exports shared memory buffers for GPU-accelerated rendering.

/// DMA-BUF export pipeline for wgpu → Wayland surfaces.
///
/// Converts a wgpu::Texture into a Wayland wl_buffer
/// via the wp_linux_dmabuf_v1 protocol, so the compositor can display
/// GPU-rendered frames without CPU round-trips.
///
/// Pipeline:
/// 1. Create a dmabuf-exportable wgpu texture via wgpu_dmabuf::export()
/// 2. Render scene to that texture
/// 3. Submit GPU command buffer
/// 4. Create a Wayland wl_buffer from the dmabuf planes
/// 5. Attach wl_buffer to the Wayland surface and commit

use std::collections::HashMap;
use std::os::fd::AsFd;

use smithay_client_toolkit::dmabuf::DmabufState;
use wayland_client::protocol::{wl_buffer::WlBuffer, wl_surface::WlSurface};
use wayland_client::QueueHandle;

use crate::state::AppState;

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

/// A pool of dmabuf render targets keyed by (width, height).
/// Reuses textures across frames to avoid GPU memory exhaustion.
/// Uses interior mutability (RefCell) to avoid borrow conflicts.
pub struct DmabufPool {
    targets: std::cell::RefCell<HashMap<(u32, u32), DmabufRenderTarget>>,
}

impl DmabufPool {
    pub fn new() -> Self {
        Self {
            targets: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// Get or create a render target for the given dimensions.
    /// If a target with the same (width, height) already exists, returns it.
    /// Otherwise creates a new one and caches it.
    /// Returns the target directly, dropping the borrow on self so the caller
    /// can mutably borrow gpu_context without conflict.
    pub fn get_or_create(
        &self,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        qh: &QueueHandle<AppState>,
        dmabuf_state: &DmabufState,
        width: u32,
        height: u32,
    ) -> Result<DmabufRenderTarget, Box<dyn std::error::Error>> {
        let key = (width, height);
        {
            let targets = self.targets.borrow();
            if let Some(target) = targets.get(&key) {
                // Clone the buffer reference; texture is cheap to clone
                let cloned = DmabufRenderTarget {
                    texture: target.texture.clone(),
                    buffer: target.buffer.clone(),
                    width: target.width,
                    height: target.height,
                };
                return Ok(cloned);
            }
        }
        let target = create_dmabuf_target(adapter, device, qh, dmabuf_state, width, height)?;
        self.targets.borrow_mut().insert(key, target.clone());
        Ok(target)
    }

    /// Clear the pool, dropping all cached textures and buffers.
    /// Call this when the GPU context is being destroyed.
    pub fn clear(&self) {
        self.targets.borrow_mut().clear();
    }
}

/// Internal helper: create a single dmabuf render target.
fn create_dmabuf_target(
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    qh: &QueueHandle<AppState>,
    dmabuf_state: &DmabufState,
    width: u32,
    height: u32,
) -> Result<DmabufRenderTarget, Box<dyn std::error::Error>> {
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

    let params = dmabuf_state.create_params(qh)
        .map_err(|e| format!("Failed to create dmabuf params: {}", e))?;

    for (i, plane) in dmabuf.planes.iter().enumerate() {
        params.add(
            plane.fd.as_fd(),
            i as u32,
            plane.offset,
            plane.stride,
            dmabuf.modifier,
        );
    }

    let (buffer, _params) = params.create_immed(
        width as i32,
        height as i32,
        dmabuf.fourcc as u32,
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags::empty(),
        qh,
    );

    Ok(DmabufRenderTarget {
        texture,
        buffer,
        width,
        height,
    })
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
    // Get shareable format modifiers from the GPU adapter
    let fourcc = wgpu_dmabuf::format::Fourcc::Argb8888;
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
    let modifiers = wgpu_dmabuf::modifiers::shareable(adapter, fourcc, usage)
        .map_err(|e| format!("Failed to get shareable modifiers: {}", e))?;
    if modifiers.is_empty() {
        return Err("No shareable dmabuf modifiers available".into());
    }
    let modifier_values: Vec<u64> = modifiers.iter().map(|m| m.value).collect();

    // Create a dmabuf-exportable texture
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

    // Create a Wayland wl_buffer from the dmabuf planes
    let params = dmabuf_state.create_params(qh)
        .map_err(|e| format!("Failed to create dmabuf params: {}", e))?;

    for (i, plane) in dmabuf.planes.iter().enumerate() {
        params.add(
            plane.fd.as_fd(),
            i as u32,
            plane.offset,
            plane.stride,
            dmabuf.modifier,
        );
    }

    let (buffer, _params) = params.create_immed(
        width as i32,
        height as i32,
        dmabuf.fourcc as u32,
        wayland_protocols::wp::linux_dmabuf::zv1::client::zwp_linux_buffer_params_v1::Flags::empty(),
        qh,
    );

    Ok(DmabufRenderTarget {
        texture,
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
