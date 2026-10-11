#![deny(missing_docs)]

//! Rendering assets shared by the Terrarium engine and renderer.

pub use glam;
pub use wgpu;

mod backend;
mod color3;
mod face;
mod lighting;
mod material;
mod render_scene;
mod shape;
mod skybox;
mod vertex;

#[cfg(feature = "default-materials")]
mod builtin_material;

pub use backend::*;
#[cfg(feature = "default-materials")]
pub use builtin_material::*;
pub use color3::*;
pub use face::*;
pub use lighting::*;
pub use material::*;
pub use render_scene::*;
pub use shape::*;
pub use skybox::*;
pub use vertex::*;
