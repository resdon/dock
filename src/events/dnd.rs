//! Drag-and-drop domain events.

use crate::models::system::OfferId;

/// Domain-level drag-and-drop events. Wayland data offers themselves remain in
/// the protocol adapter; only stable IDs and transferred data cross the
/// listener boundary.
#[derive(Clone, Debug)]
pub enum DragDropEvent {
    DataOffer {
        offer_id: OfferId,
    },
    OfferMimeType {
        offer_id: OfferId,
        mime_type: String,
    },
    Enter {
        offer_id: Option<OfferId>,
        serial: u32,
        x: f64,
        y: f64,
    },
    Motion {
        x: f64,
        y: f64,
    },
    Leave,
    Drop {
        x: f64,
        y: f64,
        paths: Vec<std::path::PathBuf>,
    },
}
