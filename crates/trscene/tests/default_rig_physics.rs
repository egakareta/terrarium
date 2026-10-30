#![cfg(all(feature = "default-player-rig", feature = "physics"))]

use trscene::{glam::*, *};

const DT: f32 = 1.0 / 120.0;

fn advance(
    workspace: &mut Workspace,
    player: &mut PlayerController,
    actions: ControllerActions,
    frames: usize,
) {
    for _ in 0..frames {
        workspace.step_animations(DT);
        player.step_workspace_with_actions(workspace, actions, DT);
        workspace.step_physics(DT);
        player.post_physics(workspace, DT);
    }
}

#[test]
fn soft_tissue_weights_preserve_the_original_animation_without_displacement() {
    let mut workspace = Workspace::new();
    let player = PlayerController::spawn(&mut workspace);
    let id = player.rig().unwrap();
    let mesh = workspace
        .get_mut::<Rig>(id)
        .unwrap()
        .find_first_child::<MeshPart>("Mannequin")
        .unwrap()
        .0;
    let mut reference =
        Rig::from_gltf(include_bytes!("../assets/UAL1_Lite.glb"), &mut workspace).unwrap();
    reference
        .find_first_child::<Rig>("Armature")
        .unwrap()
        .1
        .with_orientation(Vec3::new(0.0, 180.0, 0.0));
    let reference_mesh = reference
        .find_first_child::<MeshPart>("Mannequin")
        .unwrap()
        .0;
    for name in ["Idle_Loop", "Walk_Loop", "Sprint_Loop", "Jump_Loop"] {
        let clip = workspace.get::<Rig>(id).unwrap().animation_clips()[name].clone();
        let rig = workspace.get_mut::<Rig>(id).unwrap();
        rig.play_animation(AnimationPlayer::new(clip.clone()))
            .unwrap();
        reference
            .play_animation(AnimationPlayer::new(clip))
            .unwrap();
        for dt in [0.0, 0.2, 0.3] {
            rig.step_animation(dt);
            reference.step_animation(dt);
            for (actual, expected) in rig
                .view::<MeshPart>(mesh)
                .unwrap()
                .mesh()
                .vertices
                .iter()
                .zip(
                    &reference
                        .view::<MeshPart>(reference_mesh)
                        .unwrap()
                        .mesh()
                        .vertices,
                )
            {
                assert!(
                    Vec3::from_array(actual.position)
                        .abs_diff_eq(Vec3::from_array(expected.position), 0.0001)
                );
                assert!(Vec3::from_array(actual.normal).is_normalized());
            }
        }
    }
}

#[test]
fn chest_belly_and_glute_impulses_deform_only_their_local_skin_regions() {
    for (name, min, max) in [
        (
            "SoftChestL",
            Vec3::new(-0.02, 1.23, -0.05),
            Vec3::new(0.23, 1.57, 0.23),
        ),
        (
            "SoftChestR",
            Vec3::new(-0.23, 1.23, -0.05),
            Vec3::new(0.02, 1.57, 0.23),
        ),
        (
            "SoftBelly",
            Vec3::new(-0.18, 0.96, -0.06),
            Vec3::new(0.18, 1.30, 0.24),
        ),
        (
            "SoftGluteL",
            Vec3::new(-0.04, 0.77, -0.22),
            Vec3::new(0.22, 1.07, 0.04),
        ),
        (
            "SoftGluteR",
            Vec3::new(-0.22, 0.77, -0.22),
            Vec3::new(0.04, 1.07, 0.04),
        ),
    ] {
        let mut workspace = Workspace::new();
        let mut player = PlayerController::spawn(&mut workspace);
        let rig = workspace.get_mut::<Rig>(player.rig().unwrap()).unwrap();
        rig.with_anchored(true);
        rig.animation_mut().unwrap().set_paused(true);
        let mesh = rig.find_first_child::<MeshPart>("Mannequin").unwrap().0;
        let tissues: Vec<_> = rig
            .descendants()
            .filter(|node| node.name() == name)
            .map(Instance::id)
            .collect();
        workspace.step_physics(0.0);
        let before = workspace.get::<MeshPart>(mesh).unwrap().mesh().clone();
        for id in tissues {
            workspace
                .physics_mut()
                .body_mut(id)
                .unwrap()
                .apply_impulse(rapier3d::math::Vector::new(0.0, 0.0, 0.3), true);
        }
        advance(
            &mut workspace,
            &mut player,
            ControllerActions::default(),
            12,
        );
        let part = workspace.get::<MeshPart>(mesh).unwrap();
        let mut changed = 0;
        for ((before, after), rest) in before
            .vertices
            .iter()
            .zip(&part.mesh().vertices)
            .zip(&part.mesh_handle().mesh().vertices)
        {
            let distance =
                Vec3::from_array(before.position).distance(Vec3::from_array(after.position));
            let position = Vec3::from_array(rest.position);
            if position.cmpge(min).all() && position.cmple(max).all() {
                if distance > 0.0001 {
                    changed += 1;
                }
            } else {
                assert!(
                    distance < 0.0001,
                    "{name}: unrelated vertex {position:?} moved {distance}"
                );
            }
            assert!(Vec3::from_array(after.normal).is_normalized());
        }
        assert!(changed > 0, "{name} did not deform the skin");
    }
}

#[test]
fn translation_jiggle_is_bounded_settles_and_does_not_push_the_character() {
    let mut rig = Rig::new().with_anchored(false);
    let tissue = rig.add_child(Rig::new().with_name("Tissue").with_position(Vec3::Y));
    rig.set_physics_joints(vec![RigJoint::new(["Tissue"]).with_motion(
        RigJointMotion::SoftTissue {
            stiffness: 180.0,
            damping: 14.0,
            max_displacement: Vec3::splat(0.035),
        },
    )])
    .unwrap();
    let mut workspace = Workspace::new();
    workspace.physics_mut().with_gravity(Vec3::ZERO);
    let root = workspace.add_child(rig);
    workspace.step_physics(0.0);
    workspace
        .physics_mut()
        .body_mut(tissue)
        .unwrap()
        .apply_impulse(rapier3d::math::Vector::new(3.0, 0.0, 0.0), true);
    let mut displaced = false;
    for _ in 0..360 {
        workspace.step(DT);
        let position = workspace.get::<Rig>(tissue).unwrap().position();
        displaced |= position.x.abs() > 0.005;
        assert!(position.distance(Vec3::Y) < 0.045, "{position:?}");
        assert!(workspace.get::<Rig>(root).unwrap().position().length() < 0.0001);
    }
    assert!(displaced);
    assert!(
        workspace
            .get::<Rig>(tissue)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::Y, 0.001)
    );
}

#[test]
fn built_in_ragdoll_collides_holds_animation_and_can_resume_player_movement() {
    let mut workspace = Workspace::new();
    workspace.add_child(
        Part::new()
            .with_size(Vec3::new(20.0, 1.0, 20.0))
            .with_position(Vec3::NEG_Y * 0.5),
    );
    let mut player = PlayerController::spawn(&mut workspace);
    let root = player.rig().unwrap();
    workspace
        .get_mut::<Rig>(root)
        .unwrap()
        .with_position(Vec3::Y * 2.0);
    advance(&mut workspace, &mut player, ControllerActions::default(), 1);
    let rig = workspace.get_mut::<Rig>(root).unwrap();
    let head = rig.find_first_child::<Rig>("Head").unwrap().0;
    let pelvis = rig.find_first_child::<Rig>("pelvis").unwrap().0;
    let mesh = rig.find_first_child::<MeshPart>("Mannequin").unwrap().0;
    let time = rig.animation().unwrap().time();
    rig.set_ragdoll_enabled(true).unwrap();
    let actions = ControllerActions {
        movement: Vec3::Z,
        ..Default::default()
    };
    advance(&mut workspace, &mut player, actions, 360);
    let rig = workspace.get::<Rig>(root).unwrap();
    assert_eq!(rig.animation().unwrap().time(), time);
    assert!(rig.position().z.abs() < 1.5, "{:?}", rig.position());
    for id in [head, pelvis] {
        let position = workspace.get::<Rig>(id).unwrap().position();
        assert!(
            position.is_finite() && position.y > -0.05 && position.y < 1.9,
            "{position:?}"
        );
    }
    for vertex in &workspace.get::<MeshPart>(mesh).unwrap().mesh().vertices {
        assert!(Vec3::from_array(vertex.position).is_finite());
        assert!(Vec3::from_array(vertex.normal).is_normalized());
    }
    let tissue = workspace
        .get::<Rig>(root)
        .unwrap()
        .descendants()
        .find(|node| node.name() == "SoftBelly")
        .unwrap();
    let (tissue, parent) = (tissue.id(), tissue.parent().unwrap());
    let relative = |workspace: &Workspace| {
        workspace
            .get::<Rig>(parent)
            .unwrap()
            .pivot()
            .inverse()
            .transform_point3(workspace.get::<Rig>(tissue).unwrap().position())
    };
    let rest = relative(&workspace);
    workspace
        .physics_mut()
        .body_mut(tissue)
        .unwrap()
        .apply_impulse(rapier3d::math::Vector::new(0.0, 0.0, 0.3), true);
    advance(&mut workspace, &mut player, actions, 6);
    let displacement = relative(&workspace).distance(rest);
    assert!(
        displacement > 0.0002 && displacement < 0.065,
        "{displacement}"
    );
    workspace
        .get_mut::<Rig>(root)
        .unwrap()
        .set_ragdoll_enabled(false)
        .unwrap();
    let before = workspace.get::<Rig>(root).unwrap().position();
    advance(&mut workspace, &mut player, actions, 120);
    let rig = workspace.get::<Rig>(root).unwrap();
    assert!(!rig.ragdoll_enabled());
    assert!(
        rig.position().z < before.z - 0.9,
        "{:?} -> {:?}",
        before,
        rig.position()
    );
    assert!((rig.pose().transform_vector3(Vec3::Y)).abs_diff_eq(Vec3::Y, 0.001));
    assert_ne!(rig.animation().unwrap().time(), time);
}
