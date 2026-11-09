//! Application state resolver.

use super::find_desktop_entry;
use crate::models::system::{
    AppId, ApplicationCapabilities, ApplicationRecord, DisplayServer, OperatingSystem,
    PlatformContext, ResolutionConfidence, ResolutionEvidence, ResolutionTrace,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct ApplicationResolver;

impl ApplicationResolver {
    pub fn resolve(
        &self,
        app_id: &AppId,
        title: &str,
    ) -> (ApplicationRecord, ResolutionConfidence) {
        let (record, trace) = self.resolve_with_trace(app_id, title);
        (record, trace.confidence)
    }

    pub fn resolve_with_trace(
        &self,
        app_id: &AppId,
        title: &str,
    ) -> (ApplicationRecord, ResolutionTrace) {
        let mut record = ApplicationRecord::new(app_id.clone());
        let mut trace = ResolutionTrace::new(
            ResolutionConfidence::Heuristic,
            [ResolutionEvidence::ExactAppId],
        );
        record.capabilities = ApplicationCapabilities::default();

        if let Some(desktop) = find_desktop_entry(app_id.as_str()) {
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DesktopEntry],
            );
            record.identity.identifier = desktop.desktop_id.clone();
            record.identity.name = desktop
                .name
                .clone()
                .or(desktop.generic_name.clone())
                .unwrap_or_else(|| app_id.to_string());
            record.identity.version = desktop.version.clone();
            record.identity.developer = desktop.developer.clone();
            record.identity.description = desktop.comment.clone();
            record.identity.category = desktop.categories.first().cloned();
            record.binary.command_line = desktop.exec.clone();
            record.launch.command = desktop.exec.clone();
            record.installation.desktop_file = Some(desktop.path.clone());
            record.installation.desktop_actions =
                desktop.actions.iter().map(|a| a.name.clone()).collect();
            record.capabilities.can_show_jump_list = !desktop.actions.is_empty();
            record.capabilities.can_open_settings = desktop.actions.iter().any(|a| {
                let n = a.name.to_lowercase();
                n.contains("settings") || n.contains("preferences")
            });
        } else if !title.trim().is_empty() {
            record.identity.name = title.to_string();
            trace.strengthen(
                ResolutionConfidence::Probable,
                [ResolutionEvidence::TitleHeuristic],
            );
        }

        record.platform = PlatformContext {
            os: OperatingSystem::Linux,
            display_server: DisplayServer::Wayland,
            architecture: Some(std::env::consts::ARCH.to_string()),
        };

        record.status.running = true;
        record.resolution_confidence = trace.confidence;
        record.resolution_trace = trace.clone();
        (record, trace)
    }
}
