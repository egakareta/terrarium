#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[doc = include_str!("../docs/index.md")]
pub mod cookbook {
    #[doc = include_str!("../docs/quick_start.md")]
    pub mod quick_start {}
}

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod instance;
mod interpolation;
#[cfg(feature = "persistence")]
mod persistence;
mod primitive;
mod renderer;
mod scene;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
/// Cross-platform audio sources, spatial settings, and playback APIs.
#[cfg(feature = "sound")]
pub use euphorium;
pub use glam;
pub use instance::*;
pub use interpolation::*;
#[cfg(feature = "persistence")]
pub use persistence::*;
pub use primitive::*;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use renderer::*;
pub use scene::*;
pub use winit;
