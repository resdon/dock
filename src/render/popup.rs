//! CPU popup renderer.
//!
//! Renders the window list popup and context menu to pixel buffers
//! using the Slint font rasterizer for smooth text.

use crate::config::{DockConfig, PopupBorderStyle};
use crate::geometry::popup::PopupGeometry;
use crate::geometry::popup::SurfaceGeometry;
use crate::state::WindowState;

use crate::render::font::FontManager;
use crate::render::text::draw_text;
use crate::render::theme;
use crate::types::{MenuItemType, MenuState};

pub const HOVER_ITEM_HEIGHT: i32 = PopupGeometry::BASE_ITEM_HEIGHT;
pub const DOCK_HEIGHT: i32 = 60;

pub const MENU_WIDTH: i32 = PopupGeometry::BASE_MENU_WIDTH;

/// Draws a border around a rectangle with the specified style.
///
/// Supports full, dashed, and no-border styles. Colors are in BGRA format.
fn draw_border(
    buffer: &mut [u8],
    width: i32,
    _height: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    color: &[u8; 4],
    style: &PopupBorderStyle,
    dash_gap: i32,
) {
    if matches!(style, PopupBorderStyle::None) {
        return;
    }

    let is_dashes = matches!(style, PopupBorderStyle::Dashes);
    let dash_gap = dash_gap.max(1);

    // Top edge
    for dx in 0..w {
        if is_dashes && (dx / dash_gap) % 2 != 0 {
            continue;
        }
        let px = x + dx;
        let py = y;
        if px >= 0 && px < width && py >= 0 && py < _height {
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
    // Bottom edge
    for dx in 0..w {
        if is_dashes && (dx / dash_gap) % 2 != 0 {
            continue;
        }
        let px = x + dx;
        let py = y + h - 1;
        if px >= 0 && px < width && py >= 0 && py < _height {
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
    // Left edge
    for dy in 0..h {
        if is_dashes && (dy / dash_gap) % 2 != 0 {
            continue;
        }
        let px = x;
        let py = y + dy;
        if px >= 0 && px < width && py >= 0 && py < _height {
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
    // Right edge
    for dy in 0..h {
        if is_dashes && (dy / dash_gap) % 2 != 0 {
            continue;
        }
        let px = x + w - 1;
        let py = y + dy;
        if px >= 0 && px < width && py >= 0 && py < _height {
            let idx = ((py * width + px) * 4) as usize;
            if idx + 3 < buffer.len() {
                buffer[idx..idx + 4].copy_from_slice(color);
            }
        }
    }
}
pub const HOVER_MENU_WIDTH: i32 = 200;

pub const MENU_ITEM_COUNT: i32 = 5;
pub const MENU_HEIGHT: i32 = PopupGeometry::BASE_ITEM_HEIGHT * MENU_ITEM_COUNT;

// `draw_text()` expects a baseline Y, while popup row centering is calculated
// from the row's top edge.  Font rasterization places the glyph bitmap above
// that baseline.  The nudge compensates for the font's ascent so the visual
// center of the text aligns with the row's vertical center.
const POPUP_TEXT_BASELINE_NUDGE_PX: i32 = 14;

/// Draws a radio button mark (circle) for menu items.
///
/// Used for the "Focus" menu item in the context menu to indicate
/// which window is currently focused.
fn radio_mark(canvas: &mut [u8], width: i32, height: i32, cx: i32, cy: i32, selected: bool) {
    // Canvas is ARGB8888 - use ARGB color constants
    let outer = if selected {
        theme::RED_ARGB
    } else {
        theme::BORDER_ARGB
    };
    for y in -5..=5 {
        for x in -5..=5 {
            let d = x * x + y * y;
            if d <= 25 && d >= 12 {
                let px = cx + x;
                let py = cy + y;
                if px >= 0 && py >= 0 && px < width && py < height {
                    let i = ((py * width + px) * 4) as usize;
                    if i + 3 < canvas.len() {
                        canvas[i..i + 4].copy_from_slice(&outer);
                    }
                }
            }
        }
    }
    if selected {
        for y in -2..=2 {
            for x in -2..=2 {
                if x * x + y * y <= 4 {
                    let px = cx + x;
                    let py = cy + y;
                    if px >= 0 && py >= 0 && px < width && py < height {
                        let i = ((py * width + px) * 4) as usize;
                        if i + 3 < canvas.len() {
                            canvas[i..i + 4].copy_from_slice(&theme::YELLOW_SOFT_ARGB);
                        }
                    }
                }
            }
        }
    }
}

/// PipeWire capture frame data for the window list popup.
///
/// Holds the RGBA pixel data for a single captured window frame,
/// downsampled by the configured `pipewire_scale_factor`. Used by
/// `draw_capture_frame` to render the thumbnail at the top of the popup.
pub struct CaptureFrameData {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    /// Original window dimensions (before scaling) for aspect ratio calculation
    pub original_width: u32,
    pub original_height: u32,
}

/// Draw a scaled PipeWire capture frame image into the canvas.
///
/// Scales the frame to fit within the specified width/height while
/// maintaining aspect ratio. The source data comes from
/// `PipewireCaptureService::get_active_frame()` which returns RGBA pixels.
fn draw_capture_frame(
    canvas: &mut [u8],
    canvas_width: i32,
    _canvas_height: i32,
    frame_x: i32,
    frame_y: i32,
    frame_w: i32,
    frame_h: i32,
    src_data: &CaptureFrameData,
) {
    let src_w = src_data.width as i32;
    let src_h = src_data.height as i32;
    if src_w <= 0
        || src_h <= 0
        || frame_w <= 0
        || frame_h <= 0
        || src_data.rgba.len() < (src_w * src_h * 4) as usize
    {
        return;
    }

    // Frame background is transparent — skip the black fill.
    // Canvas is already zeroed (transparent) by the caller.

    // Scale and draw the frame image
    let x_step = src_w as f32 / frame_w as f32;
    let y_step = src_h as f32 / frame_h as f32;

    for fy in 0..frame_h {
        for fx in 0..frame_w {
            let sx = (fx as f32 * x_step) as i32;
            let sy = (fy as f32 * y_step) as i32;
            let sx = sx.clamp(0, src_w - 1);
            let sy = sy.clamp(0, src_h - 1);

            let src_idx = ((sy * src_w + sx) * 4) as usize;
            if src_idx + 3 >= src_data.rgba.len() {
                continue;
            }

            let px = frame_x + fx;
            let py = frame_y + fy;
            if px >= 0 && py >= 0 && px < canvas_width && py < _canvas_height {
                let dst_idx = ((py * canvas_width + px) * 4) as usize;
                if dst_idx + 3 < canvas.len() {
                    // Source is RGBA, canvas is Argb8888 little-endian [B, G, R, A]
                    let r = src_data.rgba[src_idx];
                    let g = src_data.rgba[src_idx + 1];
                    let b = src_data.rgba[src_idx + 2];
                    let a = src_data.rgba[src_idx + 3];
                    canvas[dst_idx] = b; // B
                    canvas[dst_idx + 1] = g; // G
                    canvas[dst_idx + 2] = r; // R
                    canvas[dst_idx + 3] = a; // A
                }
            }
        }
    }
}

/// Renders the window list popup to a pixel buffer.
///
/// Draws the PipeWire capture frame at the top and window rows below it.
/// Each row shows the window title and a close button. The active stream's
/// row and hovered rows get visual highlights.
///
/// # Arguments
/// * `canvas` - ARGB8888 pixel buffer to render into
/// * `geometry` - Popup dimensions and layout
/// * `matched_windows` - Windows matched to PipeWire streams
/// * `active_row` - Index of the currently displayed stream
/// * `hovered_item` - Index of the hovered row (for highlight)
/// * `font_manager` - Font manager for text rendering
/// * `config` - Dock configuration for colors and sizes
#[allow(clippy::too_many_arguments)]
pub fn render_window_list_surface(
    canvas: &mut [u8],
    geometry: &SurfaceGeometry,
    scale_factor: f32,
    windows: &[&WindowState],
    font_manager: &mut FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
    capture_frame: Option<&CaptureFrameData>,
    _active_row_idx: usize,
    config: &DockConfig,
) {
    if windows.is_empty() || geometry.phys_width <= 0 || geometry.phys_height <= 0 {
        canvas.fill(0);
        return;
    }

    let expected_len = (geometry.phys_width as usize) * (geometry.phys_height as usize) * 4;
    if canvas.len() < expected_len {
        return;
    }

    let item_count = windows.len();
    let menu_width = geometry.phys_width;
    let menu_height = geometry.phys_height;

    // Derive frame height from the geometry's total height minus the rows area.
    // The geometry is calculated in window_list.rs using the first stream's aspect ratio,
    // so the renderer must use the same frame height to avoid clipping rows.
    let frame_border = (config.popup_frame_border as f32 * scale_factor).round() as i32;
    let frame_inner_w = menu_width - frame_border * 2;

    // Row height from config
    let item_h_log = config.popup_row_height as f32;
    let phys_item_h = (item_h_log * scale_factor).round() as i32;
    let rows_phys_h = item_count as i32 * phys_item_h;
    let frame_phys_h = menu_height - rows_phys_h;

    let rows_area_y = frame_phys_h;

    // Parse colors from config (canvas is BGRA: [B, G, R, A])
    // Text colors default to green (#00FF00) for high contrast against dark backgrounds
    let close_hover_color = DockConfig::parse_hex_color_bgra(&config.popup_close_hover_color);
    // Popup border color also matches the dashed block text blue
    let popup_border_color = DockConfig::parse_hex_color_bgra(&config.dashed_block_text_color);
    let close_button_color = DockConfig::parse_hex_color_bgra(&config.popup_close_button_color);
    let close_button_hover_color =
        DockConfig::parse_hex_color_bgra(&config.popup_close_button_hover_color);
    let close_cross_color = DockConfig::parse_hex_color_bgra(&config.popup_close_cross_color);
    let _frame_bg_color = DockConfig::parse_hex_color_bgra(&config.popup_frame_bg_color);
    // Window title text — blue (#007AFF) to match dashed block text color
    let text_color = DockConfig::parse_hex_color(&config.popup_text_color);
    let close_btn_size_cfg = config.popup_close_button_size as f32;
    let font_size_cfg = config.popup_font_size as f32;

    // Transparent popup: no background fill, no border, no rounded corners.
    // Canvas is Argb8888 little-endian - use _ARGB color constants.
    canvas[..expected_len].fill(0);

    // Draw the PipeWire capture frame at the top
    if let Some(frame_data) = capture_frame {
        let frame_inner_h = frame_phys_h - frame_border * 2;
        if frame_inner_w > 0 && frame_inner_h > 0 {
            draw_capture_frame(
                canvas,
                menu_width,
                menu_height,
                frame_border,
                frame_border,
                frame_inner_w,
                frame_inner_h,
                frame_data,
            );
        }
    }
    // No capture frame — leave the frame area empty (transparent)

    // 1. Render row backgrounds — transparent by default, red only when close button is hovered
    // Convert logical pointer coordinates to physical pixels for close button bounds check
    let ptr_phys_x = (local_pointer_x as f32 * scale_factor).round() as i32;
    let ptr_phys_y = (local_pointer_y as f32 * scale_factor).round() as i32;

    // Pre-compute which row's close button is hovered (if any)
    let close_hovered_row = (0..item_count).find(|&i| {
        let item_top_phys = rows_area_y + i as i32 * phys_item_h;
        let close_btn_size = (close_btn_size_cfg * scale_factor).round() as i32;
        let close_btn_x = menu_width
            - close_btn_size
            - ((config.popup_close_button_margin as f32 * scale_factor).round() as i32);
        let close_btn_y = item_top_phys + (phys_item_h - close_btn_size) / 2;
        ptr_phys_x >= close_btn_x
            && ptr_phys_x < close_btn_x + close_btn_size
            && ptr_phys_y >= close_btn_y
            && ptr_phys_y < close_btn_y + close_btn_size
    });

    let hovered_row_idx = (0..item_count).find(|&i| {
        let item_top_phys = rows_area_y + i as i32 * phys_item_h;
        ptr_phys_x >= 0
            && ptr_phys_x < menu_width
            && ptr_phys_y >= item_top_phys
            && ptr_phys_y < item_top_phys + phys_item_h
    });

    for i in 0..item_count {
        let top = rows_area_y + i as i32 * phys_item_h;
        let pad = (config.popup_row_padding as f32 * scale_factor).round() as i32;
        let row_x = (config.popup_frame_border as f32 * scale_factor).round() as i32;
        let row_y = top + pad;
        let row_w = menu_width - (config.popup_frame_border as f32 * scale_factor).round() as i32;
        let row_h = (phys_item_h - pad * 2).max(1);

        // Draw row background: red when close button is hovered
        if Some(i) == close_hovered_row {
            let color = close_hover_color;
            for ry in row_y..row_y + row_h {
                for rx in row_x..row_x + row_w {
                    if rx >= 0 && ry >= 0 && rx < menu_width && ry < menu_height {
                        let idx = ((ry * menu_width + rx) * 4) as usize;
                        if idx + 3 < canvas.len() {
                            canvas[idx..idx + 4].copy_from_slice(&color);
                        }
                    }
                }
            }
        }

        if Some(i) == hovered_row_idx {
            draw_border(
                canvas,
                menu_width,
                menu_height,
                row_x,
                row_y,
                row_w,
                row_h,
                &popup_border_color,
                &PopupBorderStyle::Dashes,
                (config.popup_dash_gap as f32 * scale_factor).round() as i32,
            );
        }
    }

    // 2. Render dashed separator lines between items (offset by rows_area_y)
    // Use the same blue as the dashed block text: #007AFF
    let separator_color = popup_border_color;
    let dash_gap = config.popup_dash_gap.max(1) as i32;
    for i in 1..item_count {
        let line_y = rows_area_y + i as i32 * phys_item_h;
        if line_y < menu_height {
            for x in 0..menu_width {
                if (x / dash_gap) % 2 != 0 {
                    continue;
                }
                let idx = ((line_y * menu_width + x) * 4) as usize;
                if idx + 3 < canvas.len() {
                    canvas[idx..idx + 4].copy_from_slice(&separator_color);
                }
            }
        }
    }

    // 3. Render Text labels & Close buttons (offset by rows_area_y)
    let font_size = (font_size_cfg * 2.0 * scale_factor) as i32;
    let text_x = (config.popup_text_padding as f32 * scale_factor).round() as i32;

    for (i, w) in windows.iter().enumerate() {
        let item_top_phys = rows_area_y + i as i32 * phys_item_h;
        let current_item_h_phys = phys_item_h;

        let truncate_len = config.popup_text_truncate as usize;
        let title = if w.title.chars().count() > truncate_len {
            format!(
                "{}...",
                w.title
                    .chars()
                    .take(truncate_len.saturating_sub(3))
                    .collect::<String>()
            )
        } else {
            w.title.clone()
        };

        let text_offset_y =
            ((current_item_h_phys as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;
        let text_y = item_top_phys
            + text_offset_y
            + POPUP_TEXT_BASELINE_NUDGE_PX
            + (font_size_cfg * scale_factor / 2.0).round() as i32
            - 6;

        if text_y < menu_height {
            crate::render::text::draw_text(
                canvas,
                menu_width,
                menu_height,
                font_manager,
                &title,
                font_size,
                text_x,
                text_y,
                text_color,
            );
        }

        // Render Close Button
        let close_btn_size = (close_btn_size_cfg * scale_factor).round() as i32;
        let close_btn_x = menu_width
            - close_btn_size
            - ((config.popup_close_button_margin as f32 * scale_factor).round() as i32);
        let close_btn_y = item_top_phys + (current_item_h_phys - close_btn_size) / 2;

        let is_close_hovered = ptr_phys_x >= close_btn_x
            && ptr_phys_x < close_btn_x + close_btn_size
            && ptr_phys_y >= close_btn_y
            && ptr_phys_y < close_btn_y + close_btn_size;

        let btn_color = if is_close_hovered {
            close_button_hover_color
        } else {
            close_button_color
        };
        // Draw close button as a square (no rounded corners)
        for cy in close_btn_y..close_btn_y + close_btn_size {
            for cx in close_btn_x..close_btn_x + close_btn_size {
                if cx >= 0 && cy >= 0 && cx < menu_width && cy < menu_height {
                    let idx = ((cy * menu_width + cx) * 4) as usize;
                    if idx + 3 < canvas.len() {
                        canvas[idx..idx + 4].copy_from_slice(&btn_color);
                    }
                }
            }
        }
        // Cross mark replaces the old solid blue square while retaining the
        // exact same hitbox and interaction behavior.
        for d in 4..close_btn_size.saturating_sub(4) {
            for (px, py) in [
                (close_btn_x + d, close_btn_y + d),
                (close_btn_x + close_btn_size - 1 - d, close_btn_y + d),
            ] {
                if px >= 0 && py >= 0 && px < menu_width && py < menu_height {
                    let idx = ((py * menu_width + px) * 4) as usize;
                    if idx + 3 < canvas.len() {
                        canvas[idx..idx + 4].copy_from_slice(&close_cross_color);
                    }
                }
            }
        }
    }
}

/// Renders the context menu popup to a pixel buffer.
///
/// Draws menu items with colored text for special actions (Focus, Unpin,
/// Launch, Quit). Supports configurable border styles (full, dashes, none).
///
/// # Arguments
/// * `buffer` - ARGB8888 pixel buffer to render into
/// * `geometry` - Menu dimensions and layout
/// * `items` - Menu items to render
/// * `hovered_idx` - Index of the hovered item (for border highlight)
/// * `font_manager` - Font manager for text rendering
/// * `config` - Dock configuration for colors and sizes
#[allow(clippy::too_many_arguments)]
pub fn render_context_menu_surface(
    buffer: &mut [u8],
    geometry: &SurfaceGeometry,
    scale_factor: f32,
    menu_state: &MenuState,
    font_manager: &mut FontManager,
    local_pointer_x: i32,
    local_pointer_y: i32,
    config: &DockConfig,
) {
    if menu_state.items.is_empty() || geometry.phys_width <= 0 || geometry.phys_height <= 0 {
        buffer.fill(0);
        return;
    }

    let expected_len = (geometry.phys_width as usize) * (geometry.phys_height as usize) * 4;
    if buffer.len() < expected_len {
        return;
    }

    let item_count = menu_state.items.len();
    if item_count == 0 {
        return;
    }

    let width = geometry.phys_width;
    let height = geometry.phys_height;

    // Logical height of the menu and proportional item height
    let logical_menu_h = geometry.logical_height as f32;
    let item_h_log = logical_menu_h / item_count as f32;

    // Evaluate hovered item using logical coordinates matching handle_menu_release
    let hovered_item_idx = if (local_pointer_x as f32) >= 0.0
        && (local_pointer_x as f32) < (geometry.logical_width as f32)
        && (local_pointer_y as f32) >= 0.0
        && (local_pointer_y as f32) < logical_menu_h
    {
        let idx = ((local_pointer_y as f32 / item_h_log) as i32).clamp(0, (item_count - 1) as i32)
            as usize;
        Some(idx)
    } else {
        None
    };

    // Transparent context menu: no background fill.
    // Canvas pixels stay at alpha 0 (transparent).
    buffer[..expected_len].fill(0);

    // Parse config colors — text defaults to blue (#007AFF) to match dashed block
    let text_color_cfg = DockConfig::parse_hex_color(&config.popup_text_color);
    let border_color_bgra = DockConfig::parse_hex_color_bgra(&config.popup_border_color);
    // Context menu action colors — all green for consistency
    let focus_color = DockConfig::parse_hex_color(&config.popup_focus_color);
    let unpin_color = DockConfig::parse_hex_color(&config.popup_unpin_color);
    let launch_color = DockConfig::parse_hex_color(&config.popup_launch_color);
    let quit_color = DockConfig::parse_hex_color(&config.popup_quit_color);
    let font_size_cfg = config.popup_font_size as f32;
    let font_size = (font_size_cfg * scale_factor) as i32;
    let text_x = (config.popup_text_padding as f32 * scale_factor).round() as i32;

    for (i, item) in menu_state.items.iter().enumerate() {
        let item_top_phys = (i as i32 * height) / item_count as i32;
        let next_item_top_phys = ((i as i32 + 1) * height) / item_count as i32;
        let current_item_h_phys = next_item_top_phys - item_top_phys;

        // Draw border highlight on hovered row
        if Some(i) == hovered_item_idx {
            draw_border(
                buffer,
                width,
                height,
                0,
                item_top_phys,
                width,
                current_item_h_phys,
                &border_color_bgra,
                &PopupBorderStyle::Dashes,
                (config.popup_dash_gap as f32 * scale_factor).round() as i32,
            );
        }

        let text_offset_y =
            ((current_item_h_phys as f32 - 14.0 * scale_factor) / 2.0).max(0.0) as i32;
        let text_y = item_top_phys
            + text_offset_y
            + POPUP_TEXT_BASELINE_NUDGE_PX
            + (config.context_menu_text_offset_y as f32 * scale_factor).round() as i32;

        if text_y >= height {
            break;
        }

        let text_color = match &item.item_type {
            MenuItemType::CloseApp => quit_color,
            MenuItemType::LaunchNew => launch_color,
            MenuItemType::Focus => focus_color,
            MenuItemType::TogglePin => unpin_color,
            _ => text_color_cfg,
        };

        if matches!(&item.item_type, MenuItemType::TogglePin) {
            let is_enabled = item.label.to_ascii_lowercase().contains("unpin");
            let cy = item_top_phys + current_item_h_phys / 2;
            radio_mark(
                buffer,
                width,
                height,
                width - (16.0 * scale_factor).round() as i32,
                cy,
                is_enabled,
            );
        }

        draw_text(
            buffer,
            width,
            height,
            font_manager,
            &item.label,
            font_size,
            text_x,
            text_y,
            text_color,
        );
    }
}
