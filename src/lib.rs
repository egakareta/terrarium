//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod engine;
mod instance;
mod lighting;
mod material;
mod primitive;
mod renderer;
mod skybox;
mod tween;
mod vertex;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use glam;
pub use instance::*;
pub use lighting::*;
pub use material::*;
pub use primitive::*;
pub use renderer::*;
pub use skybox::*;
pub use tween::*;
pub use vertex::*;
pub use winit;
