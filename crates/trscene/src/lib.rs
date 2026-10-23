#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

extern crate self as trscene;

pub use egui;
#[cfg(feature = "sound")]
pub use euphorium;
#[cfg(feature = "physics")]
pub use rapier3d;
#[doc(hidden)]
pub use serde_json;
pub use trrendert::{glam, *};
#[doc(hidden)]
pub use trscene_derive::*;
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

/// Common instance methods and composition macros.
pub mod prelude {
    #[cfg(feature = "meshpart")]
    pub use crate::MeshPartExt;
    pub use crate::{
        BasePartExt, CameraExt, Instance, InstanceAttributes, InstanceExt, InstanceReceiver,
        LightPropertiesExt, OutlineExt, PVInstanceExt, PartExt, PointLightExt, RigExt,
        SpotLightExt, SurfaceLightExt, instance,
    };
    #[cfg(feature = "sound")]
    pub use crate::{SoundExt, SoundSettingsExt};
}
