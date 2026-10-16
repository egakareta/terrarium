use trscene::{
    AnimationClip, AnimationError, AnimationPlayer, AnimationTrack, BasePart, Camera, Easing,
    HasPVInstance, Instance, Keyframe, Part, PointLight, Rig, Tween, Workspace,
    glam::{Mat4, Quat, Vec3},
};

fn translation(x: f32) -> Mat4 {
    Mat4::from_translation(Vec3::new(x, 0.0, 0.0))
}

fn moving_arm() -> AnimationClip {
    AnimationClip::new([AnimationTrack::new(
        ["Arm"],
        [
            Keyframe::new(0.0, translation(1.0)),
            Keyframe::new(1.0, translation(3.0)),
        ],
    )])
}

#[test]
fn rig_setters_preserve_descendant_transforms_and_existing_world_space_parenting() {
    let mut rig = Rig::new().with_position(Vec3::new(10.0, 0.0, 0.0));
    let offset = Mat4::from_scale_rotation_translation(
        Vec3::new(1.0, 2.0, 3.0),
        Quat::from_rotation_x(0.4),
        Vec3::X,
    );
    let part_id = rig.add_child_local(Part::new().with_pivot(offset));
    #[cfg(feature = "meshpart")]
    let mesh_id = rig.add_child_local(
        trscene::MeshPart::new(trscene::Mesh::cube(1.0, [1.0; 4])).with_pivot(offset),
    );
    let mut nested = Rig::new().with_pivot(translation(12.0));
    let mut light = PointLight::new();
    let camera_id = light.add_child(Camera::default().with_pivot(translation(14.0)));
    nested.add_child(light);
    let nested_id = rig.add_child(nested);
    let mut workspace = Workspace::new();
    let rig_id = workspace.add_child(rig);
    let unrelated = workspace.add_child(Part::new().with_pivot(translation(50.0)));
    let target = Mat4::from_rotation_translation(Quat::from_rotation_z(1.0), Vec3::splat(5.0));

    workspace.get_mut::<Rig>(rig_id).unwrap().with_pivot(target);

    assert!(
        workspace
            .get::<Part>(part_id)
            .unwrap()
            .pivot()
            .abs_diff_eq(target * offset, 0.0001)
    );
    #[cfg(feature = "meshpart")]
    assert!(
        workspace
            .get::<trscene::MeshPart>(mesh_id)
            .unwrap()
            .pivot()
            .abs_diff_eq(target * offset, 0.0001)
    );
    assert!(
        workspace
            .get::<Rig>(nested_id)
            .unwrap()
            .pivot()
            .abs_diff_eq(target * translation(2.0), 0.0001)
    );
    assert!(
        workspace
            .get::<Camera>(camera_id)
            .unwrap()
            .pivot()
            .abs_diff_eq(target * translation(4.0), 0.0001)
    );
    assert_eq!(
        workspace.get::<Part>(unrelated).unwrap().pivot(),
        translation(50.0)
    );

    let rig = workspace.get_mut::<Rig>(rig_id).unwrap();
    rig.with_position(Vec3::ZERO)
        .with_orientation(Vec3::ZERO)
        .with_size(Vec3::splat(2.0));
    assert!(
        workspace
            .get::<Part>(part_id)
            .unwrap()
            .pivot()
            .abs_diff_eq(Mat4::from_scale(Vec3::splat(2.0)) * offset, 0.0001)
    );
}

#[test]
fn timestamped_tracks_ease_each_segment_and_take_the_short_rotation_path() {
    let track = AnimationTrack::new(
        ["Arm"],
        [
            Keyframe::new(0.0, translation(0.0)).with_easing(Easing::EaseIn),
            Keyframe::new(2.0, translation(8.0)),
            Keyframe::new(3.0, translation(12.0)),
        ],
    );
    assert_eq!(track.sample(1.0), translation(2.0));
    assert_eq!(track.sample(2.5), translation(10.0));
    assert_eq!(track.sample(-1.0), translation(0.0));
    assert_eq!(track.sample(100.0), translation(12.0));
    let rotation = AnimationTrack::new(
        ["Arm"],
        [
            Keyframe::new(0.0, Mat4::from_rotation_y(170.0_f32.to_radians())),
            Keyframe::new(1.0, Mat4::from_rotation_y(-170.0_f32.to_radians())),
        ],
    );
    assert!(
        rotation
            .sample(0.5)
            .transform_vector3(Vec3::Z)
            .abs_diff_eq(Vec3::NEG_Z, 0.0001)
    );
}

#[test]
fn zero_duration_clips_and_large_or_invalid_deltas_have_finite_playback() {
    let clip = AnimationClip::new([AnimationTrack::new(
        ["Arm"],
        [Keyframe::new(0.0, translation(3.0))],
    )]);
    let mut player = AnimationPlayer::new(clip).with_looping(true);
    player.update(0.0);
    assert!(player.is_finished());
    assert_eq!(player.time(), 0.0);
    let mut player = AnimationPlayer::new(moving_arm())
        .with_looping(true)
        .with_speed(f32::MAX);
    for delta in [f32::NAN, f32::INFINITY, -1.0] {
        player.update(delta);
    }
    assert_eq!(player.time(), 0.0);
    player.update(f32::MAX);
    assert!(player.time().is_finite() && player.time() < 1.0);
    assert!(matches!(
        player.set_speed(-1.0),
        Err(AnimationError::InvalidSpeed)
    ));
    player.set_speed(0.0).unwrap();
    player.update(1.0);
    assert_eq!(player.time(), 0.0);
}

#[test]
fn nested_rig_animations_compose_and_render_with_the_parent_pose() {
    let mut nested = Rig::new().with_name("Nested");
    let arm = nested.add_child(Part::new().with_name("Arm"));
    nested
        .play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    let mut outer = Rig::new();
    outer.add_child(PointLight::new().with_name("Light"));
    let nested_id = outer.add_child(nested);
    let clip = AnimationClip::new([AnimationTrack::new(
        ["Nested"],
        [
            Keyframe::new(0.0, translation(1.0)),
            Keyframe::new(1.0, translation(3.0)),
        ],
    )]);
    outer.play_animation(AnimationPlayer::new(clip)).unwrap();
    let mut workspace = Workspace::new();
    workspace.add_child(outer.with_pivot(translation(10.0)));
    workspace.update_animations(0.5);
    assert_eq!(workspace.get::<Rig>(nested_id).unwrap().position().x, 12.0);
    assert_eq!(workspace.get::<Part>(arm).unwrap().position().x, 14.0);
    let scene = workspace.render_scene();
    let (_, rendered_pivot) = scene.nodes().find(|(node, _)| node.name == "Arm").unwrap();
    assert!(rendered_pivot.abs_diff_eq(workspace.get::<Part>(arm).unwrap().pivot(), 0.0001));
    let (light, light_pivot) = scene
        .nodes()
        .find(|(node, _)| node.name == "Light")
        .unwrap();
    assert!(light.light.is_some());
    assert_eq!(light_pivot, translation(10.0));
}

#[test]
fn animation_applies_parent_tracks_before_children_and_carries_untracked_descendants() {
    let mut rig = Rig::new();
    let mut arm = Part::new().with_name("Arm").with_pivot(translation(1.0));
    let hand_id = arm.add_child(
        BasePart::new()
            .with_name("Hand")
            .with_pivot(translation(2.0)),
    );
    let ornament_id = arm.add_child(Camera::default().with_pivot(translation(3.0)));
    let arm_id = rig.add_child(arm);
    let clip = AnimationClip::new([
        AnimationTrack::new(
            ["Arm", "Hand"],
            [
                Keyframe::new(0.0, translation(1.0)),
                Keyframe::new(1.0, translation(2.0)),
            ],
        ),
        moving_arm().tracks()[0].clone(),
    ]);
    rig.play_animation(AnimationPlayer::new(clip)).unwrap();
    let mut workspace = Workspace::new();
    let rig_id = workspace.add_child(rig);
    workspace.tweens_mut().add_position(
        rig_id,
        Tween::new(Vec3::ZERO, Vec3::new(10.0, 0.0, 0.0), 1.0),
    );
    workspace.update(0.5);

    assert_eq!(
        workspace.get::<Part>(arm_id).unwrap().position(),
        Vec3::new(7.0, 0.0, 0.0)
    );
    assert_eq!(
        workspace.get::<BasePart>(hand_id).unwrap().position(),
        Vec3::new(8.5, 0.0, 0.0)
    );
    assert_eq!(
        workspace.get::<Camera>(ornament_id).unwrap().position(),
        Vec3::new(9.0, 0.0, 0.0)
    );
}

#[test]
fn playback_supports_pause_seek_speed_loop_restart_and_completion() {
    let mut rig = Rig::new();
    let arm_id = rig.add_child(Part::new().with_name("Arm"));
    rig.play_animation(
        AnimationPlayer::new(moving_arm())
            .with_speed(2.0)
            .with_looping(true),
    )
    .unwrap();
    rig.update_animation(0.75);
    assert_eq!(rig.animation().unwrap().time(), 0.5);
    assert_eq!(
        rig.find_descendant(arm_id)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .position(),
        Vec3::new(2.0, 0.0, 0.0)
    );
    rig.animation_mut().unwrap().pause();
    rig.update_animation(10.0);
    assert_eq!(rig.animation().unwrap().time(), 0.5);
    rig.animation_mut().unwrap().seek(0.75).unwrap();
    rig.update_animation(0.0);
    assert_eq!(
        rig.find_descendant(arm_id)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .position(),
        Vec3::new(2.5, 0.0, 0.0)
    );
    rig.animation_mut().unwrap().resume();
    rig.update_animation(0.125);
    assert_eq!(rig.animation().unwrap().time(), 0.0);
    rig.play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    rig.update_animation(5.0);
    assert!(rig.animation().unwrap().is_finished());
    assert_eq!(
        rig.find_descendant(arm_id)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .position(),
        Vec3::new(3.0, 0.0, 0.0)
    );
    rig.animation_mut().unwrap().restart();
    rig.update_animation(0.25);
    assert_eq!(rig.animation().unwrap().time(), 0.25);
    assert!(!rig.animation().unwrap().is_finished());
    assert!(rig.stop_animation().is_some());
    let held = rig
        .find_descendant(arm_id)
        .unwrap()
        .downcast_ref::<Part>()
        .unwrap()
        .pivot();
    rig.update_animation(1.0);
    assert_eq!(
        rig.find_descendant(arm_id)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .pivot(),
        held
    );
}

#[test]
fn cloned_rigs_reuse_animation_paths_and_removed_targets_are_safe() {
    let mut rig = Rig::new();
    let original_arm = rig.add_child(Part::new().with_name("Arm"));
    rig.play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    let mut workspace = Workspace::new();
    let first = workspace.add_child(rig);
    let second = workspace.add_child(
        workspace
            .get::<Rig>(first)
            .unwrap()
            .clone()
            .with_pivot(translation(10.0)),
    );
    workspace.update_animations(0.5);
    assert_eq!(
        workspace.get::<Part>(original_arm).unwrap().position().x,
        2.0
    );
    assert_eq!(
        workspace
            .get_mut::<Rig>(second)
            .unwrap()
            .find_first_child::<Part>("Arm")
            .unwrap()
            .1
            .position()
            .x,
        12.0
    );
    workspace.get_mut::<Part>(original_arm).unwrap().destroy();
    workspace.update_animations(0.5);
    assert!(workspace.get::<Part>(original_arm).is_none());
    assert_eq!(
        workspace
            .get_mut::<Rig>(second)
            .unwrap()
            .find_first_child::<Part>("Arm")
            .unwrap()
            .1
            .position()
            .x,
        13.0
    );
}

#[test]
fn playback_skips_a_singular_target_and_resumes_when_its_pivot_is_restored() {
    let mut rig = Rig::new();
    let arm = rig.add_child(Part::new().with_name("Arm"));
    rig.play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    let mut workspace = Workspace::new();
    workspace.add_child(rig);
    workspace
        .get_mut::<Part>(arm)
        .unwrap()
        .with_size(Vec3::ZERO);
    workspace.update_animations(0.5);
    assert_eq!(
        workspace
            .get::<Part>(arm)
            .unwrap()
            .pivot()
            .to_scale_rotation_translation()
            .0,
        Vec3::ZERO
    );
    workspace
        .get_mut::<Part>(arm)
        .unwrap()
        .with_pivot(Mat4::IDENTITY);
    workspace.update_animations(0.25);
    assert_eq!(workspace.get::<Part>(arm).unwrap().position().x, 2.5);
}

#[test]
fn invalid_clips_and_unresolved_targets_are_rejected_without_changing_the_pose() {
    for time in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(matches!(
            AnimationTrack::try_new(["Arm"], [Keyframe::new(time, Mat4::IDENTITY)]),
            Err(AnimationError::InvalidTime)
        ));
    }
    assert!(matches!(
        AnimationTrack::try_new(
            ["Arm"],
            [
                Keyframe::new(1.0, Mat4::IDENTITY),
                Keyframe::new(1.0, Mat4::IDENTITY)
            ]
        ),
        Err(AnimationError::InvalidTime)
    ));
    assert!(matches!(
        AnimationTrack::try_new(["Arm"], []),
        Err(AnimationError::Empty)
    ));
    for transform in [
        Mat4::from_scale(Vec3::ZERO),
        Mat4::from_scale(Vec3::NEG_ONE),
        Mat4::from_cols_array(&[f32::NAN; 16]),
    ] {
        assert!(matches!(
            AnimationTrack::try_new(["Arm"], [Keyframe::new(0.0, transform)]),
            Err(AnimationError::InvalidTransform)
        ));
    }
    let track = moving_arm().tracks()[0].clone();
    assert!(matches!(
        AnimationClip::try_new([track.clone(), track]),
        Err(AnimationError::DuplicateTarget)
    ));
    let mut rig = Rig::new();
    let arm_id = rig.add_child(Part::new().with_name("Arm"));
    rig.play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    rig.update_animation(0.5);
    let bad_clip = AnimationClip::new([AnimationTrack::new(
        ["Missing"],
        [Keyframe::new(0.0, Mat4::IDENTITY)],
    )]);
    assert!(matches!(
        rig.play_animation(AnimationPlayer::new(bad_clip)),
        Err(AnimationError::TargetNotFound(_))
    ));
    assert_eq!(rig.animation().unwrap().time(), 0.5);
    assert_eq!(
        rig.find_descendant(arm_id)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .position()
            .x,
        2.0
    );
    rig.add_child(Part::new().with_name("Arm"));
    assert!(matches!(
        rig.play_animation(AnimationPlayer::new(moving_arm())),
        Err(AnimationError::AmbiguousTarget(_))
    ));
}

#[cfg(feature = "devtools")]
#[test]
fn inspector_edits_carry_rig_parts_and_reject_singular_scale() {
    let mut rig = Rig::new();
    let arm = rig.add_child(Part::new().with_pivot(translation(1.0)));
    rig.set_property(
        "position",
        trscene::devtools::PropertyValue::Vector(vec![5.0, 0.0, 0.0]),
    )
    .unwrap();
    assert_eq!(
        rig.find_descendant(arm)
            .unwrap()
            .downcast_ref::<Part>()
            .unwrap()
            .position()
            .x,
        6.0
    );
    assert!(
        rig.set_property(
            "size",
            trscene::devtools::PropertyValue::Vector(vec![0.0; 3])
        )
        .is_err()
    );
    assert_eq!(rig.position().x, 5.0);
}

#[cfg(feature = "physics")]
#[test]
fn dynamic_rig_parts_share_motion_and_keep_their_offsets_under_impulses() {
    use trscene::{HasBasePart, rapier3d::math::Vector};
    let mut rig = Rig::new().with_anchored(false);
    let left = rig.add_child(Part::new().with_name("Left").with_pivot(translation(-1.0)));
    let right = rig.add_child(
        Part::new()
            .with_name("Right")
            .with_anchored(false)
            .with_pivot(translation(1.0)),
    );
    rig.play_animation(AnimationPlayer::new(AnimationClip::new([
        AnimationTrack::new(
            ["Right"],
            [
                Keyframe::new(0.0, translation(1.0)),
                Keyframe::new(1.0, translation(2.0)),
            ],
        ),
    ])))
    .unwrap();
    let mut workspace = Workspace::new();
    let rig_id = workspace.add_child(rig.with_position(Vec3::new(0.0, 5.0, 0.0)));
    workspace.update(0.0);
    workspace
        .physics_mut()
        .body_mut(right)
        .unwrap()
        .apply_impulse(Vector::new(2.0, 1.0, 0.0), true);
    workspace
        .physics_mut()
        .body_mut(rig_id)
        .unwrap()
        .apply_torque_impulse(Vector::new(0.0, 0.0, 1.0), true);
    for _ in 0..30 {
        workspace.update(1.0 / 60.0);
    }
    let rig = workspace.get::<Rig>(rig_id).unwrap();
    assert!(rig.position().y < 5.0);
    assert!(rig.position().x > 0.0);
    assert!(
        rig.pivot()
            .inverse()
            .mul_mat4(&workspace.get::<Part>(left).unwrap().pivot())
            .abs_diff_eq(translation(-1.0), 0.0001)
    );
    assert!(
        rig.pivot()
            .inverse()
            .mul_mat4(&workspace.get::<Part>(right).unwrap().pivot())
            .abs_diff_eq(translation(1.5), 0.0001)
    );
    assert_eq!(
        workspace.physics().body_handle(left),
        workspace.physics().body_handle(right)
    );
}

#[cfg(feature = "physics")]
#[test]
fn animated_colliders_follow_local_poses_and_report_child_part_ids() {
    use trscene::{BoxOverlapQuery, HasBasePart};
    let mut rig = Rig::new();
    let arm_id = rig.add_child(Part::new().with_name("Arm").with_anchored(false));
    let ghost = rig.add_child(
        Part::new()
            .with_can_collide(false)
            .with_pivot(translation(3.0)),
    );
    rig.play_animation(AnimationPlayer::new(moving_arm()))
        .unwrap();
    let mut workspace = Workspace::new();
    let rig_id = workspace.add_child(rig);
    workspace.update(1.0);
    assert_eq!(
        workspace.get_part_bounds_in_box(&BoxOverlapQuery::new(translation(3.0))),
        vec![arm_id]
    );
    assert!(
        workspace
            .get_part_bounds_in_box(&BoxOverlapQuery::new(translation(1.0)))
            .is_empty()
    );
    assert!(workspace.physics().body_handle(ghost).is_some());
    workspace
        .get_mut::<Part>(arm_id)
        .unwrap()
        .with_can_collide(false);
    workspace.update(1.0 / 60.0);
    assert!(
        workspace
            .get_part_bounds_in_box(&BoxOverlapQuery::new(translation(3.0)))
            .is_empty()
    );
    workspace.get_mut::<Rig>(rig_id).unwrap().destroy();
    workspace.update(1.0 / 60.0);
    assert!(workspace.physics().body_handle(arm_id).is_none());
}
