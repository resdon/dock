//! Media player state change events.

use crate::models::system::{AppId, WindowId};
use crate::models::MediaWindowLink;

#[derive(Clone, Debug)]
pub enum MediaEvent {
    LinkWindow {
        app_id: Option<AppId>,
        window_id: WindowId,
        link: MediaWindowLink,
    },
    ClearWindow {
        window_id: WindowId,
    },
}
