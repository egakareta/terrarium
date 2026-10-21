//! Input handling and per-frame behavior for a [`Workspace`].
//!
//! [`ControllerInput`] translates device events into [`ControllerActions`],
//! separating input collection from per-frame behavior. Actions can also be
//! created directly for scripted control. [`Controllers`] stores behaviors in
//! update order; the workspace owns its camera controller separately.
//!
//! ```
//! use trscene::{Controller, ControllerActions, PVInstanceExt, Instance, InstanceId, Rig, Workspace};
//! use trscene::glam::{Quat, Vec3};
//!
//! #[derive(Clone, Debug)]
//! struct Autopilot { rig: Option<InstanceId> }
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
//! workspace.controllers.add(Autopilot { rig: Some(rig) });
//! workspace.update(0.1);
//! assert!(workspace.get::<Rig>(rig).unwrap().position().z < 0.0);
//! ```

use std::{any::Any, fmt::Debug};

use crate::{
    InstanceId, Workspace, egui,
    winit::event::{DeviceEvent, WindowEvent},
};

mod actions;
mod input;
mod player;
mod touch;

pub(crate) use actions::frame_delta;
pub use actions::*;
pub use input::*;
pub use player::*;

/// Per-frame behavior registered with a [`Workspace`].
///
/// Implementations must be cloneable because cloning a workspace clones its
/// controllers. The default event handlers use [`Self::input_mut`]; override
/// them when input should be translated differently or shared with other state.
/// The workspace sends each event to every controller, then calls [`Self::update`]
/// in registration order before physics and camera updates.
pub trait Controller: Any + Debug + ControllerClone {
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
    fn process_eframe_input(&mut self, context: &egui::Context) {
        if let Some(input) = self.input_mut() {
            input.process_eframe_input(context);
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
    /// Runs after animation updates and before physics and the default camera.
    /// `delta_seconds` is finite and clamped to `0..=0.1` by the workspace.
    fn update(&mut self, workspace: &mut Workspace, delta_seconds: f32);

    /// Replaces scene instance IDs when the workspace is cloned.
    ///
    /// The mapping returns the corresponding cloned ID, or `None` when the
    /// original ID is not part of the cloned workspace. Update any stored scene
    /// references here; the default leaves controller state unchanged.
    fn remap_instances(&mut self, _map: &dyn Fn(InstanceId) -> Option<InstanceId>) {}
}

/// Object-safe clone support used by [`Controller`] trait objects.
#[doc(hidden)]
pub trait ControllerClone {
    /// Returns an owned clone with the same concrete controller type.
    fn clone_controller(&self) -> Box<dyn Controller>;
}

impl<T: Controller + Clone> ControllerClone for T {
    fn clone_controller(&self) -> Box<dyn Controller> {
        Box::new(self.clone())
    }
}

impl Clone for Box<dyn Controller> {
    fn clone(&self) -> Self {
        self.clone_controller()
    }
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

slotmap::new_key_type! {
/// A handle to one entry in a workspace's [`Controllers`] collection.
    pub struct ControllerId;
}

/// Controllers stored in their per-frame update order.
#[derive(Clone, Debug, Default)]
pub struct Controllers {
    entries: slotmap::SlotMap<ControllerId, Option<Box<dyn Controller>>>,
    order: Vec<ControllerId>,
}

impl Controllers {
    /// Adds a controller to the end of the update order.
    pub fn add(&mut self, controller: impl Controller) -> ControllerId {
        let id = self.entries.insert(Some(Box::new(controller)));
        self.order.push(id);
        id
    }

    /// Looks up a controller by handle and concrete type.
    ///
    /// Returns `None` for a removed handle, a different concrete type, or while
    /// that controller is executing its update.
    pub fn get<T: Controller>(&self, id: ControllerId) -> Option<&T> {
        self.entries.get(id)?.as_ref()?.downcast_ref()
    }

    /// Looks up a controller mutably by handle and concrete type.
    ///
    /// Returns `None` for a removed handle, a different concrete type, or while
    /// that controller is executing its update.
    pub fn get_mut<T: Controller>(&mut self, id: ControllerId) -> Option<&mut T> {
        self.entries.get_mut(id)?.as_mut()?.downcast_mut()
    }

    /// Removes a controller from the update order and returns its owned value.
    ///
    /// Returns `None` if the handle is stale or the controller is currently
    /// executing its update.
    pub fn remove(&mut self, id: ControllerId) -> Option<Box<dyn Controller>> {
        let entry = self.entries.remove(id)?;
        self.order.retain(|candidate| *candidate != id);
        entry
    }

    /// Iterates over registered controllers in update order.
    ///
    /// A controller is omitted while it is executing its update.
    pub fn iter(&self) -> impl Iterator<Item = (ControllerId, &dyn Controller)> {
        self.order
            .iter()
            .filter_map(|id| Some((*id, self.entries.get(*id)?.as_deref()?)))
    }

    pub(crate) fn process_eframe_input(&mut self, context: &egui::Context) {
        for id in &self.order {
            if let Some(Some(controller)) = self.entries.get_mut(*id) {
                controller.process_eframe_input(context);
            }
        }
    }

    pub(crate) fn process_window_event(&mut self, event: &WindowEvent) -> bool {
        let mut used = false;
        for id in &self.order {
            if let Some(Some(controller)) = self.entries.get_mut(*id) {
                used |= controller.process_window_event(event);
            }
        }
        used
    }

    pub(crate) fn process_device_event(&mut self, event: &DeviceEvent) {
        for id in &self.order {
            if let Some(Some(controller)) = self.entries.get_mut(*id) {
                controller.process_device_event(event);
            }
        }
    }

    pub(crate) fn remap_instances(&mut self, map: &dyn Fn(InstanceId) -> Option<InstanceId>) {
        for controller in self.entries.values_mut().flatten() {
            controller.remap_instances(map);
        }
    }

    pub(crate) fn update(workspace: &mut Workspace, delta_seconds: f32) {
        for id in workspace.controllers.order.clone() {
            let Some(mut controller) = workspace
                .controllers
                .entries
                .get_mut(id)
                .and_then(Option::take)
            else {
                continue;
            };
            controller.update(workspace, delta_seconds);
            if let Some(entry) = workspace.controllers.entries.get_mut(id) {
                *entry = Some(controller);
            }
        }
    }
}
