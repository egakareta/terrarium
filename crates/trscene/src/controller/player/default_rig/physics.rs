use std::collections::BTreeMap;

use crate::{glam::*, *};

pub(super) fn install(rig: &mut Rig) {
    fn collect(
        node: &dyn Instance,
        path: &mut Vec<String>,
        nodes: &mut BTreeMap<String, (Vec<String>, Mat4)>,
    ) {
        for child in node.children() {
            path.push(child.name().to_owned());
            if let Some(spatial) = child.spatial() {
                nodes.insert(
                    child.name().to_owned(),
                    (path.clone(), spatial.world_pivot()),
                );
            }
            collect(child.as_ref(), path, nodes);
            path.pop();
        }
    }
    let mut nodes = BTreeMap::new();
    collect(rig, &mut Vec::new(), &mut nodes);
    let mut animated = Vec::new();
    for (name, center, radius, parents, stiffness, damping) in [
        (
            "SoftChestL",
            Vec3::new(0.105, 1.40, 0.09),
            Vec3::new(0.12, 0.16, 0.13),
            &["spine_02", "spine_03", "clavicle_l"][..],
            220.0,
            18.0,
        ),
        (
            "SoftChestR",
            Vec3::new(-0.105, 1.40, 0.09),
            Vec3::new(0.12, 0.16, 0.13),
            &["spine_02", "spine_03", "clavicle_r"][..],
            220.0,
            18.0,
        ),
        (
            "SoftBelly",
            Vec3::new(0.0, 1.13, 0.09),
            Vec3::new(0.17, 0.16, 0.14),
            &["pelvis", "spine_01", "spine_02"][..],
            140.0,
            12.0,
        ),
        (
            "SoftGluteL",
            Vec3::new(0.09, 0.92, -0.09),
            Vec3::new(0.12, 0.14, 0.12),
            &["pelvis", "thigh_l"][..],
            180.0,
            14.0,
        ),
        (
            "SoftGluteR",
            Vec3::new(-0.09, 0.92, -0.09),
            Vec3::new(0.12, 0.14, 0.12),
            &["pelvis", "thigh_r"][..],
            180.0,
            14.0,
        ),
    ] {
        for parent in parents {
            let path = add_soft_tissue(rig, &nodes[*parent].0, name, center, radius, 0.8);
            animated.push(RigJoint::new(path).with_motion(RigJointMotion::SoftTissue {
                stiffness,
                damping,
                max_displacement: Vec3::new(0.025, 0.035, 0.025),
            }));
        }
    }
    let mut simulated = animated.clone();
    let mut colliders = Vec::new();
    for (bone, end, width, depth, limit) in [
        ("spine_01", "spine_02", 0.22, 0.14, 0.35),
        ("spine_02", "spine_03", 0.28, 0.16, 0.35),
        ("spine_03", "neck_01", 0.34, 0.18, 0.35),
        ("neck_01", "Head", 0.09, 0.09, 0.5),
        ("clavicle_l", "upperarm_l", 0.07, 0.07, 0.35),
        ("clavicle_r", "upperarm_r", 0.07, 0.07, 0.35),
        ("upperarm_l", "lowerarm_l", 0.09, 0.09, 1.5),
        ("upperarm_r", "lowerarm_r", 0.09, 0.09, 1.5),
        ("lowerarm_l", "hand_l", 0.075, 0.075, 1.5),
        ("lowerarm_r", "hand_r", 0.075, 0.075, 1.5),
        ("hand_l", "middle_01_l", 0.07, 0.035, 0.8),
        ("hand_r", "middle_01_r", 0.07, 0.035, 0.8),
        ("thigh_l", "calf_l", 0.16, 0.16, 1.3),
        ("thigh_r", "calf_r", 0.16, 0.16, 1.3),
        ("calf_l", "foot_l", 0.11, 0.11, 1.5),
        ("calf_r", "foot_r", 0.11, 0.11, 1.5),
        ("foot_l", "ball_l", 0.11, 0.08, 0.65),
        ("foot_r", "ball_r", 0.11, 0.08, 0.65),
    ] {
        let (path, pivot) = &nodes[bone];
        let start = pivot.w_axis.truncate();
        let end = nodes[end].1.w_axis.truncate();
        let direction = end - start;
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(width, direction.length().max(0.02), depth),
            Quat::from_rotation_arc(Vec3::Y, direction.normalize()),
            (start + end) * 0.5,
        );
        colliders.push(add_collider(rig, path, transform));
        simulated.push(RigJoint::new(path.clone()).with_angular_limits(Some(Vec3::splat(limit))));
    }
    for (bone, offset, size) in [
        ("Head", 0.10, Vec3::new(0.18, 0.22, 0.18)),
        ("pelvis", 0.035, Vec3::new(0.24, 0.18, 0.19)),
    ] {
        let (path, pivot) = &nodes[bone];
        let position = pivot.w_axis.truncate() + Vec3::Y * offset;
        colliders.push(add_collider(
            rig,
            path,
            Mat4::from_scale_rotation_translation(size, Quat::IDENTITY, position),
        ));
        if bone == "Head" {
            simulated.push(RigJoint::new(path.clone()).with_angular_limits(Some(Vec3::splat(0.6))));
        }
    }
    rig.set_physics_joints(animated.clone())
        .expect("embedded soft-tissue targets");
    rig.ragdoll = Some(RagdollPreset {
        animated,
        simulated,
        colliders,
        root_collider: vec!["Collider".to_owned()],
        pelvis: nodes["pelvis"].0.clone(),
        enabled: false,
        anchored: false,
        heading: 0.0,
    });
}

fn add_collider(rig: &mut Rig, parent: &[String], transform: Mat4) -> Vec<String> {
    let name = "RagdollCollider";
    let part = Part::new()
        .with_name(name)
        .with_pivot(transform)
        .with_transparency(1.0)
        .with_can_collide(false);
    resolve_mut(rig, parent)
        .unwrap()
        .add_child_box(Box::new(part));
    let mut path = parent.to_vec();
    path.push(name.to_owned());
    path
}
