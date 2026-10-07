use approx::assert_abs_diff_eq;
use terrarium::{glam::*, *};

fn assert_transform(actual: Mat4, expected: Mat4) {
    for (actual, expected) in actual
        .to_cols_array()
        .into_iter()
        .zip(expected.to_cols_array())
    {
        assert_abs_diff_eq!(actual, expected, epsilon = 1e-5);
    }
}

#[test]
fn humanoid_spawns_six_block_limbs_at_the_requested_pose() {
    let mut workspace = Workspace::new();
    let spawn_pose =
        Mat4::from_rotation_translation(Quat::from_rotation_y(0.7), Vec3::new(4.0, 3.0, -2.0));
    workspace.set_local_player(Some(LocalPlayerConfig {
        player_id: "alice".into(),
        spawn_pose,
        ..Default::default()
    }));
    let player = workspace.local_player().unwrap();
    assert_eq!(player.player_id, "alice");
    assert!(player.local_player);
    let humanoid = workspace.local_character().unwrap();
    assert_eq!(humanoid.player_id, "alice");
    assert_transform(humanoid.pose(), spawn_pose);
    let parts: Vec<_> = humanoid
        .descendants()
        .filter_map(|instance| instance.downcast_ref::<Part>())
        .collect();
    assert_eq!(parts.len(), 6);
    for (name, position, size) in [
        ("Head", Vec3::new(0.0, 1.5, 0.0), Vec3::new(2.0, 1.0, 1.0)),
        ("Torso", Vec3::ZERO, Vec3::new(2.0, 2.0, 1.0)),
        (
            "LeftArm",
            Vec3::new(-1.5, 0.0, 0.0),
            Vec3::new(1.0, 2.0, 1.0),
        ),
        (
            "RightArm",
            Vec3::new(1.5, 0.0, 0.0),
            Vec3::new(1.0, 2.0, 1.0),
        ),
        (
            "LeftLeg",
            Vec3::new(-0.5, -2.0, 0.0),
            Vec3::new(1.0, 2.0, 1.0),
        ),
        (
            "RightLeg",
            Vec3::new(0.5, -2.0, 0.0),
            Vec3::new(1.0, 2.0, 1.0),
        ),
    ] {
        let part = parts.iter().find(|part| part.name() == name).unwrap();
        assert_eq!(part.shape(), PartShape::Block);
        assert_transform(
            part.pivot(),
            spawn_pose * Mat4::from_translation(position) * Mat4::from_scale(size),
        );
    }
    assert_eq!(workspace.current_camera.subject, player.character);
}

#[test]
fn pivot_to_preserves_scale_and_moves_nested_spatial_descendants_once() {
    let mut root = Part::new()
        .with_size(Vec3::new(2.0, 3.0, 4.0))
        .with_position(Vec3::new(1.0, 2.0, 3.0));
    let old_pose = root.pose();
    let mut child = Part::new()
        .with_size(Vec3::new(3.0, 1.0, 2.0))
        .with_position(Vec3::new(5.0, 6.0, 7.0));
    child.add_child(Camera::new(Vec3::new(1.0, 8.0, 5.0), Vec3::ZERO, 1.0));
    // Player is a non-spatial container: traversal must still reach its child.
    let mut container = Player::new("container");
    container.add_child(BasePart::new().with_position(Vec3::new(-4.0, 2.0, 0.0)));
    root.add_child(child);
    root.add_child(container);
    let old: Vec<_> = root
        .descendants()
        .filter_map(|node| node.as_pv_instance().map(HasPVInstance::pivot))
        .collect();
    let new_pose =
        Mat4::from_rotation_translation(Quat::from_rotation_y(1.0), Vec3::new(10.0, 4.0, -6.0));
    let delta = new_pose * old_pose.inverse();
    // Reference builder chains must retain recursive pivot behavior.
    (&mut root)
        .with_size(Vec3::new(2.0, 3.0, 4.0))
        .pivot_to(new_pose * Mat4::from_scale(Vec3::splat(9.0)));
    assert_transform(
        root.pivot(),
        new_pose * Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0)),
    );
    for (actual, old) in root
        .descendants()
        .filter_map(Instance::as_pv_instance)
        .zip(old)
    {
        assert_transform(actual.pivot(), delta * old);
    }
    let descendants: Vec<_> = root
        .descendants()
        .filter_map(|node| node.as_pv_instance().map(HasPVInstance::pivot))
        .collect();
    root.pivot_to_self(Mat4::IDENTITY);
    for (actual, old) in root
        .descendants()
        .filter_map(Instance::as_pv_instance)
        .zip(descendants)
    {
        assert_transform(actual.pivot(), old);
    }
    assert_transform(root.pivot(), Mat4::from_scale(Vec3::new(2.0, 3.0, 4.0)));
}

#[test]
fn destroyed_characters_respawn_after_the_delay_and_rebind_the_camera() {
    let mut workspace = Workspace::new();
    workspace.set_local_player(Some(LocalPlayerConfig {
        respawn_time: 1.0,
        ..Default::default()
    }));
    let player_id = workspace.local_player().unwrap().id();
    let old_character = workspace.local_player().unwrap().character.unwrap();
    let old_limbs: Vec<_> = workspace
        .local_character()
        .unwrap()
        .descendants()
        .map(Instance::id)
        .collect();
    workspace.instance_mut(old_character).unwrap().destroy();
    workspace.update(0.5);
    assert!(workspace.local_player().unwrap().character.is_none());
    assert!(workspace.local_character().is_none());
    workspace.update(0.25);
    assert!(workspace.local_character().is_none());
    workspace.update(0.25);
    let player = workspace.local_player().unwrap();
    assert_eq!(player.id(), player_id);
    let character = player.character.unwrap();
    assert_ne!(character, old_character);
    assert_eq!(workspace.current_camera.subject, Some(character));
    assert!(
        old_limbs
            .into_iter()
            .all(|id| workspace.instance(id).is_none())
    );
    assert_eq!(workspace.get_all::<Humanoid>().count(), 1);
    workspace.update(10.0);
    assert_eq!(workspace.local_player().unwrap().character, Some(character));
}

#[test]
fn respawning_preserves_free_and_custom_camera_subjects() {
    for free_camera in [true, false] {
        let mut workspace = Workspace::new();
        workspace.set_local_player(Some(LocalPlayerConfig {
            respawn_time: 0.0,
            ..Default::default()
        }));
        let custom = workspace.add_child(Part::new().with_position(Vec3::new(10.0, 0.0, 0.0)));
        let subject = if free_camera { None } else { Some(custom) };
        workspace.current_camera.subject = subject;
        let character = workspace.local_player().unwrap().character.unwrap();
        workspace.remove_child(character);
        workspace.update(0.0);
        assert!(workspace.local_character().is_some());
        assert_eq!(workspace.current_camera.subject, subject);
    }
}

#[test]
fn players_have_independent_respawn_delays_and_reset_when_characters_return() {
    let mut workspace = Workspace::new();
    let first = workspace.add_child(Player::from_config(LocalPlayerConfig {
        respawn_time: 1.0,
        ..Default::default()
    }));
    let second = workspace.add_child(Player::from_config(LocalPlayerConfig {
        player_id: "second".into(),
        respawn_time: 2.0,
        ..Default::default()
    }));
    workspace.update(0.5);
    let replacement = workspace.add_child(Humanoid::new("local"));
    workspace.get_mut::<Player>(first).unwrap().character = Some(replacement);
    workspace.update(0.25);
    workspace.remove_child(replacement);
    workspace.update(0.75);
    assert!(workspace.get::<Player>(first).unwrap().character.is_none());
    assert!(workspace.get::<Player>(second).unwrap().character.is_none());
    workspace.update(0.25);
    assert!(workspace.get::<Player>(first).unwrap().character.is_some());
    assert!(workspace.get::<Player>(second).unwrap().character.is_none());
    workspace.update(0.25);
    let character = workspace.get::<Player>(second).unwrap().character.unwrap();
    assert_eq!(
        workspace.get::<Humanoid>(character).unwrap().player_id,
        "second"
    );
}

#[test]
fn automatic_spawning_can_be_disabled_and_manual_spawning_replaces_the_character() {
    let mut workspace = Workspace::new();
    let id = workspace
        .set_local_player(Some(LocalPlayerConfig {
            auto_spawn: false,
            ..Default::default()
        }))
        .unwrap();
    workspace.update(10.0);
    assert!(workspace.local_character().is_none());
    let first = workspace.spawn_character(id).unwrap();
    let second = workspace.spawn_character(id).unwrap();
    assert_ne!(first, second);
    assert!(workspace.instance(first).is_none());
    assert_eq!(workspace.local_player().unwrap().character, Some(second));
    workspace.current_camera.subject = Some(second);
    workspace.set_local_player(None);
    assert!(workspace.local_player().is_none());
    assert!(workspace.instance(second).is_none());
    assert!(workspace.instance(id).is_none());
    assert!(workspace.current_camera.subject.is_none());
    workspace.update(10.0);
    assert_eq!(workspace.get_all::<Humanoid>().count(), 0);
}

#[test]
fn invalid_frame_times_do_not_advance_respawns_and_invalid_delays_mean_zero() {
    let mut workspace = Workspace::new();
    let id = workspace.add_child(Player::from_config(LocalPlayerConfig {
        respawn_time: 1.0,
        ..Default::default()
    }));
    for delta in [-1.0, f32::NAN, f32::INFINITY] {
        workspace.update(delta);
        assert!(workspace.get::<Player>(id).unwrap().character.is_none());
    }
    for delay in [-1.0, f32::NAN, f32::INFINITY] {
        workspace.get_mut::<Player>(id).unwrap().respawn_time = delay;
        workspace.update(0.0);
        let character = workspace.get::<Player>(id).unwrap().character.unwrap();
        workspace.remove_child(character);
    }
}

#[test]
fn subject_camera_tracks_motion_orbits_zooms_and_can_return_to_free_flight() {
    let mut workspace = Workspace::new();
    let subject = workspace.add_child(Part::new().with_position(Vec3::new(5.0, 4.0, 3.0)));
    workspace.current_camera.subject = Some(subject);
    workspace.current_camera.follow.target_offset = Vec3::ZERO;
    workspace.current_camera.follow.pitch = 0.0;
    workspace.current_camera.follow.distance = 10.0;
    workspace.update_camera(0.0);
    assert_transform(
        workspace.current_camera.pose(),
        Mat4::from_translation(Vec3::new(5.0, 4.0, 13.0)),
    );
    workspace
        .get_mut::<Part>(subject)
        .unwrap()
        .with_position(Vec3::new(6.0, 4.0, 3.0));
    workspace.camera_controller_mut().forward = true;
    workspace.update_camera(0.1);
    assert_abs_diff_eq!(workspace.current_camera.position().x, 6.0);
    assert_abs_diff_eq!(workspace.current_camera.position().z, 13.0);
    workspace.camera_controller_mut().scroll_delta = 1000.0;
    workspace.camera_controller_mut().mouse_delta = (100.0, 10000.0);
    workspace.update_camera(0.1);
    let camera = &workspace.current_camera;
    assert_abs_diff_eq!(camera.follow.distance, camera.follow.min_distance);
    assert_abs_diff_eq!(camera.follow.pitch, camera.follow.min_pitch);
    let target = Vec3::new(6.0, 4.0, 3.0);
    assert_abs_diff_eq!(
        camera.position().distance(target),
        camera.follow.distance,
        epsilon = 1e-5
    );
    assert_abs_diff_eq!(
        camera
            .forward()
            .dot((target - camera.position()).normalize()),
        1.0,
        epsilon = 1e-5
    );
    let old = workspace.current_camera.position();
    workspace.current_camera.subject = None;
    workspace.update_camera(0.1);
    assert!((workspace.current_camera.position() - old).length() > 0.5);
}

#[test]
fn missing_subject_holds_the_camera_and_disabled_mouse_does_not_orbit_or_zoom() {
    let mut workspace = Workspace::new();
    let subject = workspace.add_child(Part::new());
    workspace.current_camera.subject = Some(subject);
    workspace.update_camera(0.0);
    let pose = workspace.current_camera.pose();
    workspace.camera_controller_mut().set_mouse_enabled(false);
    workspace.camera_controller_mut().mouse_delta = (10.0, 10.0);
    workspace.camera_controller_mut().scroll_delta = 100.0;
    workspace.update_camera(0.1);
    assert_transform(workspace.current_camera.pose(), pose);
    workspace.remove_child(subject);
    workspace.camera_controller_mut().forward = true;
    workspace.update_camera(0.1);
    assert_transform(workspace.current_camera.pose(), pose);
}

#[test]
fn cloned_workspace_keeps_character_and_camera_references_inside_the_clone() {
    let mut original = Workspace::new();
    original.set_local_player(Some(LocalPlayerConfig {
        respawn_time: 0.0,
        ..Default::default()
    }));
    let character = original.local_player().unwrap().character.unwrap();
    let camera = original.add_child(Camera::default().with_subject(Some(character)));
    let mut cloned = original.clone();
    let clone_character = cloned.local_player().unwrap().character.unwrap();
    assert_ne!(clone_character, character);
    assert_eq!(cloned.current_camera.subject, Some(clone_character));
    assert_eq!(
        cloned.get_all::<Camera>().next().unwrap().subject,
        Some(clone_character)
    );
    assert!(cloned.instance(camera).is_none());
    assert!(cloned.local_character().is_some());
    original.remove_child(character);
    cloned.update(0.0);
    assert_eq!(
        cloned.local_player().unwrap().character,
        Some(clone_character)
    );
    cloned.remove_child(clone_character);
    cloned.update(0.0);
    assert!(cloned.local_character().is_some());
    assert_ne!(
        cloned.local_player().unwrap().character,
        Some(clone_character)
    );
}

#[cfg(all(feature = "bevy", not(target_arch = "wasm32")))]
#[test]
fn bevy_without_a_supplied_workspace_uses_launcher_defaults() -> Result<(), AppCreationError> {
    let engine = Terrarium::new()
        .with_bevy(terrarium::bevy::app::App::new())
        .with_size([32, 32])
        .with_headless(Some(1))
        .run(|engine| {
            assert!(engine.local_character().is_some());
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert!(engine.local_character().is_some());
    assert_eq!(
        engine.current_camera.subject,
        engine.local_player().unwrap().character
    );
    Ok(())
}

#[test]
fn baseplate_preset_and_empty_workspace_are_independent_of_player_settings() {
    assert!(Workspace::new().instances().next().is_none());
    assert!(
        Workspace::preset(WorkspacePreset::Empty)
            .instances()
            .next()
            .is_none()
    );
    let workspace = Workspace::preset(WorkspacePreset::Baseplate);
    let plate = workspace.get_all::<Part>().next().unwrap();
    assert_eq!(plate.name(), "Baseplate");
    assert!(plate.anchored());
    assert!(plate.can_collide());
    assert_abs_diff_eq!(
        plate.position().y + plate.pivot().to_scale_rotation_translation().0.y / 2.0,
        0.0
    );
    assert!(workspace.local_player().is_none());
}

#[cfg(feature = "physics")]
#[test]
fn baseplate_supports_dynamic_parts() {
    let mut workspace = Workspace::preset(WorkspacePreset::Baseplate);
    let part = workspace.add_child(
        Part::new()
            .with_anchored(false)
            .with_position(Vec3::new(0.0, 3.0, 0.0)),
    );
    for _ in 0..180 {
        workspace.update(1.0 / 60.0);
    }
    let height = workspace.get::<Part>(part).unwrap().position().y;
    assert!(height > 0.4 && height < 0.6, "resting height: {height}");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn launcher_defaults_and_opt_outs_are_available_before_initialization()
-> Result<(), AppCreationError> {
    let engine = Terrarium::new()
        .with_size([32, 32])
        .with_headless(Some(0))
        .run(|engine| {
            assert_eq!(engine.local_player().unwrap().player_id, "local");
            assert!(engine.local_character().is_some());
            assert_eq!(
                engine.current_camera.subject,
                engine.local_player().unwrap().character
            );
            assert!(
                engine
                    .get_all::<Part>()
                    .any(|part| part.name() == "Baseplate")
            );
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert!(engine.local_character().is_some());
    let engine = Terrarium::preset(WorkspacePreset::Empty)
        .with_local_player(None)
        .with_size([32, 32])
        .with_headless(Some(0))
        .run(|engine| {
            assert!(engine.instances().next().is_none());
            assert!(engine.current_camera.subject.is_none());
            Ok::<(), AppCreationError>(())
        })?
        .unwrap();
    assert!(engine.local_player().is_none());
    Terrarium::preset(WorkspacePreset::Empty)
        .with_local_player(Some(LocalPlayerConfig {
            player_id: "custom".into(),
            respawn_time: 0.5,
            spawn_pose: Mat4::from_translation(Vec3::new(10.0, 3.0, 0.0)),
            ..Default::default()
        }))
        .with_size([32, 32])
        .with_headless(Some(0))
        .run(|engine| {
            assert_eq!(engine.local_player().unwrap().player_id, "custom");
            assert_eq!(engine.local_player().unwrap().respawn_time, 0.5);
            assert_eq!(
                engine.local_character().unwrap().position(),
                Vec3::new(10.0, 3.0, 0.0)
            );
            assert!(
                !engine
                    .get_all::<Part>()
                    .any(|part| part.name() == "Baseplate")
            );
            Ok::<(), AppCreationError>(())
        })?;
    Ok(())
}
