#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

extern crate self as trscene;

/// Generates the getter, in-place setter, and consuming builder for an instance property.
#[doc(hidden)]
#[macro_export]
macro_rules! __instance_property {
    (
        $receiver:tt;
        $(#[$doc:meta])+
        $vis:vis
        getter $getter:ident($($getter_arg:ident: $getter_arg_ty:ty),*) -> $return_ty:ty {
            $($getter_body:tt)*
        }
        setter $setter:ident($($arg:ident: $arg_ty:ty),+) {
            $($setter_body:tt)*
        }
        builder $builder:ident($($builder_arg:ident: $builder_arg_ty:ty),+) {
            $($builder_body:tt)*
        }
    ) => {
        #[doc = concat!("Returns the value of [`Self::", stringify!($getter), "`].")]
        $(#[$doc])*
        $vis fn $getter(&$receiver, $($getter_arg: $getter_arg_ty),*) -> $return_ty {
            $($getter_body)*
        }

        #[doc = concat!("Sets [`Self::", stringify!($getter), "`].")]
        $(#[$doc])*
        $vis fn $setter(&mut $receiver, $($arg: $arg_ty),+) {
            $($setter_body)*
        }

        #[doc = concat!("Returns this instance with [`Self::", stringify!($getter), "`] set.")]
        $(#[$doc])*
        $vis fn $builder(mut $receiver, $($builder_arg: $builder_arg_ty),+) -> Self
        where
            Self: Sized,
        {
            $($builder_body)*
            $receiver
        }
    };
    (
        $receiver:tt;
        $(#[$doc:meta])+
        $vis:vis
        getter $getter:ident($($getter_arg:ident: $getter_arg_ty:ty),*) -> $return_ty:ty {
            $($getter_body:tt)*
        }
        setter $setter:ident($($arg:ident: $arg_ty:ty),+) {
            $($setter_body:tt)*
        }
        builder $builder:ident;
    ) => {
        #[doc = concat!("Returns the value of [`Self::", stringify!($getter), "`].")]
        $(#[$doc])*
        $vis fn $getter(&$receiver, $($getter_arg: $getter_arg_ty),*) -> $return_ty {
            $($getter_body)*
        }

        #[doc = concat!("Sets [`Self::", stringify!($getter), "`].")]
        $(#[$doc])*
        $vis fn $setter(&mut $receiver, $($arg: $arg_ty),+) {
            $($setter_body)*
        }

        #[doc = concat!("Returns this instance with [`Self::", stringify!($getter), "`] set.")]
        $(#[$doc])*
        $vis fn $builder(mut $receiver, $($arg: $arg_ty),+) -> Self
        where
            Self: Sized,
        {
            $receiver.$setter($($arg),+);
            $receiver
        }
    };

}

pub use egui;
#[cfg(feature = "sound")]
pub use euphorium;
#[cfg(feature = "physics")]
pub use rapier3d;
#[doc(hidden)]
pub use serde_json;
pub use trrendert::{glam, *};
#[doc(hidden)]
pub use trscene_derive::__instance_expand;
pub use trscene_derive::instance;
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
