#![cfg(feature = "meshpart")]

use trscene::{glam::*, *};

#[test]
fn default_players_move_and_animate_without_custom_assets_or_state_bindings() {
    let mut workspace = Workspace::new();
    let camera = workspace.camera().pivot();
    let mut player = PlayerController::spawn(&mut workspace);
    let id = player.rig.unwrap();
    let rig = workspace.get_mut::<Rig>(id).unwrap();
    assert!(!rig.anchored());
    assert!(rig.position().abs_diff_eq(Vec3::ZERO, 1e-5));
    assert!(rig.animation().is_some());
    let mesh = rig.find_first_child::<MeshPart>("Mannequin").unwrap().0;
    rig.with_anchored(true)
        .with_position(Vec3::new(3.0, 4.0, 5.0));
    let idle = workspace.get::<MeshPart>(mesh).unwrap().mesh().clone();

    let walking = ControllerActions {
        movement: Vec3::Z,
        ..Default::default()
    };
    let walking_distance = player.speed * 0.1;
    player.step_workspace_with_actions(&mut workspace, walking, 0.1);
    workspace.step_animations(0.3);
    assert_eq!(player.animation_state(), PlayerAnimationState::Walk);
    assert!(
        workspace
            .get::<Rig>(id)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::new(3.0, 4.0, 5.0 - walking_distance), 1e-5)
    );
    assert!(
        idle.vertices
            .iter()
            .zip(&workspace.get::<MeshPart>(mesh).unwrap().mesh().vertices)
            .any(|(before, after)| {
                !Vec3::from_array(before.position)
                    .abs_diff_eq(Vec3::from_array(after.position), 0.001)
            })
    );

    player.step_workspace_with_actions(
        &mut workspace,
        ControllerActions {
            movement_scale: Vec3::splat(2.5),
            ..walking
        },
        0.1,
    );
    workspace.step_animations(0.3);
    assert_eq!(player.animation_state(), PlayerAnimationState::Sprint);
    assert!(
        workspace
            .get::<Rig>(id)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::new(3.0, 4.0, 5.0 - walking_distance * 3.5), 1e-5)
    );
    player.step_workspace_with_actions(&mut workspace, ControllerActions::default(), 0.1);
    workspace.step_animations(0.3);
    assert_eq!(player.animation_state(), PlayerAnimationState::Idle);
    assert!(workspace.camera().pivot().abs_diff_eq(camera, 1e-5));
}

#[test]
fn imported_skins_follow_joint_poses_without_changing_the_source_or_clones() {
    let bytes = include_bytes!("../assets/UAL1_Lite.glb");
    let mut workspace = Workspace::new();
    let mut rig = Rig::from_gltf(bytes, &mut workspace).unwrap();
    let (mesh_id, mesh) = rig.find_first_child::<MeshPart>("Mannequin").unwrap();
    let original = mesh.mesh().clone();
    let rest = rig.find_first_child::<Rig>("root").unwrap().1.pivot();
    let independent = rig.clone();
    rig.play_animation(AnimationPlayer::new(AnimationClip::new([
        AnimationTrack::new(
            ["Armature", "root"],
            [
                Keyframe::new(0.0, rest),
                Keyframe::new(1.0, Mat4::from_translation(Vec3::X) * rest),
            ],
        ),
    ])))
    .unwrap();
    rig.step_animation(0.5);
    let mesh = rig.view::<MeshPart>(mesh_id).unwrap();
    assert_eq!(mesh.mesh_handle().gltf().unwrap().source_bytes(), bytes);
    for (before, after) in original.vertices.iter().zip(&mesh.mesh().vertices) {
        assert!(
            Vec3::from_array(after.position)
                .abs_diff_eq(Vec3::from_array(before.position) + Vec3::X * 0.5, 0.0001)
        );
        assert!(
            Vec3::from_array(after.normal).abs_diff_eq(Vec3::from_array(before.normal), 0.0001)
        );
        assert!(after.tangent.into_iter().all(f32::is_finite));
    }
    let unchanged = independent
        .descendants()
        .find_map(|instance| instance.downcast_ref::<MeshPart>())
        .unwrap();
    assert_eq!(
        unchanged.mesh().vertices[0].position,
        original.vertices[0].position
    );
    rig = rig
        .with_position(Vec3::splat(10.0))
        .with_size(Vec3::splat(2.0));
    rig.step_animation(0.25);
    workspace.add_child(rig);
    let scene = workspace.render_scene();
    let (node, transform) = scene
        .nodes()
        .find(|(node, _)| node.name == "Mannequin")
        .unwrap();
    let RenderGeometry::Custom { mesh, .. } = node.mesh.as_ref().unwrap().geometry else {
        panic!("expected skinned geometry")
    };
    assert!(
        mesh.vertices
            .iter()
            .all(|vertex| Vec3::from_array(vertex.position).length()
                <= node.mesh.as_ref().unwrap().bounding_radius + 0.0001)
    );
    let expected = Vec3::splat(10.0)
        + (Vec3::from_array(original.vertices[0].position) + Vec3::X * 0.75) * 2.0;
    assert!(
        transform
            .transform_point3(Vec3::from_array(mesh.vertices[0].position))
            .abs_diff_eq(expected, 0.0001)
    );
}

#[test]
fn every_default_animation_produces_finite_skinned_geometry() {
    let mut workspace = Workspace::new();
    let player = PlayerController::spawn(&mut workspace);
    let rig = workspace.get_mut::<Rig>(player.rig.unwrap()).unwrap();
    let mesh = rig.find_first_child::<MeshPart>("Mannequin").unwrap().0;
    let clips: Vec<_> = rig.animation_clips().values().cloned().collect();
    for clip in clips {
        let duration = clip.duration();
        rig.play_animation(AnimationPlayer::new(clip)).unwrap();
        for delta in [0.0, duration * 0.5, duration * 0.5] {
            rig.step_animation(delta);
            for vertex in &rig.view::<MeshPart>(mesh).unwrap().mesh().vertices {
                assert!(Vec3::from_array(vertex.position).is_finite());
                assert!(Vec3::from_array(vertex.normal).is_normalized());
                assert!(vertex.tangent.into_iter().all(f32::is_finite));
            }
        }
    }
}
