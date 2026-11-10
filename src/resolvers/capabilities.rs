//! System capability resolver.
//!
//! Detects available features like PipeWire, Bluetooth, etc.

use crate::models::system::{ApplicationCapabilities, ApplicationRecord};

#[derive(Debug, Default, Clone, Copy)]
pub struct CapabilityResolver;

impl CapabilityResolver {
    pub fn resolve(&self, application: &mut ApplicationRecord) {
        let actions = &application.installation.desktop_actions;
        let has_desktop = application.installation.desktop_file.is_some();
        let has_exec = application
            .binary
            .path
            .as_ref()
            .map(|p| !p.as_os_str().is_empty())
            .unwrap_or(false)
            || application
                .binary
                .command_line
                .as_ref()
                .map(|c| !c.is_empty())
                .unwrap_or(false);

        application.capabilities.can_launch = has_desktop || has_exec || !actions.is_empty();
        application.capabilities.can_focus =
            application.status.running || !application.windows.is_empty();
        application.capabilities.can_close =
            application.status.running || !application.windows.is_empty();
        application.capabilities.can_pin = has_desktop || has_exec;
        application.capabilities.can_show_window_list = !application.windows.is_empty();
        application.capabilities.can_show_jump_list = !actions.is_empty();
        application.capabilities.can_open_settings = actions.iter().any(|a| {
            let n = a.to_lowercase();
            n.contains("settings") || n.contains("preferences")
        });
        application.capabilities.can_open_location = has_desktop;
        application.capabilities.can_show_telemetry =
            application.status.running || !application.processes.is_empty();
        application.capabilities.can_uninstall = false;
    }

    pub fn defaults() -> ApplicationCapabilities {
        ApplicationCapabilities::default()
    }
}
