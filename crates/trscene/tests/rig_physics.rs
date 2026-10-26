#![cfg(feature = "physics")]

use trscene::{glam::*, *};

const DT: f32 = 1.0 / 120.0;

fn advance(workspace: &mut Workspace, steps: usize) {
    for _ in 0..steps {
        workspace.step(DT);
    }
}

fn torque(workspace: &mut Workspace, id: InstanceId, z: f32) {
    workspace
        .physics_mut()
        .body_mut(id)
        .unwrap()
        .apply_torque_impulse(rapier3d::math::Vector::new(0.0, 0.0, z), true);
}

#[test]
fn ragdoll_swings_under_gravity_and_carries_unconfigured_descendants() {
    let mut rig = Rig::new().with_position(Vec3::Y * 3.0);
    let mut arm = Part::new()
        .with_name("Arm")
        .with_size(Vec3::new(2.0, 0.25, 0.25))
        .with_position(Vec3::new(1.0, 3.0, 0.0));
    let mut light = PointLight::new();
    let marker = light.add_child(
        Part::new()
            .with_can_collide(false)
            .with_position(Vec3::new(2.0, 3.0, 0.0)),
    );
    arm.add_child(light);
    let arm = rig.add_child(arm);
    rig.set_physics_joints(vec![RigJoint::new(["Arm"]).with_anchor(Vec3::NEG_X * 0.5)])
        .unwrap();
    let mut workspace = Workspace::new();
    let root = workspace.add_child(rig);
    advance(&mut workspace, 60);

    let arm = workspace.get::<Part>(arm).unwrap();
    assert!(arm.position().y < 2.5);
    assert!(
        arm.pivot()
            .transform_point3(Vec3::NEG_X * 0.5)
            .abs_diff_eq(Vec3::Y * 3.0, 0.02)
    );
    let expected = arm.pose().transform_point3(Vec3::X);
    assert!(
        workspace
            .get::<Part>(marker)
            .unwrap()
            .position()
            .abs_diff_eq(expected, 0.001)
    );
    assert_eq!(
        workspace.get::<Rig>(root).unwrap().position(),
        Vec3::Y * 3.0
    );
}

#[test]
fn dynamic_ragdolls_collide_with_the_environment() {
    let mut workspace = Workspace::new();
    workspace.add_child(
        Part::new()
            .with_size(Vec3::new(20.0, 1.0, 20.0))
            .with_position(Vec3::NEG_Y * 0.5),
    );
    let mut rig = Rig::new().with_anchored(false).with_position(Vec3::Y * 4.0);
    let torso = rig.add_child(Part::new().with_position(Vec3::Y * 4.0));
    let arm = rig.add_child(
        Part::new()
            .with_name("Arm")
            .with_position(Vec3::new(1.0, 4.0, 0.0)),
    );
    rig.set_physics_joints(vec![RigJoint::new(["Arm"]).with_anchor(Vec3::NEG_X * 0.5)])
        .unwrap();
    let root = workspace.add_child(rig);
    advance(&mut workspace, 480);

    assert!(workspace.get::<Rig>(root).unwrap().position().y < 2.0);
    for id in [torso, arm] {
        let position = workspace.get::<Part>(id).unwrap().position();
        assert!(position.is_finite());
        assert!(position.y > 0.4 && position.y < 1.5, "{position:?}");
    }
}

#[test]
fn angular_limits_constrain_ragdolls() {
    let mut rig = Rig::new();
    let arm = rig.add_child(Part::new().with_name("Arm"));
    rig.set_physics_joints(vec![
        RigJoint::new(["Arm"]).with_angular_limits(Vec3::new(0.0, 0.0, 0.3)),
    ])
    .unwrap();
    let mut workspace = Workspace::new();
    workspace.physics_mut().with_gravity(Vec3::ZERO);
    workspace.add_child(rig);
    workspace.step_physics(0.0);
    torque(&mut workspace, arm, 0.5);
    advance(&mut workspace, 120);
    let rotation = workspace
        .get::<Part>(arm)
        .unwrap()
        .pivot()
        .to_scale_rotation_translation()
        .1;
    assert!(rotation.angle_between(Quat::IDENTITY) > 0.1);
    assert!(rotation.angle_between(Quat::IDENTITY) < 0.33);
    assert!((rotation * Vec3::Z).abs_diff_eq(Vec3::Z, 0.001));
}

#[test]
fn jiggle_damps_impulses_and_returns_to_its_rest_pose() {
    let mut rig = Rig::new();
    let arm = rig.add_child(Part::new().with_name("Arm"));
    rig.set_physics_joints(vec![RigJoint::new(["Arm"]).with_motion(
        RigJointMotion::Jiggle {
            stiffness: 40.0,
            damping: 8.0,
        },
    )])
    .unwrap();
    let mut workspace = Workspace::new();
    workspace.physics_mut().with_gravity(Vec3::ZERO);
    workspace.add_child(rig);
    workspace.step_physics(0.0);
    torque(&mut workspace, arm, 0.5);
    advance(&mut workspace, 12);
    let displaced = workspace.get::<Part>(arm).unwrap().pose();
    assert!(!displaced.abs_diff_eq(Mat4::IDENTITY, 0.01));
    advance(&mut workspace, 480);
    assert!(
        workspace
            .get::<Part>(arm)
            .unwrap()
            .pose()
            .abs_diff_eq(Mat4::IDENTITY, 0.01)
    );
}

#[test]
fn jiggle_follows_completed_animation_while_ragdoll_is_not_animation_driven() {
    for motion in [
        RigJointMotion::Ragdoll,
        RigJointMotion::Jiggle {
            stiffness: 100.0,
            damping: 20.0,
        },
    ] {
        let mut rig = Rig::new();
        let arm = rig.add_child(Part::new().with_name("Arm"));
        rig.set_physics_joints(vec![RigJoint::new(["Arm"]).with_motion(motion)])
            .unwrap();
        let mut workspace = Workspace::new();
        workspace.physics_mut().with_gravity(Vec3::ZERO);
        let root = workspace.add_child(rig);
        workspace.step_physics(0.0);
        let target = Mat4::from_rotation_z(1.0);
        workspace
            .get_mut::<Rig>(root)
            .unwrap()
            .play_animation(AnimationPlayer::new(AnimationClip::new([
                AnimationTrack::new(["Arm"], [Keyframe::new(0.0, target)]),
            ])))
            .unwrap();
        workspace.step(DT);
        let first = workspace.get::<Part>(arm).unwrap().pose();
        assert!(!first.abs_diff_eq(target, 0.01));
        advance(&mut workspace, 480);
        let expected = match motion {
            RigJointMotion::Ragdoll => Mat4::IDENTITY,
            RigJointMotion::Jiggle { .. } => target,
            RigJointMotion::SoftTissue { .. } => unreachable!(),
        };
        assert!(
            workspace
                .get::<Part>(arm)
                .unwrap()
                .pose()
                .abs_diff_eq(expected, 0.01)
        );
    }
}

#[test]
fn chains_preserve_attachments_when_the_rig_is_teleported() {
    let mut rig = Rig::new();
    let mut arm = Part::new().with_name("Arm").with_position(Vec3::X);
    let hand = arm.add_child(Part::new().with_name("Hand").with_position(Vec3::X * 2.0));
    let arm = rig.add_child(arm);
    rig.set_physics_joints(vec![
        RigJoint::new(["Arm", "Hand"]).with_anchor(Vec3::NEG_X * 0.5),
        RigJoint::new(["Arm"]).with_anchor(Vec3::NEG_X * 0.5),
    ])
    .unwrap();
    let mut workspace = Workspace::new();
    let root = workspace.add_child(rig);
    advance(&mut workspace, 60);
    let before = workspace.get::<Part>(hand).unwrap().pose();
    let delta = Mat4::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::splat(10.0));
    workspace.get_mut::<Rig>(root).unwrap().with_pivot(delta);
    workspace.step(0.0);
    assert!(
        workspace
            .get::<Part>(hand)
            .unwrap()
            .pose()
            .abs_diff_eq(delta * before, 0.001)
    );
    advance(&mut workspace, 120);
    let arm = workspace.get::<Part>(arm).unwrap();
    let hand = workspace.get::<Part>(hand).unwrap();
    assert!(
        arm.pivot()
            .transform_point3(Vec3::NEG_X * 0.5)
            .abs_diff_eq(delta.transform_point3(Vec3::X * 0.5), 0.02)
    );
    assert!(
        hand.pivot()
            .transform_point3(Vec3::NEG_X * 0.5)
            .abs_diff_eq(arm.pivot().transform_point3(Vec3::X * 0.5), 0.02)
    );
}

#[test]
fn invalid_joint_configuration_is_atomic() {
    let mut rig = Rig::new();
    rig.add_child(Part::new().with_name("Arm"));
    rig.add_child(PointLight::new().with_name("Light"));
    let original = vec![RigJoint::new(["Arm"])];
    rig.set_physics_joints(original.clone()).unwrap();
    for joints in [
        vec![RigJoint::new(["Missing"])],
        vec![RigJoint::new(["Light"])],
        vec![RigJoint::new(["Arm"]), RigJoint::new(["Arm"])],
        vec![RigJoint::new(["Arm"]).with_anchor(Vec3::NAN)],
        vec![RigJoint::new(["Arm"]).with_angular_limits(Vec3::splat(-1.0))],
        vec![RigJoint::new(["Arm"]).with_motion(RigJointMotion::Jiggle {
            stiffness: f32::NAN,
            damping: 1.0,
        })],
    ] {
        assert!(rig.set_physics_joints(joints).is_err());
        assert_eq!(rig.physics_joints(), original);
    }
}

#[test]
fn joints_survive_cloning_and_can_be_removed_at_runtime() {
    let mut rig = Rig::new();
    rig.add_child(Part::new().with_name("Arm"));
    rig.set_physics_joints(vec![RigJoint::new(["Arm"])])
        .unwrap();
    let mut workspace = Workspace::new();
    workspace.physics_mut().with_gravity(Vec3::ZERO);
    let original = workspace.add_child(rig.clone());
    let clone = workspace.add_child(rig.with_position(Vec3::X * 10.0));
    let arm = workspace
        .get_mut::<Rig>(clone)
        .unwrap()
        .find_first_child::<Part>("Arm")
        .unwrap()
        .0;
    workspace.step_physics(0.0);
    torque(&mut workspace, arm, 0.5);
    advance(&mut workspace, 30);
    let original_arm = workspace
        .get_mut::<Rig>(original)
        .unwrap()
        .find_first_child::<Part>("Arm")
        .unwrap()
        .0;
    assert!(
        workspace
            .get::<Part>(original_arm)
            .unwrap()
            .pose()
            .abs_diff_eq(Mat4::IDENTITY, 0.001)
    );
    assert!(
        !workspace
            .get::<Part>(arm)
            .unwrap()
            .pose()
            .abs_diff_eq(Mat4::from_translation(Vec3::X * 10.0), 0.01)
    );

    workspace
        .get_mut::<Rig>(clone)
        .unwrap()
        .set_physics_joints(Vec::new())
        .unwrap();
    let stopped = workspace.get::<Part>(arm).unwrap().pivot();
    advance(&mut workspace, 30);
    assert!(
        workspace
            .get::<Part>(arm)
            .unwrap()
            .pivot()
            .abs_diff_eq(stopped, 0.001)
    );
    workspace.get_mut::<Part>(original_arm).unwrap().destroy();
    advance(&mut workspace, 2);
    assert!(workspace.get::<Part>(original_arm).is_none());
}

#[cfg(feature = "meshpart")]
#[test]
fn imported_skin_geometry_follows_physics_bones() {
    let mut workspace = Workspace::new();
    workspace.physics_mut().with_gravity(Vec3::ZERO);
    let mut rig =
        Rig::from_gltf(include_bytes!("../assets/UAL1_Lite.glb"), &mut workspace).unwrap();
    let (mesh_id, mesh) = rig.find_first_child::<MeshPart>("Mannequin").unwrap();
    mesh.with_can_collide(false);
    let original = mesh.mesh().clone();
    let mesh_pivot = mesh.pivot();
    let (bone, node) = rig.find_first_child::<Rig>("root").unwrap();
    let rest = node.pivot();
    rig.set_physics_joints(vec![RigJoint::new(["Armature", "root"])])
        .unwrap();
    workspace.add_child(rig);
    workspace.step_physics(0.0);
    torque(&mut workspace, bone, 1.0);
    advance(&mut workspace, 30);
    let delta = mesh_pivot.inverse()
        * workspace.get::<Rig>(bone).unwrap().pivot()
        * rest.inverse()
        * mesh_pivot;
    assert!(!delta.abs_diff_eq(Mat4::IDENTITY, 0.01));
    let mesh = workspace.get::<MeshPart>(mesh_id).unwrap();
    for (before, after) in original.vertices.iter().zip(&mesh.mesh().vertices) {
        assert!(Vec3::from_array(after.position).abs_diff_eq(
            delta.transform_point3(Vec3::from_array(before.position)),
            0.0002
        ));
        assert!(Vec3::from_array(after.normal).is_normalized());
    }
}
