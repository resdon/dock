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
        let id = application.identity.identifier.clone();
        application.identity.icon = get_icon_path(&id).or_else(|| {
            let name = application.identity.name.clone();
            if name.trim().is_empty() {
                None
            } else {
                get_icon_path(&name)
            }
        });
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
