//! GPU FemtoVG + wgpu renderer.
//!
//! Hardware-accelerated rendering using FemtoVG vector graphics
//! and wgpu (Vulkan) for the dock and popups.

/// FemtoVG + wgpu GPU renderer for the dock.
///
/// Replaces the CPU-based tiny_skia + shm pipeline with GPU-accelerated
/// FemtoVG rendering via wgpu (Vulkan on Linux).
///
/// The rendering pipeline:
/// 1. Create wgpu device/queue via Vulkan adapter
/// 2. Create a FemtoVG WGPURenderer + Canvas
/// 3. Draw all dock elements (background, icons, text, badges) via FemtoVG commands
/// 4. Render to a wgpu texture
/// 5. Share the texture with Wayland via wp_linux_dmabuf

use femtovg::{Canvas, Color, Paint, Path, ImageId, ImageSource, ImageFlags, Solidity, FontId};
use rgb::RGBA8;
use std::collections::HashMap;

use crate::render::dmabuf_export::DmabufPool;

/// GPU rendering context for FemtoVG + wgpu.
///
/// Holds a persistent FemtoVG Canvas that can be reused across frames.
/// The canvas owns the WGPURenderer which owns the wgpu Device and Queue.
/// Created asynchronously via `GpuContext::init()`.
pub struct GpuContext {
    pub canvas: Canvas<femtovg::renderer::WGPURenderer>,
    pub icon_cache: FemtoVGIconCache,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
    /// Pool of dmabuf render targets to avoid GPU memory exhaustion.
    pub dmabuf_pool: DmabufPool,
    /// Loaded font ID for text rendering
    pub font_id: Option<FontId>,
}

impl GpuContext {
    /// Initialize the GPU context: create wgpu device/queue and FemtoVG renderer.
    ///
    /// Uses Vulkan adapter on Linux, falls back to other available adapters.
    /// This is async because wgpu's adapter/device creation is async.
    pub async fn init() -> Result<Self, Box<dyn std::error::Error>> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN | wgpu::Backends::GL,
            flags: wgpu::InstanceFlags::empty(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::default(),
            display: None,
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                force_fallback_adapter: false,
                compatible_surface: None,
                apply_limit_buckets: false,
            })
            .await?;

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Dock GPU Device"),
                    required_features: wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF,
                    required_limits: wgpu::Limits::downlevel_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                    experimental_features: wgpu::ExperimentalFeatures::disabled(),
                    trace: wgpu::Trace::Off,
                },
            )
            .await?;

        // Clone device/queue before giving ownership to FemtoVG renderer
        let device_clone = device.clone();
        let queue_clone = queue.clone();
        
        // FemtoVG WGPURenderer takes owned Device and Queue
        let renderer = femtovg::renderer::WGPURenderer::new(device, queue);
        
        // Create a persistent canvas (initially 1x1, resized per frame)
        let mut canvas = Canvas::new(renderer).expect("Canvas::new failed");

        // Load a system font for text rendering
        let font_id = canvas.add_font("/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf")
            .ok();
        if let Some(_fid) = &font_id {
        } else {
        }

        Ok(Self {
            canvas,
            icon_cache: FemtoVGIconCache::new(),
            device: device_clone,
            queue: queue_clone,
            adapter,
            dmabuf_pool: DmabufPool::new(),
            font_id,
        })
    }

    /// Resize the canvas for the next frame.
    pub fn resize(&mut self, width: u32, height: u32, dpi: f32) {
        self.canvas.set_size(width, height, dpi);
    }

    /// Create a render target texture for the given size.
    /// Returns a wgpu::Texture that can be used as the FemtoVG render output.
    pub fn create_render_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Dock Render Target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    }

    /// Flush the rendered frame to the GPU texture.
    /// Returns the command buffer that needs to be submitted.
    pub fn flush_to_texture(&mut self, texture: &wgpu::Texture) -> Option<wgpu::CommandBuffer> {
        let output = femtovg::renderer::WGPURenderOutput::from(texture);
        self.canvas.flush_to_output(output)
    }
}

/// Draw the dock background (rounded rectangle with border) using FemtoVG.
pub fn draw_dock_background(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    dock_y: f32,
    dock_height: f32,
    width: f32,
    alpha: f32,
) {
    let radius = (dock_height.min(width) / 3.0).max(1.0);
    let inset = 1.0;
    let alpha = alpha.clamp(0.0, 1.0);

    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background fill - reference project: #1D1C22CC (alpha CC = 204)
    let mut bg_path = Path::new();
    bg_path.rounded_rect(inset, dock_y, width - inset * 2.0, dock_height, radius);
    let bg_paint = Paint::color(Color::rgba(29, 28, 34, 204));
    canvas.fill_path(&bg_path, &bg_paint);

    // Border - reference project: #4C4954
    let mut border_path = Path::new();
    border_path.rounded_rect(inset, dock_y, width - inset * 2.0, dock_height, radius);
    let border_paint = Paint::color(Color::rgba(76, 73, 84, 255));
    canvas.stroke_path(&border_path, &border_paint);

    canvas.restore();
}

/// Draw text using FemtoVG.
pub fn draw_text(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    text: &str,
    font_size: f32,
    x: f32,
    y: f32,
    color: (u8, u8, u8),
) {
    let mut paint = Paint::color(Color::rgb(color.0, color.1, color.2));
    paint.set_font_size(font_size);
    // Try to load font if not already loaded (lazy init)
    let font_id = canvas.add_font("/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf")
        .ok();
    if let Some(fid) = font_id {
        paint.set_font(&[fid]);
    }
    let _ = canvas.fill_text(x, y, text, &paint);
}

/// Draw an icon from RGBA pixels using FemtoVG image.
/// Returns the ImageId for caching.
pub fn draw_icon(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    pixels: &[u8],
    icon_width: u32,
    icon_height: u32,
    dest_x: f32,
    dest_y: f32,
    dest_width: f32,
    dest_height: f32,
    alpha: f32,
) -> Option<ImageId> {
    if pixels.is_empty() {
        return None;
    }

    canvas.save();
    canvas.set_global_alpha(alpha);

    // Convert raw [u8] RGBA pixels to Vec<RGBA8> for imgref
    let rgba_pixels: Vec<RGBA8> = pixels
        .chunks_exact(4)
        .map(|c| RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();
    let img = imgref::ImgRef::new(&rgba_pixels, icon_width as usize, icon_height as usize);
    let source = ImageSource::Rgba(img);
    
    if let Ok(image_id) = canvas.create_image(source, ImageFlags::empty()) {
        // Draw the image scaled to the destination rectangle
        let paint = Paint::image(image_id, dest_x + dest_width / 2.0, dest_y + dest_height / 2.0, dest_width, dest_height, 0.0, alpha);
        let mut rect_path = Path::new();
        rect_path.rect(dest_x, dest_y, dest_width, dest_height);
        canvas.fill_path(&rect_path, &paint);
        Some(image_id)
    } else {
        None
    }
}

/// Draw a rounded rectangle icon placeholder with gradient.
pub fn draw_icon_placeholder(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    x: f32,
    y: f32,
    size: f32,
    hue: f32,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    let radius = 10.0;
    let mut path = Path::new();
    path.rounded_rect(x, y, size, size, radius);

    // Create a simple color based on the hue
    let r = ((hue / 360.0 * 255.0) % 255.0) as u8;
    let g = ((hue / 360.0 * 180.0 + 50.0) % 255.0) as u8;
    let b = ((hue / 360.0 * 120.0 + 100.0) % 255.0) as u8;
    let paint = Paint::color(Color::rgba(r, g, b, 200));
    canvas.fill_path(&path, &paint);

    canvas.restore();
}

/// Draw a tracking indicator dash below an icon.
pub fn draw_tracking_indicator(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    is_active: bool,
) {
    let mut path = Path::new();
    path.rounded_rect(x, y, width, height, 1.0);

    let color = if is_active {
        // Active: red record-player accent
        Color::rgb(220, 38, 38)
    } else {
        // Inactive: quiet silver
        Color::rgb(162, 165, 146)
    };
    let paint = Paint::color(color);
    canvas.fill_path(&path, &paint);
}

/// Draw an icon with a colorize tint applied (for hover outline effect).
/// This creates a cyan-tinted version of the icon for the hover outline.
pub fn draw_icon_colorized(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    pixels: &[u8],
    icon_width: u32,
    icon_height: u32,
    dest_x: f32,
    dest_y: f32,
    dest_width: f32,
    dest_height: f32,
    colorize_color: Color,
) {
    if pixels.is_empty() {
        return;
    }

    canvas.save();

    // Convert raw [u8] RGBA pixels to Vec<RGBA8> for imgref
    let rgba_pixels: Vec<RGBA8> = pixels
        .chunks_exact(4)
        .map(|c| RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
        .collect();
    let img = imgref::ImgRef::new(&rgba_pixels, icon_width as usize, icon_height as usize);
    let source = ImageSource::Rgba(img);

    if let Ok(image_id) = canvas.create_image(source, ImageFlags::empty()) {
        // Draw the colorized icon: first the solid color, then the icon as alpha mask
        let mut rect_path = Path::new();
        rect_path.rect(dest_x, dest_y, dest_width, dest_height);
        let paint = Paint::color(colorize_color);
        canvas.fill_path(&rect_path, &paint);

        // Draw the icon on top
        let img_paint = Paint::image(
            image_id,
            dest_x + dest_width / 2.0,
            dest_y + dest_height / 2.0,
            dest_width,
            dest_height,
            0.0,
            1.0,
        );
        let mut img_path = Path::new();
        img_path.rect(dest_x, dest_y, dest_width, dest_height);
        canvas.fill_path(&img_path, &img_paint);
    }

    canvas.restore();
}

/// Draw multi-dash window count indicator below an icon.
/// Based on the reference project design: up to 5 dashes with varying widths.
/// Active/focused apps use cyan (#00E5FF), inactive use silver (#9CA3AF).
pub fn draw_multi_dash_indicator(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    icon_x: f32,
    indicator_y: f32,
    icon_size: f32,
    window_count: usize,
    is_active: bool,
) {
    // Total width of the indicator group (28px from reference)
    let total_width = 28.0f32;
    let dash_height = 2.0f32;
    let dash_spacing = 3.0f32;

    // Calculate dash widths based on count (matching reference project proportions)
    let total_spacing = dash_spacing * (window_count as f32 - 1.0);
    let available_width = total_width - total_spacing;
    let dash_width = (available_width / window_count as f32).max(4.0);

    let start_x = icon_x + (icon_size - total_width) / 2.0;

    let color = if is_active {
        // Cyan for active/focused apps (#00E5FF)
        Color::rgb(0, 229, 255)
    } else {
        // Silver for inactive running apps (#9CA3AF)
        Color::rgb(156, 163, 175)
    };

    for i in 0..window_count {
        let dash_x = start_x + i as f32 * (dash_width + dash_spacing);
        let mut path = Path::new();
        path.rounded_rect(dash_x, indicator_y, dash_width, dash_height, 1.0);
        let paint = Paint::color(color);
        canvas.fill_path(&path, &paint);
    }
}

/// Draw a loading spinner (animated dots) for icons still loading.
pub fn draw_loading_spinner(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    cx: f32,
    cy: f32,
    ring: f32,
) {
    let dots = [
        (0.0, -ring, 3.0, Color::rgba(21, 204, 250, 255)),
        (ring, 0.0, 2.0, Color::rgba(78, 173, 245, 255)),
        (0.0, ring, 2.0, Color::rgba(102, 115, 134, 255)),
        (-ring, 0.0, 2.0, Color::rgba(133, 103, 87, 255)),
        (ring / 2.0, -ring / 2.0, 2.0, Color::rgba(38, 38, 220, 255)),
        (-ring / 2.0, -ring / 2.0, 2.0, Color::rgba(68, 148, 220, 255)),
        (ring / 2.0, ring / 2.0, 2.0, Color::rgba(92, 92, 108, 255)),
        (-ring / 2.0, ring / 2.0, 2.0, Color::rgba(92, 92, 108, 255)),
    ];

    for (dx, dy, radius, color) in dots {
        let mut path = Path::new();
        path.circle(cx + dx, cy + dy, radius);
        let paint = Paint::color(color);
        canvas.fill_path(&path, &paint);
    }
}

/// Cached icon image IDs for FemtoVG rendering.
pub struct FemtoVGIconCache {
    pub images: HashMap<String, ImageId>,
}

impl FemtoVGIconCache {
    pub fn new() -> Self {
        Self {
            images: HashMap::new(),
        }
    }

    /// Get or create an image from RGBA pixels.
    pub fn get_or_create(
        &mut self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        key: &str,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) -> Option<ImageId> {
        if let Some(&id) = self.images.get(key) {
            return Some(id);
        }

        if pixels.is_empty() {
            return None;
        }

        // Convert raw [u8] RGBA pixels to Vec<RGBA8> for imgref
        let rgba_pixels: Vec<RGBA8> = pixels
            .chunks_exact(4)
            .map(|c| RGBA8 { r: c[0], g: c[1], b: c[2], a: c[3] })
            .collect();
        let img = imgref::ImgRef::new(&rgba_pixels, width as usize, height as usize);
        let source = ImageSource::Rgba(img);

        match canvas.create_image(source, ImageFlags::empty()) {
            Ok(id) => {
                self.images.insert(key.to_string(), id);
                Some(id)
            }
            Err(_) => None,
        }
    }

    /// Draw an icon from the cache.
    pub fn draw(
        &self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        key: &str,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
        alpha: f32,
    ) {
        if let Some(&image_id) = self.images.get(key) {
            canvas.save();
            canvas.set_global_alpha(alpha);

            let paint = Paint::image(
                image_id,
                dest_x + dest_width / 2.0,
                dest_y + dest_height / 2.0,
                dest_width,
                dest_height,
                0.0,
                alpha,
            );
            let mut rect_path = Path::new();
            rect_path.rect(dest_x, dest_y, dest_width, dest_height);
            canvas.fill_path(&rect_path, &paint);
            canvas.restore();
        }
    }

    /// Draw an icon from the cache with a colorize tint applied.
    /// Used for the cyan hover outline effect from the reference project.
    pub fn draw_colorized(
        &self,
        canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
        key: &str,
        dest_x: f32,
        dest_y: f32,
        dest_width: f32,
        dest_height: f32,
        colorize_color: Color,
    ) {
        if let Some(&image_id) = self.images.get(key) {
            canvas.save();

            // Draw the colorized icon using composite mode
            // First draw the solid color rectangle
            let mut rect_path = Path::new();
            rect_path.rect(dest_x, dest_y, dest_width, dest_height);
            let paint = Paint::color(colorize_color);
            canvas.fill_path(&rect_path, &paint);

            // Then draw the icon on top with multiply-like effect
            // by using the icon as an alpha mask
            let img_paint = Paint::image(
                image_id,
                dest_x + dest_width / 2.0,
                dest_y + dest_height / 2.0,
                dest_width,
                dest_height,
                0.0,
                1.0,
            );
            let mut img_path = Path::new();
            img_path.rect(dest_x, dest_y, dest_width, dest_height);
            canvas.fill_path(&img_path, &img_paint);

            canvas.restore();
        }
    }
}

// ---------------------------------------------------------------------------
// Popup rendering helpers (FemtoVG)
// ---------------------------------------------------------------------------

/// Draw a popup background (rounded rectangle with semi-transparent fill).
pub fn draw_popup_background(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    alpha: f32,
) {
    let radius = 8.0;
    let mut path = Path::new();
    path.rounded_rect(0.0, 0.0, width, height, radius);

    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background fill - dark with transparency
    let bg_paint = Paint::color(Color::rgba(30, 30, 46, 220));
    canvas.fill_path(&path, &bg_paint);

    // Border
    let border_paint = Paint::color(Color::rgba(82, 75, 48, 180));
    canvas.stroke_path(&path, &border_paint);

    canvas.restore();
}

/// Draw a popup menu item row.
pub fn draw_popup_menu_item(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    text: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32,
    is_hovered: bool,
    is_active: bool,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Hovered background
    if is_hovered || is_active {
        let mut path = Path::new();
        path.rect(x, y, width, height);
        let bg_color = if is_active {
            Color::rgba(59, 130, 246, 80)
        } else {
            Color::rgba(255, 255, 255, 30)
        };
        canvas.fill_path(&path, &Paint::color(bg_color));
    }

    // Text color — green (#00FF00) for high contrast against dark backgrounds
    // Active rows use green, inactive rows use a slightly dimmer green (#00CC00)
    let text_color: (u8, u8, u8) = if is_active {
        (0, 255, 0) // green for active/selected rows
    } else {
        (0, 204, 0) // dimmer green for inactive rows
    };
    let mut paint = Paint::color(Color::rgb(text_color.0, text_color.1, text_color.2));
    paint.set_font_size(font_size);
    let _ = canvas.fill_text(x + 12.0, y + height / 2.0 + font_size / 3.0, text, &paint);

    canvas.restore();
}

/// Draw a popup header bar.
pub fn draw_popup_header(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    text: &str,
    width: f32,
    height: f32,
    font_size: f32,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Header background
    let mut path = Path::new();
    path.rect(0.0, 0.0, width, height);
    let bg_paint = Paint::color(Color::rgba(31, 41, 55, 200));
    canvas.fill_path(&path, &bg_paint);

    // Separator line
    let mut line_path = Path::new();
    line_path.rect(0.0, height - 1.0, width, 1.0);
    let line_paint = Paint::color(Color::rgba(48, 54, 61, 200));
    canvas.fill_path(&line_path, &line_paint);

    // Title text — green (#00FF00) for high contrast against dark header background
    let mut text_paint = Paint::color(Color::rgb(0, 255, 0));
    text_paint.set_font_size(font_size);
    if let Ok(fid) = canvas.add_font("/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf") {
        text_paint.set_font(&[fid]);
    }
    let _ = canvas.fill_text(12.0, height / 2.0 + font_size / 3.0, text, &text_paint);

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Audio popup FemtoVG renderer
// ---------------------------------------------------------------------------

/// Draw a volume slider using FemtoVG.
pub fn draw_audio_slider(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    value: f32,
    muted: bool,
) {
    let radius = height / 2.0;
    let fill_width = if muted { 0.0 } else { width * value.clamp(0.0, 1.0) };

    // Track background
    let mut track_path = Path::new();
    track_path.rounded_rect(x, y, width, height, radius);
    let track_paint = Paint::color(Color::rgba(48, 54, 61, 255));
    canvas.fill_path(&track_path, &track_paint);

    // Fill
    if fill_width > 0.0 && !muted {
        let mut fill_path = Path::new();
        fill_path.rounded_rect(x, y, fill_width, height, radius);
        let fill_paint = Paint::color(Color::rgba(82, 196, 26, 255));
        canvas.fill_path(&fill_path, &fill_paint);
    }

    // Thumb
    if !muted {
        let thumb_x = x + fill_width - 4.0;
        let mut thumb_path = Path::new();
        thumb_path.circle(thumb_x, y + height / 2.0, 4.0);
        let thumb_paint = Paint::color(Color::rgb(255, 255, 255));
        canvas.fill_path(&thumb_path, &thumb_paint);
    }
}

/// Draw a mute/unmute icon using FemtoVG.
pub fn draw_mute_icon(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    x: f32,
    y: f32,
    size: f32,
    muted: bool,
) {
    // Mute icon color — red when muted, green when active
    let color = if muted {
        Color::rgb(239, 68, 68) // red for muted state
    } else {
        Color::rgb(0, 255, 0) // green for active/unmuted state
    };
    let paint = Paint::color(color);

    // Speaker shape (simplified)
    let mut path = Path::new();
    path.rounded_rect(x, y + size * 0.3, size * 0.4, size * 0.4, 1.0);
    canvas.fill_path(&path, &paint);

    // Speaker cone
    let mut cone_path = Path::new();
    cone_path.move_to(x + size * 0.4, y + size * 0.2);
    cone_path.line_to(x + size * 0.6, y + size * 0.1);
    cone_path.line_to(x + size * 0.6, y + size * 0.9);
    cone_path.line_to(x + size * 0.4, y + size * 0.8);
    canvas.fill_path(&cone_path, &paint);

    // Sound waves (if not muted)
    if !muted {
        let mut wave1 = Path::new();
        wave1.arc(x + size * 0.7, y + size * 0.5, size * 0.15, -0.5, 0.5, Solidity::Solid);
        canvas.stroke_path(&wave1, &paint);

        let mut wave2 = Path::new();
        wave2.arc(x + size * 0.8, y + size * 0.5, size * 0.25, -0.5, 0.5, Solidity::Solid);
        canvas.stroke_path(&wave2, &paint);
    } else {
        // Mute X
        let mut x_path = Path::new();
        x_path.move_to(x + size * 0.6, y + size * 0.15);
        x_path.line_to(x + size * 0.95, y + size * 0.85);
        canvas.stroke_path(&x_path, &paint);

        let mut x_path2 = Path::new();
        x_path2.move_to(x + size * 0.95, y + size * 0.15);
        x_path2.line_to(x + size * 0.6, y + size * 0.85);
        canvas.stroke_path(&x_path2, &paint);
    }
}

/// Draw the audio popup surface using FemtoVG.
pub fn render_audio_popup_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    output_volume: f32,
    output_muted: bool,
    output_device: &str,
    input_volume: f32,
    input_muted: bool,
    input_device: &str,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background
    draw_popup_background(canvas, width, height, alpha);

    // Header
    let header_h = 20.0 * scale;
    draw_popup_header(canvas, "Audio", width, header_h, 11.0 * scale, alpha);

    // Output section
    let section_y = header_h + 10.0 * scale;
    // Output section labels — green for consistency
    draw_text(canvas, "Output", 9.0 * scale, 8.0 * scale, section_y + 10.0 * scale, (0, 255, 0));
    draw_mute_icon(canvas, 8.0 * scale, section_y + 14.0 * scale, 16.0 * scale, output_muted);
    draw_audio_slider(
        canvas,
        28.0 * scale,
        section_y + 18.0 * scale,
        111.0 * scale,
        6.0 * scale,
        output_volume,
        output_muted,
    );
    let vol_text = format!("{}%", (output_volume * 100.0).round() as u32);
    // Output volume percentage — dimmer green for secondary info
    draw_text(
        canvas,
        &vol_text,
        9.0 * scale,
        145.0 * scale,
        section_y + 24.0 * scale,
        (0, 204, 0),
    );
    // Output device name — dimmer green for secondary info
    draw_text(
        canvas,
        output_device,
        9.0 * scale,
        8.0 * scale,
        section_y + 38.0 * scale,
        (0, 204, 0),
    );

    // Input section
    let input_y = section_y + 50.0 * scale;
    // Input section labels — green for consistency
    draw_text(canvas, "Input", 9.0 * scale, 8.0 * scale, input_y + 10.0 * scale, (0, 255, 0));
    draw_mute_icon(canvas, 8.0 * scale, input_y + 14.0 * scale, 16.0 * scale, input_muted);
    draw_audio_slider(
        canvas,
        28.0 * scale,
        input_y + 18.0 * scale,
        111.0 * scale,
        6.0 * scale,
        input_volume,
        input_muted,
    );
    let input_vol_text = format!("{}%", (input_volume * 100.0).round() as u32);
    // Input volume percentage — dimmer green for secondary info
    draw_text(
        canvas,
        &input_vol_text,
        9.0 * scale,
        145.0 * scale,
        input_y + 24.0 * scale,
        (0, 204, 0),
    );
    // Input device name — dimmer green for secondary info
    draw_text(
        canvas,
        input_device,
        9.0 * scale,
        8.0 * scale,
        input_y + 38.0 * scale,
        (0, 204, 0),
    );

    // Minimize button
    let btn_x = width - 28.0 * scale;
    let btn_y = 4.0 * scale;
    let mut btn_path = Path::new();
    btn_path.rounded_rect(btn_x, btn_y, 20.0 * scale, 16.0 * scale, 3.0 * scale);
    let btn_paint = Paint::color(Color::rgba(48, 54, 61, 200));
    canvas.fill_path(&btn_path, &btn_paint);
    // Audio popup close button text — green for visibility
    draw_text(canvas, "−", 12.0 * scale, btn_x + 4.0 * scale, btn_y + 12.0 * scale, (0, 255, 0));

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Bluetooth popup FemtoVG renderer
// ---------------------------------------------------------------------------

/// Draw a Bluetooth device row.
pub fn draw_bluetooth_device_row(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    name: &str,
    status: &str,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    font_size: f32,
    connected: bool,
    paired: bool,
) {
    canvas.save();

    // Device name text — green for connected, dimmer green for disconnected
    let name_color: (u8, u8, u8) = if connected { (0, 255, 0) } else { (0, 204, 0) };
    draw_text(canvas, name, font_size, x, y + height / 2.0 + font_size / 3.0, name_color);

    // Status text — green shades based on connection state
    let status_color: (u8, u8, u8) = if connected { (0, 255, 0) } else if paired { (0, 153, 0) } else { (0, 102, 0) };
    draw_text(canvas, status, font_size * 0.85, x, y + height - 2.0, status_color);

    let btn_x = x + width - 54.0;
    let btn_y = y + 6.0;
    let btn_w = 54.0;
    let btn_h = 24.0;
    let mut btn_path = Path::new();
    btn_path.rounded_rect(btn_x, btn_y, btn_w, btn_h, 4.0);
    let btn_color = if connected {
        Color::rgb(63, 185, 80)
    } else if paired {
        Color::rgb(59, 130, 246)
    } else {
        Color::rgb(48, 54, 61)
    };
    canvas.fill_path(&btn_path, &Paint::color(btn_color));

    let btn_text = if connected { "Disconnect" } else if paired { "Connect" } else { "Pair" };
    // Button text — black on green buttons, green on dark buttons
    let btn_text_color: (u8, u8, u8) = if connected || paired { (0, 0, 0) } else { (0, 255, 0) };
    draw_text(canvas, btn_text, font_size * 0.85, btn_x + 6.0, btn_y + btn_h / 2.0 + font_size / 3.0, btn_text_color);

    canvas.restore();
}

/// Draw the Bluetooth popup surface using FemtoVG.
pub fn render_bluetooth_popup_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    powered: bool,
    discovering: bool,
    adapter_name: &str,
    devices: &[(&str, &str, bool, bool)],
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    draw_popup_background(canvas, width, height, alpha);

    let header_h = 20.0 * scale;
    draw_popup_header(canvas, "Bluetooth", width, header_h, 11.0 * scale, alpha);

    // Bluetooth adapter name — dimmer green for secondary info
    draw_text(canvas, adapter_name, 9.0 * scale, 8.0 * scale, header_h + 14.0 * scale, (0, 204, 0));

    let pwr_x = 8.0 * scale;
    let pwr_y = header_h + 20.0 * scale;
    let pwr_w = 82.0 * scale;
    let pwr_h = 22.0 * scale;
    let mut pwr_path = Path::new();
    pwr_path.rounded_rect(pwr_x, pwr_y, pwr_w, pwr_h, 4.0 * scale);
    let pwr_color = if powered { Color::rgb(63, 185, 80) } else { Color::rgb(48, 54, 61) };
    canvas.fill_path(&pwr_path, &Paint::color(pwr_color));
    let pwr_text = if powered { "ON" } else { "OFF" };
    // Power button text — black on green (powered on), green on dark (powered off)
    let pwr_text_color: (u8, u8, u8) = if powered { (0, 0, 0) } else { (0, 255, 0) };
    draw_text(canvas, pwr_text, 10.0 * scale, pwr_x + 10.0 * scale, pwr_y + 16.0 * scale, pwr_text_color);

    let scn_x = pwr_x + pwr_w + 4.0 * scale;
    let scn_y = pwr_y;
    let scn_w = 116.0 * scale;
    let mut scn_path = Path::new();
    scn_path.rounded_rect(scn_x, scn_y, scn_w, pwr_h, 4.0 * scale);
    let scn_color = if discovering { Color::rgb(59, 130, 246) } else { Color::rgb(48, 54, 61) };
    canvas.fill_path(&scn_path, &Paint::color(scn_color));
    let scn_text = if discovering { "Scanning..." } else { "Scan" };
    // Scan button text — black on green (scanning), green on dark (idle)
    let scn_text_color: (u8, u8, u8) = if discovering { (0, 0, 0) } else { (0, 255, 0) };
    draw_text(canvas, scn_text, 10.0 * scale, scn_x + 10.0 * scale, scn_y + 16.0 * scale, scn_text_color);

    let list_y = header_h + 50.0 * scale;
    for (i, (name, status, connected, paired)) in devices.iter().enumerate().take(4) {
        let row_y = list_y + (i as f32) * 38.0 * scale;
        draw_bluetooth_device_row(
            canvas, name, status, 8.0 * scale, row_y, width - 16.0 * scale,
            38.0 * scale, 10.0 * scale, *connected, *paired,
        );
    }

    let btn_x = width - 28.0 * scale;
    let btn_y = 4.0 * scale;
    let mut btn_path = Path::new();
    btn_path.rounded_rect(btn_x, btn_y, 20.0 * scale, 16.0 * scale, 3.0 * scale);
    let btn_paint = Paint::color(Color::rgba(48, 54, 61, 200));
    canvas.fill_path(&btn_path, &btn_paint);
    // Bluetooth popup close button text — green for visibility
    draw_text(canvas, "−", 12.0 * scale, btn_x + 4.0 * scale, btn_y + 12.0 * scale, (0, 255, 0));

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Context menu popup FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_context_menu_popup_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    _height: f32,
    scale: f32,
    items: &[(&str, bool, bool, bool)],
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // No background — transparent

    // Context menu text — green (#00FF00) for all items including CloseApp/Quit
    // Font size 14, text_x 12, matching window list popup style
    for (i, (label, hovered, _active, is_close_app)) in items.iter().enumerate() {
        let row_y = (i as f32) * 29.0 * scale;
        let row_h = 29.0 * scale;

        // Red highlight for CloseApp (Quit) row
        if *is_close_app {
            let mut path = Path::new();
            path.rect(0.0, row_y, width, row_h);
            canvas.fill_path(&path, &Paint::color(Color::rgba(220, 50, 50, 60)));
        }

        // Blue border on hovered row
        if *hovered {
            let mut border_path = Path::new();
            border_path.rect(0.0, row_y, width, 1.0);
            canvas.fill_path(&border_path, &Paint::color(Color::rgba(100, 149, 230, 255)));
            border_path = Path::new();
            border_path.rect(0.0, row_y + row_h - 1.0, width, 1.0);
            canvas.fill_path(&border_path, &Paint::color(Color::rgba(100, 149, 230, 255)));
            border_path = Path::new();
            border_path.rect(0.0, row_y, 1.0, row_h);
            canvas.fill_path(&border_path, &Paint::color(Color::rgba(100, 149, 230, 255)));
            border_path = Path::new();
            border_path.rect(width - 1.0, row_y, 1.0, row_h);
            canvas.fill_path(&border_path, &Paint::color(Color::rgba(100, 149, 230, 255)));
        }

        // All context menu labels use green — CloseApp/Quit no longer uses red
        // Green (#00FF00) matches popup_text_color default for consistency
        let label_color: (u8, u8, u8) = if *is_close_app {
            (0, 255, 0) // green for CloseApp/Quit
        } else {
            (0, 255, 0) // green for all other items
        };
        draw_text(canvas, label, 14.0 * scale, 12.0 * scale, row_y + row_h / 2.0 + 4.0 * scale, label_color);
    }

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Process popup FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_process_popup_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    processes: &[(&str, f32, f32)],
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    draw_popup_background(canvas, width, height, alpha);

    let header_h = 20.0 * scale;
    draw_popup_header(canvas, "Processes", width, header_h, 11.0 * scale, alpha);

    let col_y = header_h + 6.0 * scale;
    // Column header text — green for consistency
    draw_text(canvas, "Name", 9.0 * scale, 8.0 * scale, col_y + 10.0 * scale, (0, 255, 0));
    draw_text(canvas, "CPU", 9.0 * scale, width * 0.6, col_y + 10.0 * scale, (0, 255, 0));
    draw_text(canvas, "Mem", 9.0 * scale, width * 0.8, col_y + 10.0 * scale, (0, 255, 0));

    let list_y = col_y + 16.0 * scale;
    for (i, (name, cpu, mem)) in processes.iter().enumerate().take(10) {
        let row_y = list_y + (i as f32) * 22.0 * scale;
        // Process name in bright green, CPU/Mem values in dimmer green
        draw_text(canvas, name, 9.0 * scale, 8.0 * scale, row_y + 14.0 * scale, (0, 255, 0));
        let cpu_text = format!("{:.1}%", cpu);
        draw_text(canvas, &cpu_text, 9.0 * scale, width * 0.6, row_y + 14.0 * scale, (0, 204, 0));
        let mem_text = format!("{:.1}%", mem);
        draw_text(canvas, &mem_text, 9.0 * scale, width * 0.8, row_y + 14.0 * scale, (0, 204, 0));
    }

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Rclone popup FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_rclone_popup_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    transfers: &[(&str, f32, String)],
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    draw_popup_background(canvas, width, height, alpha);

    let header_h = 20.0 * scale;
    draw_popup_header(canvas, "Rclone", width, header_h, 11.0 * scale, alpha);

    let list_y = header_h + 6.0 * scale;
    for (i, (name, progress, speed)) in transfers.iter().enumerate().take(8) {
        let row_y = list_y + (i as f32) * 36.0 * scale;
        // Rclone transfer name — bright green for primary info
        draw_text(canvas, name, 9.0 * scale, 8.0 * scale, row_y + 12.0 * scale, (0, 255, 0));

        let bar_x = 8.0 * scale;
        let bar_y = row_y + 18.0 * scale;
        let bar_w = width - 16.0 * scale;
        let bar_h = 6.0 * scale;
        let fill_w = bar_w * progress / 100.0;

        let mut track_path = Path::new();
        track_path.rounded_rect(bar_x, bar_y, bar_w, bar_h, 3.0 * scale);
        canvas.fill_path(&track_path, &Paint::color(Color::rgba(48, 54, 61, 255)));

        if fill_w > 0.0 {
            let mut fill_path = Path::new();
            fill_path.rounded_rect(bar_x, bar_y, fill_w, bar_h, 3.0 * scale);
            canvas.fill_path(&fill_path, &Paint::color(Color::rgba(59, 130, 246, 255)));
        }

        // Rclone transfer speed — dimmer green for secondary info
        draw_text(canvas, speed, 8.0 * scale, width - 60.0 * scale, row_y + 14.0 * scale, (0, 204, 0));
    }

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Widget bar FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_widget_bar_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    bluetooth_powered: bool,
    bluetooth_discovering: bool,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    let mut bg_path = Path::new();
    bg_path.rounded_rect(0.0, 0.0, width, height, 8.0 * scale);
    canvas.fill_path(&bg_path, &Paint::color(Color::rgba(30, 30, 46, 220)));

    let bt_x = 12.0 * scale;
    let bt_y = height / 2.0 - 10.0 * scale;
    let bt_color = if bluetooth_powered {
        Color::rgb(59, 130, 246)
    } else {
        // Bluetooth icon color — dimmer green when inactive
        Color::rgb(0, 204, 0)
    };
    let mut bt_path = Path::new();
    bt_path.rounded_rect(bt_x, bt_y, 20.0 * scale, 20.0 * scale, 4.0 * scale);
    canvas.fill_path(&bt_path, &Paint::color(bt_color));

    if bluetooth_discovering {
        let mut dot_path = Path::new();
        dot_path.circle(bt_x + 20.0 * scale, bt_y, 3.0 * scale);
        canvas.fill_path(&dot_path, &Paint::color(Color::rgb(63, 185, 80)));
    }

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Tray bar FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_tray_bar_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    icon_cache: &mut FemtoVGIconCache,
    items: &[crate::models::TrayItem],
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background
    let mut bg_path = Path::new();
    bg_path.rounded_rect(0.0, 0.0, width, height, 8.0 * scale);
    canvas.fill_path(&bg_path, &Paint::color(Color::rgba(27, 26, 32, 240)));

    // Draw tray icons
    let icon_size = 24.0 * scale;
    let spacing = 8.0 * scale;
    let _padding_y = 8.0 * scale;
    let rows = (items.len() + 1) / 2;
    let content_height = rows as f32 * icon_size + (rows.saturating_sub(1) as f32) * spacing;
    let top = (height - content_height) / 2.0;

    for (i, item) in items.iter().enumerate() {
        let row = i / 2;
        let col = (i % 2) as f32;
        let items_in_row = (items.len() - row * 2).min(2) as f32;
        let row_width = items_in_row * icon_size + (items_in_row - 1.0).max(0.0) * spacing;
        let x = (width - row_width) / 2.0 + col * (icon_size + spacing);
        let y = top + row as f32 * (icon_size + spacing);

        icon_cache.draw(canvas, &item.icon_name, x, y, icon_size, icon_size, alpha);
        // Placeholder
        let mut ph_path = Path::new();
        ph_path.rounded_rect(x, y, icon_size, icon_size, 4.0 * scale);
        canvas.fill_path(&ph_path, &Paint::color(Color::rgba(48, 54, 61, (255.0 * alpha) as u8)));
    }

    canvas.restore();
}

// ---------------------------------------------------------------------------
// System status bar FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_system_bar_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    snapshot: &crate::models::SystemBarSnapshot,
    alpha: f32,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background
    let mut bg_path = Path::new();
    bg_path.rounded_rect(0.0, 0.0, width, height, 8.0 * scale);
    canvas.fill_path(&bg_path, &Paint::color(Color::rgba(27, 26, 32, 240)));

    let text_y = height / 2.0 + 4.0 * scale;
    let text_size = 10.0 * scale;
    let mut x = 12.0 * scale;

    // Keyboard layout
    if !snapshot.keyboard_layout.is_empty() {
        // Keyboard layout text — dimmer green for secondary info
        draw_text(canvas, &snapshot.keyboard_layout, text_size, x, text_y, (0, 204, 0));
        x += snapshot.keyboard_layout.len() as f32 * text_size * 0.6 + 16.0 * scale;
    }

    // Network
    if !snapshot.download_rate.is_empty() || !snapshot.upload_rate.is_empty() {
        let net_text = format!("↑{} ↓{}", snapshot.upload_rate, snapshot.download_rate);
        // Network speed text — dimmer green for secondary info
        draw_text(canvas, &net_text, text_size, x, text_y, (0, 204, 0));
        let _net_width = net_text.len() as f32 * text_size * 0.6 + 16.0 * scale;
    }

    // Clock (right-aligned)
    let clock_text = &snapshot.time;
    let clock_w = clock_text.len() as f32 * text_size * 0.6;
    // System tray clock text — bright green for primary info
    draw_text(canvas, clock_text, text_size, width - clock_w - 12.0 * scale, text_y, (0, 255, 0));

    canvas.restore();
}

// ---------------------------------------------------------------------------
// Audio player bar FemtoVG renderer
// ---------------------------------------------------------------------------

pub fn render_audio_player_bar_femtovg(
    canvas: &mut Canvas<femtovg::renderer::WGPURenderer>,
    width: f32,
    height: f32,
    scale: f32,
    snapshot: &crate::models::AudioSnapshot,
    alpha: f32,
    _expanded: bool,
) {
    canvas.save();
    canvas.set_global_alpha(alpha);

    // Background
    let mut bg_path = Path::new();
    bg_path.rounded_rect(0.0, 0.0, width, height, 8.0 * scale);
    canvas.fill_path(&bg_path, &Paint::color(Color::rgba(27, 26, 32, 240)));

    let text_size = 10.0 * scale;
    let text_y = height / 2.0 + 4.0 * scale;

    // Track title
    if let Some(ref title) = snapshot.media_title {
        if !title.is_empty() {
            // MPRIS media title — bright green for primary info
            draw_text(canvas, title, text_size, 12.0 * scale, text_y, (0, 255, 0));
        }
    }

    // Artist
    if let Some(ref artist) = snapshot.media_artist {
        if !artist.is_empty() {
            let artist_y = text_y + text_size + 2.0 * scale;
            // MPRIS artist name — dimmer green for secondary info
            draw_text(canvas, artist, text_size * 0.9, 12.0 * scale, artist_y, (0, 204, 0));
        }
    }

    // Progress bar
    let progress_y = height - 20.0 * scale;
    let progress_w = width - 24.0 * scale;
    let progress_x = 12.0 * scale;
    let mut progress_bg = Path::new();
    progress_bg.rounded_rect(progress_x, progress_y, progress_w, 4.0 * scale, 2.0 * scale);
    canvas.fill_path(&progress_bg, &Paint::color(Color::rgba(48, 54, 61, 255)));

    if let (Some(elapsed), Some(duration)) = (snapshot.media_position_seconds, snapshot.media_duration_seconds) {
        if duration > 0.0 {
            let progress = (elapsed / duration).min(1.0);
            let fill_w = progress_w * progress as f32;
            let mut progress_fill = Path::new();
            progress_fill.rounded_rect(progress_x, progress_y, fill_w, 4.0 * scale, 2.0 * scale);
            canvas.fill_path(&progress_fill, &Paint::color(Color::rgb(82, 75, 48)));
        }
    }

    canvas.restore();
}
