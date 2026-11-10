//! GPU backend — the actual wgpu integration layer.
//!
//! This module hides all wgpu types and provides the internal implementation
//! for the Renderer. Application code never sees wgpu types directly.
//!
//! The backend manages:
//! - GPU device and queue initialization
//! - Shader pipeline creation and caching
//! - Texture upload and management
//! - Uniform buffer updates
//! - Command encoding and submission

use std::collections::HashMap;
use std::os::fd::OwnedFd;

use crate::command::Command;
use crate::renderer::{PowerPreference, RendererBackend};
use crate::shaders;
use crate::surface::{SurfaceConfig, SurfaceFormat, SurfaceId};
use crate::texture::{TextureConfig, TextureFormat as TexFormat, TextureId, TextureUsage};

/// Uniform buffer data for the 2D transform shader.
///
/// Layout: mat2x2 (32 bytes) + vec2 offset (8 bytes) + padding (8 bytes) = 48 bytes.
/// Must be aligned to 16 bytes for wgpu uniform buffer requirements.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UniformBufferObject {
    /// 2x2 transformation matrix (scale + rotation + skew).
    pub transform: [f32; 4],
    /// 2D translation offset.
    pub offset: [f32; 2],
    /// Padding for alignment.
    pub _padding: [f32; 2],
}

impl Default for UniformBufferObject {
    fn default() -> Self {
        Self {
            transform: [1.0, 0.0, 0.0, 1.0],
            offset: [0.0, 0.0],
            _padding: [0.0; 2],
        }
    }
}

impl UniformBufferObject {
    /// Size of the uniform buffer in bytes.
    pub const SIZE: u64 = std::mem::size_of::<Self>() as u64;

    /// Create a uniform buffer object from a transform matrix.
    pub fn from_transform(matrix: &crate::transform::Matrix3) -> Self {
        let e = &matrix.elements;
        Self {
            transform: [e[0], e[1], e[3], e[4]],
            offset: [e[2], e[5]],
            _padding: [0.0; 2],
        }
    }
}

/// Vertex format for textured rendering.
///
/// Each vertex has a position (2 floats), texture coordinates (2 floats),
/// and a color (4 floats) = 24 bytes total.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct TexturedVertex {
    /// Position in clip space [-1, +1].
    pub position: [f32; 2],
    /// Texture coordinates [0, 1].
    pub tex_coords: [f32; 2],
    /// Per-vertex color (RGBA).
    pub color: [f32; 4],
}

impl TexturedVertex {
    /// Size of a single vertex in bytes.
    pub const SIZE: u32 = std::mem::size_of::<Self>() as u32;

    /// Create a new textured vertex.
    pub fn new(position: [f32; 2], tex_coords: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            tex_coords,
            color,
        }
    }

    /// Build a quad of textured vertices.
    pub fn quad(x: f32, y: f32, w: f32, h: f32, uv: [f32; 4], color: [f32; 4]) -> [Self; 6] {
        [
            // Triangle 1
            Self::new([x, y], [uv[0], uv[1]], color),
            Self::new([x + w, y], [uv[2], uv[1]], color),
            Self::new([x, y + h], [uv[0], uv[3]], color),
            // Triangle 2
            Self::new([x, y + h], [uv[0], uv[3]], color),
            Self::new([x + w, y], [uv[2], uv[1]], color),
            Self::new([x + w, y + h], [uv[2], uv[3]], color),
        ]
    }
}

/// Vertex format for solid-color (untextured) rendering.
///
/// Each vertex has a position (2 floats) and a color (4 floats) = 16 bytes.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct SolidVertex {
    /// Position in clip space [-1, +1].
    pub position: [f32; 2],
    /// Per-vertex color (RGBA).
    pub color: [f32; 4],
}

impl SolidVertex {
    /// Size of a single vertex in bytes.
    pub const SIZE: u32 = std::mem::size_of::<Self>() as u32;

    /// Create a new solid vertex.
    pub fn new(position: [f32; 2], color: [f32; 4]) -> Self {
        Self { position, color }
    }

    /// Build a quad of solid vertices.
    pub fn quad(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> [Self; 6] {
        [
            // Triangle 1
            Self::new([x, y], color),
            Self::new([x + w, y], color),
            Self::new([x, y + h], color),
            // Triangle 2
            Self::new([x, y + h], color),
            Self::new([x + w, y], color),
            Self::new([x + w, y + h], color),
        ]
    }

    /// Build a circle approximated as a polygon.
    pub fn circle(
        center_x: f32,
        center_y: f32,
        radius: f32,
        segments: usize,
        color: [f32; 4],
    ) -> Vec<Self> {
        let mut vertices = Vec::with_capacity(3 * segments);
        for i in 0..segments {
            let a1 = (i as f32 / segments as f32) * std::f32::consts::PI * 2.0;
            let a2 = ((i + 1) as f32 / segments as f32) * std::f32::consts::PI * 2.0;
            vertices.push(Self::new([center_x, center_y], color));
            vertices.push(Self::new(
                [center_x + radius * a1.cos(), center_y + radius * a1.sin()],
                color,
            ));
            vertices.push(Self::new(
                [center_x + radius * a2.cos(), center_y + radius * a2.sin()],
                color,
            ));
        }
        vertices
    }
}

/// Pipeline type identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PipelineType {
    /// Textured quad pipeline.
    Textured,
    /// Solid color pipeline.
    Solid,
}

/// The internal wgpu backend state.
///
/// This struct holds the actual wgpu resources (device, queue, pipelines,
/// textures) but exposes only the operations needed by the Renderer.
pub struct GpuBackend {
    /// wgpu instance for adapter selection.
    instance: wgpu::Instance,
    /// GPU adapter (Vulkan on Linux).
    adapter: wgpu::Adapter,
    /// GPU device for resource creation.
    device: wgpu::Device,
    /// GPU queue for command submission.
    queue: wgpu::Queue,
    /// GPU adapter name.
    adapter_name: String,
    /// GPU device description.
    device_description: String,
    /// Registered GPU textures by ID.
    gpu_textures: HashMap<TextureId, wgpu::Texture>,
    /// Texture views for sampling.
    gpu_texture_views: HashMap<TextureId, wgpu::TextureView>,
    /// Registered GPU surfaces by ID.
    gpu_surfaces: HashMap<SurfaceId, wgpu::Surface<'static>>,
    /// Surface configurations.
    surface_configs: HashMap<SurfaceId, wgpu::SurfaceConfiguration>,
    /// Render pipeline for textured quads.
    textured_pipeline: Option<wgpu::RenderPipeline>,
    /// Render pipeline for solid color rendering.
    solid_pipeline: Option<wgpu::RenderPipeline>,
    /// Uniform buffer for transform data.
    uniform_buffer: Option<wgpu::Buffer>,
    /// Uniform bind group.
    uniform_bind_group: Option<wgpu::BindGroup>,
    /// Sampler for texture sampling.
    sampler: Option<wgpu::Sampler>,
    /// Bind group layout shared by pipelines and all texture bind groups.
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    /// Bind groups cached for uploaded textures.
    texture_bind_groups: HashMap<TextureId, wgpu::BindGroup>,
    /// Whether the backend is fully initialized.
    initialized: bool,
}

/// Information about a GPU texture.
#[derive(Debug, Clone)]
struct GpuTextureInfo {
    /// Whether the texture has been uploaded to GPU.
    uploaded: bool,
}

/// Information about a GPU surface.
#[derive(Debug, Clone)]
struct GpuSurfaceInfo {
    /// Surface configuration.
    config: SurfaceConfig,
}

impl GpuBackend {
    /// Initialize the GPU backend.
    ///
    /// Creates a wgpu Instance, selects a Vulkan adapter, and creates
    /// a Device/Queue pair. This is the core GPU initialization that
    /// replaces the previous stub implementation.
    pub async fn new(
        backend: RendererBackend,
        power_preference: PowerPreference,
    ) -> Result<Self, String> {
        // Determine backend flags
        let backends = match backend {
            RendererBackend::Vulkan => wgpu::Backends::VULKAN,
            RendererBackend::Gl => wgpu::Backends::GL,
            RendererBackend::Auto => wgpu::Backends::VULKAN | wgpu::Backends::GL,
            RendererBackend::Null => {
                return Err("Null backend does not support GPU initialization".to_string());
            }
            _ => wgpu::Backends::VULKAN | wgpu::Backends::GL,
        };

        let power_pref = match power_preference {
            PowerPreference::HighPerformance => wgpu::PowerPreference::HighPerformance,
            PowerPreference::LowPower => wgpu::PowerPreference::LowPower,
        };

        // Create wgpu instance
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        // Request adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: power_pref,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await
            .map_err(|e| format!("No GPU adapter found: {}", e))?;

        let adapter_name = adapter.get_info().name.clone();

        // Request device with DMA-BUF support
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("wgpu-api Device"),
                required_features: wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF,
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            })
            .await
            .map_err(|e| format!("Failed to create GPU device: {}", e))?;

        log::info!(
            "GPU backend initialized: {} (power: {:?})",
            adapter_name,
            power_preference
        );

        let mut gpu_backend = Self {
            instance,
            adapter,
            device,
            queue,
            adapter_name,
            device_description: "wgpu Device".to_string(),
            gpu_textures: HashMap::new(),
            gpu_texture_views: HashMap::new(),
            gpu_surfaces: HashMap::new(),
            surface_configs: HashMap::new(),
            textured_pipeline: None,
            solid_pipeline: None,
            uniform_buffer: None,
            uniform_bind_group: None,
            sampler: None,
            bind_group_layout: None,
            texture_bind_groups: HashMap::new(),
            initialized: true,
        };

        gpu_backend.bind_group_layout = Some(gpu_backend.create_bind_group_layout());

        // Create uniform buffer and pipelines
        gpu_backend.create_uniform_buffer();
        gpu_backend.create_pipelines();

        Ok(gpu_backend)
    }

    /// Create the bind group layout shared by all render pipelines.
    fn create_bind_group_layout(&self) -> wgpu::BindGroupLayout {
        self.device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 2,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                ],
            })
    }

    /// Create the pipeline layout shared by all render pipelines.
    fn create_pipeline_layout(
        &self,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::PipelineLayout {
        self.device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Pipeline Layout"),
                bind_group_layouts: &[Some(bind_group_layout)],
                immediate_size: 0,
            })
    }

    /// Create the uniform buffer for transform data.
    fn create_uniform_buffer(&mut self) {
        let Some(bind_group_layout) = self.bind_group_layout.as_ref() else {
            tracing::error!("Bind group layout not initialized; cannot create uniform buffer");
            return;
        };

        let uniform_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Uniform Buffer"),
            size: UniformBufferObject::SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Create sampler for texture sampling
        let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Default Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });

        // Create a placeholder texture view for the solid pipeline bind group.
        // The solid pipeline doesn't sample textures, but the bind group layout
        // requires a texture binding at slot 2.
        let placeholder_texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Placeholder Texture"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[wgpu::TextureFormat::Rgba8Unorm],
        });
        let placeholder_view =
            placeholder_texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Create bind group for uniforms
        let uniform_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Uniform Bind Group"),
            layout: bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&placeholder_view),
                },
            ],
        });

        // Initialize the uniform buffer with the identity transform
        self.queue.write_buffer(
            &uniform_buffer,
            0,
            bytemuck::cast_slice(&[UniformBufferObject::default()]),
        );

        self.uniform_buffer = Some(uniform_buffer);
        self.uniform_bind_group = Some(uniform_bind_group);
        self.sampler = Some(sampler);
    }

    /// Create render pipelines for textured and solid rendering.
    fn create_pipelines(&mut self) {
        let Some(bind_group_layout) = self.bind_group_layout.as_ref() else {
            tracing::error!("Bind group layout not initialized; cannot create pipelines");
            return;
        };
        let pipeline_layout = self.create_pipeline_layout(bind_group_layout);

        // Textured pipeline
        let vert_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Textured Vertex Shader"),
                source: wgpu::ShaderSource::Wgsl(shaders::VERTEX_SHADER.into()),
            });
        let frag_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Textured Fragment Shader"),
                source: wgpu::ShaderSource::Wgsl(shaders::FRAGMENT_SHADER.into()),
            });
        let textured_pipeline =
            self.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Textured Pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &vert_module,
                        entry_point: Some("main"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: TexturedVertex::SIZE as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![
                                0 => Float32x2,
                                1 => Float32x2,
                                2 => Float32x4,
                            ],
                        })],
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &frag_module,
                        entry_point: Some("main"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: wgpu::TextureFormat::Bgra8UnormSrgb,
                            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    primitive: wgpu::PrimitiveState {
                        topology: wgpu::PrimitiveTopology::TriangleList,
                        strip_index_format: None,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        unclipped_depth: false,
                        polygon_mode: wgpu::PolygonMode::Fill,
                        conservative: false,
                    },
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    multiview_mask: None,
                    cache: None,
                });
        self.textured_pipeline = Some(textured_pipeline);

        // Solid pipeline
        let solid_vert_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Solid Vertex Shader"),
                source: wgpu::ShaderSource::Wgsl(shaders::SOLID_VERTEX_SHADER.into()),
            });
        let solid_frag_module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Solid Fragment Shader"),
                source: wgpu::ShaderSource::Wgsl(shaders::SOLID_FRAGMENT_SHADER.into()),
            });
        let solid_pipeline = self
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Solid Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &solid_vert_module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: SolidVertex::SIZE as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x2,
                            1 => Float32x4,
                        ],
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &solid_frag_module,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8UnormSrgb,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            });
        self.solid_pipeline = Some(solid_pipeline);
    }

    /// Register a texture with the GPU backend.
    pub fn register_texture(&mut self, texture_id: TextureId, config: TextureConfig) {
        self.texture_bind_groups.remove(&texture_id);
        let wgpu_format = Self::to_wgpu_texture_format(config.format);
        let wgpu_usage = Self::to_wgpu_texture_usage(&config.usage);
        let mip_levels = if config.mip_levels > 0 {
            config.mip_levels
        } else {
            1
        };

        // For srgb textures, the view format must include the non-srgb equivalent
        // so that the texture can be sampled correctly in shaders.
        let view_formats: Vec<wgpu::TextureFormat> = match wgpu_format {
            wgpu::TextureFormat::Bgra8UnormSrgb => vec![wgpu::TextureFormat::Bgra8Unorm],
            wgpu::TextureFormat::Rgba8UnormSrgb => vec![wgpu::TextureFormat::Rgba8Unorm],
            _ => vec![],
        };

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("Texture {}", texture_id.as_u64())),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: config.array_layers,
            },
            mip_level_count: mip_levels,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu_format,
            usage: wgpu_usage,
            view_formats: &view_formats,
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        self.gpu_textures.insert(texture_id, texture);
        self.gpu_texture_views.insert(texture_id, view);
    }

    /// Mark a texture as uploaded to the GPU.
    pub fn mark_texture_uploaded(&mut self, _texture_id: TextureId) {
        // Texture is already created in register_texture
    }

    /// Upload pixel data to a GPU texture.
    pub fn upload_texture_data(
        &mut self,
        texture_id: TextureId,
        data: &[u8],
        width: u32,
        height: u32,
    ) {
        if let Some(texture) = self.gpu_textures.get(&texture_id) {
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Remove a texture from the GPU backend.
    pub fn remove_texture(&mut self, texture_id: TextureId) {
        self.gpu_textures.remove(&texture_id);
        self.gpu_texture_views.remove(&texture_id);
        self.texture_bind_groups.remove(&texture_id);
    }

    /// Return the cached texture bind group, creating it on first use.
    pub fn texture_bind_group(&mut self, texture_id: TextureId) -> Option<&wgpu::BindGroup> {
        if !self.texture_bind_groups.contains_key(&texture_id) {
            let texture_view = self.gpu_texture_views.get(&texture_id)?;
            let uniform_buffer = self.uniform_buffer.as_ref()?;
            let sampler = self.sampler.as_ref()?;
            let layout = self.bind_group_layout.as_ref()?;
            let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Texture Bind Group"),
                layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(texture_view),
                    },
                ],
            });
            self.texture_bind_groups.insert(texture_id, bind_group);
        }
        self.texture_bind_groups.get(&texture_id)
    }

    /// Get a bind group that was prepared before opening a render pass.
    pub fn get_texture_bind_group(&self, texture_id: TextureId) -> Option<&wgpu::BindGroup> {
        self.texture_bind_groups.get(&texture_id)
    }

    /// Create a wgpu Surface from raw Wayland display and surface pointers.
    ///
    /// This is the key integration point for headless Wayland rendering.
    /// It creates a wgpu Surface that can be used with `register_surface_from_handle`.
    ///
    /// # Safety
    /// The display and surface pointers must be valid for the lifetime of the surface.
    /// The caller is responsible for ensuring the Wayland connection is active.
    pub unsafe fn create_wayland_surface(
        &self,
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
    ) -> Result<wgpu::Surface<'static>, String> {
        use raw_window_handle::{
            DisplayHandle, RawDisplayHandle, WaylandDisplayHandle, WaylandWindowHandle, WindowHandle,
        };
        use std::ptr::NonNull;

        // Create a Wayland display handle from the raw pointer
        let non_null = NonNull::new(display).ok_or_else(|| "Null display pointer".to_string())?;
        let display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(non_null));

        // Create a Wayland window handle from the surface pointer
        let window_handle = WaylandWindowHandle::new(
            NonNull::new(surface).ok_or_else(|| "Null surface pointer".to_string())?,
        );

        // Create the display and window handles for wgpu
        let dh = DisplayHandle::borrow_raw(display_handle);
        let wh = WindowHandle::borrow_raw(window_handle.into());

        // Create the wgpu surface using unsafe API
        let target = wgpu::SurfaceTargetUnsafe::from_display_and_window(&dh, &wh)
            .map_err(|e| format!("Invalid window handle: {:?}", e))?;
        let surface = self
            .instance
            .create_surface_unsafe(target)
            .map_err(|e| format!("Failed to create Wayland surface: {}", e))?;
        Ok(surface)
    }

    /// Register a surface with the GPU backend.
    pub fn register_surface_from_handle(
        &mut self,
        surface_id: SurfaceId,
        config: SurfaceConfig,
        surface: wgpu::Surface<'static>,
    ) {
        let wgpu_format = Self::to_wgpu_surface_format(config.format);

        let wgpu_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu_format,
            color_space: wgpu::SurfaceColorSpace::default(),
            width: config.width,
            height: config.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        surface.configure(&self.device, &wgpu_config);

        self.gpu_surfaces.insert(surface_id, surface);
        self.surface_configs.insert(surface_id, wgpu_config);
    }

    /// Remove a surface from the GPU backend.
    pub fn remove_surface(&mut self, surface_id: SurfaceId) {
        self.gpu_surfaces.remove(&surface_id);
        self.surface_configs.remove(&surface_id);
    }

    /// Resize a surface.
    pub fn resize_surface(&mut self, surface_id: SurfaceId, width: u32, height: u32) {
        if let Some(config) = self.surface_configs.get_mut(&surface_id) {
            config.width = width;
            config.height = height;
        }
        if let Some(surface) = self.gpu_surfaces.get(&surface_id) {
            if let Some(config) = self.surface_configs.get(&surface_id) {
                surface.configure(&self.device, config);
            }
        }
    }

    /// Acquire a surface frame for rendering.
    pub fn acquire_surface_frame(&mut self, surface_id: SurfaceId) -> Option<wgpu::SurfaceTexture> {
        let surface = self.gpu_surfaces.get(&surface_id)?;
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) => Some(t),
            wgpu::CurrentSurfaceTexture::Suboptimal(t) => Some(t),
            _ => None,
        }
    }

    /// Get a reference to the wgpu instance.
    ///
    /// Used for Slint wgpu-30 integration via `WGPUConfiguration::Manual`.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Get a reference to the wgpu device.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Get a reference to the wgpu queue.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Get a reference to the wgpu adapter.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// Get the textured render pipeline.
    pub fn textured_pipeline(&self) -> Option<&wgpu::RenderPipeline> {
        self.textured_pipeline.as_ref()
    }

    /// Get the solid render pipeline.
    pub fn solid_pipeline(&self) -> Option<&wgpu::RenderPipeline> {
        self.solid_pipeline.as_ref()
    }

    /// Get the uniform buffer.
    pub fn uniform_buffer(&self) -> Option<&wgpu::Buffer> {
        self.uniform_buffer.as_ref()
    }

    /// Get the uniform bind group.
    pub fn uniform_bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.uniform_bind_group.as_ref()
    }

    /// Get the sampler.
    pub fn sampler(&self) -> Option<&wgpu::Sampler> {
        self.sampler.as_ref()
    }

    /// Update the uniform buffer with new transform data.
    pub fn update_uniforms(&mut self, uniforms: &UniformBufferObject) {
        if let Some(buf) = &self.uniform_buffer {
            self.queue.write_buffer(buf, 0, bytemuck::cast_slice(&[*uniforms]));
        }
    }

    /// Get a texture view by ID.
    pub fn get_texture_view(&self, texture_id: TextureId) -> Option<&wgpu::TextureView> {
        self.gpu_texture_views.get(&texture_id)
    }

    /// Get the adapter name.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Get the device description.
    pub fn device_description(&self) -> &str {
        &self.device_description
    }

    /// Check if the backend is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the number of GPU textures.
    pub fn texture_count(&self) -> usize {
        self.gpu_textures.len()
    }

    /// Get the number of GPU surfaces.
    pub fn surface_count(&self) -> usize {
        self.gpu_surfaces.len()
    }

    /// Convert SurfaceFormat to wgpu TextureFormat.
    pub fn to_wgpu_surface_format(format: SurfaceFormat) -> wgpu::TextureFormat {
        match format {
            SurfaceFormat::Bgra8 => wgpu::TextureFormat::Bgra8UnormSrgb,
            SurfaceFormat::Rgba8 => wgpu::TextureFormat::Rgba8UnormSrgb,
            SurfaceFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            SurfaceFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            SurfaceFormat::Hdr => wgpu::TextureFormat::Rgba16Float,
        }
    }

    /// Convert TextureFormat to wgpu TextureFormat.
    pub fn to_wgpu_texture_format(format: TexFormat) -> wgpu::TextureFormat {
        match format {
            TexFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8UnormSrgb,
            TexFormat::Bgra8UnormSrgb => wgpu::TextureFormat::Bgra8UnormSrgb,
            TexFormat::Rgba8UnormSrgb => wgpu::TextureFormat::Rgba8UnormSrgb,
            TexFormat::Rgba16Float => wgpu::TextureFormat::Rgba16Float,
            TexFormat::Rgba32Float => wgpu::TextureFormat::Rgba32Float,
            TexFormat::Depth32Float => wgpu::TextureFormat::Depth32Float,
            TexFormat::Depth24PlusStencil8 => wgpu::TextureFormat::Depth24PlusStencil8,
        }
    }

    /// Convert TextureUsage to wgpu TextureUsages.
    pub fn to_wgpu_texture_usage(usage: &TextureUsage) -> wgpu::TextureUsages {
        let mut wgpu_usage = wgpu::TextureUsages::empty();
        if usage.render_target {
            wgpu_usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
        }
        if usage.sample {
            wgpu_usage |= wgpu::TextureUsages::TEXTURE_BINDING;
        }
        if usage.copy_source {
            wgpu_usage |= wgpu::TextureUsages::COPY_SRC;
        }
        if usage.copy_destination {
            wgpu_usage |= wgpu::TextureUsages::COPY_DST;
        }
        if wgpu_usage.is_empty() {
            wgpu_usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST;
        }
        wgpu_usage
    }

    /// Convert SurfaceFormat to wgpu-compatible format string.
    pub fn surface_format_name(format: SurfaceFormat) -> &'static str {
        match format {
            SurfaceFormat::Bgra8 => "Bgra8UnormSrgb",
            SurfaceFormat::Rgba8 => "Rgba8UnormSrgb",
            SurfaceFormat::Rgba16Float => "Rgba16Float",
            SurfaceFormat::Rgba32Float => "Rgba32Float",
            SurfaceFormat::Hdr => "Rgba16Float",
        }
    }

    /// Convert TextureFormat to wgpu-compatible format string.
    pub fn texture_format_name(format: TexFormat) -> &'static str {
        match format {
            TexFormat::Rgba8Unorm => "Rgba8UnormSrgb",
            TexFormat::Bgra8UnormSrgb => "Bgra8UnormSrgb",
            TexFormat::Rgba8UnormSrgb => "Rgba8UnormSrgb",
            TexFormat::Rgba16Float => "Rgba16Float",
            TexFormat::Rgba32Float => "Rgba32Float",
            TexFormat::Depth32Float => "Depth32Float",
            TexFormat::Depth24PlusStencil8 => "Depth24PlusStencil8",
        }
    }

    /// Compute texture usage flags count.
    pub fn texture_usage_count(usage: &TextureUsage) -> u32 {
        let mut count = 0;
        if usage.render_target {
            count += 1;
        }
        if usage.sample {
            count += 1;
        }
        if usage.copy_source {
            count += 1;
        }
        if usage.copy_destination {
            count += 1;
        }
        count
    }

    /// Create a DMA-BUF exportable texture for Wayland presentation.
    ///
    /// This creates a wgpu texture that can be shared with the Wayland compositor
    /// via DMA-BUF, enabling zero-copy GPU rendering. The returned texture can be
    /// used as a render target, and the DMA-BUF info can be used to create a
    /// Wayland wl_buffer for surface attachment.
    ///
    /// # Returns
    /// A tuple of (wgpu::Texture, DmabufExportInfo) where the texture is for rendering
    /// and the info contains the file descriptors, offsets, and strides for DMA-BUF.
    pub fn create_dmabuf_texture(
        &self,
        width: u32,
        height: u32,
    ) -> Result<(wgpu::Texture, DmabufExportInfo), String> {
        let fourcc = wgpu_dmabuf::format::Fourcc::Argb8888;
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        
        let modifiers = wgpu_dmabuf::modifiers::shareable(&self.adapter, fourcc, usage)
            .map_err(|e| format!("Failed to get shareable modifiers: {}", e))?;
        if modifiers.is_empty() {
            return Err("No shareable dmabuf modifiers available".to_string());
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

        let (texture, dmabuf) = wgpu_dmabuf::export(&self.device, &desc)
            .map_err(|e| format!("Failed to export dmabuf texture: {}", e))?;

        let info = DmabufExportInfo {
            width,
            height,
            modifier: dmabuf.modifier,
            fourcc: dmabuf.fourcc,
            planes: dmabuf
                .planes
                .iter()
                .map(|p| {
                    let fd = p
                        .fd
                        .try_clone()
                        .map_err(|e| format!("Failed to clone dmabuf fd: {}", e))?;
                    Ok(DmabufPlane {
                        fd,
                        offset: p.offset,
                        stride: p.stride,
                    })
                })
                .collect::<Result<Vec<DmabufPlane>, String>>()?,
        };

        Ok((texture, info))
    }
}

/// DMA-BUF plane information for Wayland buffer creation.
#[derive(Debug)]
pub struct DmabufPlane {
    /// File descriptor for the DMA-BUF.
    pub fd: OwnedFd,
    /// Offset in bytes.
    pub offset: u32,
    /// Stride in bytes.
    pub stride: u32,
}

/// DMA-BUF export information for creating Wayland buffers.
#[derive(Debug)]
pub struct DmabufExportInfo {
    /// Texture width.
    pub width: u32,
    /// Texture height.
    pub height: u32,
    /// DMA-BUF modifier.
    pub modifier: u64,
    /// FourCC format code.
    pub fourcc: wgpu_dmabuf::format::Fourcc,
    /// Plane information.
    pub planes: Vec<DmabufPlane>,
}

/// Estimate the GPU memory usage for a texture.
pub fn estimate_texture_memory(config: &TextureConfig) -> u64 {
    let bytes_per_pixel = match config.format {
        TexFormat::Rgba8Unorm | TexFormat::Bgra8UnormSrgb | TexFormat::Rgba8UnormSrgb => 4,
        TexFormat::Rgba16Float => 8,
        TexFormat::Rgba32Float => 16,
        TexFormat::Depth32Float => 4,
        TexFormat::Depth24PlusStencil8 => 4,
    };
    (config.width as u64)
        .saturating_mul(config.height as u64)
        .saturating_mul(bytes_per_pixel as u64)
        .saturating_mul(config.array_layers as u64)
}

/// Process commands and estimate the number of draw calls.
pub fn estimate_draw_calls(commands: &[Command]) -> usize {
    commands
        .iter()
        .filter(|cmd| match cmd {
            Command::Clear(_) => false,
            Command::PushTransform(_) => false,
            Command::PopTransform => false,
            Command::SetBlendMode(_) => false,
            Command::SetOpacity(_) => false,
            Command::Clip { .. } => false,
            Command::ResetClip => false,
            _ => true,
        })
        .count()
}

/// Vertex buffer builder for batching draw calls.
#[derive(Debug, Clone)]
pub struct VertexBufferBuilder<T> {
    /// Collected vertices.
    vertices: Vec<T>,
    /// Maximum capacity.
    capacity: usize,
}

impl<T: Clone> VertexBufferBuilder<T> {
    /// Create a new vertex buffer builder with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            vertices: Vec::with_capacity(capacity),
            capacity,
        }
    }

    /// Add vertices to the buffer.
    pub fn extend(&mut self, vertices: &[T]) {
        self.vertices.extend_from_slice(vertices);
    }

    /// Add a single vertex.
    pub fn push(&mut self, vertex: T) {
        self.vertices.push(vertex);
    }

    /// Check if adding more vertices would exceed capacity.
    pub fn would_exceed(&self, count: usize) -> bool {
        self.vertices.len() + count > self.capacity
    }

    /// Get the current vertices.
    pub fn as_slice(&self) -> &[T] {
        &self.vertices
    }

    /// Get the number of vertices.
    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.vertices.clear();
    }

    /// Take ownership of the vertices.
    pub fn into_inner(self) -> Vec<T> {
        self.vertices
    }
}

impl<T: Clone> Default for VertexBufferBuilder<T> {
    fn default() -> Self {
        Self::new(1024)
    }
}

/// Index buffer builder for indexed drawing.
#[derive(Debug, Clone)]
pub struct IndexBufferBuilder {
    /// Collected indices.
    indices: Vec<u32>,
    /// Base index offset.
    base: u32,
}

impl IndexBufferBuilder {
    /// Create a new index buffer builder.
    pub fn new() -> Self {
        Self {
            indices: Vec::new(),
            base: 0,
        }
    }

    /// Set the base index offset.
    pub fn with_base(mut self, base: u32) -> Self {
        self.base = base;
        self
    }

    /// Add a triangle.
    pub fn triangle(&mut self, a: u32, b: u32, c: u32) {
        self.indices.push(a + self.base);
        self.indices.push(b + self.base);
        self.indices.push(c + self.base);
    }

    /// Add a quad (two triangles).
    pub fn quad(&mut self, a: u32, b: u32, c: u32, d: u32) {
        self.triangle(a, b, c);
        self.triangle(b, d, c);
    }

    /// Get the current indices.
    pub fn as_slice(&self) -> &[u32] {
        &self.indices
    }

    /// Get the number of indices.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Clear the buffer.
    pub fn clear(&mut self) {
        self.indices.clear();
    }
}

impl Default for IndexBufferBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uniform_buffer_object() {
        let ubo = UniformBufferObject::default();
        assert_eq!(ubo.transform, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(ubo.offset, [0.0, 0.0]);
    }

    #[test]
    fn test_textured_vertex_quad() {
        let vertices = TexturedVertex::quad(0.0, 0.0, 100.0, 50.0, [0.0, 1.0, 1.0, 0.0], [1.0; 4]);
        assert_eq!(vertices.len(), 6);
    }

    #[test]
    fn test_solid_vertex_quad() {
        let vertices = SolidVertex::quad(10.0, 20.0, 80.0, 60.0, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(vertices.len(), 6);
    }

    #[test]
    fn test_solid_vertex_circle() {
        let vertices = SolidVertex::circle(0.0, 0.0, 50.0, 32, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(vertices.len(), 32 * 3);
    }

    #[test]
    fn test_vertex_buffer_builder() {
        let mut builder: VertexBufferBuilder<SolidVertex> = VertexBufferBuilder::new(1024);
        builder.extend(&SolidVertex::quad(0.0, 0.0, 100.0, 100.0, [1.0; 4]));
        assert_eq!(builder.len(), 6);
        assert!(!builder.is_empty());
    }

    #[test]
    fn test_index_buffer_builder() {
        let mut builder = IndexBufferBuilder::new();
        builder.quad(0, 1, 2, 3);
        assert_eq!(builder.len(), 6);
    }

    #[test]
    fn test_estimate_draw_calls() {
        let commands = vec![
            Command::Clear(crate::color::Color::BLACK),
            Command::Rect {
                rect: crate::geometry::Rect::ZERO,
                fill: Some(crate::primitives::FillStyle::Solid(
                    crate::color::Color::RED,
                )),
                stroke: None,
            },
            Command::PushTransform(crate::transform::Transform::new()),
            Command::PopTransform,
        ];
        let draws = estimate_draw_calls(&commands);
        assert_eq!(draws, 1); // Only the rect is a draw call
    }

    #[tokio::test]
    async fn test_gpu_backend_creation() {
        let backend = GpuBackend::new(RendererBackend::Vulkan, PowerPreference::HighPerformance)
            .await
            .unwrap();
        assert!(backend.is_initialized());
        assert_eq!(backend.texture_count(), 0);
    }
}
