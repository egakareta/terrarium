mod basepart;
mod camera;
mod color3;
mod instance;
mod material;
mod renderer;
mod shape;
mod vertex;

use std::ops::{Deref, DerefMut};

pub use basepart::*;
pub use camera::*;
pub use color3::*;
pub use instance::*;
pub use material::*;
pub use renderer::*;
pub use shape::*;
pub use vertex::*;

#[derive(Clone, Debug)]
pub struct Part {
    basepart: BasePart,
    pub shape: PartShape,
    pub material: Material,
    pub material_slots: MeshMaterialSlots,
}

impl Part {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            basepart: BasePart::new(name),
            shape: PartShape::Block,
            material: Material::default(),
            material_slots: MeshMaterialSlots::default(),
        }
    }

    /// Assigns a material to a mesh-selected slot.
    pub fn set_material_slot(&mut self, slot: MaterialSlot, material: Material) {
        if slot == MaterialSlot::Base {
            self.material = material;
        } else {
            self.material_slots.set(slot, material);
        }
    }
}

crate::impl_instance!(Part, class_name = "Part", data = basepart.instance,);

impl Deref for Part {
    type Target = BasePart;
    fn deref(&self) -> &Self::Target {
        &self.basepart
    }
}

impl DerefMut for Part {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.basepart
    }
}

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

#[cfg(test)]
mod tests {
    use glam::Vec3;

    use super::*;

    #[test]
    fn part_position_and_orientation_accessors_use_the_pivot() {
        let position = Vec3::new(1.0, 2.0, 3.0);
        let orientation = Vec3::new(10.0, 20.0, 30.0);
        let mut part = Part::new("part");

        part.set_position(position);
        part.set_orientation(orientation);

        assert_eq!(part.position(), position);
        assert!((part.orientation() - orientation).abs().max_element() < 0.0001);
    }

    #[test]
    fn camera_mouse_look_rotates_in_place_with_the_expected_horizontal_sign() {
        let mut camera = Camera::default();
        let original_position = camera.pivot().w_axis.truncate();
        let mut controller = CameraController::new(6.0, 0.1);
        controller.mouse_delta = (1.0, 0.0);

        controller.update_camera(&mut camera, 1.0 / 60.0);

        assert_eq!(camera.pivot().w_axis.truncate(), original_position);
        assert!(camera.forward().x > 0.0);
    }

    #[test]
    fn primitive_meshes_have_valid_tangent_space_attributes() {
        let meshes = [
            Mesh::block(1.0, [1.0; 4]),
            Mesh::ball(1.0, 4, 8, [1.0; 4]),
            Mesh::cylinder(1.0, 1.0, 8, [1.0; 4]),
            Mesh::wedge([1.0; 4]),
            Mesh::corner_wedge([1.0; 4]),
            Mesh::plane(1.0, [1.0; 4]),
        ];

        for mesh in meshes {
            assert!(!mesh.vertices.is_empty());
            for vertex in mesh.vertices {
                let normal = Vec3::from_array(vertex.normal);
                let tangent = Vec3::from_array(vertex.tangent[..3].try_into().unwrap());
                assert!((normal.length() - 1.0).abs() < 0.0001);
                assert!((tangent.length() - 1.0).abs() < 0.0001);
                assert!(normal.dot(tangent).abs() < 0.0001);
                assert!(vertex.uv.iter().all(|coordinate| coordinate.is_finite()));
                assert!(vertex.tangent[3] == 1.0 || vertex.tangent[3] == -1.0);
            }
        }
    }

    #[test]
    fn textures_validate_rgba8_data_and_material_defaults_are_rough_dielectrics() {
        assert!(matches!(
            Texture::new(2, 2, vec![0; 3]),
            Err(TextureError::InvalidData {
                actual: 3,
                expected: 16
            })
        ));
        let texture = Texture::linear(1, 1, vec![128, 128, 255, 255]).unwrap();
        assert_eq!(texture.color_space, TextureColorSpace::Linear);

        let material = Material::default();
        assert_eq!(material.base_color, [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(material.metallic, 0.0);
        assert_eq!(material.roughness, 0.5);
        assert!(material.textures.base_color.is_none());
        assert!(material.textures.normal.is_none());
        assert!(material.textures.metallic_roughness.is_none());
    }

    #[test]
    fn block_mesh_uses_named_material_slots() {
        let mesh = Mesh::block(1.0, [1.0; 4]);

        assert!(
            mesh.vertices[0..4]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Side as u32)
        );
        assert!(
            mesh.vertices[8..12]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Top as u32)
        );
        assert!(
            mesh.vertices[12..16]
                .iter()
                .all(|vertex| vertex.material_slot == MaterialSlot::Bottom as u32)
        );
    }

    #[test]
    fn material_helpers_configure_a_texture_set() {
        let texture = TextureHandle(7);
        let material = Material::textured(texture)
            .with_normal_texture(TextureHandle(8))
            .with_metallic_roughness_texture(TextureHandle(9));

        assert_eq!(
            material.textures,
            TextureSet {
                base_color: Some(texture),
                normal: Some(TextureHandle(8)),
                metallic_roughness: Some(TextureHandle(9)),
            }
        );
    }

    #[test]
    fn built_in_objects_implement_instance_and_support_downcasting() {
        fn assert_instance<T: Instance>() {}

        assert_instance::<Workspace>();
        assert_instance::<BasePart>();
        assert_instance::<Camera>();
        assert_instance::<Part>();
        assert_eq!(Workspace::new().name(), "Workspace");
        assert_eq!(BasePart::new("base").name(), "base");
        assert_eq!(Camera::default().name(), "Camera");

        let mut instance: Box<dyn Instance> = Box::new(BasePart::new("base"));
        assert!(instance.is::<BasePart>());
        assert!(!instance.is::<Part>());
        assert_eq!(instance.downcast_ref::<BasePart>().unwrap().name(), "base");
        instance
            .downcast_mut::<BasePart>()
            .unwrap()
            .set_name("renamed".to_owned());
        assert_eq!(instance.name(), "renamed");

        let instance = match instance.downcast::<BasePart>() {
            Ok(instance) => instance,
            Err(_) => panic!("expected a BasePart"),
        };
        assert_eq!(instance.name(), "renamed");
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
    fn every_instance_can_own_a_nested_instance_tree() {
        let mut model = BasePart::new("model");
        let part_id = model.add_child_with(Part::new("part"), |part| {
            part.shape = PartShape::Ball;
        });
        let camera_id = Camera::default().set_parent(&mut model);
        let model_id = model.id();

        assert_eq!(model.children().len(), 2);
        assert_eq!(model.children()[0].id(), part_id);
        assert_eq!(model.children()[1].id(), camera_id);
        assert_eq!(model.children()[0].parent(), Some(model_id));

        let mut workspace = Workspace::new();
        model.set_parent(&mut workspace);
        assert_eq!(workspace.children().len(), 1);
        assert_eq!(
            workspace.instance(model_id).unwrap().class_name(),
            "BasePart"
        );
        assert_eq!(
            workspace.instance(part_id).unwrap().parent(),
            Some(model_id)
        );
        assert!(workspace.instance(camera_id).unwrap().is::<Camera>());
        assert_eq!(workspace.get_all::<Part>().count(), 1);
        assert_eq!(workspace.instances().count(), 3);
    }
}
