//! Icon staging pipeline.
//!
//! Manages the icon loading pipeline from request to cache.

use std::path::PathBuf;

use super::get_icon_path;
use crate::models::system::{ApplicationRecord, ResolutionEvidence};

#[derive(Debug, Default, Clone, Copy)]
pub struct IconResolver;

impl IconResolver {
    pub fn resolve(&self, application: &mut ApplicationRecord) {
        if application.identity.icon.is_some() {
            return;
        }

        let icon = get_icon_path(&application.identity.identifier).or_else(|| {
            let name = application.identity.name.as_str();
            if name.trim().is_empty() {
                None
            } else {
                get_icon_path(name)
            }
        });

        application.identity.icon = icon;
        if application.identity.icon.is_some() {
            application
                .resolution_trace
                .add(ResolutionEvidence::IconMetadata);
        }
    }

    pub fn resolve_path_or_name(&self, value: &str) -> Option<PathBuf> {
        if value.contains('/') {
            Some(PathBuf::from(value))
        } else {
            get_icon_path(value)
        }
    }
}
