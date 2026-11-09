//! System capability resolver.
//!
//! Detects available features like PipeWire, Bluetooth, etc.

use crate::models::system::{ApplicationCapabilities, ApplicationRecord};

#[derive(Debug, Default, Clone, Copy)]
pub struct CapabilityResolver;

impl CapabilityResolver {
    pub fn resolve(&self, application: &mut ApplicationRecord) {
        let actions = &application.installation.desktop_actions;
        application.capabilities.can_launch = true;
        application.capabilities.can_focus = true;
        application.capabilities.can_close = true;
        application.capabilities.can_pin = true;
        application.capabilities.can_show_window_list = true;
        application.capabilities.can_show_jump_list = !actions.is_empty();
        application.capabilities.can_open_settings = actions.iter().any(|a| {
            let n = a.to_lowercase();
            n.contains("settings") || n.contains("preferences")
        });
        application.capabilities.can_open_location =
            application.installation.desktop_file.is_some();
        application.capabilities.can_show_telemetry = true;
        application.capabilities.can_uninstall = false;
    }

    pub fn defaults() -> ApplicationCapabilities {
        ApplicationCapabilities::default()
    }
}
