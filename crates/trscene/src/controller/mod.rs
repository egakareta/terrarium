//! Input handling and per-frame behavior for a [`Workspace`].
//!
//! [`ControllerInput`] translates device events into [`ControllerActions`],
//! separating input collection from per-frame behavior. Actions can also be
//! created directly for scripted control. Controllers receive input and updates
//! only while parented directly to a workspace. Camera controllers update after
//! physics; other controllers update before it, in child order.
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
//! workspace.add_child(Autopilot { instance: InstanceData::new("Autopilot"), rig: Some(rig) });
//! workspace.update(0.1);
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

/// Per-frame behavior of an instance parented directly to a [`Workspace`].
///
/// Use [`crate::instance!`] to expose this capability on a custom instance.
///
/// The default event handlers use [`Self::input_mut`]; override
/// them when input should be translated differently or shared with other state.
///
/// The workspace sends each event to every direct controller child, then calls
/// [`Self::update`] in child order. Camera controllers update after physics;
/// other controllers update before physics.
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
    ///
    /// Returning `true` does not prevent the workspace from forwarding the event
    /// to other controllers.
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
    /// Runs after animation updates, before physics for general controllers or
    /// after physics for [`crate::CameraController`].
    /// `delta_seconds` is finite and clamped to `0..=0.1` by the workspace.
    fn update(&mut self, workspace: &mut Workspace, delta_seconds: f32);

    /// Replaces scene instance IDs when the workspace is cloned.
    ///
    /// The mapping returns the corresponding cloned ID, or `None` when the
    /// original ID is not part of the cloned workspace. Update any stored scene
    /// references here; the default leaves controller state unchanged.
    fn remap_instances(&mut self, _map: &dyn Fn(InstanceId) -> Option<InstanceId>) {}
}
