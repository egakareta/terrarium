#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// JSON values used for application state and web commands.
pub use serde_json;
/// Tauri APIs for configuring a consumer-provided frontend host.
#[cfg(not(target_arch = "wasm32"))]
pub use tauri;
#[cfg(feature = "devtools")]
pub use trdevtools;
pub use trfont;
pub use trrender;
pub use trscene::{self, winit};
/// WGPU APIs used by the renderer.
pub use wgpu;

mod gpu;
mod gui;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod native_surface;
mod runtime;
#[cfg(target_arch = "wasm32")]
mod web;
/// Egui APIs for internal tools and custom engine overlays.
pub use egui;
pub use gpu::*;

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod render_adapter;

pub use engine::*;
pub use render_adapter::*;
