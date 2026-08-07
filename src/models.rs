// src/models.rs

use wayland_client::protocol::wl_output::WlOutput;
use wayland_protocols_wlr::foreign_toplevel::v1::client::zwlr_foreign_toplevel_handle_v1::ZwlrForeignToplevelHandleV1;

// --- D-Bus / Dock Badge Models ---

#[derive(Debug, Clone, Default)]
pub struct BadgeUpdate {
    pub desktop_id: String,
    pub count: i64,
    pub count_visible: bool,
    pub progress: f64,
    pub progress_visible: bool,
    pub urgent: bool,
}

#[derive(Debug, Clone)]
pub struct DockItem {
    pub name: String,
    pub desktop_id: String, // e.g., "discord", "thunderbird"
    pub badge_count: i64,
    pub show_badge: bool,
    pub progress: f64,
    pub show_progress: bool,
    pub is_urgent: bool,
}

impl DockItem {
    pub fn apply_badge_update(&mut self, update: &BadgeUpdate) {
        self.badge_count = update.count;
        self.show_badge = update.count_visible && update.count > 0;
        self.progress = update.progress.clamp(0.0, 1.0);
        self.show_progress = update.progress_visible;
        self.is_urgent = update.urgent;
    }
}

// --- Window Tracking Models ---

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum LastState {
    None,
    ReceivedFocus,
    ReportedInactive,
}

#[derive(Clone, Debug)]
pub struct WindowDiagnostics {
    pub id: u64,
    pub app_name: String,
    pub title: String,
    pub matched_pid: Option<u32>,
    pub icon_name: String,
    pub terminal_icon_code: String,
    pub app_id: String,
    pub is_activated: bool,
    pub is_minimized: bool,
    pub icon_rgba: Option<Vec<u8>>,
    pub icon_size: u32,
    pub handle: ZwlrForeignToplevelHandleV1,
    pub last_state: LastState,
    pub is_pending: bool,
    pub icon_resolved: bool,
    pub outputs: Vec<WlOutput>,
}

impl WindowDiagnostics {
    pub fn new(id: u64, handle: ZwlrForeignToplevelHandleV1) -> Self {
        Self {
            id,
            app_name: "Unknown".to_string(),
            title: "Unknown".to_string(),
            matched_pid: None,
            icon_name: "".to_string(),
            terminal_icon_code: "".to_string(),
            app_id: "".to_string(),
            is_activated: false,
            is_minimized: false,
            icon_rgba: None,
            icon_size: 48,
            handle,
            last_state: LastState::None,
            is_pending: false,
            icon_resolved: false,
            outputs: Vec::new(),
        }
    }
    /// Returns the active app_id, falling back to window title or "Unknown".
    pub fn resolved_app_id(&self) -> &str {
        if !self.app_id.is_empty() {
            &self.app_id
        } else if !self.title.is_empty() {
            &self.title
        } else {
            "Unknown"
        }
    }
}
