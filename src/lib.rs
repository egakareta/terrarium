//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod camera;
mod color3;
mod engine;
mod face;
mod instance;
mod light;
mod lighting;
mod material;
#[cfg(feature = "meshpart")]
mod meshpart;
mod part;
mod renderer;
mod shape;
mod skybox;
mod tween;
mod vertex;
mod workspace;

pub use camera::*;
pub use color3::*;
pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use face::*;
pub use glam;
pub use instance::*;
pub use light::*;
pub use lighting::*;
pub use material::*;
#[cfg(feature = "meshpart")]
pub use meshpart::*;
pub use part::*;
pub use renderer::*;
pub use shape::*;
pub use skybox::*;
pub use tween::*;
pub use vertex::*;
pub use winit;
pub use workspace::*;
