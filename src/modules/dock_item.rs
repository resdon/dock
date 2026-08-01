// src/dock_item.rs

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
    pub fn apply_badge_update(&mut self, update: &crate::modules::dbus_unity::BadgeUpdate) {
        self.badge_count = update.count;
        self.show_badge = update.count_visible && update.count > 0;
        self.progress = update.progress.clamp(0.0, 1.0);
        self.show_progress = update.progress_visible;
        self.is_urgent = update.urgent;
    }
}