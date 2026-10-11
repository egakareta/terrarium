#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "devtools")]
extern crate self as trrendert;

pub use glam;

/// Field reflection and property editing for the devtools inspector.
#[cfg(feature = "devtools")]
pub mod devtools;

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
