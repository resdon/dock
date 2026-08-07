// src/graphics/mod.rs

pub mod animation;
pub mod fade;
pub mod hide;

pub use animation::{load_svg_animation_sequence, Frame, IconAnimation};
pub use hide::AutoHideState;
