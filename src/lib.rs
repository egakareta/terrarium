//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod engine;
mod instance;
mod interpolation;
#[cfg(feature = "physics")]
mod physics;
mod primitive;
mod renderer;
mod scene;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use glam;
pub use instance::*;
pub use interpolation::*;
#[cfg(feature = "physics")]
pub use physics::*;
pub use primitive::*;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use renderer::*;
pub use scene::*;
pub use winit;
