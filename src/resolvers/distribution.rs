//! Linux distribution resolver.
//!
//! Detects the current distro for distro-specific behavior.

use std::collections::HashMap;
use std::path::Path;

use crate::models::system::{
    DistributionContext, DistributionProvider, ResolutionConfidence, ResolutionEvidence,
    ResolutionTrace,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct DistributionResolver;

impl DistributionResolver {
    pub fn resolve(
        &self,
        executable: Option<&Path>,
        env: &HashMap<String, String>,
    ) -> (DistributionContext, ResolutionConfidence) {
        let (result, trace) = self.resolve_with_trace(executable, env);
        (result, trace.confidence)
    }

    pub fn resolve_with_trace(
        &self,
        executable: Option<&Path>,
        env: &HashMap<String, String>,
    ) -> (DistributionContext, ResolutionTrace) {
        let path = executable
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut result = DistributionContext::default();
        let mut trace = ResolutionTrace::default();
        if let Some(id) = env
            .get("SteamAppId")
            .or_else(|| env.get("STEAM_COMPAT_APP_ID"))
        {
            result.provider = DistributionProvider::Steam;
            result.store_id = Some(id.clone());
            trace.strengthen(
                ResolutionConfidence::Exact,
                [
                    ResolutionEvidence::DistributionMarker,
                    ResolutionEvidence::StoreId,
                ],
            );
            return (result, trace);
        }
        if path.contains("steam") {
            result.provider = DistributionProvider::Steam;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DistributionMarker],
            );
            return (result, trace);
        }
        if path.contains("lutris") {
            result.provider = DistributionProvider::Lutris;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DistributionMarker],
            );
            return (result, trace);
        }
        if path.contains("heroic") || path.contains("epicgames") {
            result.provider = DistributionProvider::Heroic;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DistributionMarker],
            );
            return (result, trace);
        }
        if path.contains("gog") {
            result.provider = DistributionProvider::GOG;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DistributionMarker],
            );
            return (result, trace);
        }
        if path.contains("bottles") {
            result.provider = DistributionProvider::Bottles;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::DistributionMarker],
            );
            return (result, trace);
        }
        result.provider = DistributionProvider::System;
        trace.confidence = ResolutionConfidence::Heuristic;
        (result, trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn steam_trace_contains_store_evidence() {
        let mut env = HashMap::new();
        env.insert("SteamAppId".into(), "730".into());
        let (result, trace) = DistributionResolver.resolve_with_trace(None, &env);
        assert_eq!(result.provider, DistributionProvider::Steam);
        assert!(trace.has(ResolutionEvidence::StoreId));
        assert_eq!(trace.confidence, ResolutionConfidence::Exact);
    }

    #[test]
    fn steam_app_id_is_exact_evidence() {
        let mut env = HashMap::new();
        env.insert("SteamAppId".into(), "730".into());
        let (result, confidence) = DistributionResolver.resolve(None, &env);
        assert_eq!(result.provider, DistributionProvider::Steam);
        assert_eq!(result.store_id.as_deref(), Some("730"));
        assert_eq!(confidence, ResolutionConfidence::Exact);
    }
}
