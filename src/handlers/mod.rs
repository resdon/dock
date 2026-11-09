//! Event handler modules.
//!
//! Dock icon handlers, popup menu handlers, and utility functions.

#[allow(clippy::module_inception)]
pub mod dock;
#[allow(clippy::module_inception)]
pub mod handlers;
pub mod popup;
pub mod utils;

pub use dock::*;
#[allow(unused_imports)]
pub use handlers::*;
pub use popup::*;
pub use utils::*;
