#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

extern crate self as trscene;

pub use egui;
/// Internal method generator shared by declarative and instance properties.
#[doc(hidden)]
pub mod __property_api {
    pub use trproperty::*;
}
#[cfg(feature = "sound")]
pub use euphorium;
#[cfg(feature = "physics")]
pub use rapier3d;
#[doc(hidden)]
pub use serde_json;
#[doc(hidden)]
pub use trmacros::__instance_expand;
pub use trmacros::instance;
pub use trrendert::{glam, *};
pub use winit;

pub mod controller;
pub mod instance;
pub mod interpolation;
pub mod scene;

mod render_scene;

pub use controller::*;
pub use instance::*;
pub use interpolation::*;
#[cfg(any(feature = "physics", feature = "sound"))]
pub use scene::*;
