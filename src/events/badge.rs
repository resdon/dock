//! Unity launcher badge events.

use crate::models::BadgeUpdate;

#[derive(Clone, Debug)]
pub enum BadgeEvent {
    Update(BadgeUpdate),
}

/// Canonical desktop/application identifier normalization used by listeners
/// and resolvers. Kept pure so it can be tested without an application state.
pub fn normalize_desktop_id(app_uri: &str) -> String {
    app_uri
        .strip_prefix("application://")
        .or_else(|| app_uri.rsplit('/').next())
        .unwrap_or(app_uri)
        .trim_end_matches(".desktop")
        .to_lowercase()
}
