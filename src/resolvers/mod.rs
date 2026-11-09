//! Resolution pipeline modules.
//!
//! Resolves app IDs, icons, desktop files, and window associations.

pub mod application;
pub mod capabilities;
pub mod context;
pub mod desktop;
pub mod distribution;
pub mod environment;
pub mod icon;
pub mod icon_list;
pub mod icon_stage;
pub mod pipeline;
pub mod process;
pub mod steam;
pub mod window;

pub use crate::models::system::ResolutionConfidence;
pub use application::ApplicationResolver;
pub use capabilities::CapabilityResolver;
pub use context::{ProcessSnapshot, ResolutionRequest, ResolverSnapshot, WindowHints};
pub use desktop::{
    clean_exec_field, find_desktop_entry, find_desktop_file_by_exec, find_desktop_file_by_name,
    get_desktop_actions, parse_desktop_actions, DesktopAction, DesktopEntryMetadata,
};
pub use distribution::DistributionResolver;
pub use environment::EnvironmentResolver;
pub use icon::{
    extract_icon_name, find_icon_by_name, find_icon_path, get_icon_from_desktop, get_icon_path,
};
pub use icon_list::{search_icon_list_file, spawn_startup_indexer};
pub use icon_stage::IconResolver;
pub use pipeline::{
    ApplicationResolverStage, CapabilityResolverStage, DistributionResolverStage,
    EnvironmentResolverStage, IconResolverStage, ProcessResolverStage, ResolutionAction,
    ResolverPipeline, ResolverStage, ResolverStages,
};
pub use process::ProcessResolver;
pub use steam::resolve_steam_game_details;
pub use window::WindowResolver;
