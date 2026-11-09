//! Domain event definitions.
//!
//! Strongly-typed events for window, output, system, media, badge, and DnD changes.

pub mod badge;
pub mod dnd;
pub mod media;
pub mod output;
pub mod system;
pub mod utils;
pub mod window;

pub use badge::{normalize_desktop_id, BadgeEvent};
pub use dnd::DragDropEvent;
pub use media::MediaEvent;
pub use output::OutputEvent;
pub use system::SystemEvent;
pub use window::WindowEvent;

/// Unified domain event crossing the listener boundary.
///
/// This enum is deliberately data-only. It does not know about `AppState`,
/// Wayland callbacks, resolvers, registry mutation, or rendering.
#[derive(Clone, Debug)]
pub enum DomainEvent {
    Badge(BadgeEvent),
    DragDrop(DragDropEvent),
    Media(MediaEvent),
    Output(OutputEvent),
    System(SystemEvent),
    Window(WindowEvent),
}
