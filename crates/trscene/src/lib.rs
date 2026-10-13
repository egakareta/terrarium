#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "sound")]
pub use euphorium;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use trrendert::{glam, *};
pub use winit;

pub mod input;
pub use input::*;

pub mod instance;
pub mod interpolation;
pub mod scene;

mod render_scene;

pub use instance::*;
pub use interpolation::*;
#[cfg(any(feature = "physics", feature = "sound"))]
pub use scene::*;
