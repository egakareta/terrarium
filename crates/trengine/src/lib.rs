#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub use eframe::{self, egui, egui_wgpu, wgpu};
#[cfg(feature = "devtools")]
pub use trdevtools;
pub use trfont;
pub use trrender;
pub use trscene;
pub use winit;

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod render_adapter;

pub use engine::*;
pub use render_adapter::*;
