//! Environment variable resolver.
//!
//! Reads and caches environment variables for app launching.

use std::collections::HashMap;
use std::path::Path;

use crate::models::system::{
    PackagingContext, PackagingKind, ResolutionConfidence, ResolutionEvidence, ResolutionTrace,
    RuntimeContext, RuntimeKind,
};

#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentResolver;

impl EnvironmentResolver {
    pub fn resolve(
        &self,
        executable: Option<&Path>,
        env: &HashMap<String, String>,
    ) -> (RuntimeContext, PackagingContext) {
        let (runtime, packaging, _) = self.resolve_with_trace(executable, env);
        (runtime, packaging)
    }

    pub fn resolve_with_trace(
        &self,
        executable: Option<&Path>,
        env: &HashMap<String, String>,
    ) -> (RuntimeContext, PackagingContext, ResolutionTrace) {
        let path = executable
            .map(|p| p.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let mut runtime = RuntimeContext::default();
        let mut packaging = PackagingContext::default();
        let mut trace = ResolutionTrace::default();
        if env.contains_key("STEAM_COMPAT_APP_ID")
            || env.contains_key("SteamAppId")
            || env.contains_key("STEAM_COMPAT_CLIENT_INSTALL_PATH")
            || path.contains("steam")
        {
            runtime.kind = if env.contains_key("STEAM_COMPAT_APP_ID")
                || env.contains_key("STEAM_COMPAT_CLIENT_INSTALL_PATH")
            {
                trace.strengthen(
                    ResolutionConfidence::Strong,
                    [ResolutionEvidence::RuntimeMarker],
                );
                RuntimeKind::Proton
            } else {
                trace.strengthen(
                    ResolutionConfidence::Probable,
                    [ResolutionEvidence::RuntimeMarker],
                );
                RuntimeKind::Native
            };
        }
        if env.contains_key("WINEPREFIX") && runtime.kind == RuntimeKind::Unknown {
            runtime.kind = RuntimeKind::Wine;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::RuntimeMarker],
            );
        }

        if path.contains("/.var/app/") || path.starts_with("/app/") {
            packaging.kind = PackagingKind::Flatpak;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::PackagingMarker],
            );
            if runtime.kind == RuntimeKind::Unknown {
                runtime.kind = RuntimeKind::Flatpak;
            }
        } else if path.contains("/snap/") {
            packaging.kind = PackagingKind::Snap;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::PackagingMarker],
            );
            if runtime.kind == RuntimeKind::Unknown {
                runtime.kind = RuntimeKind::Snap;
            }
        } else if path.ends_with(".appimage") {
            packaging.kind = PackagingKind::AppImage;
            trace.strengthen(
                ResolutionConfidence::Strong,
                [ResolutionEvidence::PackagingMarker],
            );
            if runtime.kind == RuntimeKind::Unknown {
                runtime.kind = RuntimeKind::AppImage;
            }
        } else {
            packaging.kind = PackagingKind::SystemPackage;
        }

        if runtime.kind == RuntimeKind::Unknown {
            runtime.kind = RuntimeKind::Native;
        }
        if packaging.kind == PackagingKind::SystemPackage
            && trace.confidence == ResolutionConfidence::Unknown
        {
            trace.confidence = ResolutionConfidence::Heuristic;
        }
        (runtime, packaging, trace)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;

    #[test]
    fn detects_flatpak() {
        let env = HashMap::new();
        let (runtime, packaging) =
            EnvironmentResolver.resolve(Some(Path::new("/app/bin/test")), &env);
        assert_eq!(packaging.kind, PackagingKind::Flatpak);
        assert_eq!(runtime.kind, RuntimeKind::Flatpak);
    }

    #[test]
    fn detects_proton() {
        let mut env = HashMap::new();
        env.insert("STEAM_COMPAT_APP_ID".into(), "123".into());
        let (runtime, _) = EnvironmentResolver.resolve(None, &env);
        assert_eq!(runtime.kind, RuntimeKind::Proton);
    }
}
