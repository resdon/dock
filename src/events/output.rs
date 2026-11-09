//! Wayland output (monitor) change events.

use crate::models::system::OutputId;

/// Domain-level output lifecycle events.
#[derive(Clone, Debug)]
pub enum OutputEvent {
    Seen {
        output: OutputId,
        name: Option<String>,
    },
    Updated {
        output: OutputId,
        name: Option<String>,
    },
    Destroyed {
        output: OutputId,
    },
}
