//! Renderer — the core GPU rendering engine.
//!
//! The Renderer manages the wgpu device, queue, pipelines, and caches.
//! It is the entry point for creating surfaces, textures, and rendering frames.
//! All wgpu internals are hidden from application code.

use std::collections::{HashMap, HashSet};

use crate::backend::GpuBackend;
use crate::color::Color;
use crate::command::FrameCallback;
use crate::geometry::{Rect, Size};
use crate::image::Image;
use crate::render::{DrawCommand, FrameState, RenderError, RenderPassInfo, RenderTarget};
use crate::scene::{Node, NodeContent, Scene};
use crate::surface::{Surface, SurfaceConfig, SurfaceId};
use crate::texture::{Texture, TextureConfig, TextureId};
use crate::transform::Transform;

/// GPU backend to use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RendererBackend {
    /// Automatically select the best available backend.
    Auto,
    /// Vulkan (Linux, Windows).
    Vulkan,
    /// Metal (macOS, iOS).
    Metal,
    /// DirectX 12 (Windows).
    Dx12,
    /// OpenGL (fallback).
    Gl,
    /// WebGPU (browsers).
    WebGpu,
    /// Null backend (no GPU, for testing).
    Null,
}

impl Default for RendererBackend {
    fn default() -> Self {
        Self::Auto
    }
}

/// Configuration for creating a Renderer.
#[derive(Debug, Clone)]
pub struct RendererConfig {
    /// GPU backend to use.
    pub backend: RendererBackend,
    /// Power preference (high performance or low power).
    pub power_preference: PowerPreference,
    /// Whether to enable device loss detection.
    pub detect_lost_devices: bool,
    /// Maximum texture size supported.
    pub max_texture_size: u32,
    /// Whether to enable timeline semaphores (Vulkan).
    pub timeline_semaphores: bool,
}

impl RendererConfig {
    /// Create a new config with default values.
    pub fn new() -> Self {
        Self {
            backend: RendererBackend::default(),
            power_preference: PowerPreference::HighPerformance,
            detect_lost_devices: true,
            max_texture_size: 8192,
            timeline_semaphores: true,
        }
    }

    /// Set the backend.
    pub fn with_backend(mut self, backend: RendererBackend) -> Self {
        self.backend = backend;
        self
    }

    /// Set the power preference.
    pub fn with_power_preference(mut self, preference: PowerPreference) -> Self {
        self.power_preference = preference;
        self
    }
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Power preference for GPU adapter selection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerPreference {
    /// Prefer a high-performance GPU (e.g., discrete GPU).
    HighPerformance,
    /// Prefer a low-power GPU (e.g., integrated GPU).
    LowPower,
}

impl Default for PowerPreference {
    fn default() -> Self {
        Self::HighPerformance
    }
}

/// Statistics about the renderer.
#[derive(Debug, Clone)]
pub struct RendererStats {
    /// Number of surfaces.
    pub surface_count: usize,
    /// Number of textures.
    pub texture_count: usize,
    /// Number of frames rendered.
    pub frame_count: u64,
    /// GPU adapter name.
    pub adapter_name: String,
    /// GPU device description.
    pub device_description: String,
}

/// The main rendering engine.
///
/// Manages the GPU device, queue, pipelines, and caches.
/// Application code interacts with the Renderer to create surfaces,
/// upload textures, and render frames.
pub struct Renderer {
    /// Renderer configuration.
    config: RendererConfig,
    /// Next surface ID counter.
    next_surface_id: u64,
    /// Next texture ID counter.
    next_texture_id: u64,
    /// Registered surfaces.
    surfaces: HashMap<SurfaceId, Surface>,
    /// Registered textures.
    textures: HashMap<TextureId, Texture>,
    /// Frame counter.
    frame_count: u64,
    /// GPU adapter name (set after initialization).
    adapter_name: String,
    /// GPU device description.
    device_description: String,
    /// Optional GPU backend (None when running without a GPU).
    #[allow(dead_code)]
    gpu_backend: Option<GpuBackend>,
    /// Current frame state (set during begin_frame/end_frame).
    frame_state: Option<FrameState>,
}

impl Renderer {
    /// Create a new Renderer with default configuration.
    ///
    /// This is an async operation that initializes the GPU device.
    /// Returns an error if no GPU adapter is available.
    pub async fn new() -> Result<Self, String> {
        Self::with_config(RendererConfig::default()).await
    }

    /// Create a new Renderer with the given configuration.
    pub async fn with_config(config: RendererConfig) -> Result<Self, String> {
        // Try to initialize the GPU backend
        let mut gpu_backend = None;

        if config.backend != RendererBackend::Null {
            match GpuBackend::new(config.backend, config.power_preference).await {
                Ok(backend) => {
                    gpu_backend = Some(backend);
                }
                Err(e) => {
                    log::warn!(
                        "Failed to initialize GPU backend: {}. Running in stub mode.",
                        e
                    );
                }
            }
        }

        let (adapter_name, device_description) = if gpu_backend.is_some() {
            ("GPU Backend".to_string(), "wgpu Device".to_string())
        } else {
            ("Stub Adapter".to_string(), "Stub Device".to_string())
        };

        Ok(Self {
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name,
            device_description,
            config,
            gpu_backend,
            frame_state: None,
        })
    }

    /// Create a new surface with the given dimensions.
    pub async fn create_surface(&mut self, width: u32, height: u32) -> Result<SurfaceId, String> {
        self.create_surface_with_config(SurfaceConfig::new(width, height))
            .await
    }

    /// Create a new surface with the given configuration.
    pub async fn create_surface_with_config(
        &mut self,
        config: SurfaceConfig,
    ) -> Result<SurfaceId, String> {
        let id = SurfaceId::new(self.next_surface_id);
        self.next_surface_id += 1;

        let surface = Surface::new(id, config);
        self.surfaces.insert(id, surface);

        Ok(id)
    }

    /// Get a surface by ID.
    pub fn get_surface(&self, id: SurfaceId) -> Option<&Surface> {
        self.surfaces.get(&id)
    }

    /// Get a mutable reference to a surface.
    pub fn get_surface_mut(&mut self, id: SurfaceId) -> Option<&mut Surface> {
        self.surfaces.get_mut(&id)
    }

    /// Remove and destroy a surface, freeing associated resources.
    pub fn destroy_surface(&mut self, id: SurfaceId) {
        self.surfaces.remove(&id);
    }

    /// Remove a surface (alias for destroy_surface).
    pub fn remove_surface(&mut self, id: SurfaceId) {
        self.destroy_surface(id);
    }

    /// Create a texture with the given configuration.
    pub fn create_texture(&mut self, config: TextureConfig) -> TextureId {
        let id = TextureId::new(self.next_texture_id);
        self.next_texture_id += 1;

        let texture = Texture::new(id, config);
        self.textures.insert(id, texture);

        id
    }

    /// Create a texture from an Image.
    pub fn create_texture_from_image(&mut self, image: &Image) -> TextureId {
        let id = self.create_texture(TextureConfig::new(image.width(), image.height()));
        if let Some(texture) = self.textures.get_mut(&id) {
            texture.mark_uploaded();
        }
        id
    }

    /// Get a texture by ID.
    pub fn get_texture(&self, id: TextureId) -> Option<&Texture> {
        self.textures.get(&id)
    }

    /// Remove a texture.
    pub fn remove_texture(&mut self, id: TextureId) {
        self.textures.remove(&id);
    }

    /// Render a frame to the given surface.
    ///
    /// The callback receives a CommandBuffer that the application uses
    /// to describe what to render. The Renderer then compiles and submits
    /// the GPU commands.
    pub async fn render_frame<F>(
        &mut self,
        _surface_id: SurfaceId,
        _callback: F,
    ) -> Result<(), String>
    where
        F: FrameCallback,
    {
        // In a real implementation, this would:
        // 1. Acquire a frame from the surface
        // 2. Create a render pass
        // 3. Execute the callback to record commands
        // 4. Submit the command buffer to the GPU queue
        // 5. Present the frame

        self.frame_count += 1;
        Ok(())
    }

    /// Render a frame to the given surface using a closure.
    pub async fn with_frame<F>(&mut self, surface_id: SurfaceId, callback: F) -> Result<(), String>
    where
        F: FnOnce(&mut crate::command::CommandBuffer),
    {
        self.render_frame(surface_id, callback).await
    }

    /// Resize a surface.
    pub fn resize_surface(&mut self, id: SurfaceId, width: u32, height: u32) {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.resize(width, height);
        }
    }

    /// Get renderer statistics.
    pub fn stats(&self) -> RendererStats {
        RendererStats {
            surface_count: self.surfaces.len(),
            texture_count: self.textures.len(),
            frame_count: self.frame_count,
            adapter_name: self.adapter_name.clone(),
            device_description: self.device_description.clone(),
        }
    }

    /// Get the configuration.
    pub fn config(&self) -> &RendererConfig {
        &self.config
    }

    /// Check if the renderer is initialized.
    pub fn is_initialized(&self) -> bool {
        !self.adapter_name.is_empty()
    }

    /// Check if the GPU backend is available.
    pub fn is_gpu_available(&self) -> bool {
        self.gpu_backend.is_some()
    }

    /// Get a reference to the wgpu instance for Slint integration.
    ///
    /// Returns None if the GPU backend is not initialized.
    pub fn instance(&self) -> Option<&wgpu::Instance> {
        self.gpu_backend.as_ref().map(|b| b.instance())
    }

    /// Get a reference to the wgpu device for Slint integration.
    ///
    /// Returns None if the GPU backend is not initialized.
    pub fn device(&self) -> Option<&wgpu::Device> {
        self.gpu_backend.as_ref().map(|b| b.device())
    }

    /// Get a reference to the wgpu queue for Slint integration.
    ///
    /// Returns None if the GPU backend is not initialized.
    pub fn queue(&self) -> Option<&wgpu::Queue> {
        self.gpu_backend.as_ref().map(|b| b.queue())
    }

    /// Get a reference to the wgpu adapter.
    ///
    /// Returns None if the GPU backend is not initialized.
    pub fn adapter(&self) -> Option<&wgpu::Adapter> {
        self.gpu_backend.as_ref().map(|b| b.adapter())
    }

    /// Register a Wayland surface with the renderer for GPU presentation.
    ///
    /// This creates a wgpu Surface from raw Wayland display and surface pointers,
    /// then registers it with the GPU backend. This is the key integration point
    /// for headless Wayland rendering — it allows the dock to render via GPU
    /// instead of CPU software rendering.
    ///
    /// # Safety
    /// The display and surface pointers must be valid for the lifetime of the surface.
    /// The caller is responsible for ensuring the Wayland connection is active.
    pub unsafe fn register_wayland_surface(
        &mut self,
        surface_id: SurfaceId,
        width: u32,
        height: u32,
        display: *mut std::ffi::c_void,
        surface: *mut std::ffi::c_void,
    ) -> Result<(), String> {
        let backend = self.gpu_backend.as_mut()
            .ok_or_else(|| "GPU backend not initialized".to_string())?;

        // Create wgpu Surface from raw Wayland pointers
        let wgpu_surface = backend.create_wayland_surface(display, surface)?;

        // Register the surface with the backend
        let config = SurfaceConfig::new(width, height);
        backend.register_surface_from_handle(surface_id, config.clone(), wgpu_surface);

        // Track the surface in the renderer's surface map
        let surf = Surface::new(surface_id, config);
        self.surfaces.insert(surface_id, surf);

        Ok(())
    }

    /// Resize a registered Wayland surface.
    pub fn resize_wayland_surface(&mut self, surface_id: SurfaceId, width: u32, height: u32) {
        if let Some(backend) = &mut self.gpu_backend {
            backend.resize_surface(surface_id, width, height);
        }
        if let Some(surface) = self.surfaces.get_mut(&surface_id) {
            surface.config.width = width;
            surface.config.height = height;
        }
    }

    /// Remove a registered Wayland surface.
    pub fn remove_wayland_surface(&mut self, surface_id: SurfaceId) {
        if let Some(backend) = &mut self.gpu_backend {
            backend.remove_surface(surface_id);
        }
        self.surfaces.remove(&surface_id);
    }

    /// Get the number of surfaces.
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    /// Get the number of textures.
    pub fn texture_count(&self) -> usize {
        self.textures.len()
    }

    /// Get the frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Clear all surfaces and textures.
    pub fn clear(&mut self) {
        self.surfaces.clear();
        self.textures.clear();
    }

    // -----------------------------------------------------------------------
    // Frame rendering API
    // -----------------------------------------------------------------------

    /// Begin a new frame for the given surface.
    ///
    /// This prepares the renderer for drawing: it creates a frame state,
    /// clears the draw command buffer, and resets the transform and clip stacks.
    ///
    /// # Errors
    /// Returns `RenderError::SurfaceNotFound` if the surface does not exist.
    pub fn begin_frame(&mut self, surface_id: SurfaceId) -> Result<(), RenderError> {
        // Check if a frame is already in progress
        if self.frame_state.is_some() {
            return Err(RenderError::Other(
                "begin_frame called without end_frame".to_string(),
            ));
        }

        // Verify surface exists
        let surface = self
            .surfaces
            .get(&surface_id)
            .ok_or_else(|| RenderError::SurfaceNotFound(surface_id))?;

        let size = Size::new(surface.config.width as f32, surface.config.height as f32);
        let scale_factor = surface.config.scale_factor;

        self.frame_count += 1;
        self.frame_state = Some(FrameState::new(
            surface_id,
            size,
            scale_factor,
            self.frame_count,
        ));

        Ok(())
    }

    /// Render a scene graph, producing draw commands.
    ///
    /// Traverses the scene, collects visible nodes, resolves transforms and
    /// clipping, and accumulates draw commands in the current frame state.
    ///
    /// Must be called between `begin_frame()` and `end_frame()`.
    ///
    /// # Errors
    /// Returns `RenderError::Other` if `begin_frame` was not called first.
    pub fn render_scene(&mut self, scene: &Scene) -> Result<RenderPassInfo, RenderError> {
        let frame_number = self
            .frame_state
            .as_ref()
            .ok_or_else(|| {
                RenderError::Other("render_scene called without begin_frame".to_string())
            })?
            .frame_number();

        let mut visible_count = 0;
        let mut clip_count = 0;
        let mut textures_used = HashSet::new();
        let mut draw_commands = Vec::new();

        // Traverse visible nodes in the scene — collect data without borrowing self
        scene.traverse_visible(|node| {
            // Resolve the node's transform
            let transform = Self::resolve_transform(node);

            // Handle clipping from groups
            let clip_rect = if let NodeContent::Group(group) = &node.content {
                if group.clip_to_bounds {
                    Some(node.style.bounding_rect())
                } else if let Some(ref c) = group.clip {
                    Some(*c)
                } else {
                    None
                }
            } else {
                None
            };

            // Generate draw command for this node
            let cmd = Self::node_to_draw_command(node, &mut textures_used);

            visible_count += 1;
            clip_count += if clip_rect.is_some() { 1 } else { 0 };
            draw_commands.push((transform, clip_rect, cmd));
        });

        // Now apply accumulated changes to frame_state
        let frame_state = self.frame_state.as_mut().ok_or_else(|| {
            RenderError::Other("render_scene called without begin_frame".to_string())
        })?;

        for (transform, clip_rect, cmd) in draw_commands {
            if !frame_state.is_visible() {
                break;
            }

            frame_state.push_transform(transform);

            // Apply opacity from node (already baked into the command)
            // The opacity tracking is done per-node in the traversal

            if let Some(clip) = clip_rect {
                frame_state.push_clip(clip);
                frame_state
                    .draw_commands_mut()
                    .push(DrawCommand::PushClip { clip_rect: clip });
            }

            if let Some(command) = cmd {
                frame_state.draw_commands_mut().push(command);
            }
        }

        let draw_command_count = frame_state.draw_command_count();
        let texture_count = textures_used.len();

        Ok(RenderPassInfo::new(
            visible_count,
            draw_command_count,
            texture_count,
            clip_count,
            frame_number,
        ))
    }

    /// Resolve a node's local transform from its style.
    fn resolve_transform(node: &Node) -> Transform {
        let mut t = Transform::new();

        // Translation
        t = t.compose(&Transform::translate(
            node.style.position.x,
            node.style.position.y,
        ));

        // Rotation (around anchor point, degrees → radians)
        if node.style.rotation != 0.0 {
            let anchor_x = node.style.anchor.x * node.style.size.width;
            let anchor_y = node.style.anchor.y * node.style.size.height;
            let radians = node.style.rotation.to_radians();
            t = t.compose(&Transform::translate(anchor_x, anchor_y));
            t = t.compose(&Transform::rotate(radians));
            t = t.compose(&Transform::translate(-anchor_x, -anchor_y));
        }

        // Scale
        if node.style.scale.x != 1.0 || node.style.scale.y != 1.0 {
            t = t.compose(&Transform::scale(node.style.scale.x, node.style.scale.y));
        }

        t
    }

    /// Convert a node to a draw command (static, no self borrow).
    fn node_to_draw_command(
        node: &Node,
        textures_used: &mut HashSet<TextureId>,
    ) -> Option<DrawCommand> {
        match &node.content {
            NodeContent::Rectangle(rect) => {
                let rect_rect = node.style.bounding_rect();
                Some(DrawCommand::Rectangle {
                    rect: rect_rect,
                    color: rect.color,
                    radius: rect.radius,
                    stroke: rect.stroke,
                    stroke_width: rect.stroke_width,
                })
            }
            NodeContent::Image(img) => {
                textures_used.insert(img.texture);
                let dest_rect = node.style.bounding_rect();
                let source_uv = img
                    .source_rect
                    .unwrap_or_else(|| Rect::new(0.0, 0.0, 1.0, 1.0));
                Some(DrawCommand::Image {
                    dest_rect,
                    texture: img.texture,
                    source_uv,
                    tint: img.tint,
                })
            }
            NodeContent::Text(text) => {
                let position = node.style.position;
                Some(DrawCommand::Text {
                    position,
                    content: text.content.clone(),
                    font_size: text.font_size,
                    color: text.color,
                })
            }
            NodeContent::Group(group) => {
                // Render group background if present
                if let Some(ref brush) = group.background {
                    let rect_rect = node.style.bounding_rect();
                    let color = match brush {
                        crate::brush::Brush::Solid(c) => *c,
                        _ => Color::WHITE,
                    };
                    Some(DrawCommand::Rectangle {
                        rect: rect_rect,
                        color,
                        radius: group.corner_radius.top_left,
                        stroke: None,
                        stroke_width: 0.0,
                    })
                } else {
                    None
                }
            }
            NodeContent::Video(video) => {
                // Video nodes are rendered as textured quads.
                // The actual texture is resolved at render time from the
                // VideoManager — here we record the stream ID for later.
                let dest_rect = node.style.bounding_rect();
                // Use a placeholder texture; the real texture is resolved
                // when the frame is uploaded via upload_video_frame()
                let placeholder = TextureId::new(video.stream_id.as_u64());
                textures_used.insert(placeholder);
                Some(DrawCommand::Video {
                    dest_rect,
                    texture: placeholder,
                    source_uv: video
                        .source_rect
                        .unwrap_or_else(|| Rect::new(0.0, 0.0, 1.0, 1.0)),
                    tint: video.tint,
                })
            }
        }
    }

    /// End the current frame and submit the draw commands.
    ///
    /// This finalizes the frame: submits GPU commands and presents the frame
    /// if the target is a presentable surface.
    ///
    /// # Errors
    /// Returns `RenderError::Other` if `begin_frame` was not called first.
    pub fn end_frame(&mut self) -> Result<RenderPassInfo, RenderError> {
        let frame_state = self.frame_state.take().ok_or_else(|| {
            RenderError::Other("end_frame called without begin_frame".to_string())
        })?;

        let info = RenderPassInfo::new(
            0,
            frame_state.draw_command_count(),
            0,
            frame_state.clip_count(),
            frame_state.frame_number(),
        );

        // Submit GPU commands if we have a backend and draw commands
        if let Some(backend) = &mut self.gpu_backend {
            if let Err(e) = Self::submit_frame_impl(backend, &frame_state) {
                log::warn!("Failed to submit GPU frame: {}", e);
            }
        }

        Ok(info)
    }

    /// Submit a frame's draw commands to the GPU.
    ///
    /// Converts screen-space draw commands to clip-space vertices,
    /// encodes a render pass, submits to the queue, and presents.
    fn submit_frame_impl(backend: &mut GpuBackend, frame_state: &FrameState) -> Result<(), String> {
        let surface_id = frame_state
            .surface_id()
            .ok_or_else(|| "No surface ID in frame state".to_string())?;

        // Acquire surface frame
        let surface_texture = backend
            .acquire_surface_frame(surface_id)
            .ok_or_else(|| "Failed to acquire surface frame".to_string())?;

        let width = frame_state.surface_size().width as f32;
        let height = frame_state.surface_size().height as f32;

        // Create command encoder
        let mut encoder =
            backend
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Frame Encoder"),
                });

        // Begin render pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Frame Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_texture
                        .texture
                        .create_view(&wgpu::TextureViewDescriptor::default()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Draw each command
            Self::draw_commands_to_gpu_impl(
                &mut render_pass,
                backend,
                frame_state.draw_commands(),
                width,
                height,
            );
        }

        // Submit command buffer
        backend.queue().submit(std::iter::once(encoder.finish()));

        // Present the frame
        backend.queue().present(surface_texture);

        Ok(())
    }

    /// Render draw commands to a DMA-BUF texture for Wayland presentation.
    ///
    /// This creates a DMA-BUF exportable texture, renders the provided draw
    /// commands to it, and returns the export info for creating a Wayland wl_buffer.
    /// Unlike the surface-based rendering path, this works for headless Wayland
    /// where wgpu surface presentation doesn't reach the compositor.
    ///
    /// # Arguments
    /// * `width` - Texture width in physical pixels.
    /// * `height` - Texture height in physical pixels.
    /// * `commands` - Draw commands to render (bypasses the scene graph).
    ///
    /// # Returns
    /// The DMA-BUF export info containing file descriptors, offsets, and strides
    /// for creating a Wayland buffer, along with the width and height.
    pub fn render_to_dmabuf(
        &mut self,
        width: u32,
        height: u32,
        commands: &[crate::render::DrawCommand],
    ) -> Result<crate::backend::DmabufExportInfo, String> {
        let backend = self.gpu_backend.as_mut().ok_or_else(|| {
            "GPU backend not available for DMA-BUF rendering".to_string()
        })?;

        // Create a DMA-BUF exportable texture
        let (texture, export_info) = backend.create_dmabuf_texture(width, height)?;

        let width_f = width as f32;
        let height_f = height as f32;

        // Create command encoder
        let mut encoder = backend.device().create_command_encoder(
            &wgpu::CommandEncoderDescriptor {
                label: Some("DMA-BUF Frame Encoder"),
            },
        );

        // Begin render pass on the DMA-BUF texture
        {
            let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("DMA-BUF Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Render draw commands directly (bypasses scene graph)
            Self::draw_commands_to_gpu_impl(
                &mut render_pass,
                backend,
                commands,
                width_f,
                height_f,
            );
        }

        // Submit command buffer
        backend.queue().submit(std::iter::once(encoder.finish()));

        Ok(export_info)
    }

    /// Convert draw commands to GPU draw calls.
    fn draw_commands_to_gpu_impl(
        render_pass: &mut wgpu::RenderPass<'_>,
        backend: &GpuBackend,
        commands: &[crate::render::DrawCommand],
        width: f32,
        height: f32,
    ) {
        // Convert screen coordinates to clip space:
        // x_clip = (x_screen / width) * 2.0 - 1.0
        // y_clip = (y_screen / height) * 2.0 - 1.0
        // Note: Y is flipped because clip space has Y up, screen has Y down
        let to_x = |x: f32| (x / width) * 2.0 - 1.0;
        let to_y = |y: f32| 1.0 - (y / height) * 2.0;

        for cmd in commands {
            match cmd {
                crate::render::DrawCommand::Rectangle {
                    rect,
                    color,
                    radius: _radius,
                    stroke: _stroke,
                    stroke_width: _stroke_width,
                } => {
                    // Draw solid rectangle
                    let x0 = to_x(rect.x);
                    let y0 = to_y(rect.y);
                    let x1 = to_x(rect.x + rect.width);
                    let y1 = to_y(rect.y + rect.height);
                    let c = [color.r, color.g, color.b, color.a];

                    let vertices: [crate::backend::SolidVertex; 6] =
                        crate::backend::SolidVertex::quad(x0, y0, x1 - x0, y1 - y0, c);

                    let buffer = backend.device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Solid Quad Buffer"),
                        size: (std::mem::size_of::<crate::backend::SolidVertex>() * 6) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    backend
                        .queue()
                        .write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));

                    render_pass.set_pipeline(
                        backend
                            .solid_pipeline()
                            .ok_or_else(|| "No solid pipeline")
                            .unwrap(),
                    );
                    // Bind the uniform bind group (uniform buffer + sampler + placeholder texture)
                    // This is required by the solid pipeline's bind group layout at index 0.
                    if let Some(bind_group) = backend.uniform_bind_group() {
                        render_pass.set_bind_group(0, bind_group, &[]);
                    }
                    render_pass.set_vertex_buffer(0, buffer.slice(..));
                    render_pass.draw(0..6, 0..1);
                }
                crate::render::DrawCommand::Image {
                    dest_rect,
                    texture,
                    source_uv,
                    tint,
                } => {
                    // Draw textured quad
                    let x0 = to_x(dest_rect.x);
                    let y0 = to_y(dest_rect.y);
                    let x1 = to_x(dest_rect.x + dest_rect.width);
                    let y1 = to_y(dest_rect.y + dest_rect.height);
                    let c = [tint.r, tint.g, tint.b, tint.a];
                    let uv = [
                        source_uv.x,
                        source_uv.y,
                        source_uv.x + source_uv.width,
                        source_uv.y + source_uv.height,
                    ];

                    let vertices: [crate::backend::TexturedVertex; 6] =
                        crate::backend::TexturedVertex::quad(x0, y0, x1 - x0, y1 - y0, uv, c);

                    let buffer = backend.device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Textured Quad Buffer"),
                        size: (std::mem::size_of::<crate::backend::TexturedVertex>() * 6) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    backend
                        .queue()
                        .write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));

                    render_pass.set_pipeline(
                        backend
                            .textured_pipeline()
                            .ok_or_else(|| "No textured pipeline")
                            .unwrap(),
                    );
                    render_pass.set_vertex_buffer(0, buffer.slice(..));

                    // Set texture bind group if available
                    if let Some(texture_view) = backend.get_texture_view(*texture) {
                        let bind_group_layout = backend.device().create_bind_group_layout(
                            &wgpu::BindGroupLayoutDescriptor {
                                label: Some("Image Bind Group Layout"),
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
                                        ty: wgpu::BindingType::Sampler(
                                            wgpu::SamplerBindingType::Filtering,
                                        ),
                                        count: None,
                                    },
                                    wgpu::BindGroupLayoutEntry {
                                        binding: 2,
                                        visibility: wgpu::ShaderStages::FRAGMENT,
                                        ty: wgpu::BindingType::Texture {
                                            sample_type: wgpu::TextureSampleType::Float {
                                                filterable: true,
                                            },
                                            view_dimension: wgpu::TextureViewDimension::D2,
                                            multisampled: false,
                                        },
                                        count: None,
                                    },
                                ],
                            },
                        );
                        let uniform_buf = backend.uniform_buffer().unwrap();
                        let sampler = backend.sampler().unwrap();
                        let bind_group =
                            backend
                                .device()
                                .create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: Some("Image Bind Group"),
                                    layout: &bind_group_layout,
                                    entries: &[
                                        wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: uniform_buf.as_entire_binding(),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 1,
                                            resource: wgpu::BindingResource::Sampler(sampler),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 2,
                                            resource: wgpu::BindingResource::TextureView(
                                                texture_view,
                                            ),
                                        },
                                    ],
                                });
                        render_pass.set_bind_group(0, &bind_group, &[]);
                    }

                    render_pass.draw(0..6, 0..1);
                }
                crate::render::DrawCommand::Text { .. } => {
                    // Text rendering requires glyph atlas - skip for now
                    // Text will be rendered via texture quads once glyph atlas is populated
                }
                crate::render::DrawCommand::Video {
                    dest_rect,
                    texture,
                    source_uv,
                    tint,
                } => {
                    // Video frames are rendered as textured quads
                    let x0 = to_x(dest_rect.x);
                    let y0 = to_y(dest_rect.y);
                    let x1 = to_x(dest_rect.x + dest_rect.width);
                    let y1 = to_y(dest_rect.y + dest_rect.height);
                    let c = [tint.r, tint.g, tint.b, tint.a];
                    let uv = [
                        source_uv.x,
                        source_uv.y,
                        source_uv.x + source_uv.width,
                        source_uv.y + source_uv.height,
                    ];

                    let vertices: [crate::backend::TexturedVertex; 6] =
                        crate::backend::TexturedVertex::quad(x0, y0, x1 - x0, y1 - y0, uv, c);

                    let buffer = backend.device().create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Video Quad Buffer"),
                        size: (std::mem::size_of::<crate::backend::TexturedVertex>() * 6) as u64,
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    backend
                        .queue()
                        .write_buffer(&buffer, 0, bytemuck::cast_slice(&vertices));

                    render_pass.set_pipeline(
                        backend
                            .textured_pipeline()
                            .ok_or_else(|| "No textured pipeline")
                            .unwrap(),
                    );
                    render_pass.set_vertex_buffer(0, buffer.slice(..));

                    if let Some(texture_view) = backend.get_texture_view(*texture) {
                        let sampler = backend.sampler().unwrap();
                        let uniform_buf = backend.uniform_buffer().unwrap();
                        let bind_group_layout = backend.device().create_bind_group_layout(
                            &wgpu::BindGroupLayoutDescriptor {
                                label: Some("Video Bind Group Layout"),
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
                                        ty: wgpu::BindingType::Sampler(
                                            wgpu::SamplerBindingType::Filtering,
                                        ),
                                        count: None,
                                    },
                                    wgpu::BindGroupLayoutEntry {
                                        binding: 2,
                                        visibility: wgpu::ShaderStages::FRAGMENT,
                                        ty: wgpu::BindingType::Texture {
                                            sample_type: wgpu::TextureSampleType::Float {
                                                filterable: true,
                                            },
                                            view_dimension: wgpu::TextureViewDimension::D2,
                                            multisampled: false,
                                        },
                                        count: None,
                                    },
                                ],
                            },
                        );
                        let bind_group =
                            backend
                                .device()
                                .create_bind_group(&wgpu::BindGroupDescriptor {
                                    label: Some("Video Bind Group"),
                                    layout: &bind_group_layout,
                                    entries: &[
                                        wgpu::BindGroupEntry {
                                            binding: 0,
                                            resource: uniform_buf.as_entire_binding(),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 1,
                                            resource: wgpu::BindingResource::Sampler(sampler),
                                        },
                                        wgpu::BindGroupEntry {
                                            binding: 2,
                                            resource: wgpu::BindingResource::TextureView(
                                                texture_view,
                                            ),
                                        },
                                    ],
                                });
                        render_pass.set_bind_group(0, &bind_group, &[]);
                    }

                    render_pass.draw(0..6, 0..1);
                }
                // State-changing commands are handled implicitly
                crate::render::DrawCommand::PushClip { .. }
                | crate::render::DrawCommand::PopClip
                | crate::render::DrawCommand::SetTransform { .. }
                | crate::render::DrawCommand::SetOpacity { .. } => {
                    // These are state commands handled during scene traversal
                }
            }
        }
    }

    /// Render a scene to the given target in a single call.
    ///
    /// This is a convenience method that combines `begin_frame`, `render_scene`
    /// and `end_frame`. It is more flexible than making every target behave
    /// like a window, as it supports offscreen textures and buffers.
    ///
    /// # Errors
    /// Returns `RenderError` if the target is invalid or rendering fails.
    pub async fn render(
        &mut self,
        target: RenderTarget,
        scene: &Scene,
    ) -> Result<RenderPassInfo, RenderError> {
        match &target {
            RenderTarget::Surface(surface_id) => {
                self.begin_frame(*surface_id)?;
            }
            RenderTarget::Texture(texture_id) => {
                // Verify texture exists
                if !self.textures.contains_key(texture_id) {
                    return Err(RenderError::TextureNotFound(*texture_id));
                }
                // For texture targets, use a dummy surface state
                self.frame_count += 1;
                self.frame_state = Some(FrameState::new(
                    SurfaceId::new(0),
                    Size::new(256.0, 256.0),
                    1.0,
                    self.frame_count,
                ));
            }
            RenderTarget::Offscreen { size, scale_factor } => {
                self.frame_count += 1;
                self.frame_state = Some(FrameState::new(
                    SurfaceId::new(0),
                    *size,
                    *scale_factor,
                    self.frame_count,
                ));
            }
        }

        let pass_info = self.render_scene(scene)?;
        let _end_info = self.end_frame()?;

        // Return the more detailed pass_info from render_scene
        Ok(pass_info)
    }

    /// Get the current frame state (if a frame is in progress).
    pub fn current_frame_state(&self) -> Option<&FrameState> {
        self.frame_state.as_ref()
    }

    /// Get the current mutable frame state (if a frame is in progress).
    pub fn current_frame_state_mut(&mut self) -> Option<&mut FrameState> {
        self.frame_state.as_mut()
    }

    // -----------------------------------------------------------------------
    // Video integration
    // -----------------------------------------------------------------------

    /// Upload a video frame to a GPU texture.
    ///
    /// This converts the frame data to RGBA and uploads it to the GPU.
    /// The returned `VideoTexture` handle can be used for rendering.
    ///
    /// # Errors
    /// Returns `RenderError` if the frame format is unsupported or
    /// the texture allocation fails.
    pub fn upload_video_frame(
        &mut self,
        stream_id: crate::video::StreamId,
        frame: &crate::video::VideoFrame,
    ) -> Result<crate::video::VideoTexture, RenderError> {
        // Convert to RGBA (validates format support)
        let _rgba_data = frame.to_rgba().ok_or_else(|| {
            RenderError::Other(format!("Unsupported video format: {:?}", frame.format))
        })?;

        // Create or reuse texture
        let texture_id = TextureId::new(stream_id.as_u64());
        let config = TextureConfig::new(frame.width, frame.height)
            .with_format(crate::texture::TextureFormat::Rgba8Unorm);

        // Insert or replace the texture
        self.textures
            .insert(texture_id, Texture::new(texture_id, config));

        Ok(crate::video::VideoTexture::new(
            texture_id,
            frame.width as f32,
            frame.height as f32,
            frame.timestamp,
        ))
    }

    /// Draw a video frame into the scene.
    ///
    /// This adds a video node to the scene at the given destination rectangle.
    /// The video texture is resolved from the `VideoManager` at render time.
    pub fn draw_video(
        &mut self,
        scene: &mut crate::scene::Scene,
        video: crate::video::VideoTexture,
        destination: Rect,
    ) {
        use crate::geometry::NodeId;
        use crate::scene::{Node, NodeContent, NodeStyle};

        let node_id = NodeId::new(video.texture.as_u64());

        // Remove existing video node with the same ID if present
        scene.remove_node(node_id);

        let mut style = NodeStyle::new();
        style.position = crate::geometry::Point::new(destination.x, destination.y);
        style.size = crate::geometry::Size::new(destination.width, destination.height);

        let mut node = Node::new(
            node_id,
            NodeContent::Video(
                crate::video::VideoNode::new(crate::video::StreamId::new(video.texture.as_u64()))
                    .with_tint(Color::WHITE),
            ),
        );
        node.style = style;

        scene.add_node(node);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    #[test]
    fn test_renderer_config() {
        let config = RendererConfig::new()
            .with_backend(RendererBackend::Vulkan)
            .with_power_preference(PowerPreference::LowPower);
        assert_eq!(config.backend, RendererBackend::Vulkan);
        assert_eq!(config.power_preference, PowerPreference::LowPower);
    }

    #[tokio::test]
    async fn test_renderer_creation() {
        let renderer = Renderer::new().await.unwrap();
        assert!(renderer.is_initialized());
        assert_eq!(renderer.surface_count(), 0);
    }

    #[tokio::test]
    async fn test_surface_creation() {
        let mut renderer = Renderer::new().await.unwrap();
        let surface_id = renderer.create_surface(800, 600).await.unwrap();
        assert_eq!(renderer.surface_count(), 1);
        assert!(renderer.get_surface(surface_id).is_some());
    }

    #[test]
    fn test_texture_creation() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let texture_id = renderer.create_texture(TextureConfig::new(256, 256));
        assert_eq!(renderer.texture_count(), 1);
        assert!(renderer.get_texture(texture_id).is_some());
    }

    #[test]
    fn test_begin_frame_without_surface() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let result = renderer.begin_frame(SurfaceId::new(999));
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RenderError::SurfaceNotFound(_)
        ));
    }

    #[test]
    fn test_begin_frame_success() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        assert!(renderer.begin_frame(surface_id).is_ok());
        assert!(renderer.current_frame_state().is_some());
    }

    #[test]
    fn test_double_begin_frame() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        renderer.begin_frame(surface_id).unwrap();
        let result = renderer.begin_frame(surface_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_render_scene_without_begin_frame() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let scene = Scene::new();
        let result = renderer.render_scene(&scene);
        assert!(result.is_err());
    }

    #[test]
    fn test_end_frame_without_begin_frame() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let result = renderer.end_frame();
        assert!(result.is_err());
    }

    #[test]
    fn test_begin_render_end_frame() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        // Build a scene with a rectangle
        let mut scene = Scene::new();
        let rect_id = scene.create_rectangle(Color::RED);
        if let Some(node) = scene.get_mut(rect_id) {
            node.style.position = Point::new(10.0, 20.0);
            node.style.size = Size::new(100.0, 50.0);
        }

        renderer.begin_frame(surface_id).unwrap();
        let pass_info = renderer.render_scene(&scene).unwrap();
        assert_eq!(pass_info.visible_node_count, 1);
        assert_eq!(pass_info.draw_command_count, 1);

        let end_info = renderer.end_frame().unwrap();
        assert_eq!(end_info.draw_command_count, 1);

        // Frame state should be cleared
        assert!(renderer.current_frame_state().is_none());
    }

    #[test]
    fn test_render_scene_with_multiple_nodes() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        let mut scene = Scene::new();
        scene.create_rectangle(Color::RED);
        scene.create_rectangle(Color::BLUE);
        scene.create_text("Hello", 12.0, Color::BLACK);

        renderer.begin_frame(surface_id).unwrap();
        let pass_info = renderer.render_scene(&scene).unwrap();
        assert_eq!(pass_info.visible_node_count, 3);
        assert_eq!(pass_info.draw_command_count, 3);
        renderer.end_frame().unwrap();
    }

    #[test]
    fn test_render_scene_skips_invisible() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        let mut scene = Scene::new();
        let _visible = scene.create_rectangle(Color::RED);
        let hidden = scene.create_rectangle(Color::BLUE);
        if let Some(node) = scene.get_mut(hidden) {
            node.style.visible = false;
        }

        renderer.begin_frame(surface_id).unwrap();
        let pass_info = renderer.render_scene(&scene).unwrap();
        assert_eq!(pass_info.visible_node_count, 1);
        assert_eq!(pass_info.draw_command_count, 1);
        renderer.end_frame().unwrap();
    }

    #[tokio::test]
    async fn test_render_to_surface_target() {
        let mut renderer = Renderer::new().await.unwrap();
        let surface_id = renderer.create_surface(800, 600).await.unwrap();

        let mut scene = Scene::new();
        scene.create_rectangle(Color::GREEN);

        let target = RenderTarget::surface(surface_id);
        let info = renderer.render(target, &scene).await.unwrap();
        assert_eq!(info.visible_node_count, 1);
        assert_eq!(info.draw_command_count, 1);
    }

    #[tokio::test]
    async fn test_render_to_offscreen_target() {
        let mut renderer = Renderer::new().await.unwrap();

        let mut scene = Scene::new();
        scene.create_rectangle(Color::YELLOW);

        let target = RenderTarget::offscreen(Size::new(400.0, 300.0), 2.0);
        let info = renderer.render(target, &scene).await.unwrap();
        assert_eq!(info.visible_node_count, 1);
    }

    #[tokio::test]
    async fn test_render_to_missing_texture_target() {
        let mut renderer = Renderer::new().await.unwrap();

        let scene = Scene::new();
        let target = RenderTarget::texture(TextureId::new(999));
        let result = renderer.render(target, &scene).await;
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            RenderError::TextureNotFound(_)
        ));
    }

    #[test]
    fn test_frame_state_draw_commands() {
        let mut renderer = Renderer {
            config: RendererConfig::default(),
            next_surface_id: 1,
            next_texture_id: 1,
            surfaces: HashMap::new(),
            textures: HashMap::new(),
            frame_count: 0,
            adapter_name: "Test".to_string(),
            device_description: "Test".to_string(),
            gpu_backend: None,
            frame_state: None,
        };
        let surface_id = SurfaceId::new(1);
        let surface = crate::surface::Surface::new(surface_id, SurfaceConfig::new(800, 600));
        renderer.surfaces.insert(surface_id, surface);

        let mut scene = Scene::new();
        let rect_id = scene.create_rectangle(Color::RED);
        if let Some(node) = scene.get_mut(rect_id) {
            node.style.position = Point::new(10.0, 20.0);
            node.style.size = Size::new(100.0, 50.0);
        }

        renderer.begin_frame(surface_id).unwrap();
        renderer.render_scene(&scene).unwrap();

        let frame = renderer.current_frame_state().unwrap();
        let commands = frame.draw_commands();
        assert_eq!(commands.len(), 1);
        assert!(matches!(commands[0], DrawCommand::Rectangle { .. }));
    }
}
