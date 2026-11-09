//! Network monitor.
//!
//! Tracks network interface statistics for upload/download rate display.

/// Network rendering is not yet split out in v1.4.0.
/// This module exists so the draw coordinator can call it later without
/// changing the app layout again.
pub fn draw_network_overlay() {}
