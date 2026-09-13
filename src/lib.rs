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

/// Stable identifier for an [`Instance`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstanceId(usize);

/// The 3D container that owns all renderable [`Part`] instances and its active camera.
#[derive(Clone, Debug, Default)]
pub struct Workspace {
    children: Vec<Part>,
    /// The camera used when this workspace is rendered.
    pub current_camera: Camera,
    camera_controller: CameraController,
}

impl Workspace {
    pub fn new() -> Self {
        Self::default()
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
        let mut part = Part::new(name);
        configure(&mut part);
        self.add_part(part)
    }

    pub fn add_part(&mut self, part: Part) -> InstanceId {
        let id = InstanceId(self.children.len());
        self.children.push(part);
        id
    }

    pub fn add_parts<I>(&mut self, parts: I) -> Vec<InstanceId>
    where
        I: IntoIterator<Item = Part>,
    {
        parts.into_iter().map(|part| self.add_part(part)).collect()
    }

    pub fn part(&self, id: InstanceId) -> Option<&Part> {
        self.children.get(id.0)
    }

    pub fn part_mut(&mut self, id: InstanceId) -> Option<&mut Part> {
        self.children.get_mut(id.0)
    }

    pub fn find_first_child(&mut self, name: &str) -> Option<(InstanceId, &mut Part)> {
        self.children
            .iter_mut()
            .position(|part| part.name == name)
            .map(|index| (InstanceId(index), &mut self.children[index]))
    }

    pub fn parts(&self) -> &[Part] {
        &self.children
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
}
