use crate::{
    Camera, CameraController, Instance, InstanceData, InstanceId, Texture, TextureError,
    TextureHandle, TweenManager,
};

/// The 3D root that owns its child [`Instance`] values, CPU textures, and active camera.
#[derive(Clone, Debug)]
pub struct Workspace {
    instance: InstanceData,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
    tween_manager: TweenManager,
    textures: Vec<Texture>,
}

impl Workspace {
    /// Creates an empty workspace with the default camera and controller.
    pub fn new() -> Self {
        Self {
            instance: InstanceData::new("Workspace"),
            current_camera: Camera::default(),
            camera_controller: CameraController::default(),
            tween_manager: TweenManager::default(),
            textures: Vec::new(),
        }
    }

    /// Takes ownership of a validated CPU-side texture and returns its workspace handle.
    pub fn add_texture(&mut self, texture: Texture) -> Result<TextureHandle, TextureError> {
        texture.validate()?;
        let handle = TextureHandle(self.textures.len());
        self.textures.push(texture);
        Ok(handle)
    }

    /// Returns a CPU-side texture by its workspace-local handle.
    pub fn get_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.0)
    }

    /// Takes ownership of any supported [`Instance`] and parents it here.
    pub fn add_instance<T>(&mut self, instance: T) -> InstanceId
    where
        T: Instance,
    {
        self.add_child(instance)
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

    /// Finds the first descendant of type `T` with `name` in depth-first order.
    pub fn find_first_child<T: Instance>(&mut self, name: &str) -> Option<(InstanceId, &mut T)> {
        find_child_mut::<T>(self, name)
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

    /// Applies the controller's accumulated input to the active camera.
    pub fn update_camera(&mut self, delta: f32) {
        self.camera_controller
            .update_camera(&mut self.current_camera, delta);
    }

    /// Advances camera input and all registered scene tweens.
    pub fn update(&mut self, delta_seconds: f32) {
        self.update_camera(delta_seconds);
        self.update_tweens(delta_seconds);
    }

    /// Advances all registered scene tweens without updating the camera.
    pub fn update_tweens(&mut self, delta_seconds: f32) {
        let mut tween_manager = std::mem::take(&mut self.tween_manager);
        tween_manager.update(delta_seconds, self);
        self.tween_manager = tween_manager;
    }

    /// Returns the workspace's scene tween manager.
    pub fn tweens(&self) -> &TweenManager {
        &self.tween_manager
    }

    /// Returns the workspace's scene tween manager for registration and control.
    pub fn tweens_mut(&mut self) -> &mut TweenManager {
        &mut self.tween_manager
    }

    /// Returns the active camera controller.
    pub fn camera_controller(&self) -> &CameraController {
        &self.camera_controller
    }

    /// Returns the active camera controller for configuration and input state access.
    pub fn camera_controller_mut(&mut self) -> &mut CameraController {
        &mut self.camera_controller
    }

    /// Forwards a raw device event to the active camera controller.
    pub fn process_device_event(&mut self, event: &winit::event::DeviceEvent) {
        self.camera_controller.process_device_event(event);
    }

    /// Forwards a window event to the active camera controller.
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

fn find_child_mut<'a, T: Instance>(
    instance: &'a mut dyn Instance,
    name: &str,
) -> Option<(InstanceId, &'a mut T)> {
    for child in instance.children_mut() {
        let matches = child
            .downcast_ref::<T>()
            .is_some_and(|child| child.name() == name);
        if matches {
            let id = child.id();
            return Some((id, child.downcast_mut::<T>()?));
        }
        if let Some(found) = find_child_mut::<T>(child.as_mut(), name) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, Camera, Part, PartShape, TextureColorSpace};

    #[test]
    fn find_first_child_matches_the_requested_concrete_type() {
        let mut workspace = Workspace::new();
        let basepart_id = workspace.add_instance(BasePart::new("shared"));
        let part_id = workspace.add_instance(Part::new("shared"));

        assert_eq!(
            workspace
                .find_first_child::<BasePart>("shared")
                .map(|(id, _)| id),
            Some(basepart_id)
        );
        assert_eq!(
            workspace
                .find_first_child::<Part>("shared")
                .map(|(id, _)| id),
            Some(part_id)
        );
        assert!(workspace.find_first_child::<Camera>("shared").is_none());
    }

    #[test]
    fn isolated_instances_can_be_parented_and_recovered_by_id() {
        let mut workspace = Workspace::new();
        let workspace_id = workspace.id();
        let mut part = Part::new("part");
        part.shape = PartShape::Ball;
        let part_id = part.id();
        let returned_id = part.set_parent(&mut workspace);

        assert_eq!(returned_id, part_id);
        assert_eq!(workspace.children().len(), 1);
        assert_eq!(workspace.children()[0].id(), part_id);
        let child = workspace.instance(part_id).unwrap();
        assert_eq!(child.class_name(), "Part");
        assert_eq!(child.parent(), Some(workspace_id));
        assert_eq!(
            workspace.get::<Part>(part_id).unwrap().shape,
            PartShape::Ball
        );

        let basepart_id = BasePart::new("base").set_parent(&mut workspace);
        let camera_id = Camera::default().set_parent(&mut workspace);
        workspace
            .get_mut::<BasePart>(basepart_id)
            .unwrap()
            .set_name("renamed".to_owned());
        assert_eq!(
            workspace.get::<BasePart>(basepart_id).unwrap().name(),
            "renamed"
        );
        assert!(workspace.instance(basepart_id).unwrap().is::<BasePart>());
        assert!(workspace.instance(camera_id).unwrap().is::<Camera>());
        assert!(workspace.get::<Part>(camera_id).is_none());
        assert_eq!(workspace.get_all::<Camera>().count(), 1);
        assert_eq!(workspace.get_all::<Part>().count(), 1);
        assert_eq!(
            workspace
                .instances()
                .map(Instance::class_name)
                .collect::<Vec<_>>(),
            vec!["Part", "BasePart", "Camera"]
        );

        let cloned_workspace = workspace.clone();
        assert_eq!(cloned_workspace.children().len(), 3);
        assert!(
            cloned_workspace
                .instances()
                .all(|instance| instance.parent() == Some(cloned_workspace.id()))
        );
    }

    #[test]
    fn camera_controller_can_be_configured_through_workspace() {
        let mut workspace = Workspace::new();

        workspace.camera_controller_mut().key_bindings.forward =
            vec![winit::keyboard::KeyCode::ArrowUp];

        assert_eq!(
            workspace.camera_controller().key_bindings.forward,
            vec![winit::keyboard::KeyCode::ArrowUp]
        );
    }

    #[test]
    fn workspace_owns_validated_cpu_textures() {
        let texture = Texture::linear(1, 1, vec![1, 2, 3, 4]).unwrap();
        let mut workspace = Workspace::new();
        let handle = workspace.add_texture(texture.clone()).unwrap();

        assert_eq!(workspace.get_texture(handle), Some(&texture));
        assert_eq!(workspace.clone().get_texture(handle), Some(&texture));
        assert!(
            workspace
                .add_texture(Texture {
                    width: 1,
                    height: 1,
                    pixels: vec![],
                    color_space: TextureColorSpace::Linear,
                })
                .is_err()
        );
    }
}
