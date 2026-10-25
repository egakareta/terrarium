use trscene::{glam::*, *};

mod models {
    use super::*;

    instance!({
        #[instance(path = crate::models)]
        /// A custom primitive with authored spring stiffness.
        #[derive(Clone, Debug)]
        pub struct SpringPart {
            #[extends]
            part: Part,
            /// Spring stiffness in newtons per meter.
            #[property(default = 20.0)]
            stiffness: f32,
        }
    });
}

use models::{SpringPart, SpringPartExt};

instance!({
    /// A second level of custom composition.
    #[derive(Clone, Debug)]
    pub struct WeightedSpring {
        #[extends]
        spring: SpringPart,
        /// Mass of this spring's load.
        #[property(default = 2.0)]
        mass: f32,
    }
});

instance!({
    /// A custom assembly using the built-in rig transform semantics.
    #[derive(Clone, Debug)]
    pub struct CustomRig {
        #[extends]
        rig: Rig,
    }
});

instance!({
    /// A primitive that constrains its position to the horizontal plane.
    #[derive(Clone, Debug)]
    pub struct PlanarPart {
        #[extends]
        part: Part,
    }

    impl PlanarPart {
        #[r#override(PVInstance::set_pivot)]
        fn set_pivot(&mut self, mut pivot: glam::Mat4) {
            pivot.w_axis.y = 0.0;
            self.pv_instance_mut().set_pivot(pivot);
        }

        #[r#override(BasePart::set_color)]
        fn set_color(&mut self, _color: Color3) {
            self.base_part_mut().set_color(Color3::BLACK);
        }

        #[r#override(BasePart::set_material)]
        fn set_material(&mut self, _material: Material) {
            self.base_part_mut()
                .set_material(Material::from_color(Color3::BLACK));
        }
    }
});

instance!({
    /// A descendant that retains its parent's transform override.
    #[derive(Clone, Debug)]
    pub struct CustomPlanarPart {
        #[extends]
        part: PlanarPart,
    }
});

fn paint<T: BasePartExt + ?Sized>(part: &mut T, color: Color3) {
    part.set_color(color);
}

#[test]
fn custom_instances_keep_builders_generic_methods_identity_and_exact_downcasts() {
    let mut part = WeightedSpring::default()
        .with_stiffness(35.0)
        .with_mass(4.0)
        .with_shape(PartShape::Ball)
        .with_position(Vec3::new(1.0, 2.0, 3.0));
    part.spring_part_mut().set_stiffness(45.0);
    assert_eq!(part.spring_part().stiffness(), 45.0);
    let id = part.id();
    paint(&mut part, Color3::BLACK);
    (&mut part).with_transparency(0.5).with_stiffness(50.0);

    let mut workspace = Workspace::new();
    workspace.add_child(part);
    let instance = workspace.view::<dyn Instance>(id).unwrap();
    assert_eq!(instance.class_name(), "WeightedSpring");
    assert_eq!(instance.name(), "WeightedSpring");
    assert!(instance.downcast_ref::<WeightedSpring>().is_some());
    assert!(instance.downcast_ref::<Part>().is_none());
    assert!(instance.is_a::<Part>());
    assert!(instance.is_a::<BasePart>());
    assert!(instance.is_a::<PVInstance>());
    assert!(instance.is_a::<dyn Instance>());
    assert!(!instance.is_a::<Rig>());
    assert!(WeightedSpring::descriptor().is_a::<SpringPart>());
    assert!(WeightedSpring::descriptor().is_a::<PVInstance>());
    assert!(WeightedSpring::descriptor().is_a::<dyn Instance>());

    let view = workspace.view::<SpringPart>(id).unwrap();
    assert_eq!(view.id(), id);
    assert_eq!(view.class_name(), "WeightedSpring");
    assert_eq!(view.stiffness(), 50.0);
    assert_eq!(view.color(), Color3::BLACK);
    assert_eq!(view.transparency(), 0.5);
    assert_eq!(view.position(), Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(workspace.views::<BasePart>().count(), 1);
    assert!(workspace.get_all::<BasePart>().next().is_none());

    workspace
        .view_mut::<BasePart>(id)
        .unwrap()
        .set_color(Color3::WHITE);
    assert_eq!(
        workspace.get::<WeightedSpring>(id).unwrap().color(),
        Color3::WHITE
    );

    let cloned = workspace.get::<WeightedSpring>(id).unwrap().clone();
    assert_ne!(cloned.id(), id);
    assert_eq!(cloned.mass(), 4.0);
}

#[test]
fn spatial_instances_parent_children_in_local_coordinates() {
    fn check<T: Instance + PVInstanceExt>(mut parent: T) {
        let pivot = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 4.0),
            glam::Quat::from_rotation_y(0.6),
            Vec3::new(10.0, 5.0, -3.0),
        );
        let local = Mat4::from_scale_rotation_translation(
            Vec3::new(1.0, 2.0, 1.0),
            glam::Quat::from_rotation_x(0.4),
            Vec3::new(1.0, 2.0, 3.0),
        );
        parent.set_pivot(pivot);
        let child = parent.add_child_local(Part::new().with_pivot(local));
        let world_child = parent.add_child(Part::new().with_pivot(local));
        let parent_id = parent.id();
        let mut workspace = Workspace::new();
        workspace.add_child(parent);
        assert!(
            workspace
                .get::<Part>(child)
                .unwrap()
                .pivot()
                .abs_diff_eq(pivot * local, 0.0001)
        );
        assert_eq!(
            workspace.get::<Part>(child).unwrap().parent(),
            Some(parent_id)
        );
        assert_eq!(workspace.get::<Part>(world_child).unwrap().pivot(), local);
        let mut view = workspace.view_mut::<PVInstance>(parent_id).unwrap();
        let child_from_view = (&mut view).add_child_local(Part::new().with_pivot(local));
        assert!(
            workspace
                .get::<Part>(child_from_view)
                .unwrap()
                .pivot()
                .abs_diff_eq(pivot * local, 0.0001)
        );
        assert_eq!(
            workspace.get::<Part>(child_from_view).unwrap().parent(),
            Some(parent_id)
        );
    }
    check(BasePart::new());
    check(Part::new());
    check(Rig::new());
    check(CustomRig::default());
    check(SpringPart::default());
    #[cfg(feature = "meshpart")]
    check(MeshPart::new(Mesh::cube(1.0, [1.0; 4])));
}

#[test]
fn base_part_materials_are_available_on_every_subtype() {
    fn check<T: Instance + BasePartExt>(part: T) {
        let uniform = Material::from_color(Color3::BLACK);
        let top = Material::from_color(Color3::WHITE);
        assert_eq!(part.material(), Some(&Material::default()));
        let mut part = part
            .with_material(uniform)
            .with_material_slot(Face::Top, top);
        assert_eq!(part.material(), None);
        assert_eq!(part.material_slot(Face::Bottom), &uniform);
        assert_eq!(part.material_slot(Face::Top), &top);
        (&mut part).with_material(uniform);
        assert_eq!(part.material(), Some(&uniform));
        let mut workspace = Workspace::new();
        let id = workspace.add_child(part);
        workspace
            .view_mut::<BasePart>(id)
            .unwrap()
            .set_material_slot(Face::Top, top);
        assert_eq!(
            workspace
                .view::<BasePart>(id)
                .unwrap()
                .material_slot(Face::Top),
            &top
        );
    }
    check(BasePart::new());
    check(Part::new());
    check(SpringPart::default());
    #[cfg(feature = "meshpart")]
    check(MeshPart::new(Mesh::cube(1.0, [1.0; 4])));
}

#[test]
fn custom_primitives_render_with_the_outer_identity_and_materials() {
    let material = Material::from_color(Color3::BLACK);
    let mut workspace = Workspace::new();
    let id = workspace.add_child(
        SpringPart::default()
            .with_shape(PartShape::Ball)
            .with_material(material),
    );

    let scene = workspace.render_scene();
    let node = &scene.nodes[0];
    assert_eq!(node.name, "SpringPart");
    let mesh = node.mesh.as_ref().unwrap();
    assert!(matches!(
        mesh.geometry,
        RenderGeometry::Primitive(PartShape::Ball)
    ));
    assert_eq!(mesh.materials.get(Face::Top), Some(&material));
    assert_eq!(workspace.view::<BasePart>(id).unwrap().id(), id);
}

#[test]
fn rig_overrides_move_children_through_mutable_references_and_runtime_views() {
    let mut rig = CustomRig::default();
    let child = rig.add_child(Part::new().with_position(Vec3::X));
    (&mut rig).with_position(Vec3::new(5.0, 0.0, 0.0));
    assert_eq!(
        rig.view::<PVInstance>(child).unwrap().position(),
        Vec3::new(6.0, 0.0, 0.0)
    );

    let mut workspace = Workspace::new();
    let id = workspace.add_child(rig);
    workspace
        .view_mut::<PVInstance>(id)
        .unwrap()
        .set_position(Vec3::new(10.0, 0.0, 0.0));
    assert_eq!(
        workspace.get::<Part>(child).unwrap().position(),
        Vec3::new(11.0, 0.0, 0.0)
    );
    assert!(
        workspace
            .view::<CustomRig>(id)
            .unwrap()
            .accepts_pivot(Mat4::IDENTITY)
    );
    assert!(
        !workspace
            .view::<CustomRig>(id)
            .unwrap()
            .accepts_pivot(Mat4::ZERO)
    );
}

#[test]
fn instance_views_preserve_scope_identity_and_overrides_inside_and_outside_a_workspace() {
    fn check(parent: &mut dyn Instance, child: InstanceId) {
        let id = parent.id();
        assert_eq!(parent.view::<dyn Instance>(id).unwrap().id(), id);
        assert_eq!(parent.view::<Rig>(id).unwrap().id(), id);
        assert_eq!(parent.view::<BasePart>(child).unwrap().id(), child);
        assert!(parent.view::<Rig>(child).is_none());
        assert!(parent.view_mut::<Rig>(child).is_none());

        parent
            .view_mut::<PVInstance>(child)
            .unwrap()
            .set_position(Vec3::new(2.0, 8.0, 3.0));
        assert_eq!(
            parent.view::<PVInstance>(child).unwrap().position(),
            Vec3::new(2.0, 0.0, 3.0)
        );
        let view = parent.view::<Rig>(id).unwrap();
        assert_eq!(view.view::<CustomPlanarPart>(child).unwrap().id(), child);
    }

    let mut parent = Rig::new();
    let child = parent.add_child(CustomPlanarPart::default());
    check(&mut parent, child);

    let mut workspace = Workspace::new();
    let parent = workspace.add_child(parent);
    let sibling = workspace.add_child(Part::new());
    let root = workspace.id();
    {
        let parent = workspace.view_mut::<dyn Instance>(parent).unwrap();
        check(parent, child);
        assert!(parent.view::<dyn Instance>(root).is_none());
        assert!(parent.view_mut::<dyn Instance>(root).is_none());
        assert!(parent.view::<dyn Instance>(sibling).is_none());
        assert!(parent.view_mut::<dyn Instance>(sibling).is_none());
    }
    assert!(workspace.view_mut::<dyn Instance>(child).unwrap().destroy());
    assert!(workspace.view::<BasePart>(child).is_none());
    assert!(workspace.view_mut::<BasePart>(child).is_none());
}

#[test]
fn derived_setters_dispatch_to_custom_overrides_across_multiple_levels() {
    let mut part = CustomPlanarPart::default().with_position(Vec3::new(1.0, 7.0, 3.0));
    assert_eq!(part.position(), Vec3::new(1.0, 0.0, 3.0));
    (&mut part).with_material(Material::from_color(Color3::WHITE));
    assert_eq!(part.material(), Some(&Material::from_color(Color3::BLACK)));
    let mut workspace = Workspace::new();
    let id = workspace.add_child(part);
    workspace
        .view_mut::<PVInstance>(id)
        .unwrap()
        .set_position(Vec3::new(4.0, 8.0, 6.0));
    assert_eq!(
        workspace.get::<CustomPlanarPart>(id).unwrap().position(),
        Vec3::new(4.0, 0.0, 6.0)
    );
}

#[cfg(feature = "physics")]
#[test]
fn composed_primitives_participate_in_physics_and_overlap_queries() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(
        SpringPart::default()
            .with_anchored(false)
            .with_position(Vec3::new(0.0, 10.0, 0.0)),
    );
    for _ in 0..20 {
        workspace.step(1.0 / 60.0);
    }
    let position = workspace.view::<PVInstance>(id).unwrap().position();
    assert!(position.y < 10.0);
    let hits = workspace.get_part_bounds_in_box(&BoxOverlapQuery::new(
        Mat4::from_scale_rotation_translation(Vec3::splat(4.0), glam::Quat::IDENTITY, position),
    ));
    assert!(hits.contains(&id));
}

#[cfg(feature = "devtools")]
#[test]
fn inspector_edits_custom_properties_and_preserves_inherited_transform_semantics() {
    let mut workspace = Workspace::new();
    let spring = workspace.add_child(SpringPart::default());
    workspace
        .view_mut::<dyn Instance>(spring)
        .unwrap()
        .set_property("stiffness", devtools::PropertyValue::Number(42.0))
        .unwrap();
    assert_eq!(
        workspace.get::<SpringPart>(spring).unwrap().stiffness(),
        42.0
    );

    let mut rig = CustomRig::default();
    let child = rig.add_child(Part::new().with_position(Vec3::X));
    let id = workspace.add_child(rig);
    workspace
        .view_mut::<dyn Instance>(id)
        .unwrap()
        .set_property(
            "position",
            devtools::PropertyValue::Vector(vec![5.0, 0.0, 0.0]),
        )
        .unwrap();
    assert_eq!(
        workspace.get::<Part>(child).unwrap().position(),
        Vec3::new(6.0, 0.0, 0.0)
    );

    let planar = workspace.add_child(CustomPlanarPart::default());
    workspace
        .view_mut::<dyn Instance>(planar)
        .unwrap()
        .set_property("color", devtools::PropertyValue::Color([1.0; 3]))
        .unwrap();
    assert_eq!(
        workspace.view::<BasePart>(planar).unwrap().color(),
        Color3::BLACK
    );
    assert!(
        workspace
            .view_mut::<dyn Instance>(id)
            .unwrap()
            .set_property("size", devtools::PropertyValue::Vector(vec![0.0; 3]))
            .is_err()
    );
    assert_eq!(
        workspace.get::<Part>(child).unwrap().position(),
        Vec3::new(6.0, 0.0, 0.0)
    );
}

instance!({
    /// An independently reusable capability without a scene identity.
    #[derive(Clone, Debug)]
    pub struct Motor {
        /// Authored motor speed.
        #[property(default = 2.0)]
        speed: f32,
    }
});

instance!({
    /// A primitive composed with an independent motor capability.
    #[derive(Clone, Debug)]
    pub struct MotorPart {
        #[extends]
        part: Part,
        #[compose]
        motor: Motor,
    }
});

instance!({
    /// A scene node authored without a built-in parent type.
    #[derive(Clone, Debug)]
    pub struct Group {
        #[data]
        data: InstanceData,
        /// Application-owned group label.
        #[property(borrow, default = "group".into())]
        label: String,
    }
});

#[test]
fn independent_capabilities_and_new_root_types_keep_one_scene_identity() {
    let mut group = Group::default().with_label("assembly".into());
    let part = group.add_child(MotorPart::default().with_speed(8.0).with_position(Vec3::Y));
    let mut workspace = Workspace::new();
    let group = workspace.add_child(group);
    assert_eq!(workspace.view::<Group>(group).unwrap().label(), "assembly");
    assert_eq!(
        workspace.view::<dyn Instance>(part).unwrap().parent(),
        Some(group)
    );
    let motor = workspace.view_mut::<Motor>(part).unwrap();
    assert_eq!(motor.id(), part);
    assert_eq!(motor.class_name(), "MotorPart");
    motor.set_speed(12.0);
    assert_eq!(workspace.get::<MotorPart>(part).unwrap().speed(), 12.0);
    assert!(workspace.view::<BasePart>(group).is_none());
}

instance!({
    /// A custom light using the built-in point-light capability.
    #[derive(Clone, Debug)]
    pub struct CustomLight {
        #[extends]
        light: PointLight,
    }
});

#[test]
fn custom_lights_are_included_in_render_snapshots() {
    let mut part = Part::new();
    part.add_child(CustomLight::default().with_range(12.0).with_brightness(2.0));
    let mut workspace = Workspace::new();
    workspace.add_child(part);
    let scene = workspace.render_scene();
    let light = scene.nodes[0].children[0].light.as_ref().unwrap();
    assert!(matches!(light.kind, RenderLightKind::Point { range: 12.0 }));
    assert_eq!(light.brightness, 2.0);
}

#[cfg(feature = "meshpart")]
instance!({
    /// A custom mesh whose provider has an explicit default initializer.
    #[derive(Clone, Debug)]
    pub struct CustomMesh {
        #[extends]
        #[default(MeshPart::new(Mesh::block(1.0, [1.0; 4])))]
        mesh: MeshPart,
    }
});

#[cfg(feature = "meshpart")]
#[test]
fn custom_meshes_render_updated_geometry() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(CustomMesh::default().with_mesh(Mesh::block(2.0, [1.0; 4])));
    assert_eq!(
        workspace.view::<MeshPart>(id).unwrap().class_name(),
        "CustomMesh"
    );
    let scene = workspace.render_scene();
    let mesh = scene.nodes[0].mesh.as_ref().unwrap();
    assert!(matches!(
        mesh.geometry,
        RenderGeometry::Custom { revision: 1, .. }
    ));
}

#[cfg(feature = "sound")]
instance!({
    /// A custom sound using the built-in audio capability.
    #[derive(Clone, Debug)]
    pub struct CustomSound {
        #[extends]
        sound: Sound,
    }
});

#[cfg(feature = "sound")]
#[test]
fn custom_sounds_are_updated_by_the_audio_runtime() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(CustomSound::default().with_volume(0.4));
    assert!(workspace.step_audio(0.0).is_empty());
    assert!(workspace.view::<Sound>(id).unwrap().playback().is_some());
    assert_eq!(workspace.view::<Sound>(id).unwrap().volume(), 0.4);
}
