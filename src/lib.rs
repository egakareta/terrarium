//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod camera;
mod color3;
mod framework;
mod instance;
mod material;
mod part;
mod renderer;
mod shape;
mod tween;
mod vertex;
mod workspace;

pub use camera::*;
pub use color3::*;
pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use framework::*;
pub use glam;
pub use instance::*;
pub use material::*;
pub use part::*;
pub use renderer::*;
pub use shape::*;
pub use tween::*;
pub use vertex::*;
pub use winit;
pub use workspace::*;
