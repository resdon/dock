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
    /// PipeWire capture frame downscale factor (0.01–1.0, lower = smaller thumbnails).
    ///
    /// Controls the resolution of PipeWire window capture thumbnails in the popup grid.
    /// A value of 0.25 means a 1920×1080 window becomes a 480×270 thumbnail.
    /// Lower values reduce memory usage and rendering cost at the expense of quality.
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
    /// Popup "No capture" placeholder text color in hex format.
    /// Default: #00FF00 (green) — displayed when PipeWire capture has no frame for a window.
    /// Green provides high contrast against the dark frame background.
    #[serde(default = "default_popup_no_capture_color")]
    pub popup_no_capture_color: String,
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

/// Default color for popup "No capture" placeholder text.
/// Green (#00FF00) provides high contrast against dark backgrounds.
fn default_popup_no_capture_color() -> String {
    "#00FF00".to_string()
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
fn default_pipewire_scale_factor() -> f32 { 0.25 }

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
            popup_no_capture_color: default_popup_no_capture_color(),
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
            popup_no_capture_color: Self::validate_color(&self.popup_no_capture_color, "popup_no_capture_color", &default_cfg.popup_no_capture_color),
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
