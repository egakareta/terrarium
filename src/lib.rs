//! A small, data-oriented 3D library built around `wgpu`.
#![deny(missing_docs)]

mod camera;
mod color3;
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
#[cfg(feature = "eframe")]
pub use eframe;
#[cfg(feature = "eframe")]
pub use eframe::egui;
#[cfg(feature = "eframe")]
pub use eframe::egui_wgpu;
pub use glam;
pub use instance::*;
pub use material::*;
pub use part::*;
pub use renderer::*;
pub use shape::*;
pub use tween::*;
pub use vertex::*;
pub use wgpu;
pub use winit;
pub use workspace::*;
