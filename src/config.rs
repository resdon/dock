//! Dock configuration.
//!
//! Loads and saves user preferences from ~/.config/dock/config.json.
//! All UI parameters (colors, sizes, animations) are configurable.

use serde::{Deserialize, Serialize};
use std::fs;

fn get_config_dir() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/.config/dock", home)
}

fn get_config_path() -> String {
    format!("{}/config.json", get_config_dir())
}

/// Dock alignment anchor point
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DockAnchor {
    Bottom,
    Top,
    Left,
    Right,
}

impl Default for DockAnchor {
    fn default() -> Self {
        DockAnchor::Bottom
    }
}

/// Popup border style
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PopupBorderStyle {
    Full,
    #[default]
    Dashes,
    None,
}

/// Dock configuration settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockConfig {
    // === Dock layout ===
    /// The anchor point for the dock on the screen
    #[serde(default)]
    pub anchor: DockAnchor,
    /// Dock height in pixels (logical)
    #[serde(default = "default_dock_height")]
    pub dock_height: u32,
    /// Icon size in pixels
    #[serde(default = "default_icon_size")]
    pub icon_size: u32,
    /// Spacing between icons in pixels
    #[serde(default = "default_spacing")]
    pub spacing: u32,
    /// Margin from the anchored edge in pixels (floating dock)
    #[serde(default = "default_margin")]
    pub margin: i32,

    // === Dock appearance ===
    /// Dock background color in hex format (e.g., "#1D1C22")
    #[serde(default = "default_background_color")]
    pub background_color: String,
    /// Dock background transparency (0.0 = fully transparent, 1.0 = fully opaque)
    #[serde(default = "default_background_alpha")]
    pub background_alpha: f32,
    /// Dock border color in hex format (e.g., "#4C4954")
    #[serde(default = "default_border_color")]
    pub border_color: String,
    /// Dock border transparency (0.0 = fully transparent, 1.0 = fully opaque)
    #[serde(default = "default_border_alpha")]
    pub border_alpha: f32,
    /// Dock border radius in pixels (0 = square corners)
    #[serde(default = "default_border_radius")]
    pub border_radius: u32,

    // === Dock hover ===
    /// Enable hover highlight outline on icons
    #[serde(default = "default_hover_highlight")]
    pub hover_highlight: bool,
    /// Hover highlight color in hex format (e.g., "#00E5FF")
    #[serde(default = "default_hover_color")]
    pub hover_color: String,
    /// Hover highlight border thickness in pixels
    #[serde(default = "default_hover_border_thickness")]
    pub hover_border_thickness: u32,

    // === Dock activity dashes ===
    /// Dash color for focused/active apps in hex format
    #[serde(default = "default_dash_focused_color")]
    pub dash_focused_color: String,
    /// Dash color for inactive apps in hex format
    #[serde(default = "default_dash_inactive_color")]
    pub dash_inactive_color: String,
    /// Dash line thickness in pixels
    #[serde(default = "default_dash_thickness")]
    pub dash_thickness: u32,
    /// Dash segment length in pixels
    #[serde(default = "default_dash_segment_length")]
    pub dash_segment_length: u32,
    /// Dash gap length in pixels
    #[serde(default = "default_dash_gap_length")]
    pub dash_gap_length: u32,

    // === Click animation ===
    /// Enable click animation (scale up and back)
    #[serde(default = "default_click_animation")]
    pub click_animation: bool,
    /// Click animation scale factor (e.g., 1.15 = 115% size)
    #[serde(default = "default_click_animation_scale")]
    pub click_animation_scale: f32,
    /// Click animation scale-up duration in seconds
    #[serde(default = "default_click_animation_up_duration")]
    pub click_animation_up_duration: f32,
    /// Click animation scale-down duration in seconds
    #[serde(default = "default_click_animation_down_duration")]
    pub click_animation_down_duration: f32,

    // === Bounce (launch) animation ===
    /// Enable bounce animation when launching apps
    #[serde(default = "default_bounce_animation")]
    pub bounce_animation: bool,
    /// Bounce animation scale factor (e.g., 1.2 = 120% size)
    #[serde(default = "default_bounce_animation_scale")]
    pub bounce_animation_scale: f32,
    /// Bounce animation duration in seconds
    #[serde(default = "default_bounce_animation_duration")]
    pub bounce_animation_duration: f32,

    /// Minimize animation: scale factor (1.0 = no change, 0.7 = shrink to 70%)
    #[serde(default = "default_minimize_animation_scale")]
    pub minimize_animation_scale: f32,
    /// Minimize animation duration in seconds
    #[serde(default = "default_minimize_animation_duration")]
    pub minimize_animation_duration: f32,

    // === Popup window list ===
    /// Popup window list width in pixels
    #[serde(default = "default_popup_width")]
    pub popup_width: u32,
    /// Popup row height in pixels
    #[serde(default = "default_popup_row_height")]
    pub popup_row_height: u32,
    /// Popup PipeWire capture frame height in pixels (logical)
    #[serde(default = "default_popup_frame_height")]
    pub popup_frame_height: u32,
    /// Popup font size in pixels
    #[serde(default = "default_popup_font_size")]
    pub popup_font_size: u32,
    /// Popup close button size in pixels
    #[serde(default = "default_popup_close_button_size")]
    pub popup_close_button_size: u32,
    /// Popup text padding from left edge in pixels
    #[serde(default = "default_popup_text_padding")]
    pub popup_text_padding: u32,
    /// Popup row padding (top/bottom) in pixels
    #[serde(default = "default_popup_row_padding")]
    pub popup_row_padding: u32,
    /// Popup text truncation length (characters)
    #[serde(default = "default_popup_text_truncate")]
    pub popup_text_truncate: u32,
    /// Popup frame border thickness in pixels
    #[serde(default = "default_popup_frame_border")]
    pub popup_frame_border: u32,
    /// Popup close button margin from right edge in pixels
    #[serde(default = "default_popup_close_button_margin")]
    pub popup_close_button_margin: u32,
    /// PipeWire capture frame maximum quality cap (0.01–1.0).
    ///
    /// The frame is automatically downscaled to match the popup's display area.
    /// This value acts as an upper bound: even if the display area is large, the
    /// frame won't exceed `source × pipewire_scale_factor`. Set to 1.0 for full
    /// quality, or lower (e.g., 0.25) to cap memory usage for very large windows.
    /// Clamped to [0.01, 1.0] during config validation.
    #[serde(default = "default_pipewire_scale_factor")]
    pub pipewire_scale_factor: f32,

    // === Popup colors ===
    /// Popup row hover border color in hex format (e.g., "#6495E6")
    #[serde(default = "default_popup_hover_border_color")]
    pub popup_hover_border_color: String,
    /// Popup close button hover row highlight color in hex format (e.g., "#DC3232")
    #[serde(default = "default_popup_close_hover_color")]
    pub popup_close_hover_color: String,
    /// Popup close button default color in hex format (e.g., "#DC2626")
    #[serde(default = "default_popup_close_button_color")]
    pub popup_close_button_color: String,
    /// Popup close button hover color in hex format (e.g., "#1919AE")
    #[serde(default = "default_popup_close_button_hover_color")]
    pub popup_close_button_hover_color: String,
    /// Popup close button cross mark color in hex format (e.g., "#FFE6F6")
    #[serde(default = "default_popup_close_cross_color")]
    pub popup_close_cross_color: String,
    /// Popup frame background color in hex format (e.g., "#000000")
    #[serde(default = "default_popup_frame_bg_color")]
    pub popup_frame_bg_color: String,
    /// Popup window list text color in hex format.
    /// Default: #00FF00 (green) — used for window titles in the popup grid.
    /// Green provides high contrast against the dark popup background.
    #[serde(default = "default_popup_text_color")]
    pub popup_text_color: String,
    /// Popup border style: "full", "dashes", or "none"
    #[serde(default)]
    pub popup_border_style: PopupBorderStyle,
    /// Popup border color in hex format (e.g., "#6495E6")
    #[serde(default = "default_popup_border_color")]
    pub popup_border_color: String,
    /// Popup border dash gap size in pixels
    #[serde(default = "default_popup_dash_gap")]
    pub popup_dash_gap: u32,

    // === Context menu ===
    /// Context menu width in pixels
    #[serde(default = "default_context_menu_width")]
    pub context_menu_width: u32,
    /// Context menu row height in pixels
    #[serde(default = "default_context_menu_row_height")]
    pub context_menu_row_height: u32,
    /// Context menu Focus action text color in hex format.
    /// Default: #00FF00 (green) — used for the "Focus" menu item to bring a window to front.
    #[serde(default = "default_popup_focus_color")]
    pub popup_focus_color: String,
    /// Context menu Unpin/TogglePin action text color in hex format.
    /// Default: #00FF00 (green) — used for the pin/unpin toggle menu item.
    #[serde(default = "default_popup_unpin_color")]
    pub popup_unpin_color: String,
    /// Context menu Launch action text color in hex format.
    /// Default: #00FF00 (green) — used for the "Launch" menu item to start a new instance.
    #[serde(default = "default_popup_launch_color")]
    pub popup_launch_color: String,
    /// Context menu Quit/CloseApp action text color in hex format.
    /// Default: #00FF00 (green) — used for the "Quit" menu item to close the application.
    /// Previously red (#E66464), changed to green for consistent text color scheme.
    #[serde(default = "default_popup_quit_color")]
    pub popup_quit_color: String,

    // === Fade/timing ===
    /// Window list popup fade-in duration in seconds
    #[serde(default = "default_popup_fade_in")]
    pub popup_fade_in: f32,
    /// Context menu fade-in duration in seconds
    #[serde(default = "default_context_menu_fade_in")]
    pub context_menu_fade_in: f32,
    /// Window list popup auto-close timeout in milliseconds
    #[serde(default = "default_popup_timeout")]
    pub popup_timeout: u64,
    /// Context menu auto-close timeout in milliseconds
    #[serde(default = "default_context_menu_timeout")]
    pub context_menu_timeout: u64,
    /// Dock hard inactivity timeout in milliseconds (closes all popups)
    #[serde(default = "default_hard_timeout")]
    pub hard_timeout: u64,

    // === Icon badge ===
    /// Badge notification number text color in hex format.
    /// Default: #00FF00 (green) — displayed on the red badge circle (#DC2626).
    /// Green provides high contrast against the red background for visibility.
    #[serde(default = "default_badge_text_color")]
    pub badge_text_color: String,
    /// Badge background color in hex format
    #[serde(default = "default_badge_bg_color")]
    pub badge_bg_color: String,
    /// Badge font size in pixels
    #[serde(default = "default_badge_font_size")]
    pub badge_font_size: u32,

    // === Date/time block widget (right side of dock) ===
    /// Enable the date/time block widget floating to the right of the dock.
    /// The block displays time (HH:MM), day+month, and year.
    #[serde(default = "default_date_time_block_enabled")]
    pub date_time_block_enabled: bool,
    /// Date/time block width in pixels (logical).
    /// Default: 104px — wide enough for time and date text.
    #[serde(default = "default_date_time_block_width")]
    pub date_time_block_width: u32,
    /// Date/time block height in pixels (logical).
    /// Default: 62px — accommodates time, day+month, and year.
    #[serde(default = "default_date_time_block_height")]
    pub date_time_block_height: u32,
    /// Date/time block border color in hex format (e.g., "#00FF00").
    /// Default: #00FF00 (green) — matches the dock's text color scheme.
    #[serde(default = "default_date_time_block_border_color")]
    pub date_time_block_border_color: String,
    /// Date/time block border thickness in pixels.
    /// Default: 2px — visible but not overwhelming.
    #[serde(default = "default_date_time_block_border_thickness")]
    pub date_time_block_border_thickness: u32,
    /// Date/time block dash segment length in pixels.
    /// Default: 6px — each dash line is 6 pixels long.
    #[serde(default = "default_date_time_block_dash_length")]
    pub date_time_block_dash_length: u32,
    /// Date/time block dash gap length in pixels.
    /// Default: 4px — gap between each dash segment.
    #[serde(default = "default_date_time_block_gap_length")]
    pub date_time_block_gap_length: u32,
    /// Date/time block text color in hex format (e.g., "#007AFF").
    /// Default: #007AFF (blue) — used for time, date, and year text.
    #[serde(default = "default_date_time_block_text_color")]
    pub date_time_block_text_color: String,
    /// Date/time block time font size in points.
    /// Default: 32pt — large, prominent time display (HH:MM).
    #[serde(default = "default_date_time_block_time_font_size")]
    pub date_time_block_time_font_size: u32,
    /// Date/time block time text y-position in pixels (from top).
    /// Default: 25px — time text below the dashed border.
    #[serde(default = "default_date_time_block_time_text_y")]
    pub date_time_block_time_text_y: u32,
    /// Date/time block date font size in points.
    /// Default: 16pt — half the time font size for date lines.
    #[serde(default = "default_date_time_block_date_font_size")]
    pub date_time_block_date_font_size: u32,
    /// Date/time block day+month text y-position in pixels (from top).
    /// Default: 43px — day+month text below the time.
    #[serde(default = "default_date_time_block_day_month_y")]
    pub date_time_block_day_month_y: u32,
    /// Date/time block year text y-position in pixels (from top).
    /// Default: 59px — year text below the day+month.
    #[serde(default = "default_date_time_block_year_y")]
    pub date_time_block_year_y: u32,
    /// Date/time block gap from the dock's right edge in pixels.
    /// Default: 4px — positions the block next to the dock.
    #[serde(default = "default_date_time_block_gap_from_dock")]
    pub date_time_block_gap_from_dock: u32,

    // === Dashed block widget (icons and keyboard layout, left side of dock) ===
    /// Enable the dashed block widget floating to the left of the dock.
    /// The block displays status icons and keyboard layout indicator.
    #[serde(default = "default_dashed_block_enabled")]
    pub dashed_block_enabled: bool,
    /// Dashed block width in pixels (logical).
    /// Default: 74px — wide enough for 2×2 icon grid and ENG text.
    #[serde(default = "default_dashed_block_width")]
    pub dashed_block_width: u32,
    /// Dashed block height in pixels (logical).
    /// Default: 48px — accommodates 2×2 icon grid and keyboard layout text.
    #[serde(default = "default_dashed_block_height")]
    pub dashed_block_height: u32,
    /// Dashed block border color in hex format (e.g., "#00FF00").
    /// Default: #00FF00 (green) — matches the dock's text color scheme.
    #[serde(default = "default_dashed_block_border_color")]
    pub dashed_block_border_color: String,
    /// Dashed block border thickness in pixels.
    /// Default: 2px — visible but not overwhelming.
    #[serde(default = "default_dashed_block_border_thickness")]
    pub dashed_block_border_thickness: u32,
    /// Dashed block dash segment length in pixels.
    /// Default: 6px — each dash line is 6 pixels long.
    #[serde(default = "default_dashed_block_dash_length")]
    pub dashed_block_dash_length: u32,
    /// Dashed block dash gap length in pixels.
    /// Default: 4px — gap between each dash segment.
    #[serde(default = "default_dashed_block_gap_length")]
    pub dashed_block_gap_length: u32,
    /// Dashed block text color in hex format (e.g., "#007AFF").
    /// Default: #007AFF (blue) — used for keyboard layout text.
    #[serde(default = "default_dashed_block_text_color")]
    pub dashed_block_text_color: String,
    /// Dashed block icon size in pixels.
    /// Default: 16px — each status icon (rclonetray, bluetooth, speaker, pac) is 16×16.
    #[serde(default = "default_dashed_block_icon_size")]
    pub dashed_block_icon_size: u32,
    /// Dashed block icon gap in pixels.
    /// Default: 4px — space between adjacent icons in a row.
    #[serde(default = "default_dashed_block_icon_gap")]
    pub dashed_block_icon_gap: u32,
    /// Dashed block icon row gap in pixels.
    /// Default: 4px — vertical space between the top and bottom icon rows.
    #[serde(default = "default_dashed_block_row_gap")]
    pub dashed_block_row_gap: u32,
    /// Dashed block top icon row y-position in pixels (from top).
    /// Default: 4px — near the top of the block.
    #[serde(default = "default_dashed_block_top_row_y")]
    pub dashed_block_top_row_y: u32,
    /// Dashed block row x-offset in pixels.
    /// Default: 6px — icon rows offset 6px to the right from center.
    #[serde(default = "default_dashed_block_row_offset")]
    pub dashed_block_row_offset: u32,
    /// Dashed block keyboard layout font size in points.
    /// Default: 12pt — smaller font for the "ENG" indicator.
    #[serde(default = "default_dashed_block_kb_font_size")]
    pub dashed_block_kb_font_size: u32,
    /// Dashed block keyboard layout text (e.g., "ENG").
    /// Default: "ENG" — hardcoded placeholder; making it dynamic requires system integration.
    #[serde(default = "default_dashed_block_kb_layout")]
    pub dashed_block_kb_layout: String,
    /// Dashed block gap from the dock's left edge in pixels.
    /// Default: 4px — positions the block next to the dock.
    #[serde(default = "default_dashed_block_gap_from_dock")]
    pub dashed_block_gap_from_dock: u32,
}

fn default_dock_height() -> u32 {
    60
}

fn default_icon_size() -> u32 {
    48
}

fn default_spacing() -> u32 {
    8
}

fn default_margin() -> i32 {
    10
}

fn default_hover_highlight() -> bool {
    false
}

fn default_click_animation() -> bool {
    true
}

fn default_hover_color() -> String {
    "#00E5FF".to_string()
}

fn default_dash_focused_color() -> String {
    "#00E5FF".to_string()
}

fn default_dash_inactive_color() -> String {
    "#9CA3AF".to_string()
}

fn default_click_animation_scale() -> f32 {
    1.10
}

fn default_click_animation_up_duration() -> f32 {
    0.30
}

fn default_click_animation_down_duration() -> f32 {
    0.30
}

fn default_bounce_animation_scale() -> f32 {
    1.2
}

fn default_bounce_animation_duration() -> f32 {
    0.8
}

fn default_bounce_animation() -> bool {
    true
}

fn default_minimize_animation_scale() -> f32 {
    0.85
}

fn default_minimize_animation_duration() -> f32 {
    0.8
}

fn default_background_color() -> String {
    "#1D1C22".to_string()
}

fn default_background_alpha() -> f32 {
    0.0
}

fn default_border_color() -> String {
    "#4C4954".to_string()
}

fn default_border_alpha() -> f32 {
    0.0
}

fn default_popup_width() -> u32 {
    320
}

fn default_popup_row_height() -> u32 {
    30
}

fn default_popup_frame_height() -> u32 {
    480
}

fn default_popup_font_size() -> u32 {
    14
}

fn default_popup_close_button_size() -> u32 {
    16
}

fn default_popup_hover_border_color() -> String {
    "#6495E6".to_string()
}

fn default_popup_close_hover_color() -> String {
    "#DC3232".to_string()
}

fn default_popup_close_button_color() -> String {
    "#DC2626".to_string()
}

fn default_popup_close_button_hover_color() -> String {
    "#1919AE".to_string()
}

fn default_popup_close_cross_color() -> String {
    "#FFE6F6".to_string()
}

fn default_popup_frame_bg_color() -> String {
    "#000000".to_string()
}


/// Default color for popup window list text labels.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_text_color() -> String {
    "#00FF00".to_string()
}

fn default_popup_border_color() -> String {
    "#6495E6".to_string()
}

/// Default text color for context menu Focus action item.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_focus_color() -> String {
    "#00FF00".to_string()
}

/// Default text color for context menu Unpin/TogglePin action item.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_unpin_color() -> String {
    "#00FF00".to_string()
}

/// Default text color for context menu Launch action item.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_launch_color() -> String {
    "#00FF00".to_string()
}

/// Default text color for context menu Quit/CloseApp action item.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_quit_color() -> String {
    "#00FF00".to_string()
}

fn default_border_radius() -> u32 { 0 }
fn default_hover_border_thickness() -> u32 { 2 }
fn default_dash_thickness() -> u32 { 2 }
fn default_dash_segment_length() -> u32 { 4 }
fn default_dash_gap_length() -> u32 { 4 }
fn default_popup_text_padding() -> u32 { 12 }
fn default_popup_row_padding() -> u32 { 3 }
fn default_popup_text_truncate() -> u32 { 22 }
fn default_popup_frame_border() -> u32 { 4 }
fn default_popup_close_button_margin() -> u32 { 8 }
fn default_popup_dash_gap() -> u32 { 4 }
fn default_context_menu_width() -> u32 { 144 }
fn default_context_menu_row_height() -> u32 { 29 }
pub fn default_popup_fade_in() -> f32 { 0.15 }
fn default_context_menu_fade_in() -> f32 { 0.25 }
fn default_popup_timeout() -> u64 { 5000 }
fn default_context_menu_timeout() -> u64 { 1000 }
fn default_hard_timeout() -> u64 { 10000 }
/// Default text color for icon badge notification numbers.
/// Green (#00FF00) provides high contrast against the red badge background.
fn default_badge_text_color() -> String { "#00FF00".to_string() }
fn default_badge_bg_color() -> String { "#DC2626".to_string() }
fn default_badge_font_size() -> u32 { 10 }
fn default_pipewire_scale_factor() -> f32 { 0.5 }

// === Date/time block widget defaults (right side of dock) ===

/// Default: true — the date/time block widget is enabled by default.
fn default_date_time_block_enabled() -> bool { true }
/// Default: 104px — wide enough for time and date text.
fn default_date_time_block_width() -> u32 { 104 }
/// Default: 62px — accommodates time, day+month, and year.
fn default_date_time_block_height() -> u32 { 62 }
/// Default: #00FF00 (green) — matches the dock's text color scheme.
fn default_date_time_block_border_color() -> String { "#00FF00".to_string() }
/// Default: 2px — visible border without overwhelming the content.
fn default_date_time_block_border_thickness() -> u32 { 2 }
/// Default: 6px — each dash segment is 6 pixels long.
fn default_date_time_block_dash_length() -> u32 { 6 }
/// Default: 4px — gap between dash segments.
fn default_date_time_block_gap_length() -> u32 { 4 }
/// Default: #007AFF (blue) — used for time, date, and year text.
fn default_date_time_block_text_color() -> String { "#007AFF".to_string() }
/// Default: 32pt — large, prominent time display (HH:MM).
fn default_date_time_block_time_font_size() -> u32 { 32 }
/// Default: 25px — time text below the dashed border.
fn default_date_time_block_time_text_y() -> u32 { 25 }
/// Default: 16pt — half the time font size for date lines.
fn default_date_time_block_date_font_size() -> u32 { 16 }
/// Default: 43px — day+month text below the time.
fn default_date_time_block_day_month_y() -> u32 { 43 }
/// Default: 59px — year text below the day+month.
fn default_date_time_block_year_y() -> u32 { 59 }
/// Default: 4px — gap from the dock's right edge to the block.
fn default_date_time_block_gap_from_dock() -> u32 { 4 }

// === Dashed block widget defaults (icons and keyboard layout, left side of dock) ===

/// Default: true — the dashed block widget is enabled by default.
fn default_dashed_block_enabled() -> bool { true }
/// Default: 74px — wide enough for 2×2 icon grid and ENG text.
fn default_dashed_block_width() -> u32 { 74 }
/// Default: 48px — accommodates 2×2 icon grid and keyboard layout text.
fn default_dashed_block_height() -> u32 { 48 }
/// Default: #00FF00 (green) — matches the dock's text color scheme.
fn default_dashed_block_border_color() -> String { "#00FF00".to_string() }
/// Default: 2px — visible border without overwhelming the content.
fn default_dashed_block_border_thickness() -> u32 { 2 }
/// Default: 6px — each dash segment is 6 pixels long.
fn default_dashed_block_dash_length() -> u32 { 6 }
/// Default: 4px — gap between dash segments.
fn default_dashed_block_gap_length() -> u32 { 4 }
/// Default: #007AFF (blue) — used for keyboard layout text.
fn default_dashed_block_text_color() -> String { "#007AFF".to_string() }
/// Default: 16px — each status icon is 16×16 pixels.
fn default_dashed_block_icon_size() -> u32 { 16 }
/// Default: 4px — horizontal gap between icons in a row.
fn default_dashed_block_icon_gap() -> u32 { 4 }
/// Default: 4px — vertical gap between icon rows.
fn default_dashed_block_row_gap() -> u32 { 4 }
/// Default: 4px — top icon row near the top of the block.
fn default_dashed_block_top_row_y() -> u32 { 4 }
/// Default: 16px — icon rows offset 16px to the right from center.
fn default_dashed_block_row_offset() -> u32 { 16 }
/// Default: 12pt — smaller font for keyboard layout indicator.
fn default_dashed_block_kb_font_size() -> u32 { 12 }
/// Default: "ENG" — placeholder keyboard layout text.
fn default_dashed_block_kb_layout() -> String { "ENG".to_string() }
/// Default: 4px — gap from the dock's left edge to the block.
fn default_dashed_block_gap_from_dock() -> u32 { 4 }

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            // Dock layout
            anchor: DockAnchor::default(),
            dock_height: default_dock_height(),
            icon_size: default_icon_size(),
            spacing: default_spacing(),
            margin: default_margin(),
            // Dock appearance
            background_color: default_background_color(),
            background_alpha: default_background_alpha(),
            border_color: default_border_color(),
            border_alpha: default_border_alpha(),
            border_radius: default_border_radius(),
            // Dock hover
            hover_highlight: default_hover_highlight(),
            hover_color: default_hover_color(),
            hover_border_thickness: default_hover_border_thickness(),
            // Dock activity dashes
            dash_focused_color: default_dash_focused_color(),
            dash_inactive_color: default_dash_inactive_color(),
            dash_thickness: default_dash_thickness(),
            dash_segment_length: default_dash_segment_length(),
            dash_gap_length: default_dash_gap_length(),
            // PipeWire capture
            pipewire_scale_factor: default_pipewire_scale_factor(),
            // Click animation
            click_animation: default_click_animation(),
            click_animation_scale: default_click_animation_scale(),
            click_animation_up_duration: default_click_animation_up_duration(),
            click_animation_down_duration: default_click_animation_down_duration(),
            // Bounce animation
            bounce_animation: default_bounce_animation(),
            bounce_animation_scale: default_bounce_animation_scale(),
            bounce_animation_duration: default_bounce_animation_duration(),
            minimize_animation_scale: default_minimize_animation_scale(),
            minimize_animation_duration: default_minimize_animation_duration(),
            // Popup window list
            popup_width: default_popup_width(),
            popup_row_height: default_popup_row_height(),
            popup_frame_height: default_popup_frame_height(),
            popup_font_size: default_popup_font_size(),
            popup_close_button_size: default_popup_close_button_size(),
            popup_text_padding: default_popup_text_padding(),
            popup_row_padding: default_popup_row_padding(),
            popup_text_truncate: default_popup_text_truncate(),
            popup_frame_border: default_popup_frame_border(),
            popup_close_button_margin: default_popup_close_button_margin(),
            // Popup colors
            popup_hover_border_color: default_popup_hover_border_color(),
            popup_close_hover_color: default_popup_close_hover_color(),
            popup_close_button_color: default_popup_close_button_color(),
            popup_close_button_hover_color: default_popup_close_button_hover_color(),
            popup_close_cross_color: default_popup_close_cross_color(),
            popup_frame_bg_color: default_popup_frame_bg_color(),
            popup_text_color: default_popup_text_color(),
            popup_border_style: PopupBorderStyle::default(),
            popup_border_color: default_popup_border_color(),
            popup_dash_gap: default_popup_dash_gap(),
            // Context menu
            context_menu_width: default_context_menu_width(),
            context_menu_row_height: default_context_menu_row_height(),
            popup_focus_color: default_popup_focus_color(),
            popup_unpin_color: default_popup_unpin_color(),
            popup_launch_color: default_popup_launch_color(),
            popup_quit_color: default_popup_quit_color(),
            // Fade/timing
            popup_fade_in: default_popup_fade_in(),
            context_menu_fade_in: default_context_menu_fade_in(),
            popup_timeout: default_popup_timeout(),
            context_menu_timeout: default_context_menu_timeout(),
            hard_timeout: default_hard_timeout(),
            // Icon badge
            badge_text_color: default_badge_text_color(),
            badge_bg_color: default_badge_bg_color(),
            badge_font_size: default_badge_font_size(),
            // Date/time block widget (right side of dock)
            date_time_block_enabled: default_date_time_block_enabled(),
            date_time_block_width: default_date_time_block_width(),
            date_time_block_height: default_date_time_block_height(),
            date_time_block_border_color: default_date_time_block_border_color(),
            date_time_block_border_thickness: default_date_time_block_border_thickness(),
            date_time_block_dash_length: default_date_time_block_dash_length(),
            date_time_block_gap_length: default_date_time_block_gap_length(),
            date_time_block_text_color: default_date_time_block_text_color(),
            date_time_block_time_font_size: default_date_time_block_time_font_size(),
            date_time_block_time_text_y: default_date_time_block_time_text_y(),
            date_time_block_date_font_size: default_date_time_block_date_font_size(),
            date_time_block_day_month_y: default_date_time_block_day_month_y(),
            date_time_block_year_y: default_date_time_block_year_y(),
            date_time_block_gap_from_dock: default_date_time_block_gap_from_dock(),
            // Dashed block widget (icons and keyboard layout, left side of dock)
            dashed_block_enabled: default_dashed_block_enabled(),
            dashed_block_width: default_dashed_block_width(),
            dashed_block_height: default_dashed_block_height(),
            dashed_block_border_color: default_dashed_block_border_color(),
            dashed_block_border_thickness: default_dashed_block_border_thickness(),
            dashed_block_dash_length: default_dashed_block_dash_length(),
            dashed_block_gap_length: default_dashed_block_gap_length(),
            dashed_block_text_color: default_dashed_block_text_color(),
            dashed_block_icon_size: default_dashed_block_icon_size(),
            dashed_block_icon_gap: default_dashed_block_icon_gap(),
            dashed_block_row_gap: default_dashed_block_row_gap(),
            dashed_block_top_row_y: default_dashed_block_top_row_y(),
            dashed_block_row_offset: default_dashed_block_row_offset(),
            dashed_block_kb_font_size: default_dashed_block_kb_font_size(),
            dashed_block_kb_layout: default_dashed_block_kb_layout(),
            dashed_block_gap_from_dock: default_dashed_block_gap_from_dock(),
        }
    }
}

impl DockConfig {
    /// Parse a hex color string like "#RRGGBB" or "RRGGBB" into (R, G, B) tuple.
    /// Returns None if the format is invalid.
    pub fn parse_hex_color_opt(hex: &str) -> Option<(u8, u8, u8)> {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            if let Ok(rgb) = u32::from_str_radix(hex, 16) {
                return Some((
                    ((rgb >> 16) & 0xFF) as u8,
                    ((rgb >> 8) & 0xFF) as u8,
                    (rgb & 0xFF) as u8,
                ));
            }
        }
        None
    }

    /// Parse a hex color string like "#RRGGBB" or "RRGGBB" into (R, G, B) tuple
    pub fn parse_hex_color(hex: &str) -> (u8, u8, u8) {
        Self::parse_hex_color_opt(hex).unwrap_or((255, 255, 255))
    }

    /// Parse a hex color string into BGRA array for canvas rendering [B, G, R, A]
    pub fn parse_hex_color_bgra(hex: &str) -> [u8; 4] {
        let (r, g, b) = Self::parse_hex_color(hex);
        [b, g, r, 255]
    }

    /// Validate a color field, returning the default value if invalid
    fn validate_color(value: &str, field: &str, default: &str) -> String {
        if Self::parse_hex_color_opt(value).is_some() {
            value.to_string()
        } else {
            eprintln!("[CONFIG] Invalid color \"{}\" for \"{}\", using default \"{}\"", value, field, default);
            default.to_string()
        }
    }

    /// Validate a size field, clamping to reasonable bounds
    fn validate_size(value: u32, field: &str, min: u32, max: u32, default: u32) -> u32 {
        if value == 0 && min > 0 {
            eprintln!("[CONFIG] Zero value for \"{}\", using default {}", field, default);
            return default;
        }
        if value > max {
            eprintln!("[CONFIG] \"{}\" value {} exceeds max {}, clamping", field, value, max);
            max
        } else {
            value
        }
    }

    /// Validate an alpha value, clamping to [0.0, 1.0]
    fn validate_alpha(value: f32, field: &str, default: f32) -> f32 {
        if value.is_nan() || value.is_infinite() {
            eprintln!("[CONFIG] Invalid alpha for \"{}\", using default {}", field, default);
            default
        } else if value < 0.0 {
            eprintln!("[CONFIG] \"{}\" alpha {} < 0.0, clamping to 0.0", field, value);
            0.0
        } else if value > 1.0 {
            eprintln!("[CONFIG] \"{}\" alpha {} > 1.0, clamping to 1.0", field, value);
            1.0
        } else {
            value
        }
    }

    /// Validate a float duration/scale value
    fn validate_positive_float(value: f32, field: &str, default: f32) -> f32 {
        if value.is_nan() || value.is_infinite() || value <= 0.0 {
            eprintln!("[CONFIG] Invalid \"{}\" value {}, using default {}", field, value, default);
            default
        } else {
            value
        }
    }

    /// Validate and fix all config values, returning a corrected config
    fn validate(self) -> Self {
        let default_cfg = Self::default();
        
        Self {
            // Dock layout
            anchor: self.anchor,
            dock_height: Self::validate_size(self.dock_height, "dock_height", 20, 200, default_cfg.dock_height),
            icon_size: Self::validate_size(self.icon_size, "icon_size", 16, 128, default_cfg.icon_size),
            spacing: self.spacing,
            margin: self.margin,
            
            // Dock appearance
            background_color: Self::validate_color(&self.background_color, "background_color", &default_cfg.background_color),
            background_alpha: Self::validate_alpha(self.background_alpha, "background_alpha", default_cfg.background_alpha),
            border_color: Self::validate_color(&self.border_color, "border_color", &default_cfg.border_color),
            border_alpha: Self::validate_alpha(self.border_alpha, "border_alpha", default_cfg.border_alpha),
            border_radius: self.border_radius,
            
            // Dock hover
            hover_highlight: self.hover_highlight,
            hover_color: Self::validate_color(&self.hover_color, "hover_color", &default_cfg.hover_color),
            hover_border_thickness: Self::validate_size(self.hover_border_thickness, "hover_border_thickness", 1, 10, default_cfg.hover_border_thickness),
            
            // Dock activity dashes
            dash_focused_color: Self::validate_color(&self.dash_focused_color, "dash_focused_color", &default_cfg.dash_focused_color),
            dash_inactive_color: Self::validate_color(&self.dash_inactive_color, "dash_inactive_color", &default_cfg.dash_inactive_color),
            dash_thickness: Self::validate_size(self.dash_thickness, "dash_thickness", 1, 10, default_cfg.dash_thickness),
            dash_segment_length: Self::validate_size(self.dash_segment_length, "dash_segment_length", 1, 50, default_cfg.dash_segment_length),
            dash_gap_length: Self::validate_size(self.dash_gap_length, "dash_gap_length", 1, 50, default_cfg.dash_gap_length),
            
            // Click animation
            click_animation: self.click_animation,
            click_animation_scale: Self::validate_positive_float(self.click_animation_scale, "click_animation_scale", default_cfg.click_animation_scale),
            click_animation_up_duration: Self::validate_positive_float(self.click_animation_up_duration, "click_animation_up_duration", default_cfg.click_animation_up_duration),
            click_animation_down_duration: Self::validate_positive_float(self.click_animation_down_duration, "click_animation_down_duration", default_cfg.click_animation_down_duration),
            
            // Bounce animation
            bounce_animation: self.bounce_animation,
            bounce_animation_scale: Self::validate_positive_float(self.bounce_animation_scale, "bounce_animation_scale", default_cfg.bounce_animation_scale),
            bounce_animation_duration: Self::validate_positive_float(self.bounce_animation_duration, "bounce_animation_duration", default_cfg.bounce_animation_duration),
            minimize_animation_scale: Self::validate_positive_float(self.minimize_animation_scale, "minimize_animation_scale", default_cfg.minimize_animation_scale),
            minimize_animation_duration: Self::validate_positive_float(self.minimize_animation_duration, "minimize_animation_duration", default_cfg.minimize_animation_duration),
            
            // Popup window list
            popup_width: Self::validate_size(self.popup_width, "popup_width", 100, 800, default_cfg.popup_width),
            popup_row_height: Self::validate_size(self.popup_row_height, "popup_row_height", 16, 80, default_cfg.popup_row_height),
            popup_frame_height: Self::validate_size(self.popup_frame_height, "popup_frame_height", 50, 500, default_cfg.popup_frame_height),
            popup_font_size: Self::validate_size(self.popup_font_size, "popup_font_size", 8, 32, default_cfg.popup_font_size),
            popup_close_button_size: Self::validate_size(self.popup_close_button_size, "popup_close_button_size", 8, 40, default_cfg.popup_close_button_size),
            popup_text_padding: self.popup_text_padding,
            popup_row_padding: self.popup_row_padding,
            popup_text_truncate: Self::validate_size(self.popup_text_truncate, "popup_text_truncate", 5, 100, default_cfg.popup_text_truncate),
            popup_frame_border: self.popup_frame_border,
            popup_close_button_margin: self.popup_close_button_margin,
            pipewire_scale_factor: self.pipewire_scale_factor.clamp(0.01, 1.0),
            
            // Popup colors
            popup_hover_border_color: Self::validate_color(&self.popup_hover_border_color, "popup_hover_border_color", &default_cfg.popup_hover_border_color),
            popup_close_hover_color: Self::validate_color(&self.popup_close_hover_color, "popup_close_hover_color", &default_cfg.popup_close_hover_color),
            popup_close_button_color: Self::validate_color(&self.popup_close_button_color, "popup_close_button_color", &default_cfg.popup_close_button_color),
            popup_close_button_hover_color: Self::validate_color(&self.popup_close_button_hover_color, "popup_close_button_hover_color", &default_cfg.popup_close_button_hover_color),
            popup_close_cross_color: Self::validate_color(&self.popup_close_cross_color, "popup_close_cross_color", &default_cfg.popup_close_cross_color),
            popup_frame_bg_color: Self::validate_color(&self.popup_frame_bg_color, "popup_frame_bg_color", &default_cfg.popup_frame_bg_color),
            popup_text_color: Self::validate_color(&self.popup_text_color, "popup_text_color", &default_cfg.popup_text_color),
            popup_border_style: self.popup_border_style,
            popup_border_color: Self::validate_color(&self.popup_border_color, "popup_border_color", &default_cfg.popup_border_color),
            popup_dash_gap: self.popup_dash_gap,
            
            // Context menu
            context_menu_width: Self::validate_size(self.context_menu_width, "context_menu_width", 80, 400, default_cfg.context_menu_width),
            context_menu_row_height: Self::validate_size(self.context_menu_row_height, "context_menu_row_height", 16, 60, default_cfg.context_menu_row_height),
            popup_focus_color: Self::validate_color(&self.popup_focus_color, "popup_focus_color", &default_cfg.popup_focus_color),
            popup_unpin_color: Self::validate_color(&self.popup_unpin_color, "popup_unpin_color", &default_cfg.popup_unpin_color),
            popup_launch_color: Self::validate_color(&self.popup_launch_color, "popup_launch_color", &default_cfg.popup_launch_color),
            popup_quit_color: Self::validate_color(&self.popup_quit_color, "popup_quit_color", &default_cfg.popup_quit_color),
            
            // Fade/timing
            popup_fade_in: Self::validate_positive_float(self.popup_fade_in, "popup_fade_in", default_cfg.popup_fade_in),
            context_menu_fade_in: Self::validate_positive_float(self.context_menu_fade_in, "context_menu_fade_in", default_cfg.context_menu_fade_in),
            popup_timeout: if self.popup_timeout == 0 {
                eprintln!("[CONFIG] Zero popup_timeout, using default {}", default_cfg.popup_timeout);
                default_cfg.popup_timeout
            } else {
                self.popup_timeout
            },
            context_menu_timeout: if self.context_menu_timeout == 0 {
                eprintln!("[CONFIG] Zero context_menu_timeout, using default {}", default_cfg.context_menu_timeout);
                default_cfg.context_menu_timeout
            } else {
                self.context_menu_timeout
            },
            hard_timeout: if self.hard_timeout == 0 {
                eprintln!("[CONFIG] Zero hard_timeout, using default {}", default_cfg.hard_timeout);
                default_cfg.hard_timeout
            } else {
                self.hard_timeout
            },
            
            // Icon badge
            badge_text_color: Self::validate_color(&self.badge_text_color, "badge_text_color", &default_cfg.badge_text_color),
            badge_bg_color: Self::validate_color(&self.badge_bg_color, "badge_bg_color", &default_cfg.badge_bg_color),
            badge_font_size: Self::validate_size(self.badge_font_size, "badge_font_size", 6, 24, default_cfg.badge_font_size),
            
            // Date/time block widget (right side of dock)
            date_time_block_enabled: self.date_time_block_enabled,
            date_time_block_width: Self::validate_size(self.date_time_block_width, "date_time_block_width", 48, 256, default_cfg.date_time_block_width),
            date_time_block_height: Self::validate_size(self.date_time_block_height, "date_time_block_height", 32, 256, default_cfg.date_time_block_height),
            date_time_block_border_color: Self::validate_color(&self.date_time_block_border_color, "date_time_block_border_color", &default_cfg.date_time_block_border_color),
            date_time_block_border_thickness: Self::validate_size(self.date_time_block_border_thickness, "date_time_block_border_thickness", 1, 8, default_cfg.date_time_block_border_thickness),
            date_time_block_dash_length: Self::validate_size(self.date_time_block_dash_length, "date_time_block_dash_length", 1, 20, default_cfg.date_time_block_dash_length),
            date_time_block_gap_length: Self::validate_size(self.date_time_block_gap_length, "date_time_block_gap_length", 1, 20, default_cfg.date_time_block_gap_length),
            date_time_block_text_color: Self::validate_color(&self.date_time_block_text_color, "date_time_block_text_color", &default_cfg.date_time_block_text_color),
            date_time_block_time_font_size: Self::validate_size(self.date_time_block_time_font_size, "date_time_block_time_font_size", 8, 64, default_cfg.date_time_block_time_font_size),
            date_time_block_time_text_y: self.date_time_block_time_text_y,
            date_time_block_date_font_size: Self::validate_size(self.date_time_block_date_font_size, "date_time_block_date_font_size", 8, 32, default_cfg.date_time_block_date_font_size),
            date_time_block_day_month_y: self.date_time_block_day_month_y,
            date_time_block_year_y: self.date_time_block_year_y,
            date_time_block_gap_from_dock: self.date_time_block_gap_from_dock,

            // Dashed block widget (icons and keyboard layout, left side of dock)
            dashed_block_enabled: self.dashed_block_enabled,
            dashed_block_width: Self::validate_size(self.dashed_block_width, "dashed_block_width", 32, 256, default_cfg.dashed_block_width),
            dashed_block_height: Self::validate_size(self.dashed_block_height, "dashed_block_height", 32, 256, default_cfg.dashed_block_height),
            dashed_block_border_color: Self::validate_color(&self.dashed_block_border_color, "dashed_block_border_color", &default_cfg.dashed_block_border_color),
            dashed_block_border_thickness: Self::validate_size(self.dashed_block_border_thickness, "dashed_block_border_thickness", 1, 8, default_cfg.dashed_block_border_thickness),
            dashed_block_dash_length: Self::validate_size(self.dashed_block_dash_length, "dashed_block_dash_length", 1, 20, default_cfg.dashed_block_dash_length),
            dashed_block_gap_length: Self::validate_size(self.dashed_block_gap_length, "dashed_block_gap_length", 1, 20, default_cfg.dashed_block_gap_length),
            dashed_block_text_color: Self::validate_color(&self.dashed_block_text_color, "dashed_block_text_color", &default_cfg.dashed_block_text_color),
            dashed_block_icon_size: Self::validate_size(self.dashed_block_icon_size, "dashed_block_icon_size", 8, 32, default_cfg.dashed_block_icon_size),
            dashed_block_icon_gap: self.dashed_block_icon_gap,
            dashed_block_row_gap: self.dashed_block_row_gap,
            dashed_block_top_row_y: self.dashed_block_top_row_y,
            dashed_block_row_offset: self.dashed_block_row_offset,
            dashed_block_kb_font_size: Self::validate_size(self.dashed_block_kb_font_size, "dashed_block_kb_font_size", 6, 24, default_cfg.dashed_block_kb_font_size),
            dashed_block_kb_layout: self.dashed_block_kb_layout,
            dashed_block_gap_from_dock: self.dashed_block_gap_from_dock,
        }
    }

    /// Load config from file, or return defaults if file doesn't exist
    pub fn load() -> Self {
        let path = get_config_path();
        if let Ok(data) = fs::read_to_string(&path) {
            match serde_json::from_str::<DockConfig>(&data) {
                Ok(config) => {
                    config.validate()
                }
                Err(e) => {
                    eprintln!("[CONFIG] Failed to parse config.json: {}", e);
                    eprintln!("[CONFIG] Using default configuration");
                    Self::default()
                }
            }
        } else {
            Self::default()
        }
    }

    /// Save config to file
    pub fn save(&self) {
        let dir = get_config_dir();
        let path = get_config_path();
        let _ = fs::create_dir_all(dir);
        if let Ok(data) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, data);
        } else {
        }
    }
}
