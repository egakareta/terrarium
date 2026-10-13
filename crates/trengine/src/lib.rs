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
mod interop;
#[cfg(not(target_arch = "wasm32"))]
mod native;
#[cfg(not(target_arch = "wasm32"))]
mod native_surface;
#[cfg(target_arch = "wasm32")]
mod web;
pub use gpu::*;
pub use interop::*;

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod render_adapter;

pub use engine::*;
pub use render_adapter::*;
