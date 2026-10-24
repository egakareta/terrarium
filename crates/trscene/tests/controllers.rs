use trscene::{
    egui::{
        self, Context, Event, Pos2, RawInput, Rect, TouchDeviceId, TouchId, TouchPhase, pos2, vec2,
    },
    glam::*,
    winit::{
        event::{DeviceEvent, WindowEvent},
        keyboard::KeyCode,
    },
    *,
};

fn frame(context: &Context, workspace: &mut Workspace, events: Vec<Event>) {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        events,
        ..Default::default()
    };
    let mut output = context.run_ui(input, |ui| workspace.process_input(ui.ctx()));
    output.textures_delta.clear();
}

fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch {
        device_id: TouchDeviceId(0),
        id: TouchId(id),
        phase,
        pos,
        force: None,
    }
}

fn key(key: egui::Key, pressed: bool) -> Event {
    Event::Key {
        key,
        physical_key: Some(key),
        pressed,
        repeat: false,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn player_movement_carries_scaled_rig_descendants_and_normalizes_diagonals() {
    let mut rig = Rig::new().with_pivot(Mat4::from_scale_rotation_translation(
        Vec3::new(2.0, 3.0, 4.0),
        Quat::from_rotation_y(std::f32::consts::FRAC_PI_2),
        Vec3::new(3.0, 2.0, 1.0),
    ));
    let child = rig.add_child_local(Part::new().with_position(Vec3::Y));
    let relative = rig.pivot().inverse() * rig.children()[0].spatial().unwrap().world_pivot();
    let mut player = PlayerController::new(rig.id());
    player.process_key(KeyCode::KeyW, true);
    player.process_key(KeyCode::KeyD, true);
    player.process_key(KeyCode::Space, true);
    player.process_key(KeyCode::ShiftLeft, true);
    let initial = rig.position();
    player.update_rig(&mut rig, 0.1);

    assert!(
        rig.position()
            .abs_diff_eq(initial + Vec3::new(1.0, 0.0, -1.0).normalize() * 1.5, 1e-5)
    );
    assert!(
        rig.pivot()
            .to_scale_rotation_translation()
            .0
            .abs_diff_eq(Vec3::new(2.0, 3.0, 4.0), 1e-5)
    );
    assert_eq!(rig.children()[0].id(), child);
    assert!(
        rig.children()[0]
            .spatial()
            .unwrap()
            .world_pivot()
            .abs_diff_eq(rig.pivot() * relative, 1e-5)
    );
}

#[test]
fn player_ignores_idle_look_and_linearly_faces_world_aligned_travel() {
    let mut rig = Rig::new();
    let mut player = PlayerController::new(rig.id());
    player.process_mouse_motion((1.0, 5.0));
    player.update_rig(&mut rig, 0.1);
    assert_eq!(rig.position(), Vec3::ZERO);
    assert!(rig.forward().abs_diff_eq(Vec3::NEG_Z, 1e-5));
    player.process_key(KeyCode::KeyD, true);
    player.update_rig(&mut rig, 1.0 / 60.0);
    assert!(rig.position().abs_diff_eq(Vec3::X * 0.1, 1e-5));
    let facing = rig.forward();
    assert!(facing.x > 0.0 && facing.x < 0.9);
    player.process_key(KeyCode::KeyD, false);
    player.update_rig(&mut rig, 0.1);
    assert!(rig.forward().abs_diff_eq(facing, 1e-5));
}

#[test]
fn reusable_actions_drive_camera_and_player_without_enabled_devices() {
    let actions = ControllerActions {
        movement: Vec3::new(0.3, 0.0, 0.4),
        look: Vec2::ZERO,
        sprint: true,
    };
    let mut rig = Rig::new();
    let player = PlayerController::new(rig.id()).with_enabled(false);
    let mut camera = Camera::default().with_pose(Mat4::IDENTITY);
    let mut camera_controller = CameraController::default().with_enabled(false);
    player.update_rig_with_actions(&mut rig, actions, 0.1);
    camera_controller.update_camera_with_actions(&mut camera, None, actions, 0.1);
    assert!(rig.position().abs_diff_eq(camera.position(), 1e-5));
    assert!((rig.position().length() - 0.75).abs() < 1e-5);
    assert!(rig.forward().abs_diff_eq(Vec3::new(0.6, 0.0, -0.8), 1e-5));
}

#[test]
fn queued_look_rotates_a_disabled_camera_once() {
    let mut camera = Camera::default();
    let mut controller = CameraController::default().with_enabled(false);
    let initial = camera.pose();
    controller.queue_look(Vec2::new(0.2, -0.1));
    controller.update_camera(&mut camera, None, 1.0 / 60.0);
    let rotated = camera.pose();
    assert!(!rotated.abs_diff_eq(initial, 1e-5));
    assert!(camera.forward().x > 0.1);
    controller.update_camera(&mut camera, None, 1.0 / 60.0);
    assert!(camera.pose().abs_diff_eq(rotated, 1e-5));
}

#[cfg(feature = "physics")]
fn dynamic_player_rig(position: Vec3) -> Rig {
    let mut rig = Rig::new().with_anchored(false).with_position(position);
    rig.add_child_local(Part::new().with_position(Vec3::Y * 0.5));
    rig
}

#[cfg(feature = "physics")]
#[test]
fn player_up_action_jumps_when_a_dynamic_rig_is_grounded() {
    let mut workspace = Workspace::new();
    workspace.add_child(
        Part::new()
            .with_size(Vec3::new(10.0, 1.0, 10.0))
            .with_position(Vec3::new(0.0, -0.5, 0.0)),
    );
    let rig = workspace.add_child(dynamic_player_rig(Vec3::Y * 3.0));

    for _ in 0..120 {
        workspace.update(1.0 / 60.0);
    }
    let grounded_position = workspace.get::<Rig>(rig).unwrap().position();

    let mut player = PlayerController::new(rig);
    assert!(player.process_key(KeyCode::Space, true));
    workspace.add_child(player);
    workspace.update(1.0 / 60.0);

    let airborne_position = workspace.get::<Rig>(rig).unwrap().position();
    assert!(
        airborne_position.y > grounded_position.y + 0.05,
        "grounded at {grounded_position:?}, after jump at {airborne_position:?}"
    );
    for _ in 0..44 {
        workspace.update(1.0 / 60.0);
    }
    let held_jump_position = workspace.get::<Rig>(rig).unwrap().position();
    assert!(
        held_jump_position.y < grounded_position.y + 3.0,
        "held jump rose to {held_jump_position:?}"
    );
}

#[test]
fn player_mouse_wheel_changes_camera_orbit_distance() {
    let mut workspace = Workspace::new();
    let rig = workspace.add_child(Rig::new());
    workspace.current_camera.set_subject(Some(rig));
    workspace.camera_controller_mut().unwrap().subject_distance = 5.0;
    workspace.add_child(PlayerController::new(rig));

    let context = Context::default();
    frame(
        &context,
        &mut workspace,
        vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, 40.0),
            phase: TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    workspace.update(1.0 / 60.0);

    assert!((workspace.camera_controller().unwrap().subject_distance - 4.0).abs() < 1e-5);
}

#[cfg(feature = "physics")]
#[test]
fn dynamic_players_move_from_the_first_frame_without_interrupting_gravity() {
    let mut workspace = Workspace::new();
    workspace
        .camera_controller_mut()
        .unwrap()
        .set_enabled(false);
    let rig = workspace.add_child(dynamic_player_rig(Vec3::Y * 4.0));
    let falling = workspace.add_child(dynamic_player_rig(Vec3::new(10.0, 4.0, 0.0)));
    workspace.current_camera.set_subject(Some(rig));
    let mut player = PlayerController::new(rig);
    player.process_key(KeyCode::KeyW, true);
    let controller = workspace.add_child(player);
    workspace.update(1.0 / 60.0);
    let first_position = workspace.get::<Rig>(rig).unwrap().position();
    assert!(first_position.z < -0.09, "{first_position:?}");
    assert!(first_position.y < 4.0, "{first_position:?}");
    for _ in 0..30 {
        workspace.process_device_event(&DeviceEvent::MouseMotion { delta: (2.0, 0.0) });
        workspace.update(1.0 / 60.0);
    }
    let player = workspace.get::<Rig>(rig).unwrap();
    assert!(player.position().z < -2.8);
    assert!(
        (player.position().y - workspace.get::<Rig>(falling).unwrap().position().y).abs() < 1e-4
    );
    let position = player.position();
    let facing = player.forward();
    workspace
        .get_mut::<PlayerController>(controller)
        .unwrap()
        .process_key(KeyCode::KeyW, false);
    workspace.update(1.0 / 60.0);
    let player = workspace.get::<Rig>(rig).unwrap();
    assert!((player.position() - position).with_y(0.0).length() < 1e-4);
    assert!(player.position().y < position.y);
    assert!(player.forward().abs_diff_eq(facing, 1e-5));
}

#[cfg(feature = "physics")]
#[test]
fn dynamic_player_movement_applies_immediately_after_repositioning() {
    let mut workspace = Workspace::new();
    workspace
        .camera_controller_mut()
        .unwrap()
        .set_enabled(false);
    let rig = workspace.add_child(dynamic_player_rig(Vec3::Y * 4.0));
    let mut player = PlayerController::new(rig);
    player.process_key(KeyCode::KeyW, true);
    workspace.add_child(player);
    for _ in 0..30 {
        workspace.update(1.0 / 60.0);
    }
    workspace
        .get_mut::<Rig>(rig)
        .unwrap()
        .with_position(Vec3::new(2.0, 8.0, 3.0));
    workspace.update(1.0 / 60.0);
    let position = workspace.get::<Rig>(rig).unwrap().position();
    assert!((position.x - 2.0).abs() < 1e-4, "{position:?}");
    assert!(position.z < 2.91, "{position:?}");
    assert!(position.y > 7.9 && position.y < 8.0, "{position:?}");
}

#[test]
fn every_device_subset_can_drive_a_registered_player_independently() {
    for mask in 0..8 {
        let keyboard = mask & 1 != 0;
        let mouse = mask & 2 != 0;
        let touchscreen = mask & 4 != 0;
        let mut workspace = Workspace::new();
        workspace
            .camera_controller_mut()
            .unwrap()
            .set_enabled(false);
        let rig = workspace.add_child(Rig::new());
        workspace.current_camera.set_subject(Some(rig));
        workspace.camera_controller_mut().unwrap().subject_offset = Vec3::Y;
        workspace.update_camera(0.0);
        let id = workspace.add_child(
            PlayerController::new(rig)
                .with_keyboard_enabled(keyboard)
                .with_mouse_enabled(mouse)
                .with_touch_enabled(touchscreen),
        );
        let context = Context::default();

        frame(&context, &mut workspace, vec![key(egui::Key::W, true)]);
        workspace.update(0.1);
        let position = workspace.get::<Rig>(rig).unwrap().position();
        assert!(position.abs_diff_eq(
            if keyboard {
                Vec3::NEG_Z * 0.6
            } else {
                Vec3::ZERO
            },
            1e-5
        ));

        frame(&context, &mut workspace, vec![key(egui::Key::W, false)]);
        workspace.process_device_event(&DeviceEvent::MouseMotion { delta: (20.0, 0.0) });
        workspace.update(0.1);
        let forward = workspace.current_camera.forward();
        assert!((forward.x - if mouse { 0.05_f32.sin() } else { 0.0 }).abs() < 1e-5);
        assert!(
            workspace
                .get::<Rig>(rig)
                .unwrap()
                .forward()
                .abs_diff_eq(Vec3::NEG_Z, 1e-5)
        );

        frame(
            &context,
            &mut workspace,
            vec![
                touch(1, TouchPhase::Start, pos2(88.0, 512.0)),
                touch(2, TouchPhase::Start, pos2(400.0, 250.0)),
            ],
        );
        frame(
            &context,
            &mut workspace,
            vec![
                touch(1, TouchPhase::Move, pos2(88.0, 452.0)),
                touch(2, TouchPhase::Move, pos2(420.0, 250.0)),
            ],
        );
        let before = workspace.get::<Rig>(rig).unwrap().position();
        workspace.update(0.1);
        let player_rig = workspace.get::<Rig>(rig).unwrap();
        assert!(
            (player_rig.position().distance(before) - if touchscreen { 0.6 } else { 0.0 }).abs()
                < 1e-5
        );
        assert!(
            (workspace.current_camera.forward().x
                - ((usize::from(mouse) + usize::from(touchscreen)) as f32 * 0.05).sin())
            .abs()
                < 1e-5
        );

        workspace
            .get_mut::<PlayerController>(id)
            .unwrap()
            .set_enabled(false);
        let pose = workspace.get::<Rig>(rig).unwrap().pose();
        workspace.update(0.1);
        assert!(
            workspace
                .get::<Rig>(rig)
                .unwrap()
                .pose()
                .abs_diff_eq(pose, 1e-5)
        );
    }
}

#[test]
fn player_respects_independent_ui_capture_and_clears_input_on_focus_loss() {
    let mut workspace = Workspace::new();
    workspace
        .camera_controller_mut()
        .unwrap()
        .set_enabled(false);
    let rig = workspace.add_child(Rig::new());
    workspace.current_camera.set_subject(Some(rig));
    workspace.camera_controller_mut().unwrap().subject_offset = Vec3::Y;
    workspace.update_camera(0.0);
    let id = workspace.add_child(PlayerController::new(rig));
    let keyboard_context = Context::default();
    let mut text = String::new();
    let mut output = keyboard_context.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            ..Default::default()
        },
        |ui| {
            ui.text_edit_singleline(&mut text).request_focus();
        },
    );
    output.textures_delta.clear();
    let mut output = keyboard_context.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events: vec![key(egui::Key::W, true)],
            ..Default::default()
        },
        |ui| {
            ui.text_edit_singleline(&mut text);
            let player = workspace.get_mut::<PlayerController>(id).unwrap();
            player.process_mouse_motion((20.0, 0.0));
            player.process_input(ui.ctx());
        },
    );
    output.textures_delta.clear();
    workspace.update(0.1);
    assert_eq!(workspace.get::<Rig>(rig).unwrap().position(), Vec3::ZERO);
    assert!(workspace.current_camera.forward().x > 0.0);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .forward()
            .abs_diff_eq(Vec3::NEG_Z, 1e-5)
    );

    let forward = workspace.current_camera.forward();
    let pointer_context = Context::default();
    let mut output = pointer_context.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            ..Default::default()
        },
        |ui| {
            let _ = ui.button("capture");
        },
    );
    output.textures_delta.clear();
    let pointer_pos = pos2(20.0, 16.0);
    let mut output = pointer_context.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            events: vec![
                Event::PointerMoved(pointer_pos),
                Event::PointerButton {
                    pos: pointer_pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                key(egui::Key::W, true),
            ],
            ..Default::default()
        },
        |ui| {
            let _ = ui.button("capture");
            let player = workspace.get_mut::<PlayerController>(id).unwrap();
            player.process_mouse_motion((20.0, 0.0));
            player.process_input(ui.ctx());
        },
    );
    output.textures_delta.clear();
    workspace.update(0.1);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .position()
            .abs_diff_eq(forward * 0.6, 1e-5)
    );
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .forward()
            .abs_diff_eq(forward, 1e-5)
    );

    let player = workspace.get_mut::<PlayerController>(id).unwrap();
    player.process_mouse_motion((20.0, 0.0));
    player.process_window_event(&WindowEvent::Focused(false));
    let pose = workspace.get::<Rig>(rig).unwrap().pose();
    let camera_pose = workspace.current_camera.pose();
    workspace.update(0.1);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .pose()
            .abs_diff_eq(pose, 1e-5)
    );
    assert!(
        workspace
            .current_camera
            .pose()
            .abs_diff_eq(camera_pose, 1e-5)
    );
}

instance! {
    #[instance(default = false)]
    #[derive(Clone, Debug)]
    struct CustomController {
        #[data]
        instance: InstanceData,
        rig: InstanceId,
        input: Option<ControllerInput>,
        actions: ControllerActions,
    }
}

impl Controller for CustomController {
    fn input(&self) -> Option<&ControllerInput> {
        self.input.as_ref()
    }
    fn input_mut(&mut self) -> Option<&mut ControllerInput> {
        self.input.as_mut()
    }
    fn update(&mut self, workspace: &mut Workspace, delta_seconds: f32) {
        let actions = self
            .input
            .as_mut()
            .map_or(self.actions, ControllerInput::take_actions);
        if let Some(rig) = workspace.get_mut::<Rig>(self.rig) {
            let position = rig.position() + actions.translation(Quat::IDENTITY, 4.0, delta_seconds);
            rig.with_position(position);
        }
    }
    fn remap_instances(&mut self, map: &dyn Fn(InstanceId) -> Option<InstanceId>) {
        if let Some(id) = map(self.rig) {
            self.rig = id;
        }
    }
}

#[test]
fn downstream_controllers_can_compose_reusable_input_or_run_without_devices() {
    let mut workspace = Workspace::new();
    workspace
        .camera_controller_mut()
        .unwrap()
        .set_enabled(false);
    let controlled = workspace.add_child(Rig::new());
    let autonomous = workspace.add_child(Rig::new());
    let mut input = ControllerInput::default()
        .with_mouse_enabled(false)
        .with_touch_enabled(false);
    input.key_bindings.forward = vec![KeyCode::KeyJ];
    workspace.add_child(CustomController {
        instance: InstanceData::new("CustomController"),
        rig: controlled,
        input: Some(input),
        actions: ControllerActions::default(),
    });
    workspace.add_child(CustomController {
        instance: InstanceData::new("CustomController"),
        rig: autonomous,
        input: None,
        actions: ControllerActions {
            movement: Vec3::X,
            ..Default::default()
        },
    });

    frame(
        &Context::default(),
        &mut workspace,
        vec![key(egui::Key::J, true)],
    );
    workspace.update(0.1);
    assert!(
        workspace
            .get::<Rig>(controlled)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::NEG_Z * 0.4, 1e-5)
    );
    assert!(
        workspace
            .get::<Rig>(autonomous)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::X * 0.4, 1e-5)
    );
}

instance! {
    #[instance(default = false)]
    #[derive(Clone, Debug)]
    struct PlaceRig {
        #[data]
        instance: InstanceData,
        rig: InstanceId,
        position: Vec3,
    }
}

impl Controller for PlaceRig {
    fn update(&mut self, workspace: &mut Workspace, _: f32) {
        workspace
            .get_mut::<Rig>(self.rig)
            .unwrap()
            .with_position(self.position);
    }
}

#[test]
fn child_order_removal_and_reconfiguration_control_the_observable_result() {
    let mut workspace = Workspace::new();
    let rig = workspace.add_child(Rig::new());
    let place = workspace.add_child(PlaceRig {
        instance: InstanceData::new("PlaceRig"),
        rig,
        position: Vec3::X,
    });
    let mut player = PlayerController::new(rig);
    player.process_key(KeyCode::KeyW, true);
    let player = workspace.add_child(player);
    workspace.update_controllers(0.1);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::new(1.0, 0.0, -0.6), 1e-5)
    );
    assert!(workspace.remove_child(place));
    workspace.get_mut::<PlayerController>(player).unwrap().speed = 2.0;
    workspace.update_controllers(0.1);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .position()
            .abs_diff_eq(Vec3::new(1.0, 0.0, -0.8), 1e-5)
    );
    workspace.add_child(PlaceRig {
        instance: InstanceData::new("PlaceRig"),
        rig,
        position: Vec3::Y,
    });
    workspace.update_controllers(0.1);
    assert_eq!(workspace.get::<Rig>(rig).unwrap().position(), Vec3::Y);
}

#[test]
fn workspace_cloning_retargets_players_and_custom_controllers_to_cloned_rigs() {
    let mut workspace = Workspace::new();
    workspace
        .camera_controller_mut()
        .unwrap()
        .set_enabled(false);
    let rig = workspace.add_child(Rig::new().with_name("Player"));
    workspace.current_camera.set_subject(Some(rig));
    let mut player = PlayerController::new(rig);
    player.process_key(KeyCode::KeyW, true);
    workspace.add_child(player);
    workspace.add_child(CustomController {
        instance: InstanceData::new("CustomController"),
        rig,
        input: None,
        actions: ControllerActions {
            movement: Vec3::X,
            ..Default::default()
        },
    });
    let mut cloned = workspace.clone();
    cloned.update(0.1);
    let cloned_rig = cloned.get_all::<Rig>().next().unwrap();
    assert_ne!(cloned_rig.id(), rig);
    assert!(
        cloned_rig
            .position()
            .abs_diff_eq(Vec3::new(0.4, 0.0, -0.6), 1e-5)
    );
    assert_eq!(workspace.get::<Rig>(rig).unwrap().position(), Vec3::ZERO);
    assert_eq!(cloned.current_camera.subject(), Some(cloned_rig.id()));
    assert!(
        (cloned
            .current_camera
            .position()
            .distance(cloned_rig.position())
            - 5.0)
            .abs()
            < 1e-5
    );
}

#[test]
fn missing_and_non_rig_targets_do_not_replay_look_when_a_rig_is_later_selected() {
    let mut workspace = Workspace::new();
    let missing = workspace.add_child(Rig::new());
    workspace.remove_child(missing);
    let id = workspace.add_child(PlayerController::new(missing));
    workspace.process_device_event(&DeviceEvent::MouseMotion {
        delta: (100.0, 0.0),
    });
    workspace.update_controllers(0.1);
    let part = workspace.add_child(Part::new());
    workspace.get_mut::<PlayerController>(id).unwrap().rig = Some(part);
    workspace.process_device_event(&DeviceEvent::MouseMotion {
        delta: (100.0, 0.0),
    });
    workspace.update_controllers(0.1);
    assert!(
        workspace
            .get::<Part>(part)
            .unwrap()
            .pose()
            .abs_diff_eq(Mat4::IDENTITY, 1e-5)
    );
    let rig = workspace.add_child(Rig::new());
    workspace.get_mut::<PlayerController>(id).unwrap().rig = Some(rig);
    workspace.update_controllers(0.1);
    assert!(
        workspace
            .get::<Rig>(rig)
            .unwrap()
            .pose()
            .abs_diff_eq(Mat4::IDENTITY, 1e-5)
    );
}

#[test]
fn invalid_frame_durations_do_not_move_players_and_long_frames_are_capped() {
    let mut rig = Rig::new();
    let mut player = PlayerController::new(rig.id());
    player.process_key(KeyCode::KeyW, true);
    for delta in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -1.0, 0.0] {
        player.update_rig(&mut rig, delta);
        assert_eq!(rig.position(), Vec3::ZERO);
    }
    player.update_rig(&mut rig, 10.0);
    assert!(rig.position().abs_diff_eq(Vec3::NEG_Z * 0.6, 1e-5));
}

instance! {
    #[instance(default = false)]
    #[derive(Clone, Debug)]
    struct ReplaceController {
        #[data]
        instance: InstanceData,
        rig: InstanceId,
        remove: Option<InstanceId>,
    }
}

impl Controller for ReplaceController {
    fn update(&mut self, workspace: &mut Workspace, _: f32) {
        workspace.remove_child(self.id());
        if let Some(id) = self.remove {
            workspace.remove_child(id);
        }
        workspace.add_child(PlaceRig {
            instance: InstanceData::new("PlaceRig"),
            rig: self.rig,
            position: Vec3::Y,
        });
    }
}

#[test]
fn controllers_can_remove_children_and_defer_new_behaviors_until_the_next_update() {
    let mut workspace = Workspace::new();
    let rig = workspace.add_child(Rig::new());
    let id = workspace.add_child(ReplaceController {
        instance: InstanceData::new("ReplaceController"),
        rig,
        remove: None,
    });
    let removed = workspace.add_child(PlaceRig {
        instance: InstanceData::new("PlaceRig"),
        rig,
        position: Vec3::X,
    });
    workspace.get_mut::<ReplaceController>(id).unwrap().remove = Some(removed);
    workspace.update_controllers(0.1);
    assert_eq!(workspace.get::<Rig>(rig).unwrap().position(), Vec3::ZERO);
    assert!(workspace.view::<dyn Instance>(id).is_none());
    workspace.update_controllers(0.1);
    assert_eq!(workspace.get::<Rig>(rig).unwrap().position(), Vec3::Y);
}

#[test]
fn nested_controllers_do_not_receive_input_or_update_the_scene() {
    let mut workspace = Workspace::new();
    let camera = workspace.camera_controller().unwrap().id();
    workspace.remove_child(camera);
    let nested_rig = workspace.add_child(Rig::new());
    let direct_rig = workspace.add_child(Rig::new());
    let mut parent = Part::new();
    let mut player = PlayerController::new(nested_rig);
    player.process_key(KeyCode::KeyW, true);
    parent.add_child(player);
    let mut camera = CameraController::default();
    camera.process_key(KeyCode::KeyW, true);
    let camera = parent.add_child(camera);
    workspace.add_child(parent);
    let player = workspace.add_child(PlayerController::new(direct_rig).with_name("Player input"));
    let initial_camera = workspace.current_camera.pose();
    let context = Context::default();
    frame(&context, &mut workspace, vec![key(egui::Key::W, true)]);
    workspace.process_device_event(&DeviceEvent::MouseMotion {
        delta: (100.0, 0.0),
    });
    workspace.process_window_event(&WindowEvent::Focused(false));
    frame(&context, &mut workspace, vec![]);
    workspace.update(0.1);
    assert_eq!(
        workspace.get::<Rig>(nested_rig).unwrap().position(),
        Vec3::ZERO
    );
    assert!(workspace.get::<Rig>(direct_rig).unwrap().position().z < -0.5);
    assert_eq!(workspace.current_camera.pose(), initial_camera);
    assert!(workspace.camera_controller().is_none());
    assert!(workspace.camera_controller_mut().is_none());
    assert!(workspace.get::<CameraController>(camera).unwrap().forward);
    assert_eq!(
        workspace.get::<PlayerController>(player).unwrap().name(),
        "Player input"
    );

    workspace
        .get_mut::<PlayerController>(player)
        .unwrap()
        .destroy();
    let position = workspace.get::<Rig>(direct_rig).unwrap().position();
    frame(&context, &mut workspace, vec![key(egui::Key::W, true)]);
    workspace.update(0.1);
    assert_eq!(
        workspace.get::<Rig>(direct_rig).unwrap().position(),
        position
    );
}

#[test]
fn camera_control_requires_a_direct_child_and_stops_after_destruction() {
    let mut workspace = Workspace::new();
    let camera = workspace.camera_controller().unwrap().id();
    assert_eq!(
        workspace.get::<CameraController>(camera).unwrap().parent(),
        Some(workspace.id())
    );
    workspace
        .get_mut::<CameraController>(camera)
        .unwrap()
        .destroy();
    let initial = workspace.current_camera.pose();
    frame(
        &Context::default(),
        &mut workspace,
        vec![key(egui::Key::W, true)],
    );
    workspace.process_device_event(&DeviceEvent::MouseMotion { delta: (20.0, 0.0) });
    workspace.update(0.1);
    assert_eq!(workspace.current_camera.pose(), initial);

    let id = CameraController::default()
        .with_name("Flight")
        .set_parent(&mut workspace);
    frame(
        &Context::default(),
        &mut workspace,
        vec![key(egui::Key::W, true)],
    );
    workspace.process_device_event(&DeviceEvent::MouseMotion { delta: (20.0, 0.0) });
    workspace.update(0.1);
    assert!(!workspace.current_camera.pose().abs_diff_eq(initial, 1e-5));
    assert!(workspace.current_camera.forward().x > 0.0);
    workspace.get_mut::<CameraController>(id).unwrap().destroy();
    let stopped = workspace.current_camera.pose();
    workspace.update(0.1);
    assert_eq!(workspace.current_camera.pose(), stopped);
}
