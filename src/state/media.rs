//! Media player state.
//!
//! Tracks media playback state linked to windows.

use crate::models::system::{AppId, WindowId};
use crate::models::MediaWindowLink;

#[derive(Clone, Debug, Default)]
pub struct MediaState {
    pub active_app: Option<AppId>,
    pub target_window: Option<WindowId>,
    pub media_link: Option<MediaWindowLink>,
    pub is_playing: bool,
    pub paused: bool,
    pub muted: bool,
    pub volume: Option<f32>,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
}

impl MediaState {
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
