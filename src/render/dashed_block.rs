//! Dashed block widget renderer (icons and keyboard layout).
//!
//! Draws a configurable block with dashed borders displaying status icons
//! (rclonetray, bluetooth, speaker, pac) and a keyboard layout indicator.
//!
//! # Layout
//!
//! The block is rectangular (default 64×48px) with dashed green borders on all four sides.
//! Content is arranged:
//!
//! 1. **Status icons** — 2×2 grid: rclonetray | bluetooth (top), speaker | pac (bottom)
//! 2. **Keyboard layout** (e.g., "ENG") — left-aligned with the icon grid, smaller font (12pt)
//!
//! # Color Format
//!
//! The canvas uses BGRA8888 pixel format. All color writes use the `write_bgra_color` and
//! `write_bgra_pixel` helpers from `color_check` to ensure correct channel order.
//!
//! # Configuration
//!
//! All visual parameters are controlled by `DockConfig` fields prefixed with `dashed_block_`.
//! See `config.rs` for the full list of configurable options.

use crate::config::DockConfig;
use crate::render::color_check::{draw_dashed_border, write_bgra_pixel};
use crate::render::font::FontManager;
use crate::render::text::draw_text;

/// Hit-test the Bluetooth icon inside the dashed block widget.
///
/// Returns `true` if `(x, y)` falls within the 16×16 Bluetooth icon area
/// (top row, right position). Uses the same config-driven layout as the renderer.
pub fn bluetooth_icon_at(x: f64, y: f64, config: &DockConfig) -> bool {
    let x = x.round() as usize;
    let y = y.round() as usize;
    let icon_size = config.dashed_block_icon_size as usize;
    let icon_gap = config.dashed_block_icon_gap as usize;
    let width = config.dashed_block_width as usize;
    let row_offset = config.dashed_block_row_offset as usize;
    let top_row_y = config.dashed_block_top_row_y as usize;

    let row_width = icon_size * 2 + icon_gap;
    let icons_start_x = (width - row_width) / 2;
    let row_start_x = icons_start_x + row_offset;
    let bt_icon_x = row_start_x; // bluetooth is the right icon in top row (index 1)
    let bt_x = bt_icon_x + icon_size + icon_gap;

    (bt_x..bt_x + icon_size).contains(&x) && (top_row_y..top_row_y + icon_size).contains(&y)
}

use resvg::tiny_skia::{Pixmap, Transform};
use resvg::usvg::{Options, Tree};

/// Logical width of the dashed block widget (from config, fallback default).
pub const DASHED_BLOCK_WIDTH: i32 = 64;
/// Logical height of the dashed block widget (from config, fallback default).
pub const DASHED_BLOCK_HEIGHT: i32 = 48;

/// Renders the dashed block into a BGRA8888 canvas.
///
/// Draws a dashed border around the block with a transparent interior.
/// The border color, dash pattern, and thickness are all configurable.
/// Inside the block, the status icons and keyboard layout indicator
/// are drawn using the configured colors, font sizes, and positions.
///
/// # Parameters
///
/// - `canvas` — mutable slice of BGRA8888 pixels (4 bytes per pixel)
/// - `width` — canvas width in pixels
/// - `height` — canvas height in pixels
/// - `scale` — scale factor used to scale the keyboard-layout text offset
/// - `alpha` — global alpha multiplier (0.0 = fully transparent, 1.0 = fully opaque)
/// - `font_manager` — font cache for glyph rendering
/// - `config` — dock configuration with all dashed_block_* settings
pub fn render_dashed_block(
    canvas: &mut [u8],
    width: u32,
    height: u32,
    scale: f64,
    alpha: f32,
    font_manager: &mut FontManager,
    config: &DockConfig,
) {
    if canvas.is_empty() || width == 0 || height == 0 {
        return;
    }

    let w = width as usize;
    let h = height as usize;
    let a = (alpha * 255.0) as u8;

    // === Dashed border parameters (from config) ===
    // Border color parsed from hex string (e.g., "#00FF00" → (0, 255, 0) as (R, G, B) tuple)
    let border_color = DockConfig::parse_hex_color(&config.dashed_block_border_color);
    let dash_len = config.dashed_block_dash_length as usize; // pixels per dash segment
    let gap_len = config.dashed_block_gap_length as usize; // pixels per gap segment
    let thickness = config.dashed_block_border_thickness as usize; // border thickness in pixels

    draw_dashed_border(canvas, w, h, dash_len, gap_len, thickness, border_color, a);

    // === Text color for keyboard layout (from config)
    let text_color = DockConfig::parse_hex_color(&config.dashed_block_text_color);

    // === Draw status icons in a 2×2 grid ===
    // Top row: rclonetray | bluetooth
    // Bottom row: speaker | pac
    let icon_size: usize = config.dashed_block_icon_size as usize;
    let icon_gap: usize = config.dashed_block_icon_gap as usize; // Gap between icons in a row
    let row_gap: usize = config.dashed_block_row_gap as usize; // Gap between rows
    let row_width = icon_size * 2 + icon_gap; // Width of one row (2 icons + 1 gap)
    let icons_start_x: usize = ((width as usize) - row_width) / 2; // Centered horizontally
    let top_row_y: usize = config.dashed_block_top_row_y as usize;
    let bottom_row_y: usize = top_row_y + icon_size + row_gap;

    // Embedded SVG data for status icons.
    // SVGs are embedded as byte strings to avoid runtime file I/O when the dock runs
    // from a different working directory. Each SVG is rendered via resvg at the configured icon size.

    // rclonetray.svg — RClone tray icon with original colors (blue base #00539e, white robot shape, highlight)
    let rclone_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" version=\"1\"> <path style=\"opacity:0.2\" d=\"M 13.510187,61.965337 C 11.877496,61.753478 10.483172,61.144449 9.1205293,60.047988 8.6578137,59.675642 7.6533182,58.64678 7.2416547,58.123533 3.6910422,53.610452 2.1856255,46.856903 3.4319546,39.976102 4.3610538,34.846727 6.4435922,30.558191 9.5092908,28.497641 L 10.05745,28.12921 10.08993,27.749846 c 0.219664,-2.564629 0.607302,-4.236204 1.414434,-6.099279 0.619583,-1.430151 1.355234,-2.537863 2.357104,-3.549238 1.283433,-1.295604 2.587093,-1.935054 4.107241,-2.01463 1.516218,-0.07931 3.363565,0.549666 4.615404,1.754442 0.320173,0.308125 0.435689,0.319783 0.489756,0.04945 0.06017,-0.300614 0.610463,-1.767448 1.013311,-2.70077 1.641391,-3.802857 4.009675,-7.5309082 6.759244,-9.3605182 0.276024,-0.183657 0.840902,-0.505412 1.255276,-0.714995 2.58457,-1.3072185 5.368807,-1.467574 8.055393,-0.463913 4.952374,1.850084 9.343398,7.7523232 11.039524,15.3416672 0.620976,2.778533 0.836811,4.791148 0.944511,8.807198 l 0.01305,0.475948 0.453204,0.1247 c 4.401765,1.210881 7.562255,6.657748 8.262787,13.634321 0.523279,5.211271 -0.551755,10.42898 -2.926158,14.202257 -1.390581,2.209829 -3.309213,3.83931 -5.232557,4.443946 -1.102234,0.346492 0.289783,0.324843 -20.275168,0.315534 -10.31573,-0.0044 -18.832361,-0.01842 -18.925837,-0.0306 z\"/> <path style=\"fill:#00539e\" d=\"M 13.510187,60.965376 C 11.877496,60.753517 10.483172,60.144488 9.1205293,59.048027 8.6578137,58.675681 7.6533182,57.646819 7.2416547,57.123572 3.6910422,52.610491 2.1856255,45.856942 3.4319546,38.976141 4.3610538,33.846766 6.4435922,29.55823 9.5092908,27.49768 l 0.5481592,-0.368431 0.03248,-0.379364 c 0.219664,-2.564629 0.607302,-4.236204 1.414434,-6.099279 0.619583,-1.430151 1.355234,-2.537863 2.357104,-3.549238 1.283433,-1.295604 2.587093,-1.935054 4.107241,-2.01463 1.516218,-0.07931 3.363565,0.549666 4.615404,1.754442 0.320173,0.308125 0.435689,0.319783 0.489756,0.04945 0.06017,-0.300614 0.610463,-1.767448 1.013311,-2.70077 1.641391,-3.802858 4.009675,-7.5309085 6.759244,-9.3605185 0.276024,-0.183657 0.840902,-0.505412 1.255276,-0.714995 2.58457,-1.3072185 5.368807,-1.467574 8.055393,-0.463913 4.952374,1.850084 9.343398,7.7523235 11.039524,15.3416675 0.620976,2.778533 0.836811,4.791148 0.944511,8.807198 l 0.01305,0.475948 0.453204,0.1247 c 4.401765,1.210881 7.562255,6.657748 8.262787,13.634321 0.523279,5.211271 -0.551755,10.42898 -2.926158,14.202257 -1.390581,2.209829 -3.309213,3.83931 -5.232557,4.443946 -1.102234,0.346492 0.289783,0.324843 -20.275168,0.315534 -10.31573,-0.0044 -18.832361,-0.01842 -18.925837,-0.0306 z\"/> <path style=\"opacity:0.2\" d=\"m 27.521484,22.003867 c -3.037339,0.03956 -6.144588,0.330679 -8.802734,0.867188 -0.689384,0.13912 -1.352943,0.286683 -1.472656,0.326172 L 17.029297,23.269492 17.013672,39.60543 17,55.945273 l 2.703125,-0.01758 3.384766,-0.01367 0.01758,-3.896484 c 0.01007,-2.746736 -0.01492,-3.110485 0.0293,-3.164062 0.03447,-0.04185 0.119078,-0.0655 0.185546,-0.05078 0.06647,0.01473 0.616568,0.07169 1.394532,0.126954 1.762307,0.125123 5.845288,0.141529 7.177734,0.02734 1.91308,-0.163832 3.019549,-0.412368 4.466797,-0.771484 l 0.779297,-0.193359 0.136719,0.177734 c 0.457779,0.607308 1.360202,2.244937 1.787109,3.339844 0.584675,1.49946 0.953703,2.600756 0.955078,3.953125 2.75e-4,0.253308 0.0182,0.47715 0.03906,0.498047 0.02071,0.02091 1.797918,0.03906 3.33789,0.03906 h 2.800781 l -0.03516,-0.669922 c -0.154338,-2.84079 -1.420474,-6.02127 -3.05664,-8.818359 -0.237296,-0.405648 -0.431641,-0.762594 -0.431641,-0.792969 0,-0.03032 0.145279,-0.160199 0.324219,-0.289062 1.967687,-1.417144 3.846665,-4.673909 4.410156,-7.177735 0.565588,-2.5132 0.898323,-5.462023 0.197266,-7.490234 -1.048768,-3.034124 -3.766671,-5.347515 -7.121094,-6.742188 -1.455995,-0.605366 -2.911992,-1.040807 -4.585938,-1.373047 -2.372982,-0.470984 -5.337661,-0.682137 -8.375,-0.642578 z m 2.847657,5.996094 c 0.707092,0.0021 1.371497,0.02824 1.960937,0.07813 3.726869,0.315369 6.127899,1.139443 7.75586,2.660156 1.366787,1.276728 2.006992,2.927371 1.904296,4.917969 -0.06134,1.189556 -0.250918,1.931561 -0.71875,2.826172 -0.70569,1.349437 -1.856527,2.376703 -3.472656,3.101562 -1.347077,0.604195 -3.182199,1.058972 -5.103516,1.263672 -1.359784,0.144863 -5.167836,0.202548 -6.55664,0.09961 -1.440245,-0.10675 -3.005091,-0.342018 -3.087891,-0.464844 -0.02736,-0.04053 -0.05036,-3.040798 -0.05078,-6.861328 -7.63e-4,-6.102725 0.0067,-6.796034 0.08398,-6.869141 0.04766,-0.04476 0.104426,-0.08008 0.125,-0.08008 0.02059,0 0.33759,-0.05613 0.701172,-0.125 1.853152,-0.350746 4.337702,-0.552986 6.458985,-0.546875 z\"/> <path style=\"fill:#ffffff\" d=\"m 27.521484,21.003906 c -3.037339,0.03956 -6.144588,0.330679 -8.802734,0.867188 -0.689384,0.13912 -1.352943,0.286683 -1.472656,0.326172 l -0.216797,0.07227 -0.01563,16.335938 L 17,54.945312 l 2.703125,-0.01758 3.384766,-0.01367 0.01758,-3.896484 c 0.01007,-2.746736 -0.01492,-3.110485 0.0293,-3.164062 0.03447,-0.04185 0.119078,-0.0655 0.185546,-0.05078 0.06647,0.01473 0.616568,0.07169 1.394532,0.126954 1.762307,0.125123 5.845288,0.141529 7.177734,0.02734 1.91308,-0.163832 3.019549,-0.412368 4.466797,-0.771484 l 0.779297,-0.193359 0.136719,0.177734 c 0.457779,0.607308 1.360202,2.244937 1.787109,3.339844 0.584675,1.49946 0.953703,2.600756 0.955078,3.953125 2.75e-4,0.253308 0.0182,0.47715 0.03906,0.498047 C 40.077351,54.981848 41.854559,55 43.394531,55 h 2.800781 l -0.03516,-0.669922 c -0.154338,-2.84079 -1.420474,-6.02127 -3.05664,-8.818359 -0.237296,-0.405648 -0.431641,-0.762594 -0.431641,-0.792969 0,-0.03032 0.145279,-0.160199 0.324219,-0.289062 1.967687,-1.417144 3.846665,-4.673909 4.410156,-7.177735 0.565588,-2.5132 0.898323,-5.462023 0.197266,-7.490234 -1.048768,-3.034124 -3.766671,-5.347515 -7.121094,-6.742188 -1.455995,-0.605366 -2.911992,-1.040807 -4.585938,-1.373047 -2.372982,-0.470984 -5.337661,-0.682137 -8.375,-0.642578 z M 30.369141,27 c 0.707092,0.0021 1.371497,0.02823 1.960937,0.07813 3.726869,0.315369 6.127899,1.139443 7.75586,2.660156 1.366787,1.276728 2.006992,2.927371 1.904296,4.917969 -0.06134,1.189556 -0.250918,1.931561 -0.71875,2.826172 -0.70569,1.349437 -1.856527,2.376703 -3.472656,3.101562 -1.347077,0.604195 -3.182199,1.058972 -5.103516,1.263672 -1.359784,0.144863 -5.167836,0.202548 -6.55664,0.09961 -1.440245,-0.10675 -3.005091,-0.342018 -3.087891,-0.464844 -0.02736,-0.04053 -0.05036,-3.040798 -0.05078,-6.861328 -7.63e-4,-6.102725 0.0067,-6.796034 0.08398,-6.869141 0.04766,-0.04476 0.104426,-0.08008 0.125,-0.08008 0.02059,0 0.33759,-0.05613 0.701172,-0.125 C 25.763308,27.196129 28.247858,26.993889 30.369141,27 Z\"/> <path style=\"opacity:0.1;fill:#ffffff\" d=\"m 37.109375,3.0117191 c -1.698759,-0.083163 -3.392456,0.286504 -5.007813,1.1035156 -0.414374,0.209583 -0.979835,0.5311867 -1.255859,0.7148437 -2.749569,1.82961 -5.116421,5.5565166 -6.757812,9.3593746 -0.402848,0.933322 -0.953502,2.400558 -1.013672,2.701172 -0.05407,0.270333 -0.170062,0.259297 -0.490235,-0.04883 -1.251839,-1.204776 -3.099016,-1.835169 -4.615234,-1.755859 -1.520148,0.07958 -2.823989,0.72002 -4.107422,2.015624 -1.00187,1.011376 -1.737839,2.118678 -2.357422,3.548829 -0.807132,1.863075 -1.194398,3.53498 -1.414062,6.099609 l -0.0332,0.378906 -0.5468754,0.369141 C 6.444067,29.558597 4.3607398,33.847188 3.4316406,38.976562 3.1061192,40.773719 2.9724432,42.560591 3.0097656,44.302734 3.033871,42.87886 3.1680493,41.431812 3.4316406,39.976562 4.3607398,34.847188 6.444067,30.558597 9.5097656,28.498047 L 10.056641,28.128906 10.089844,27.75 c 0.219664,-2.564629 0.60693,-4.236534 1.414062,-6.099609 0.619583,-1.430151 1.355552,-2.537453 2.357422,-3.548829 1.283433,-1.295604 2.587274,-1.936049 4.107422,-2.015624 1.516218,-0.07931 3.363395,0.551083 4.615234,1.755859 0.320173,0.308125 0.436168,0.319161 0.490235,0.04883 0.06017,-0.300614 0.610824,-1.76785 1.013672,-2.701172 1.641391,-3.802857 4.008243,-7.5297646 6.757812,-9.3593746 0.276024,-0.183657 0.841485,-0.5052607 1.255859,-0.7148437 2.58457,-1.3072185 5.368102,-1.4685048 8.054688,-0.4648438 4.952374,1.850084 9.34489,7.7524531 11.041016,15.3417971 0.620976,2.778532 0.835659,4.79059 0.943359,8.80664 l 0.01367,0.476563 0.453125,0.125 c 4.401765,1.210881 7.56314,6.658192 8.263672,13.634765 0.05958,0.593356 0.09079,1.185962 0.109375,1.777344 0.0067,-0.921639 -0.01604,-1.847883 -0.109375,-2.777344 -0.700532,-6.976573 -3.861907,-12.423884 -8.263672,-13.634765 l -0.453125,-0.125 -0.01367,-0.476563 c -0.1077,-4.01605 -0.322383,-6.028108 -0.943359,-8.80664 C 49.50114,11.402844 45.108624,5.500475 40.15625,3.6503909 39.14878,3.2740181 38.12863,3.0616169 37.109375,3.0117191 Z\"/> </svg>";

    // bluetooth.svg — Bluetooth icon with original colors (dark blue background #324090, white stroke path)
    let bt_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"22\" height=\"22\" version=\"1\"><g><rect style=\"fill:#324090\" width=\"20\" height=\"14\" x=\"-21\" y=\"-18\" rx=\"7\" ry=\"7\" transform=\"matrix(0,-1,-1,0,0,0)\"/><path style=\"fill:none;stroke:#ffffff;stroke-linecap:square\" d=\"m 7.7066,7.7929 7,6.5 -3.2066,3.5 0,-13.5 3.2066,3.5 -7,6.5\"/><path style=\"opacity:0.1;fill:#ffffff\" d=\"M 11,1 C 7.122,1 4,4.122 4,8 l 0,0.5 c 0,-3.878 3.122,-7 7,-7 3.878,0 7,3.122 7,7 L 18,8 C 18,4.122 14.878,1 11,1 Z\"/><path style=\"opacity:0.2\" d=\"m 4,14 0,0.5 c 0,3.878 3.122,7 7,7 3.878,0 7,-3.122 7,-7 L 18,14 c 0,3.878 -3.122,7 -7,7 -3.878,0 -7,-3.122 -7,-7 z\"/></g></svg>";

    // speaker-high.svg — Speaker volume icon with original colors (blue stroke #3b82f6, no fill)
    let speaker_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\" fill=\"none\" stroke=\"#3b82f6\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\"> <polygon points=\"11 5 6 9 2 9 2 15 6 15 11 19 11 5\"/> <path d=\"M15.5 8.5a5 5 0 0 1 0 7\"/> <path d=\"M19.5 4.5a9 9 0 0 1 0 15\"/> </svg>";

    // pac.svg — Pacman package manager icon with original colors (blue base #2c80c2, yellow accents #f8c44e)
    let pac_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\" version=\"1\"> <path style=\"opacity:0.2\" d=\"M 20,57 16,53 H 8 C 5.784,53 4,51.216 4,49 V 13 C 4,10.784 5.784,9 8,9 H 56 C 58.216,9 60,10.784 60,13 V 49 C 60,51.216 58.216,53 56,53 H 48 L 44,57 Z\"/> <path style=\"fill:#2c80c2\" d=\"M 20,56 16,52 H 8 C 5.784,52 4,50.216 4,48 V 12 C 4,9.784 5.784,8 8,8 H 56 C 58.216,8 60,9.784 60,12 V 48 C 60,50.216 58.216,52 56,52 H 48 L 44,56 Z\"/> <path style=\"opacity:0.5\" d=\"M 24,48 20,44 H 12 V 16 H 52 V 44 H 44 L 40,48 Z\"/> <g style=\"fill:#f8c44e\" transform=\"scale(1,-1)\"> <rect width=\"4\" height=\"4\" x=\"18\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"24\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"30\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"36\" y=\"-20\"/> <rect width=\"4\" height=\"4\" x=\"42\" y=\"-20\"/> </g> <path style=\"fill:#ffffff;opacity:0.2\" d=\"M 8 8 C 5.784 8 4 9.784 4 12 L 4 13 C 4 10.784 5.784 9 8 9 L 56 9 C 58.216 9 60 10.784 60 13 L 60 12 C 60 9.784 58.216 8 56 8 L 8 8 z\"/> <path style=\"opacity:0.2;fill:#ffffff\" d=\"M 12,44 V 45 H 20 L 24,49 H 40 L 44,45 H 52 V 44 H 44 L 40,48 H 24 L 20,44 Z\"/> </svg>";

    let options = Options::default();

    // Helper closure to render an SVG icon onto the canvas.
    // Uses `write_bgra_pixel` to ensure correct BGRA8888 channel order.
    // Resvg's Pixmap outputs RGBA; `write_bgra_pixel` handles the R↔B swap.
    //
    // The icon is scaled to fit the configured `icon_size` and blended with the global alpha.
    let render_svg_icon = |svg_data: &[u8],
                           icon_x: usize,
                           icon_y: usize,
                           icon_size: usize,
                           w: usize,
                           h: usize,
                           canvas: &mut [u8],
                           a: u8,
                           options: &Options| {
        if let Ok(tree) = Tree::from_data(svg_data, options) {
            let mut pixmap = Pixmap::new(icon_size as u32, icon_size as u32).unwrap();
            let transform = Transform::from_scale(
                icon_size as f32 / tree.size().width(),
                icon_size as f32 / tree.size().height(),
            );
            resvg::render(&tree, transform, &mut pixmap.as_mut());

            let svg_pixels = pixmap.data();
            for row in 0..icon_size {
                for col in 0..icon_size {
                    let px = icon_x + col;
                    let py = icon_y + row;
                    if px < w && py < h {
                        let svg_idx = (row * icon_size + col) * 4;
                        let svg_a = svg_pixels[svg_idx + 3];
                        if svg_a > 0 {
                            let canvas_idx = (py * w + px) * 4;
                            let final_a = (svg_a as f32 / 255.0 * a as f32) as u8;
                            // resvg pixmap is RGBA [R,G,B,A]; write_bgra_pixel writes BGRA8888
                            write_bgra_pixel(
                                canvas,
                                canvas_idx,
                                svg_pixels[svg_idx],     // R
                                svg_pixels[svg_idx + 1], // G
                                svg_pixels[svg_idx + 2], // B
                                final_a,
                            ); // A
                        }
                    }
                }
            }
        }
    };

    // Both icon rows aligned to the same x offset (from config)
    let row_offset: usize = config.dashed_block_row_offset as usize;
    let row_start_x = icons_start_x + row_offset;

    // Top row: rclonetray | bluetooth
    let rclone_icon_x = row_start_x;
    let bt_icon_x = row_start_x + icon_size + icon_gap;

    // Bottom row: speaker | pac
    let speaker_icon_x = row_start_x;
    let pac_icon_x = row_start_x + icon_size + icon_gap;

    // === Keyboard layout indicator text to the left of the bottom row icons ===
    // The text (e.g., "ENG") is left-aligned with the icon grid's left edge,
    // vertically centered with the bottom row of icons.
    let kb_layout = &config.dashed_block_kb_layout;
    let kb_font_size = config.dashed_block_kb_font_size as usize;
    let mut kb_text_width = 0i32;
    for c in kb_layout.chars() {
        if c == '\u{200b}' || c.is_control() {
            continue;
        }
        let glyph = font_manager.get_glyph(c, kb_font_size as f32);
        // FIX (v1.5.5): Changed from glyph.metrics.advance_width to glyph.advance_width.
        kb_text_width += glyph.advance_width.round() as i32;
    }
    // Position text to the left of the bottom row icons, vertically centered with the row
    let kb_text_x = row_start_x as i32 - kb_text_width - 4;
    let kb_text_y = bottom_row_y as i32
        + icon_size as i32 / 2
        + kb_font_size as i32 / 3
        + (config.dashed_block_kb_text_offset_y as f64 * scale).round() as i32;
    draw_text(
        canvas,
        width as i32,
        height as i32,
        font_manager,
        kb_layout,
        kb_font_size as i32,
        kb_text_x,
        kb_text_y,
        text_color,
    );

    // Render all 4 SVG icons onto the canvas
    render_svg_icon(
        rclone_svg,
        rclone_icon_x,
        top_row_y,
        icon_size,
        w,
        h,
        canvas,
        a,
        &options,
    );
    render_svg_icon(
        bt_svg, bt_icon_x, top_row_y, icon_size, w, h, canvas, a, &options,
    );
    render_svg_icon(
        speaker_svg,
        speaker_icon_x,
        bottom_row_y,
        icon_size,
        w,
        h,
        canvas,
        a,
        &options,
    );
    render_svg_icon(
        pac_svg,
        pac_icon_x,
        bottom_row_y,
        icon_size,
        w,
        h,
        canvas,
        a,
        &options,
    );
}
