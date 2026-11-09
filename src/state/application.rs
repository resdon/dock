//! Application state tracking.
//!
//! Tracks per-app state: running, pinned, badge count.

use std::time::Instant;

use crate::models::system::{
    AppId, ApplicationCapabilities, BinaryContext, DistributionContext, PackagingContext,
    PlatformContext, ResolutionConfidence, ResolutionTrace, RuntimeContext,
};
use crate::models::MediaWindowLink;

/// Live, UI-facing projection of application metadata and runtime state. The registry remains authoritative; this is a fast read model.
#[derive(Clone, Debug)]
pub struct ApplicationState {
    pub app_id: AppId,
    pub identifier: String,
    pub name: String,
    pub version: Option<String>,
    pub icon: Option<std::path::PathBuf>,
    pub category: Option<String>,
    pub binary: BinaryContext,
    pub running: bool,
    pub foreground: bool,
    pub background: bool,
    pub attention: bool,
    pub pending: bool,
    pub paused: bool,
    pub stuck: bool,
    pub locked: bool,
    pub media_active: bool,
    pub media: Option<MediaWindowLink>,
    pub capabilities: ApplicationCapabilities,
    pub platform: PlatformContext,
    pub runtime: RuntimeContext,
    pub packaging: PackagingContext,
    pub distribution: DistributionContext,
    pub process_count: usize,
    pub window_count: usize,
    pub last_activity: Option<Instant>,
    pub resolution_confidence: ResolutionConfidence,
    pub resolution_trace: ResolutionTrace,
}

impl ApplicationState {
    pub fn new(app_id: AppId) -> Self {
        Self {
            identifier: app_id.to_string(),
            name: app_id.to_string(),
            version: None,
            icon: None,
            category: None,
            binary: BinaryContext::default(),
            app_id,
            running: false,
            foreground: false,
            background: false,
            attention: false,
            pending: false,
            paused: false,
            stuck: false,
            locked: false,
            media_active: false,
            media: None,
            capabilities: ApplicationCapabilities::default(),
            platform: PlatformContext::default(),
            runtime: RuntimeContext::default(),
            packaging: PackagingContext::default(),
            distribution: DistributionContext::default(),
            process_count: 0,
            window_count: 0,
            last_activity: None,
            resolution_confidence: ResolutionConfidence::Unknown,
            resolution_trace: ResolutionTrace::default(),
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Some(Instant::now());
    }
}
