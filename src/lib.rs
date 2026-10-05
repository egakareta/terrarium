#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod events;
mod instance;
mod interpolation;
mod primitive;
mod renderer;
mod scene;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
/// Cross-platform audio sources, spatial settings, and playback APIs.
#[cfg(feature = "sound")]
pub use euphorium;
pub use events::*;
pub use glam;
pub use instance::*;
pub use interpolation::*;
pub use primitive::*;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use renderer::*;
pub use scene::*;
pub use winit;
