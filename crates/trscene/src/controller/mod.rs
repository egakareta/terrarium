//! Input handling and per-frame behavior for a [`Workspace`].
//!
//! [`ControllerInput`] translates device events into [`ControllerActions`],
//! separating input collection from per-frame behavior. Actions can also be
//! created directly for scripted control. Controllers receive input and updates
//! when selected by the host. An engine owns its active controller as a property;
//! scene parenting does not activate behavior.
//!
//! ```
//! use trscene::{Controller, ControllerActions, PVInstanceExt, Instance, InstanceData, InstanceId, Rig, Workspace, instance};
//! use trscene::glam::{Quat, Vec3};
//!
//! instance! {
//!     #[instance(default = false)]
//!     #[derive(Clone, Debug)]
//!     struct Autopilot {
//!         #[data]
//!         instance: InstanceData,
//!         rig: Option<InstanceId>,
//!     }
//! }
//!
//! impl Controller for Autopilot {
//!     fn update(&mut self, workspace: &mut Workspace, delta: f32) {
//!         let actions = ControllerActions { movement: Vec3::Z, ..Default::default() };
//!         if let Some(rig) = self.rig.and_then(|id| workspace.get_mut::<Rig>(id)) {
//!             let position = rig.position() + actions.translation(Quat::IDENTITY, 2.0, delta);
//!             rig.with_position(position);
//!         }
//!     }
//!
//!     fn remap_instances(&mut self, map: &dyn Fn(InstanceId) -> Option<InstanceId>) {
//!         self.rig = self.rig.and_then(map);
//!     }
//! }
//!
//! let mut workspace = Workspace::new();
//! let rig = workspace.add_child(Rig::new());
//! let mut controller = Autopilot { instance: InstanceData::new("Autopilot"), rig: Some(rig) };
//! controller.update(&mut workspace, 0.1);
//! assert!(workspace.get::<Rig>(rig).unwrap().position().z < 0.0);
//! ```

use crate::{
    winit::event::{DeviceEvent, WindowEvent},
    *,
};

mod actions;
mod input;
mod player;
mod touch;

pub(crate) use actions::frame_delta;
pub use actions::*;
pub use input::*;
pub use player::*;

/// Input and per-frame behavior selected by the host.
///
/// Use [`crate::instance!`] to expose this capability on a custom instance.
///
/// The default event handlers use [`Self::input_mut`]; override
/// them when input should be translated differently or shared with other state.
///
/// The host forwards input to its active controller. [`Self::update`] runs before
/// physics, and [`Self::post_update`] runs afterward.
pub trait Controller: Instance {
    /// Returns input state for inspection, or `None` for controllers without it.
    ///
    /// Override this together with [`Self::input_mut`] to use the default event
    /// handlers and make movement bindings available to workspace integrations.
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
    /// `delta_seconds` is finite and clamped to `0..=0.1` by the host.
    fn update(&mut self, workspace: &mut Workspace, delta_seconds: f32);

    /// Applies camera or other behavior after physics has updated scene poses.
    fn post_update(&mut self, _workspace: &mut Workspace, _delta_seconds: f32) {}

    /// Remaps stored targets to another scene using a caller-supplied mapping.
    ///
    /// The mapping returns the corresponding cloned ID, or `None` when the
    /// original ID is not part of the destination scene. Update any stored scene
    /// references here; the default leaves controller state unchanged.
    fn remap_instances(&mut self, _map: &dyn Fn(InstanceId) -> Option<InstanceId>) {}
}

impl<T: Controller> From<T> for Box<dyn Controller> {
    fn from(controller: T) -> Self {
        Box::new(controller)
    }
}

impl Default for Box<dyn Controller> {
    fn default() -> Self {
        Box::new(CameraController::default())
    }
}

impl dyn Controller {
    /// Borrows the selected controller as its concrete type.
    pub fn downcast_ref<T: Controller>(&self) -> Option<&T> {
        self.as_any().downcast_ref()
    }

    /// Mutably borrows the selected controller as its concrete type.
    pub fn downcast_mut<T: Controller>(&mut self) -> Option<&mut T> {
        self.as_any_mut().downcast_mut()
    }
}
