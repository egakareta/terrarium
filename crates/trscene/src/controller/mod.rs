//! Base controller instances and per-frame behavior for a [`Workspace`].
//!
//! [`Controller`] composes [`ControllerInput`] and provides overridable frame hooks.
//! [`CameraController`] and [`PlayerController`] extend it, and custom controller
//! instances can do the same. Actions can also be created directly for scripted control.
//! Each engine owns one controller separately from its scene tree and defaults to
//! [`CameraController`].
//!
//! ```
//! use trscene::{Controller, ControllerInputExt};
//!
//! trscene::instance! {
//!     #[instance(default = false)]
//!     #[derive(Clone, Debug)]
//!     struct Autopilot {
//!         #[extends]
//!         controller: Controller,
//!     }
//! }
//!
//! let mut controller = Autopilot {
//!     controller: Controller::default(),
//! };
//! controller.set_forward(true);
//! assert_eq!(controller.take_actions().movement.z, 1.0);
//! ```

use crate::*;

mod actions;
mod input;
mod player;
mod player_animation;
mod touch;

pub(crate) use actions::frame_delta;
pub use actions::*;
pub use input::*;
pub use player::*;
pub use player_animation::*;

instance! {
    /// Base instance for input-driven and scripted controller behavior.
    ///
    /// Extend this type with `#[extends]` and override [`Self::step`] or
    /// [`Self::post_physics`] to implement custom behavior. It composes
    /// [`ControllerInput`] and inherits its generated input properties and event APIs.
    #[derive(Clone, Debug)]
    pub struct Controller {
        #[data]
        data: InstanceData,
        #[compose]
        input: ControllerInput,
    }

    impl Controller {
        /// Applies this controller's behavior for one frame.
        ///
        /// The base implementation does nothing. Runs after animation updates and before physics.
        /// `delta_seconds` is finite and clamped to `0..=0.1` by the engine.
        pub fn step(&mut self, _workspace: &mut Workspace, _delta_seconds: f32) {}

        /// Runs after physics updates and before audio updates.
        ///
        /// The base implementation does nothing. Override it for behavior such as following
        /// subjects after dynamic scene transforms have been synchronized.
        pub fn post_physics(&mut self, _workspace: &mut Workspace, _delta_seconds: f32) {}
    }
}
