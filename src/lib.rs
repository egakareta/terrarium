#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

mod engine;
mod instance;
mod interpolation;
mod primitive;
mod renderer;
mod scene;
#[cfg(feature = "javascript")]
mod scripting;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use glam;
pub use instance::*;
pub use interpolation::*;
pub use primitive::*;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use renderer::*;
#[cfg(feature = "javascript")]
pub use rquickjs;
pub use scene::*;
#[cfg(feature = "javascript")]
pub use scripting::*;
pub use winit;
