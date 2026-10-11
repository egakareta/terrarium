#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

pub use egui;
#[cfg(feature = "sound")]
pub use euphorium;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use trrendert::{glam, *};
pub use winit;

/// Scene instances and workspace ownership.
pub mod instance;
/// Tweening and interpolation for scene properties.
pub mod interpolation;
/// Optional physics and audio integrations.
pub mod scene;

/// Deferred signals and scoped subscriptions.
pub mod events;
mod render_scene;

pub use events::*;
pub use instance::*;
pub use interpolation::*;
#[cfg(any(feature = "physics", feature = "sound"))]
pub use scene::*;
