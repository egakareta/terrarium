//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod color3;
mod engine;
mod face;
mod instance;
mod lighting;
mod material;
mod renderer;
mod shape;
mod skybox;
mod tween;
mod vertex;

pub use color3::*;
pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use face::*;
pub use glam;
pub use instance::*;
pub use lighting::*;
pub use material::*;
pub use renderer::*;
pub use shape::*;
pub use skybox::*;
pub use tween::*;
pub use vertex::*;
pub use winit;
