//! Date/time block surface management.
//!
//! Allocates a shared memory buffer, renders the date/time block via the
//! `render_date_time_block` function, and commits the buffer to the layer surface.
//! The block is positioned to the right of the dock as a floating widget.

use smithay_client_toolkit::{shell::WaylandSurface, shm::slot::SlotPool};
use wayland_client::protocol::wl_shm;

use crate::config::DockConfig;
use crate::render::date_time_block::render_date_time_block;
use crate::render::font::FontManager;
use crate::types::DockInstance;

/// Draws the date/time block widget and commits the buffer.
///
/// The layer surface is pre-created during init; this function renders the
/// dashed border pattern with the current time, day+month, and year,
/// then attaches the buffer each frame.
///
/// # Parameters
///
/// - `dock` — dock instance containing the date/time block layer surface
/// - `pool` — shared memory buffer pool for allocating frame buffers
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all date_time_block_* settings
pub fn draw_date_time_block(
    dock: &mut DockInstance,
    pool: &mut SlotPool,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
) {
    // Skip rendering if the date/time block is disabled in config
    if !config.date_time_block_enabled {
        return;
    }

    let block = match &mut dock.date_time_block_surface {
        Some(b) => b,
        None => return,
    };

    // Use configured dimensions, falling back to defaults for the surface size constants
    let block_width = config.date_time_block_width.max(1) as i32;
    let block_height = config.date_time_block_height.max(1) as i32;
    let scale = dock.scale_factor.max(0.25);
    let phys_width = (block_width as f64 * scale).round().max(1.0) as i32;
    let phys_height = (block_height as f64 * scale).round().max(1.0) as i32;
    let a = alpha.clamp(0.0, 1.0);

    let expected_len = phys_width as usize * phys_height as usize * 4;
    block.cached_content.resize(expected_len, 0);
    block.cached_content.fill(0);

    // Render the date/time block with all config-driven parameters
    render_date_time_block(
        &mut block.cached_content,
        phys_width as u32,
        phys_height as u32,
        scale,
        a,
        font_manager,
        config,
    );
    block.cached_phys_width = phys_width;
    block.cached_phys_height = phys_height;

    // Allocate SHM buffer and commit
    let (buffer, canvas) = match pool.create_buffer(
        phys_width,
        phys_height,
        phys_width * 4,
        wl_shm::Format::Argb8888,
    ) {
        Ok(b) => b,
        Err(_) => return,
    };

    if !crate::app::copy_popup_content(canvas, &block.cached_content) {
        return;
    }

    block
        .surface
        .wl_surface()
        .set_buffer_scale(scale.round().max(1.0) as i32);
    if buffer.attach_to(block.surface.wl_surface()).is_err() {
        return;
    }
    block
        .surface
        .wl_surface()
        .damage_buffer(0, 0, phys_width, phys_height);
    block.surface.wl_surface().commit();
    block.current_buffer = Some(buffer);
}
