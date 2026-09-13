use crate::{Camera, CameraController, Instance, InstanceData, InstanceId, Part};

/// The 3D root that owns its child [`Instance`] values and its active camera.
#[derive(Clone, Debug)]
pub struct Workspace {
    instance: InstanceData,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
}

impl Workspace {
    pub fn new() -> Self {
        Self {
            instance: InstanceData::new("Workspace"),
            current_camera: Camera::default(),
            camera_controller: CameraController::default(),
        }
    }

    /// Creates and parents a new Part to this workspace.
    pub fn create_part(&mut self, name: impl Into<String>) -> InstanceId {
        self.add_part(Part::new(name))
    }

    pub fn create_part_with(
        &mut self,
        name: impl Into<String>,
        configure: impl FnOnce(&mut Part),
    ) -> InstanceId {
        self.add_child_with(Part::new(name), configure)
    }

    pub fn add_part(&mut self, part: Part) -> InstanceId {
        self.add_instance(part)
    }

    /// Takes ownership of any supported [`Instance`] and parents it here.
    pub fn add_instance<T>(&mut self, instance: T) -> InstanceId
    where
        T: Instance,
    {
        self.add_child(instance)
    }

    pub fn add_parts<I>(&mut self, parts: I) -> Vec<InstanceId>
    where
        I: IntoIterator<Item = Part>,
    {
        parts.into_iter().map(|part| self.add_part(part)).collect()
    }

    /// Returns a descendant by ID, downcast to its concrete instance type.
    pub fn get<T: Instance>(&self, id: InstanceId) -> Option<&T> {
        self.instance(id)?.downcast_ref::<T>()
    }

    /// Returns a mutable descendant by ID, downcast to its concrete instance type.
    pub fn get_mut<T: Instance>(&mut self, id: InstanceId) -> Option<&mut T> {
        self.instance_mut(id)?.downcast_mut::<T>()
    }

    /// Returns descendants of the requested concrete instance type in insertion order.
    pub fn get_all<T: Instance>(&self) -> impl Iterator<Item = &T> {
        self.descendants()
            .filter_map(|instance| instance.downcast_ref::<T>())
    }

    pub fn find_first_child(&mut self, name: &str) -> Option<(InstanceId, &mut Part)> {
        find_part_mut(self, name)
    }

    /// Returns every child, preserving its concrete type behind [`Instance`].
    pub fn instances(&self) -> impl Iterator<Item = &dyn Instance> {
        self.descendants()
    }

    /// Returns a child by its stable identifier.
    pub fn instance(&self, id: InstanceId) -> Option<&dyn Instance> {
        self.find_descendant(id)
    }

    /// Returns a mutable child by its stable identifier.
    pub fn instance_mut(&mut self, id: InstanceId) -> Option<&mut dyn Instance> {
        self.find_descendant_mut(id)
    }

    pub fn update_camera(&mut self, delta: f32) {
        self.camera_controller
            .update_camera(&mut self.current_camera, delta);
    }

    pub fn process_device_event(&mut self, event: &winit::event::DeviceEvent) {
        self.camera_controller.process_device_event(event);
    }

    pub fn process_window_event(&mut self, event: &winit::event::WindowEvent) {
        self.camera_controller.process_window_event(event);
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_instance!(Workspace, class_name = "Workspace", data = instance,);

fn find_part_mut<'a>(
    instance: &'a mut dyn Instance,
    name: &str,
) -> Option<(InstanceId, &'a mut Part)> {
    for child in instance.children_mut() {
        let matches = child
            .downcast_ref::<Part>()
            .is_some_and(|part| part.name() == name);
        if matches {
            let id = child.id();
            return Some((id, child.downcast_mut::<Part>()?));
        }
        if let Some(found) = find_part_mut(child.as_mut(), name) {
            return Some(found);
        }
    }
    None
}
