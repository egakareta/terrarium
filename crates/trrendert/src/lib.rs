#![deny(missing_docs)]
//! Rendering data types and assets shared by Terrarium.

pub use glam;

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
