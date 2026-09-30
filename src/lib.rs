#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "bevy")]
pub mod bevy;
mod engine;
mod instance;
mod interpolation;
mod primitive;
mod renderer;
mod scene;
#[cfg(feature = "javascript")]
mod scripting;

pub use eframe::{self, egui, egui_wgpu, wgpu};
pub use engine::*;
pub use glam;
#[cfg(feature = "javascript")]
#[doc(hidden)]
pub use include_dir;
pub use instance::*;
pub use interpolation::*;
pub use primitive::*;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use renderer::*;
#[cfg(feature = "javascript")]
pub use rquickjs;
pub use scene::*;
#[cfg(feature = "javascript")]
pub use scripting::*;
pub use winit;

/// Loads the given script directory relative to the consuming crate's manifest.
///
/// For example, `scripts!("assets/scripts")` watches local files on native
/// development builds and fetches from `./assets/scripts` on web development builds.
/// Serve the selected directory alongside the page and generate its web manifest
/// with `terrarium setup --scripts-dir assets/scripts`.
///
/// To rebuild an existing release when scripts are added or removed, have `build.rs`
/// print `cargo:rerun-if-changed=YOUR_SCRIPT_DIRECTORY`.
///
/// `scripts!()` defaults to `scripts!("scripts")`.
#[cfg(feature = "javascript")]
#[macro_export]
macro_rules! scripts {
    () => {
        $crate::scripts!("scripts")
    };
    ($path:tt) => {{
        #[cfg(debug_assertions)]
        {
            $crate::ScriptProject::development(
                ::std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join($path),
                format!("./{}", $path),
            )
        }
        #[cfg(not(debug_assertions))]
        {
            use $crate::include_dir;
            static BUNDLE: $crate::include_dir::Dir<'static> =
                $crate::include_dir::include_dir!($path);
            $crate::ScriptProject::embedded(&BUNDLE, $path)
        }
    }};
}
