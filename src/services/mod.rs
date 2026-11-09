//! Background service modules.
//!
//! - [`pipewire_capture`] — PipeWire window capture service for the dock's
//!   window list popup. Captures live video frames from all open Wayland
//!   windows via XDG Desktop Portal and provides RGBA thumbnails.
//! - [`resource_monitor`] — CPU and memory usage monitoring.

pub mod pipewire_capture;
pub mod resource_monitor;
