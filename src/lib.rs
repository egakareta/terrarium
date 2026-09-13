mod camera;
mod color3;
mod instance;
mod material;
mod part;
mod renderer;
mod shape;
mod vertex;
mod workspace;

pub use camera::*;
pub use color3::*;
pub use instance::*;
pub use material::*;
pub use part::*;
pub use renderer::*;
pub use shape::*;
pub use vertex::*;
pub use workspace::*;

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
