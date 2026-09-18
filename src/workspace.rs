use std::rc::Rc;

use crate::{
    Camera, CameraController, Instance, InstanceData, InstanceId, InstanceLookup, Skybox, Texture,
    TextureError, TextureHandle, TweenManager,
};
#[cfg(feature = "meshpart")]
use crate::{GltfError, MeshHandle, MeshPart, MeshSource};

/// The 3D root that owns its child [`Instance`] values, mesh assets, CPU textures, and active camera.
#[derive(Debug)]
pub struct Workspace {
    instance: Box<InstanceData>,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
    tween_manager: TweenManager,
    #[cfg(feature = "meshpart")]
    meshes: Vec<MeshHandle>,
    textures: Vec<Texture>,
    texture_revisions: Vec<u64>,
    texture_revision: u64,
    skybox: Option<Skybox>,
    skybox_revision: u64,
    lookup: Rc<InstanceLookup>,
}

impl Workspace {
    /// Creates an empty workspace with the default camera and controller.
    pub fn new() -> Self {
        let mut instance = Box::new(InstanceData::new("Workspace"));
        let lookup = Rc::new(InstanceLookup::default());
        lookup.set_root(&mut instance);
        instance.set_lookup(Some(&lookup));
        Self {
            instance,
            current_camera: Camera::default(),
            camera_controller: CameraController::default(),
            tween_manager: TweenManager::default(),
            #[cfg(feature = "meshpart")]
            meshes: Vec::new(),
            textures: Vec::new(),
            texture_revisions: Vec::new(),
            texture_revision: 0,
            skybox: Some(Skybox::default()),
            skybox_revision: 1,
            lookup,
        }
    }

    /// Takes ownership of mesh data or imports a glTF document and returns its mesh handle.
    ///
    /// A handle can be cloned and passed to multiple [`MeshPart`] values. Each
    /// part gets independent transforms and material overrides while sharing
    /// the registered geometry. Imported glTF materials are copied into each
    /// part when it is constructed.
    #[cfg(feature = "meshpart")]
    pub fn add_mesh<'a>(
        &mut self,
        source: impl Into<MeshSource<'a>>,
    ) -> Result<MeshHandle, GltfError> {
        let handle = match source.into() {
            MeshSource::Data(mesh) => MeshHandle::from(mesh),
            MeshSource::Gltf(bytes) => MeshPart::import_gltf(bytes, self)?,
        };
        self.meshes.push(handle.clone());
        Ok(handle)
    }

    /// Returns the geometry referenced by a mesh handle owned by this workspace.
    #[cfg(feature = "meshpart")]
    pub fn get_mesh(&self, handle: &MeshHandle) -> Option<&crate::Mesh> {
        self.meshes
            .iter()
            .find(|registered| registered.same_asset(handle))
            .map(MeshHandle::mesh)
    }

    /// Takes ownership of a validated CPU-side texture and returns its workspace handle.
    pub fn add_texture(&mut self, texture: Texture) -> Result<TextureHandle, TextureError> {
        texture.validate()?;
        let handle = TextureHandle(self.textures.len());
        self.textures.push(texture);
        self.texture_revision = self.texture_revision.wrapping_add(1);
        self.texture_revisions.push(self.texture_revision);
        Ok(handle)
    }

    /// Returns a CPU-side texture by its workspace-local handle.
    pub fn get_texture(&self, handle: TextureHandle) -> Option<&Texture> {
        self.textures.get(handle.0)
    }

    /// Returns the revision of a workspace texture, bumped by every
    /// [`add_texture`](Self::add_texture), [`get_texture_mut`](Self::get_texture_mut),
    /// and [`set_texture`](Self::set_texture) call for its handle.
    ///
    /// The renderer uses this to re-upload edited textures before the next
    /// frame. It is also useful for external caches keyed by texture content.
    pub fn texture_version(&self, handle: TextureHandle) -> Option<u64> {
        self.texture_revisions.get(handle.0).copied()
    }

    /// Returns a mutable CPU-side texture by its workspace-local handle.
    ///
    /// The texture is marked dirty even if it is not modified, so the
    /// renderer re-uploads it before the next frame. The texture must stay
    /// valid tightly packed RGBA8 data: keep `pixels` at
    /// `width * height * 4` bytes (the pixel helpers and transforms uphold
    /// this automatically). Invalid data surfaces as a render error instead
    /// of reaching the GPU.
    pub fn get_texture_mut(&mut self, handle: TextureHandle) -> Option<&mut Texture> {
        let texture = self.textures.get_mut(handle.0)?;
        self.texture_revision = self.texture_revision.wrapping_add(1);
        self.texture_revisions[handle.0] = self.texture_revision;
        Some(texture)
    }

    /// Replaces the texture stored under `handle` with a validated texture.
    ///
    /// The replacement is marked dirty so the renderer re-uploads it before
    /// the next frame. Dimensions and color space may change.
    pub fn set_texture(
        &mut self,
        handle: TextureHandle,
        texture: Texture,
    ) -> Result<(), TextureError> {
        texture.validate()?;
        let slot = self
            .textures
            .get_mut(handle.0)
            .ok_or(TextureError::InvalidHandle { index: handle.0 })?;
        *slot = texture;
        self.texture_revision = self.texture_revision.wrapping_add(1);
        self.texture_revisions[handle.0] = self.texture_revision;
        Ok(())
    }

    /// Returns the workspace skybox, if one is set.
    ///
    /// New workspaces default to an embedded cross-layout skybox.
    /// When no skybox is set, the renderer clears to its clear color
    /// instead.
    pub fn skybox(&self) -> Option<&Skybox> {
        self.skybox.as_ref()
    }

    /// Replaces the workspace skybox.
    ///
    /// The renderer re-uploads the six faces before the next frame.
    pub fn set_skybox(&mut self, skybox: Skybox) {
        self.skybox = Some(skybox);
        self.skybox_revision = self.skybox_revision.wrapping_add(1);
    }

    /// Removes the workspace skybox.
    ///
    /// The renderer clears to its clear color until a new skybox is set.
    pub fn clear_skybox(&mut self) {
        self.skybox = None;
        self.skybox_revision = self.skybox_revision.wrapping_add(1);
    }

    /// Returns the revision of the workspace skybox, bumped by every
    /// [`set_skybox`](Self::set_skybox) and [`clear_skybox`](Self::clear_skybox)
    /// call.
    ///
    /// The renderer uses this to re-upload the skybox faces before the next
    /// frame. It is also useful for external caches keyed by skybox content.
    pub fn skybox_revision(&self) -> u64 {
        self.skybox_revision
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

    /// Removes a direct child by its stable identifier.
    pub fn remove_child(&mut self, id: InstanceId) -> bool {
        self.instance.remove_child(id)
    }

    /// Returns every child, preserving its concrete type behind [`Instance`].
    pub fn instances(&self) -> impl Iterator<Item = &dyn Instance> {
        self.descendants()
    }

    /// Returns a child by its stable identifier.
    pub fn instance(&self, id: InstanceId) -> Option<&dyn Instance> {
        if id == self.id() {
            return None;
        }
        let instance = self.lookup.get(id)?;
        // The index only stores pointers to boxed children owned by this workspace.
        Some(unsafe { instance.as_ref() })
    }

    /// Returns a mutable child by its stable identifier.
    pub fn instance_mut(&mut self, id: InstanceId) -> Option<&mut dyn Instance> {
        if id == self.id() {
            return None;
        }
        let mut instance = self.lookup.get(id)?;
        // Boxed children have stable addresses while they are owned by the workspace.
        Some(unsafe { instance.as_mut() })
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

    /// Forwards eframe input to the active camera controller.
    pub fn process_eframe_input(&mut self, input: &crate::egui::InputState) {
        self.camera_controller.process_eframe_input(input);
    }
}

impl Clone for Workspace {
    fn clone(&self) -> Self {
        let lookup = Rc::new(InstanceLookup::default());
        let mut instance = Box::new((*self.instance).clone());
        lookup.set_root(&mut instance);
        instance.set_lookup(Some(&lookup));
        let mut workspace = Self {
            instance,
            current_camera: self.current_camera.clone(),
            camera_controller: self.camera_controller.clone(),
            tween_manager: self.tween_manager.clone(),
            #[cfg(feature = "meshpart")]
            meshes: self.meshes.clone(),
            textures: self.textures.clone(),
            texture_revisions: self.texture_revisions.clone(),
            texture_revision: self.texture_revision,
            skybox: self.skybox.clone(),
            skybox_revision: self.skybox_revision,
            lookup: lookup.clone(),
        };
        for child in workspace.instance.children_mut() {
            child.set_instance_lookup(Some(lookup.clone()));
        }
        workspace
    }
}

impl Default for Workspace {
    fn default() -> Self {
        Self::new()
    }
}

crate::impl_instance!(Workspace, class_name = "Workspace", data = instance,);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BasePart, Camera, Part, PartShape, TextureColorSpace};
    #[cfg(feature = "meshpart")]
    use crate::{Mesh, MeshPart};

    #[cfg(feature = "meshpart")]
    #[test]
    fn workspace_mesh_handles_can_create_multiple_part_variants() {
        let mut workspace = Workspace::new();
        let handle = workspace.add_mesh(Mesh::block(1.0, [1.0; 4])).unwrap();

        let first_id = workspace.add_child(MeshPart::new("first", handle.clone()));
        let second_id = workspace.add_child(MeshPart::new("second", handle.clone()));

        assert_eq!(workspace.get::<MeshPart>(first_id).unwrap().name(), "first");
        assert_eq!(
            workspace.get::<MeshPart>(second_id).unwrap().name(),
            "second"
        );
        assert!(workspace.get_mesh(&handle).is_some());
    }

    #[test]
    fn find_first_child_matches_the_requested_concrete_type() {
        let mut workspace = Workspace::new();
        let basepart_id = workspace.add_child(BasePart::new("shared"));
        let part_id = workspace.add_child(Part::new("shared"));

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
        let cloned_part_id = cloned_workspace.children()[0].id();
        assert!(cloned_workspace.get::<Part>(cloned_part_id).is_some());
    }

    #[test]
    fn removing_a_direct_child_removes_it_from_the_workspace_lookup() {
        let mut workspace = Workspace::new();
        let part_id = Part::new("part").set_parent(&mut workspace);

        assert!(workspace.remove_child(part_id));
        assert!(workspace.instance(part_id).is_none());
        assert!(workspace.get_all::<Part>().next().is_none());
        assert!(!workspace.remove_child(part_id));
    }

    #[test]
    fn removing_a_child_keeps_the_swapped_child_removable() {
        let mut workspace = Workspace::new();
        let first_id = Part::new("first").set_parent(&mut workspace);
        let removed_id = Part::new("removed").set_parent(&mut workspace);
        let last_id = Part::new("last").set_parent(&mut workspace);

        assert!(workspace.remove_child(removed_id));
        assert!(workspace.instance(first_id).is_some());
        assert!(workspace.instance(last_id).is_some());
        assert!(workspace.remove_child(last_id));
        assert!(workspace.instance(first_id).is_some());
    }

    #[test]
    fn destroying_an_instance_removes_the_instance_and_its_descendants() {
        let mut workspace = Workspace::new();
        let parent_id = workspace.add_child(Part::new("parent"));
        let child_id = workspace
            .get_mut::<Part>(parent_id)
            .unwrap()
            .add_child(Part::new("child"));

        assert!(workspace.instance_mut(parent_id).unwrap().destroy());
        assert!(workspace.instance(parent_id).is_none());
        assert!(workspace.instance(child_id).is_none());
        assert!(workspace.children().is_empty());
    }

    #[test]
    fn destroying_a_nested_instance_keeps_its_parent() {
        let mut workspace = Workspace::new();
        let parent_id = workspace.add_child(Part::new("parent"));
        let child_id = workspace
            .get_mut::<Part>(parent_id)
            .unwrap()
            .add_child(Part::new("child"));

        assert!(workspace.instance_mut(child_id).unwrap().destroy());
        assert!(workspace.instance(parent_id).is_some());
        assert!(workspace.instance(child_id).is_none());
        assert!(
            workspace
                .get::<Part>(parent_id)
                .unwrap()
                .children()
                .is_empty()
        );
    }

    #[test]
    fn lookup_index_tracks_nested_instances_and_stays_workspace_local() {
        let mut workspace = Workspace::new();
        let parent_id = workspace.add_child(Part::new("parent"));
        let child_id = {
            workspace
                .get_mut::<Part>(parent_id)
                .unwrap()
                .add_child(Part::new("child"))
        };

        assert_eq!(workspace.get::<Part>(child_id).unwrap().name(), "child");

        let other_workspace = Workspace::new();
        assert!(other_workspace.instance(child_id).is_none());
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
    fn workspace_textures_can_be_replaced_and_edited_in_place() {
        use crate::TextureError;

        let mut workspace = Workspace::new();
        let handle = workspace
            .add_texture(Texture::linear(1, 1, vec![200, 100, 50, 255]).unwrap())
            .unwrap();
        let added_version = workspace.texture_version(handle).unwrap();

        workspace
            .set_texture(
                handle,
                Texture::linear(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            )
            .unwrap();
        assert!(workspace.texture_version(handle).unwrap() != added_version);
        assert_eq!(workspace.get_texture(handle).unwrap().width, 2);

        workspace
            .get_texture_mut(handle)
            .unwrap()
            .set_pixel(0, 0, [9, 9, 9, 9]);
        assert_eq!(
            workspace.get_texture(handle).unwrap().pixel(0, 0),
            [9, 9, 9, 9]
        );

        assert!(matches!(
            workspace.set_texture(
                TextureHandle(999),
                Texture::linear(1, 1, vec![0, 0, 0, 0]).unwrap()
            ),
            Err(TextureError::InvalidHandle { index: 999 })
        ));
        assert!(workspace.get_texture_mut(TextureHandle(999)).is_none());
        assert!(workspace.texture_version(TextureHandle(999)).is_none());
        assert_eq!(
            workspace.clone().get_texture(handle),
            workspace.get_texture(handle)
        );
    }

    #[test]
    fn workspace_defaults_to_the_embedded_skybox() {
        use crate::{CubemapFace, Image, Skybox};

        let workspace = Workspace::new();
        let skybox = workspace.skybox().expect("default skybox is set");
        assert_eq!(skybox.face_size(), 512);
        assert!(skybox.face(CubemapFace::Front).pixels().len() == 512 * 512 * 4);

        let mut workspace = workspace;
        let revision = workspace.skybox_revision();
        workspace.clear_skybox();
        assert!(workspace.skybox().is_none());
        assert_ne!(workspace.skybox_revision(), revision);

        let pixels = vec![1, 2, 3, 255];
        let faces = CubemapFace::ALL
            .map(|_| Image::from_rgba8(1, 1, pixels.clone()).expect("1x1 test face is valid"));
        workspace.set_skybox(Skybox::from_faces(faces).unwrap());
        assert_eq!(workspace.skybox().unwrap().face_size(), 1);
        assert_eq!(
            workspace.clone().skybox().unwrap().face_size(),
            1,
            "cloned workspaces keep their skybox"
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
