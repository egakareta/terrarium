use std::rc::Rc;

use crate::{
    Camera, CameraController, Instance, InstanceData, InstanceId, InstanceLookup, Lighting, Skybox,
    Texture, TextureError, TextureHandle, TweenManager, glam::Vec3,
};
#[cfg(feature = "meshpart")]
use crate::{GltfError, MeshHandle, MeshPart, MeshSource};
#[cfg(feature = "physics")]
use crate::{PhysicsInstance, PhysicsWorld, apply_transform};

/// The 3D root that owns its child [`Instance`] values, mesh assets, CPU textures, and active camera.
#[derive(Debug)]
pub struct Workspace {
    instance: Box<InstanceData>,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
    tween_manager: TweenManager,
    #[cfg(feature = "physics")]
    physics: PhysicsWorld,
    #[cfg(feature = "meshpart")]
    meshes: Vec<MeshHandle>,
    textures: Vec<Texture>,
    texture_revisions: Vec<u64>,
    texture_revision: u64,
    /// Scene-wide lighting configuration.
    pub lighting: Lighting,
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
            #[cfg(feature = "physics")]
            physics: PhysicsWorld::default(),
            #[cfg(feature = "meshpart")]
            meshes: Vec::new(),
            textures: Vec::new(),
            texture_revisions: Vec::new(),
            texture_revision: 0,
            lighting: Lighting::default(),
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
    /// and [`with_texture`](Self::with_texture) call for its handle.
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
    pub fn with_texture(
        &mut self,
        handle: TextureHandle,
        texture: Texture,
    ) -> Result<&mut Self, TextureError> {
        texture.validate()?;
        let slot = self
            .textures
            .get_mut(handle.0)
            .ok_or(TextureError::InvalidHandle { index: handle.0 })?;
        *slot = texture;
        self.texture_revision = self.texture_revision.wrapping_add(1);
        self.texture_revisions[handle.0] = self.texture_revision;
        Ok(self)
    }

    /// Returns the lighting skybox, if one is set.
    ///
    /// New workspaces default to an embedded cross-layout skybox.
    /// When no skybox is set, the renderer clears to its clear color
    /// instead.
    pub fn skybox(&self) -> Option<&Skybox> {
        self.lighting.skybox()
    }

    /// Replaces the lighting skybox.
    ///
    /// The renderer re-uploads the six faces before the next frame.
    pub fn with_skybox(&mut self, skybox: Skybox) -> &mut Self {
        self.lighting.with_skybox(skybox);
        self
    }

    /// Removes the lighting skybox.
    ///
    /// The renderer clears to its clear color until a new skybox is set.
    pub fn clear_skybox(&mut self) {
        self.lighting.clear_skybox();
    }

    /// Returns the revision of the lighting skybox, bumped by every
    /// [`with_skybox`](Self::with_skybox) and [`clear_skybox`](Self::clear_skybox)
    /// call.
    ///
    /// The renderer uses this to re-upload the skybox faces before the next
    /// frame. It is also useful for external caches keyed by skybox content.
    pub fn skybox_revision(&self) -> u64 {
        self.lighting.skybox_revision()
    }

    /// Default direction toward the sun.
    pub const DEFAULT_SUN_DIRECTION: Vec3 = Vec3::new(-0.45, 0.85, 0.35);

    /// Returns the unit vector pointing from the scene toward the sun.
    ///
    /// The renderer uses this for the key light direction and its shadow
    /// cascades. New workspaces point at the midday sun.
    pub fn sun_direction(&self) -> Vec3 {
        self.lighting.render_sun_direction()
    }

    /// Returns the direction of the sun.
    pub fn get_sun_direction(&self) -> Vec3 {
        self.sun_direction()
    }

    /// Sets the direction toward the sun.
    ///
    /// The direction is normalized before it is stored. A zero-length or
    /// non-finite direction falls back to straight up (`+Y`).
    /// [`clock_time`](Self::clock_time) is updated to the nearest hour whose
    /// sun-path direction best matches the new direction.
    pub fn with_sun_direction(&mut self, direction: Vec3) -> &mut Self {
        self.lighting.with_sun_direction(direction);
        self
    }

    /// Returns the time of day in hours, in `[0, 24)`.
    ///
    /// This is the last value passed to [`with_clock_time`](Self::with_clock_time),
    /// or the nearest matching hour after
    /// [`with_sun_direction`](Self::with_sun_direction). `12.0` is midday and
    /// `0.0` is midnight. New workspaces start at midday.
    pub fn clock_time(&self) -> f32 {
        self.lighting.clock_time()
    }

    /// Sets the time of day in hours and moves the sun along its arc.
    ///
    /// Values wrap into `[0, 24)`, so `25.0` is `1.0` and `-6.0` is `18.0`.
    /// Non-finite values fall back to midday (`12.0`). On the sun path,
    /// `6.0` is sunrise on the eastern (`+X`) horizon, `12.0` is the midday
    /// sun, `18.0` is sunset on the western (`-X`) horizon, and `0.0` points
    /// below the horizon at midnight.
    pub fn with_clock_time(&mut self, hours: f32) -> &mut Self {
        self.lighting.with_clock_time(hours);
        self
    }

    /// Advances the time of day by `hours_per_second` for the elapsed real time.
    pub fn advance_clock_time(&mut self, delta_seconds: f32, hours_per_second: f32) -> &mut Self {
        self.lighting
            .advance_clock_time(delta_seconds, hours_per_second);
        self
    }

    /// Returns the sun direction for a time of day in hours.
    ///
    /// This is the same mapping [`with_clock_time`](Self::with_clock_time)
    /// uses, without modifying the workspace. Values wrap into `[0, 24)`;
    /// non-finite values map to the midday sun.
    pub fn sun_direction_from_clock_time(hours: f32) -> Vec3 {
        Lighting::sun_direction_from_clock_time(hours)
    }

    /// Returns the time as a `HH:MM:SS` string.
    pub fn time_of_day(&self) -> String {
        self.lighting.time_of_day()
    }

    /// Parses and sets a `HH:MM:SS` time.
    pub fn with_time_of_day(&mut self, time: &str) -> Result<&mut Self, crate::TimeOfDayError> {
        self.lighting.with_time_of_day(time)?;
        Ok(self)
    }

    /// Returns minutes elapsed since midnight.
    pub fn get_minutes_after_midnight(&self) -> f32 {
        self.lighting.get_minutes_after_midnight()
    }

    /// Sets the time using minutes elapsed since midnight.
    pub fn with_minutes_after_midnight(&mut self, minutes: f32) -> &mut Self {
        self.lighting.with_minutes_after_midnight(minutes);
        self
    }

    /// Returns the direction of the moon, opposite the sun direction.
    pub fn moon_direction(&self) -> Vec3 {
        self.lighting.moon_direction()
    }

    /// Returns the direction of the moon.
    pub fn get_moon_direction(&self) -> Vec3 {
        self.lighting.get_moon_direction()
    }

    /// Returns an approximate normalized lunar phase in `[0, 1)`.
    pub fn get_moon_phase(&self) -> f32 {
        self.lighting.get_moon_phase()
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

    /// Advances camera input, registered scene tweens, and physics when enabled.
    pub fn update(&mut self, delta_seconds: f32) {
        self.update_camera(delta_seconds);
        self.update_tweens(delta_seconds);
        #[cfg(feature = "physics")]
        self.update_physics(delta_seconds);
    }

    /// Returns the workspace's Rapier physics world.
    #[cfg(feature = "physics")]
    pub fn physics(&self) -> &PhysicsWorld {
        &self.physics
    }

    /// Returns mutable access to the workspace's Rapier physics world.
    #[cfg(feature = "physics")]
    pub fn physics_mut(&mut self) -> &mut PhysicsWorld {
        &mut self.physics
    }

    #[cfg(feature = "physics")]
    fn update_physics(&mut self, delta_seconds: f32) {
        let instances: Vec<_> = self
            .instances()
            .filter_map(PhysicsInstance::from_instance)
            .collect();
        let transforms = self.physics.step(&instances, delta_seconds);
        for (id, transform) in transforms {
            if let Some(instance) = self.instance_mut(id) {
                apply_transform(instance, transform);
            }
        }
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
            #[cfg(feature = "physics")]
            physics: self.physics.clone_configuration(),
            #[cfg(feature = "meshpart")]
            meshes: self.meshes.clone(),
            textures: self.textures.clone(),
            texture_revisions: self.texture_revisions.clone(),
            texture_revision: self.texture_revision,
            lighting: self.lighting.clone(),
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
    use crate::{BasePart, Camera, HasPart, Part, PartShape, TextureColorSpace};
    #[cfg(feature = "meshpart")]
    use crate::{Mesh, MeshPart};

    #[cfg(feature = "meshpart")]
    #[test]
    fn workspace_mesh_handles_can_create_multiple_part_variants() {
        let mut workspace = Workspace::new();
        let handle = workspace.add_mesh(Mesh::block(1.0, [1.0; 4])).unwrap();

        let first_id = workspace.add_child(MeshPart::new(handle.clone()).with_name("first"));
        let second_id = workspace.add_child(MeshPart::new(handle.clone()).with_name("second"));

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
        let basepart_id = workspace.add_child(BasePart::new().with_name("shared"));
        let part_id = workspace.add_child(Part::new().with_name("shared"));

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
        let part = Part::new().with_shape(PartShape::Ball).with_name("part");
        let part_id = part.id();
        let returned_id = part.set_parent(&mut workspace);

        assert_eq!(returned_id, part_id);
        assert_eq!(workspace.children().len(), 1);
        assert_eq!(workspace.children()[0].id(), part_id);
        let child = workspace.instance(part_id).unwrap();
        assert_eq!(child.class_name(), "Part");
        assert_eq!(child.parent(), Some(workspace_id));
        assert_eq!(
            workspace.get::<Part>(part_id).unwrap().shape(),
            PartShape::Ball
        );

        let basepart_id = BasePart::new()
            .with_name("renamed")
            .set_parent(&mut workspace);
        let camera_id = Camera::default().set_parent(&mut workspace);
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
        let part_id = Part::new().with_name("part").set_parent(&mut workspace);

        assert!(workspace.remove_child(part_id));
        assert!(workspace.instance(part_id).is_none());
        assert!(workspace.get_all::<Part>().next().is_none());
        assert!(!workspace.remove_child(part_id));
    }

    #[test]
    fn removing_a_child_keeps_the_swapped_child_removable() {
        let mut workspace = Workspace::new();
        let first_id = Part::new().with_name("first").set_parent(&mut workspace);
        let removed_id = Part::new().with_name("removed").set_parent(&mut workspace);
        let last_id = Part::new().with_name("last").set_parent(&mut workspace);

        assert!(workspace.remove_child(removed_id));
        assert!(workspace.instance(first_id).is_some());
        assert!(workspace.instance(last_id).is_some());
        assert!(workspace.remove_child(last_id));
        assert!(workspace.instance(first_id).is_some());
    }

    #[test]
    fn destroying_an_instance_removes_the_instance_and_its_descendants() {
        let mut workspace = Workspace::new();
        let parent_id = workspace.add_child(Part::new().with_name("parent"));
        let child_id = workspace
            .get_mut::<Part>(parent_id)
            .unwrap()
            .add_child(Part::new().with_name("child"));

        assert!(workspace.instance_mut(parent_id).unwrap().destroy());
        assert!(workspace.instance(parent_id).is_none());
        assert!(workspace.instance(child_id).is_none());
        assert!(workspace.children().is_empty());
    }

    #[test]
    fn destroying_a_nested_instance_keeps_its_parent() {
        let mut workspace = Workspace::new();
        let parent_id = workspace.add_child(Part::new().with_name("parent"));
        let child_id = workspace
            .get_mut::<Part>(parent_id)
            .unwrap()
            .add_child(Part::new().with_name("child"));

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
        let parent_id = workspace.add_child(Part::new().with_name("parent"));
        let child_id = {
            workspace
                .get_mut::<Part>(parent_id)
                .unwrap()
                .add_child(Part::new().with_name("child"))
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
            .with_texture(
                handle,
                Texture::linear(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            )
            .unwrap();
        assert!(workspace.texture_version(handle).unwrap() != added_version);
        assert_eq!(workspace.get_texture(handle).unwrap().width, 2);

        workspace
            .get_texture_mut(handle)
            .unwrap()
            .with_pixel(0, 0, [9, 9, 9, 9]);
        assert_eq!(
            workspace.get_texture(handle).unwrap().pixel(0, 0),
            [9, 9, 9, 9]
        );

        assert!(matches!(
            workspace.with_texture(
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
        use crate::{Face, Image, Skybox};

        let workspace = Workspace::new();
        let skybox = workspace.skybox().expect("default skybox is set");
        assert!(workspace.lighting.skybox().is_some());
        assert_eq!(skybox.face_size(), 512);
        assert!(skybox.face(Face::Front).pixels().len() == 512 * 512 * 4);

        let mut workspace = workspace;
        let revision = workspace.skybox_revision();
        workspace.clear_skybox();
        assert!(workspace.skybox().is_none());
        assert_ne!(workspace.skybox_revision(), revision);

        let pixels = vec![1, 2, 3, 255];
        let faces = Face::ALL_CUBEMAP
            .map(|_| Image::from_rgba8(1, 1, pixels.clone()).expect("1x1 test face is valid"));
        workspace
            .lighting
            .with_skybox(Skybox::from_faces(faces).unwrap());
        assert_eq!(workspace.skybox().unwrap().face_size(), 1);
        assert_eq!(
            workspace.clone().skybox().unwrap().face_size(),
            1,
            "cloned workspaces keep their skybox"
        );
    }

    #[test]
    fn workspace_defaults_to_a_midday_sun() {
        let workspace = Workspace::new();
        let expected = Vec3::new(-0.45, 0.85, 0.35).normalize();
        assert!(
            (workspace.sun_direction() - expected).length() < 1e-6,
            "default sun should match DEFAULT_SUN_DIRECTION, got {:?}",
            workspace.sun_direction()
        );
        assert!((workspace.clock_time() - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clock_time_moves_the_sun_along_its_arc() {
        let noon = Workspace::sun_direction_from_clock_time(12.0);
        let expected_noon = Vec3::new(-0.45, 0.85, 0.35).normalize();
        assert!((noon - expected_noon).length() < 1e-6);

        let sunrise = Workspace::sun_direction_from_clock_time(6.0);
        assert!(
            (sunrise - Vec3::X).length() < 1e-5,
            "sunrise should sit on the eastern horizon, got {sunrise:?}"
        );

        let sunset = Workspace::sun_direction_from_clock_time(18.0);
        assert!(
            (sunset + Vec3::X).length() < 1e-5,
            "sunset should sit on the western horizon, got {sunset:?}"
        );

        let midnight = Workspace::sun_direction_from_clock_time(0.0);
        assert!(
            midnight.y < -0.5,
            "midnight sun should point below the horizon, got {midnight:?}"
        );

        assert!((Workspace::sun_direction_from_clock_time(24.0) - midnight).length() < 1e-6);
        assert!((Workspace::sun_direction_from_clock_time(36.0) - noon).length() < 1e-6);
        assert!((Workspace::sun_direction_from_clock_time(-6.0) - sunset).length() < 1e-6);
        assert!((Workspace::sun_direction_from_clock_time(f32::NAN) - noon).length() < 1e-6);
    }

    #[test]
    fn clock_time_round_trips_through_the_workspace() {
        let mut workspace = Workspace::new();
        for hours in [0.0, 5.5, 6.0, 9.25, 12.0, 15.75, 18.0, 23.5] {
            workspace.with_clock_time(hours);
            assert!((workspace.clock_time() - hours).abs() < f32::EPSILON);
            let expected = Workspace::sun_direction_from_clock_time(hours);
            assert!((workspace.sun_direction() - expected).length() < 1e-6);
        }

        workspace.with_clock_time(25.0);
        assert!((workspace.clock_time() - 1.0).abs() < f32::EPSILON);
        workspace.with_clock_time(f32::NAN);
        assert!((workspace.clock_time() - 12.0).abs() < f32::EPSILON);
    }

    #[test]
    fn custom_sun_directions_report_the_nearest_clock_time() {
        let mut workspace = Workspace::new();

        workspace.with_sun_direction(Vec3::X);
        assert!((workspace.sun_direction() - Vec3::X).length() < 1e-6);
        assert!((workspace.clock_time() - 6.0).abs() < 0.02);

        workspace.with_sun_direction(-Vec3::X);
        assert!((workspace.clock_time() - 18.0).abs() < 0.02);

        workspace.with_sun_direction(Vec3::NEG_Y);
        // Straight down is not on the sun path (midnight points below the
        // horizon along the tilted arc), so it reports a nearby nighttime hour.
        assert!(
            workspace.clock_time() < 2.0 || workspace.clock_time() > 20.0,
            "straight down should report a nighttime hour, got {}",
            workspace.clock_time()
        );

        // Directions on the sun path invert back to their hour.
        workspace.with_sun_direction(Workspace::sun_direction_from_clock_time(9.26));
        assert!((workspace.clock_time() - 9.26).abs() < 0.02);

        workspace.with_sun_direction(Vec3::new(1.0, 2.0, 3.0));
        assert!((workspace.sun_direction().length() - 1.0).abs() < 1e-6);

        workspace.with_sun_direction(Vec3::ZERO);
        assert_eq!(workspace.sun_direction(), Vec3::Y);

        workspace.with_sun_direction(Vec3::new(f32::NAN, 0.0, 0.0));
        assert_eq!(workspace.sun_direction(), Vec3::Y);
        assert!(workspace.sun_direction().is_finite());
    }

    #[test]
    fn cloned_workspaces_keep_their_sun_settings() {
        let mut workspace = Workspace::new();
        workspace.with_clock_time(17.5);
        workspace.lighting.brightness = 0.75;
        workspace.lighting.global_shadows = false;
        let cloned = workspace.clone();
        assert!((cloned.clock_time() - 17.5).abs() < f32::EPSILON);
        assert!((cloned.sun_direction() - workspace.sun_direction()).length() < 1e-6);
        assert_eq!(cloned.lighting.brightness, 0.75);
        assert!(!cloned.lighting.global_shadows);
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
