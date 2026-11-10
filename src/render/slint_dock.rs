//! Slint-based dock renderer.
//!
//! Software renderer that converts dock state to Slint-compatible
//! image buffers for the main dock row.

use std::rc::Rc;
use std::time::Instant;

use slint::platform::software_renderer::{MinimalSoftwareWindow, PremultipliedRgbaColor};
use slint::platform::{Platform, PlatformError};
use slint::{Image, ModelRc, PhysicalSize, SharedPixelBuffer, SharedString};

use crate::config::DockConfig;
use crate::types::DockState;

// Include both Slint-generated modules manually.
// `include_modules!()` only picks up the last compiled file, so we include both.
include!(env!("SLINT_INCLUDE_GENERATED"));
include!(env!("SLINT_INCLUDE_BLUETOOTH"));

/// A custom Slint platform that uses MinimalSoftwareWindow for headless rendering.
struct SlintPlatform {
    window: Rc<MinimalSoftwareWindow>,
    start_instant: Instant,
}

impl SlintPlatform {
    fn new(window: Rc<MinimalSoftwareWindow>) -> Self {
        Self {
            window,
            start_instant: Instant::now(),
        }
    }
}

impl Platform for SlintPlatform {
    fn create_window_adapter(
        &self,
    ) -> Result<Rc<dyn slint::platform::WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        self.start_instant.elapsed()
    }
}

/// Dock renderer using Slint's software renderer for direct buffer rendering.
pub struct SlintDockRenderer {
    widget: DockWidget,
    window: Rc<MinimalSoftwareWindow>,
    /// Cached Slint images keyed by app_id, converted from pre-loaded RGBA pixels
    pub icon_cache: std::collections::HashMap<String, Image>,
    /// Track which apps are currently bouncing (app_id -> remaining frames)
    bounce_animations: std::collections::HashMap<String, u32>,
    /// Track which apps are currently playing click animation (app_id -> remaining frames)
    click_animations: std::collections::HashMap<String, u32>,
    /// Track which apps are currently playing minimize animation (app_id -> remaining frames)
    minimize_animations: std::collections::HashMap<String, u32>,
    /// Reusable render buffer to avoid per-frame allocation
    render_buffer: Vec<PremultipliedRgbaColor>,
    /// Last rendered state hash to skip rendering when nothing changed
    last_state_hash: u64,
    /// Last time the frame debug stats were printed (for throttling)
    last_debug_instant: std::time::Instant,
}

impl SlintDockRenderer {
    /// Initialize the Slint renderer with a custom platform.
    pub fn new() -> Result<Self, String> {
        // Create the minimal software window (headless, no display server needed)
        let window = MinimalSoftwareWindow::new(Default::default());

        // Set up the custom platform before creating any Slint components
        let platform = SlintPlatform::new(window.clone());
        slint::platform::set_platform(Box::new(platform))
            .map_err(|e| format!("Failed to set Slint platform: {:?}", e))?;

        // Create the dock widget
        let widget = DockWidget::new()
            .map_err(|e| format!("Failed to create DockWidget: {:?}", e))?;

        Ok(Self { widget, window, icon_cache: std::collections::HashMap::new(), bounce_animations: std::collections::HashMap::new(), click_animations: std::collections::HashMap::new(), minimize_animations: std::collections::HashMap::new(), render_buffer: Vec::new(), last_state_hash: 0, last_debug_instant: std::time::Instant::now() })
    }

    /// Simple hash for dock state to detect changes
    /// Includes all fields that affect visual appearance: pin app_ids, running status,
    /// focus state, window counts, badges, hover, drag state, and drag target slot.
    /// Excludes animation counters (bounce_animations, click_animations) since those
    /// are handled separately via has_active_animation check.
    /// During drag, hashes the target slot index instead of raw cursor X so Slint only
    /// re-renders when the cursor crosses into a new icon slot, not every pixel.
    fn compute_state_hash(
        dock_state: &DockState,
        hovered_index: Option<usize>,
        is_dragging: bool,
        dragged_app_id: Option<&String>,
        drag_target_slot: i32,
    ) -> u64 {
        let mut hash: u64 = 0;
        for pin in &dock_state.pins {
            hash = hash.wrapping_add(pin.app_id.len() as u64).wrapping_mul(0x100000001B3);
            hash = hash.wrapping_add((pin.is_running as u64) << 1).wrapping_mul(0x100000001B3);
            hash = hash.wrapping_add((pin.is_activated as u64) << 2).wrapping_mul(0x100000001B3);
            hash = hash.wrapping_add(pin.running_count as u64).wrapping_mul(0x100000001B3);
            hash = hash.wrapping_add(pin.badge_count.unwrap_or(0) as u64).wrapping_mul(0x100000001B3);
        }
        hash = hash.wrapping_add(hovered_index.unwrap_or(usize::MAX) as u64).wrapping_mul(0x100000001B3);
        // Include drag state so the model updates during drag
        hash = hash.wrapping_add((is_dragging as u64) << 1).wrapping_mul(0x100000001B3);
        if let Some(id) = dragged_app_id {
            hash = hash.wrapping_add(id.len() as u64).wrapping_mul(0x100000001B3);
        }
        // Include drag target slot during drag so offset_x updates when cursor crosses a slot boundary
        if is_dragging {
            hash = hash.wrapping_add(drag_target_slot as u64).wrapping_mul(0x100000001B3);
        }
        hash
    }

    /// Render the dock into the SHM canvas buffer.
    ///
    /// The canvas is in ARGB8888 format (4 bytes per pixel, stride = width * 4).
    /// Slint's software renderer outputs PremultipliedRgbaColor which is RGBA.
    /// We convert RGBA -> BGRA for the Wayland SHM buffer (little-endian ARGB8888).
    ///
    /// The Slint widget size is set to match the Wayland surface dimensions exactly.
    pub fn render(
        &mut self,
        canvas: &mut [u8],
        dock_state: &DockState,
        phys_width: u32,
        phys_height: u32,
        icon_cache: &std::collections::HashMap<String, (Vec<u8>, u32)>,
        hovered_index: Option<usize>,
        is_dragging: bool,
        dragged_app_id: Option<&String>,
        pointer_x: i32,
        pointer_y: i32,
        config: &DockConfig,
    ) {
        let start = Instant::now();

        // Clear canvas to transparent black first — SHM pool reuses buffers
        canvas.fill(0);
        
        // The Slint widget matches the Wayland surface dimensions exactly
        let slint_width = phys_width as usize;
        let slint_height = phys_height as usize;
        let total_pixels = slint_width * slint_height;

        // Resize render buffer if needed (avoids per-frame allocation)
        if self.render_buffer.len() < total_pixels {
            self.render_buffer.resize(total_pixels, PremultipliedRgbaColor { red: 0, green: 0, blue: 0, alpha: 0 });
        }
        let slint_buffer = &mut self.render_buffer[..total_pixels];

        // Set the Slint widget size to match the surface
        self.widget.set_dock_width(slint_width as i32);
        self.widget.set_dock_height(slint_height as i32);
        
        // Set configurable properties from config
        // Parse hex color strings to slint::Brush::SolidColor
        fn hex_to_brush(hex: &str) -> slint::Brush {
            let hex = hex.trim_start_matches('#');
            if hex.len() == 6 {
                if let (Ok(r), Ok(g), Ok(b)) = (
                    u8::from_str_radix(&hex[0..2], 16),
                    u8::from_str_radix(&hex[2..4], 16),
                    u8::from_str_radix(&hex[4..6], 16),
                ) {
                    return slint::Brush::SolidColor(slint::Color::from_rgb_u8(r, g, b));
                }
            }
            slint::Brush::SolidColor(slint::Color::from_rgb_u8(0, 229, 255)) // fallback to cyan
        }
        self.widget.set_hover_color(hex_to_brush(&config.hover_color));
        self.widget.set_dash_focused_color(hex_to_brush(&config.dash_focused_color));
        self.widget.set_dash_inactive_color(hex_to_brush(&config.dash_inactive_color));
        self.widget.set_hover_highlight(config.hover_highlight);
        self.widget.set_click_animation(config.click_animation);
        self.widget.set_bounce_animation(config.bounce_animation);
        // Background color with alpha embedded directly in the color
        let bg_hex = config.background_color.trim_start_matches('#');
        if bg_hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&bg_hex[0..2], 16),
                u8::from_str_radix(&bg_hex[2..4], 16),
                u8::from_str_radix(&bg_hex[4..6], 16),
            ) {
                let a = (config.background_alpha * 255.0) as u8;
                self.widget.set_background_color(
                    slint::Brush::SolidColor(slint::Color::from_argb_u8(a, r, g, b))
                );
            }
        }
        // Border color with alpha embedded directly in the color
        let br_hex = config.border_color.trim_start_matches('#');
        if br_hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&br_hex[0..2], 16),
                u8::from_str_radix(&br_hex[2..4], 16),
                u8::from_str_radix(&br_hex[4..6], 16),
            ) {
                let a = (config.border_alpha * 255.0) as u8;
                self.widget.set_border_color(
                    slint::Brush::SolidColor(slint::Color::from_argb_u8(a, r, g, b))
                );
            }
        }
    
        // Set the Slint window size
        self.window.set_size(PhysicalSize::new(slint_width as u32, slint_height as u32));

        // Calculate drag target slot index for sliding animation
        let item_slot_width = 56f32; // icon_size (48) + spacing (8)
        let padding = 12f32; // 12px padding on left side of items container
        let (drag_source_idx, drag_target_slot) = if is_dragging {
            if let Some(app_id) = dragged_app_id {
                let source_idx = dock_state.pins.iter().position(|p| p.app_id == *app_id);
                if let Some(si) = source_idx {
                    // Calculate which slot the cursor is over
                    let cursor_x = pointer_x as f32;
                    let slot_x = cursor_x - padding;
                    let target_slot = (slot_x / item_slot_width).round() as i32;
                    let clamped = target_slot.max(0).min(dock_state.pins.len() as i32);
                    (si as i32, clamped)
                } else {
                    (-1, -1)
                }
            } else {
                (-1, -1)
            }
        } else {
            (-1, -1)
        };

        // Build dock items from state, using pre-loaded RGBA icon pixels
        let mut icon_cache_hits = 0u32;
        let mut icon_cache_misses = 0u32;
        // Track which app_ids have icons in the cache so the state hash detects
        // when a newly loaded icon should update the Slint model.
        let mut icon_cache_keys: Vec<&str> = icon_cache.keys().map(|s| s.as_str()).collect();
        icon_cache_keys.sort();
        let items: Vec<DockItem> = dock_state
            .pins
            .iter()
            .enumerate()
            .map(|(i, pin)| {
                // Check Slint image cache first
                let icon = if let Some(cached) = self.icon_cache.get(&pin.app_id) {
                    icon_cache_hits += 1;
                    cached.clone()
                } else if let Some((rgba, size)) = icon_cache.get(&pin.app_id) {
                    // Convert pre-loaded RGBA pixels to Slint Image
                    // The existing icon loader provides 512x512 RGBA pixels
                    let s = *size as usize;
                    if rgba.len() == s * s * 4 {
                        let buf = SharedPixelBuffer::clone_from_slice(rgba.as_slice(), s as u32, s as u32);
                        let img = Image::from_rgba8(buf);
                        self.icon_cache.insert(pin.app_id.clone(), img.clone());
                        icon_cache_misses += 1;
                        img
                    } else {
                        icon_cache_misses += 1;
                        Image::default()
                    }
                } else {
                    // Icon not yet loaded by the async icon loader — use placeholder
                    icon_cache_misses += 1;
                    Image::default()
                };

                // Bounce animation: smooth sine wave over configurable duration
                let bounce_counter = self.bounce_animations.get(&pin.app_id).copied().unwrap_or(0);
                // Decrement bounce counter
                if let Some(count) = self.bounce_animations.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let bounce_scale = if bounce_counter > 0 && config.bounce_animation {
                    let total_bounce_frames = (config.bounce_animation_duration * 60.0) as u32;
                    if total_bounce_frames == 0 {
                        1.0f32
                    } else {
                        let elapsed = total_bounce_frames - bounce_counter;
                        let progress = elapsed as f32 / total_bounce_frames as f32;
                        let angle = progress * std::f32::consts::PI;
                        1.0 + angle.sin() * (config.bounce_animation_scale - 1.0)
                    }
                } else {
                    1.0f32
                };
                // Click animation: smooth sine wave over configurable duration
                let click_counter = self.click_animations.get(&pin.app_id).copied().unwrap_or(0);
                // Decrement click animation counter
                if let Some(count) = self.click_animations.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let click_scale = if click_counter > 0 {
                    let total_frames = ((config.click_animation_up_duration + config.click_animation_down_duration) * 60.0) as u32;
                    if total_frames == 0 {
                        1.0
                    } else {
                        let elapsed = total_frames - click_counter;
                        let progress = elapsed as f32 / total_frames as f32;
                        let angle = progress * std::f32::consts::PI;
                        1.0 + angle.sin() * (config.click_animation_scale - 1.0)
                    }
                } else {
                    1.0f32
                };
                // Minimize animation: shrink down then restore (inverse sine wave)
                let minimize_counter = self.minimize_animations.get(&pin.app_id).copied().unwrap_or(0);
                // Decrement minimize animation counter
                if let Some(count) = self.minimize_animations.get_mut(&pin.app_id) {
                    *count = count.saturating_sub(1);
                }
                let minimize_scale = if minimize_counter > 0 {
                    let total_frames = (config.minimize_animation_duration * 60.0) as u32;
                    if total_frames == 0 {
                        1.0
                    } else {
                        let elapsed = total_frames - minimize_counter;
                        let progress = elapsed as f32 / total_frames as f32;
                        // Shrink down (1.0 -> min_scale) then back up (min_scale -> 1.0)
                        let angle = progress * std::f32::consts::PI;
                        1.0 - angle.sin() * (1.0 - config.minimize_animation_scale)
                    }
                } else {
                    1.0f32
                };
                // Combined animation scale: minimize > click > bounce priority
                let anim_scale = if minimize_counter > 0 { minimize_scale } else if click_counter > 0 { click_scale } else { bounce_scale };
                // Compute scaled icon dimensions and centered position
                let base_icon_size = 48f32;
                let base_icon_y = 6f32;
                let scaled_w = base_icon_size * anim_scale;
                let scaled_h = base_icon_size * anim_scale;
                let dx = (scaled_w - base_icon_size) / 2.0;
                let dy = (scaled_h - base_icon_size) / 2.0;
                let icon_x = -dx;
                let icon_y = base_icon_y - dy;
                // Hover state from Rust (MinimalSoftwareWindow has no pointer input)
                let is_hovered = hovered_index == Some(i);
                // Hide this icon in the dock only when actually dragging (not just on press)
                // `dragged_app_id` is set on press, but `is_dragging` is only true after moving > 5px
                let is_dragging_item = is_dragging && dragged_app_id.is_some_and(|id| *id == pin.app_id);

                // Calculate horizontal offset for sliding animation
                let offset_x = if is_dragging && drag_source_idx >= 0 {
                    let idx = i as i32;
                    if is_dragging_item {
                        0.0 // Dragged icon is hidden, offset doesn't matter
                    } else if drag_target_slot > drag_source_idx {
                        // Target is to the right of source
                        if idx > drag_source_idx && idx <= drag_target_slot {
                            -item_slot_width // Shift left to fill gap
                        } else if idx >= drag_source_idx && idx < drag_target_slot {
                            -item_slot_width
                        } else {
                            0.0
                        }
                    } else if drag_target_slot < drag_source_idx {
                        // Target is to the left of source
                        if idx >= drag_target_slot && idx < drag_source_idx {
                            item_slot_width // Shift right to make space
                        } else {
                            0.0
                        }
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };

                DockItem {
                    name: SharedString::from(&pin.app_id),
                    icon,
                    running: pin.is_running,
                    loading: false,
                    badge_count: pin.badge_count.unwrap_or(0) as i32,
                    badge_color: "#dc2626".into(),
                    window_count: pin.running_count as i32,
                    app_id: i as i32,
                    index_on_dock: i as i32,
                    focused: pin.is_activated,
                    open_animation: bounce_counter > 0,
                    pinned: true,
                    minimized: false,
                    is_hovered,
                    is_dragging: is_dragging_item,
                    offset_x,
                    anim_scale,
                    icon_width: scaled_w,
                    icon_height: scaled_h,
                    icon_x,
                    icon_y,
                }
            })
            .collect();

        let _data_time = start.elapsed();

        // Compute state hash to detect if anything changed
        // Use drag_target_slot instead of raw pointer_x so Slint only re-renders when
        // the cursor crosses into a new icon slot, not every pixel movement
        let mut state_hash = Self::compute_state_hash(dock_state, hovered_index, is_dragging, dragged_app_id, drag_target_slot);
        // Include icon cache keys so newly loaded icons trigger a model update
        for key in &icon_cache_keys {
            state_hash = state_hash.wrapping_add(key.len() as u64).wrapping_mul(0x100000001B3);
            for byte in key.as_bytes() {
                state_hash = state_hash.wrapping_add(*byte as u64).wrapping_mul(0x100000001B3);
            }
        }
        let state_changed = state_hash != self.last_state_hash;

        // Check if animations are active
        let has_active_animation = self.bounce_animations.values().any(|c| *c > 0)
            || self.click_animations.values().any(|c| *c > 0)
            || self.minimize_animations.values().any(|c| *c > 0);

        // Update the model if state changed or animations are active
        // Using ModelRc::from(slice) to ensure Slint's property bindings update
        if state_changed || has_active_animation {
            self.widget.set_dock_items(ModelRc::from(items.as_slice()));
            if state_changed {
                self.last_state_hash = state_hash;
            }
        }

        // Clear the Slint render buffer to transparent before rendering
        // This ensures that areas outside the dock_bg are transparent
        for pixel in slint_buffer.iter_mut() {
            *pixel = PremultipliedRgbaColor { red: 0, green: 0, blue: 0, alpha: 0 };
        }

        // Render via Slint
        self.window.request_redraw();
        slint::platform::update_timers_and_animations();

        let render_start = Instant::now();
        self.window.draw_if_needed(|renderer| {
            renderer.render(slint_buffer, slint_width);
        });
        let _render_time = render_start.elapsed();

        // Convert Slint's RGBA buffer to BGRA for Wayland SHM
        let copy_width = slint_width.min(phys_width as usize);
        let copy_height = slint_height.min(phys_height as usize);
        let copy_pixels = copy_width * copy_height;

        for idx in 0..copy_pixels {
            if idx < slint_buffer.len() && idx * 4 + 3 < canvas.len() {
                let pixel = &slint_buffer[idx];
                let offset = idx * 4;
                canvas[offset] = pixel.blue;
                canvas[offset + 1] = pixel.green;
                canvas[offset + 2] = pixel.red;
                canvas[offset + 3] = pixel.alpha;
            }
        }

        let _total_time = start.elapsed();
        // Print frame debug stats every 3 seconds instead of every frame
        if self.last_debug_instant.elapsed() >= std::time::Duration::from_secs(3) {
            self.last_debug_instant = std::time::Instant::now();
        }

        // Render the dragged icon ghost at the cursor position
        if is_dragging {
            if let Some(app_id) = dragged_app_id {
                if let Some(icon) = self.icon_cache.get(app_id) {
                    // Find the dragged item's properties
                    let (wc, foc) = dock_state.pins.iter().find(|p| p.app_id == *app_id)
                        .map(|p| (p.running_count, p.is_activated))
                        .unwrap_or((0, false));
                    self.draw_dragged_icon(canvas, icon, phys_width as usize, phys_height as usize, pointer_x, pointer_y, wc, foc);
                }
            }
        }
    }

    /// Trigger a bounce animation for the given app_id.
    /// The animation duration is based on config values.
    /// Skipped if another animation (click or bounce) is already active on this icon.
    pub fn start_bounce(&mut self, app_id: &str, config: &DockConfig) {
        if !config.bounce_animation {
            return;
        }
        // Mutual exclusion: don't start if click animation is already running
        let click_active = self.click_animations.get(app_id).copied().unwrap_or(0) > 0;
        let bounce_active = self.bounce_animations.get(app_id).copied().unwrap_or(0) > 0;
        if click_active || bounce_active {
            return;
        }
        let total_frames = (config.bounce_animation_duration * 60.0) as u32;
        self.bounce_animations.insert(app_id.to_string(), total_frames.max(1));
    }

    /// Trigger a click animation for the given app_id.
    /// The animation duration is based on config values (up_duration + down_duration, in seconds).
    /// Skipped if another animation (bounce, click, or minimize) is already active on this icon.
    pub fn start_click_animation(&mut self, app_id: &str, config: &DockConfig) {
        // Mutual exclusion: don't start if another animation is already running
        let click_active = self.click_animations.get(app_id).copied().unwrap_or(0) > 0;
        let bounce_active = self.bounce_animations.get(app_id).copied().unwrap_or(0) > 0;
        let minimize_active = self.minimize_animations.get(app_id).copied().unwrap_or(0) > 0;
        if click_active || bounce_active || minimize_active {
            return;
        }
        let total_frames = ((config.click_animation_up_duration + config.click_animation_down_duration) * 60.0) as u32;
        self.click_animations.insert(app_id.to_string(), total_frames.max(1));
    }

    /// Trigger a minimize animation for the given app_id.
    /// The icon shrinks down then restores to normal size.
    /// Skipped if another animation is already active on this icon.
    pub fn start_minimize_animation(&mut self, app_id: &str, config: &DockConfig) {
        // Mutual exclusion: don't start if another animation is already running
        let click_active = self.click_animations.get(app_id).copied().unwrap_or(0) > 0;
        let bounce_active = self.bounce_animations.get(app_id).copied().unwrap_or(0) > 0;
        let minimize_active = self.minimize_animations.get(app_id).copied().unwrap_or(0) > 0;
        if click_active || bounce_active || minimize_active {
            return;
        }
        let total_frames = (config.minimize_animation_duration * 60.0) as u32;
        self.minimize_animations.insert(app_id.to_string(), total_frames.max(1));
    }

    /// Check if any animations are currently active (bounce, click, or minimize).
    /// Returns true if the render loop should continue redrawing at 60fps.
    pub fn has_active_animation(&self) -> bool {
        self.bounce_animations.values().any(|c| *c > 0)
            || self.click_animations.values().any(|c| *c > 0)
            || self.minimize_animations.values().any(|c| *c > 0)
    }

    /// Draw the dragged icon ghost at the cursor position.
    /// The icon is fully opaque and includes window count dashes below it.
    /// It draws directly into the main dock canvas so the ghost follows the cursor
    /// while visually "unsticking" from the dock — the original icon slot becomes
    /// empty during drag, and the ghost floats at the pointer position.
    fn draw_dragged_icon(
        &self,
        canvas: &mut [u8],
        icon: &slint::Image,
        canvas_width: usize,
        canvas_height: usize,
        pointer_x: i32,
        pointer_y: i32,
        window_count: usize,
        focused: bool,
    ) {
        // Get the icon pixel data using to_rgba8()
        let pixel_buffer = match icon.to_rgba8() {
            Some(buf) => buf,
            None => return,
        };
        let icon_width = pixel_buffer.width() as usize;
        let icon_height = pixel_buffer.height() as usize;
        let pixels = pixel_buffer.as_bytes();

        if icon_width == 0 || icon_height == 0 || pixels.is_empty() {
            return;
        }

        // Target size: 48x48 for the drag ghost
        let target_size = 48usize;
        let half_size = target_size / 2;

        // Center the icon on the cursor
        let start_x = (pointer_x as i32 - half_size as i32).max(0) as usize;
        let start_y = (pointer_y as i32 - half_size as i32).max(0) as usize;

        // Draw the icon fully opaque (no alpha blending)
        for dy in 0..target_size {
            if start_y + dy >= canvas_height {
                break;
            }

            let src_y = if icon_height == 1 { 0 } else { dy * (icon_height - 1) / (target_size - 1) };

            for dx in 0..target_size {
                let dst_x = start_x + dx;
                let dst_y = start_y + dy;

                if dst_x >= canvas_width || dst_y >= canvas_height {
                    continue;
                }

                let src_x = if icon_width == 1 { 0 } else { dx * (icon_width - 1) / (target_size - 1) };

                let src_idx = (src_y * icon_width + src_x) * 4;
                if src_idx + 3 >= pixels.len() {
                    continue;
                }

                let dst_idx = (dst_y * canvas_width + dst_x) * 4;
                if dst_idx + 3 >= canvas.len() {
                    continue;
                }

                // Source pixel is RGBA, write directly as BGRA (fully opaque)
                canvas[dst_idx] = pixels[src_idx + 2];       // B
                canvas[dst_idx + 1] = pixels[src_idx + 1];   // G
                canvas[dst_idx + 2] = pixels[src_idx];       // R
                canvas[dst_idx + 3] = pixels[src_idx + 3];   // A
            }
        }

        // Draw window count dashes below the icon
        if window_count >= 1 && window_count <= 5 {
            let dash_y = start_y + target_size; // 2px below icon (y=58 for 60px dock)
            let dash_color = if focused {
                // Cyan #00E5FF: B=0xFF, G=0xE5, R=0x00
                (0xFFu8, 0xE5u8, 0x00u8)
            } else {
                // Silver #9CA3AF: B=0xAF, G=0xA3, R=0x9C
                (0xAFu8, 0xA3u8, 0x9Cu8)
            };
            self.draw_dashes(canvas, start_x, dash_y, canvas_width, canvas_height, window_count, dash_color);
        }
    }

    /// Draw window count dashes (up to 5) in a 32px span, matching the reference design.
    fn draw_dashes(
        &self,
        canvas: &mut [u8],
        base_x: usize,
        base_y: usize,
        canvas_width: usize,
        canvas_height: usize,
        window_count: usize,
        color: (u8, u8, u8), // (B, G, R)
    ) {
        // Dash configurations: (count, width_each, gap)
        // Total span: 32px, starting at base_x + 8
        let configs: &[(usize, f32, f32)] = &[
            (1, 32.0, 0.0),    // 1 dash: 32px wide
            (2, 14.5, 3.0),    // 2 dashes: 14.5px each, 3px gap
            (3, 8.67, 3.0),    // 3 dashes: 8.67px each, 3px gap
            (4, 6.25, 3.0),    // 4 dashes: 6.25px each, 3px gap
            (5, 4.8, 3.0),     // 5 dashes: 4.8px each, 3px gap
        ];

        let config = match configs.get(window_count - 1) {
            Some(c) => c,
            None => return,
        };

        let (count, width, gap) = *config;
        let dash_height = 2;

        for i in 0..count {
            let x_offset = base_x + 8 + (i as f32 * (width + gap)) as usize;
            let w = width as usize;

            for dy in 0..dash_height {
                let y = base_y + dy;
                if y >= canvas_height {
                    break;
                }
                for dx in 0..w {
                    let x = x_offset + dx;
                    if x >= canvas_width {
                        break;
                    }
                    let idx = (y * canvas_width + x) * 4;
                    if idx + 2 < canvas.len() {
                        canvas[idx] = color.0;   // B
                        canvas[idx + 1] = color.1; // G
                        canvas[idx + 2] = color.2; // R
                        canvas[idx + 3] = 255;    // A (fully opaque)
                    }
                }
            }
        }
    }
}
