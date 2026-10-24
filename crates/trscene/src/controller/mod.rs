//! Input handling and per-frame behavior for a [`Workspace`].
//!
//! [`ControllerInput`] translates device events into [`ControllerActions`],
//! separating input collection from per-frame behavior. Actions can also be
//! created directly for scripted control. Each engine owns one controller, which
//! defaults to [`CameraController`].
//!
//! ```
//! use trscene::{Controller, ControllerActions, PVInstanceExt, Instance, InstanceId, Rig, Workspace};
//! use trscene::glam::{Quat, Vec3};
//!
//! #[derive(Debug)]
//! struct Autopilot { rig: Option<InstanceId> }
//!
//! impl Controller for Autopilot {
//!     fn step(&mut self, workspace: &mut Workspace, delta: f32) {
//!         let actions = ControllerActions { movement: Vec3::Z, ..Default::default() };
//!         if let Some(rig) = self.rig.and_then(|id| workspace.get_mut::<Rig>(id)) {
//!             let position = rig.position() + actions.translation(Quat::IDENTITY, 2.0, delta);
//!             rig.with_position(position);
//!         }
//!     }
//!
//! }
//!
//! let mut workspace = Workspace::new();
//! let rig = workspace.add_child(Rig::new());
//! let mut controller: Box<dyn Controller> = Box::new(Autopilot { rig: Some(rig) });
//! controller.step(&mut workspace, 0.1);
//! assert!(workspace.get::<Rig>(rig).unwrap().position().z < 0.0);
//! ```

use std::{any::Any, fmt::Debug};

use crate::{
    winit::event::{DeviceEvent, WindowEvent},
    *,
};

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

/// A per-frame behavior owned by an engine.
///
/// The default event handlers use [`Self::input_mut`]; override them when input
/// should be translated differently or shared with other state. The engine
/// forwards input to this controller, steps it before physics, then calls
/// [`Self::post_physics`] after physics has synchronized scene transforms.
pub trait Controller: Any + Debug {
    /// Returns input state for inspection, or `None` for controllers without it.
    ///
    /// Override this together with [`Self::input_mut`] to use the default event
    /// handlers and make movement bindings available to engine integrations.
    fn input(&self) -> Option<&ControllerInput> {
        None
    }

    /// Returns input state used by the default event handlers, if any.
    fn input_mut(&mut self) -> Option<&mut ControllerInput> {
        None
    }

    /// Processes one frame of egui input and paints touch controls, if enabled.
    fn process_input(&mut self, context: &egui::Context) {
        if let Some(input) = self.input_mut() {
            input.process_input(context);
        }
    }

    /// Updates input state from a winit window event and reports whether it handled it.
    ///
    /// Returning `true` reports that this controller handled the event.
    fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        self.input_mut()
            .is_some_and(|input| input.process_window_event(event))
    }

    /// Processes a raw device event, such as unfiltered mouse motion.
    fn process_device_event(&mut self, event: &DeviceEvent) {
        if let Some(input) = self.input_mut() {
            input.process_device_event(event);
        }
    }

    /// Applies this controller's behavior for one frame.
    ///
    /// Runs after animation updates and before physics.
    /// `delta_seconds` is finite and clamped to `0..=0.1` by the engine.
    fn step(&mut self, workspace: &mut Workspace, delta_seconds: f32);

    /// Runs after physics updates and before audio updates.
    ///
    /// Controllers that update the camera can use this phase to follow subjects
    /// after dynamic scene transforms have been synchronized.
    fn post_physics(&mut self, _workspace: &mut Workspace, _delta_seconds: f32) {}
}

impl dyn Controller {
    /// Recovers this controller's concrete type.
    pub fn downcast_ref<T: Controller>(&self) -> Option<&T> {
        (self as &dyn Any).downcast_ref()
    }

    /// Recovers mutable access to this controller's concrete type.
    pub fn downcast_mut<T: Controller>(&mut self) -> Option<&mut T> {
        (self as &mut dyn Any).downcast_mut()
    }
}
