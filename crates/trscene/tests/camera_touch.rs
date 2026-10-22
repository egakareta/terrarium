use trscene::{
    Camera, CameraController, Instance, PVInstanceExt,
    egui::{
        self, Context, Event, Pos2, RawInput, Rect, TouchDeviceId, TouchId, TouchPhase, pos2, vec2,
    },
    glam::Mat4,
};

fn touch(device: u64, id: u64, phase: TouchPhase, position: Pos2) -> Event {
    Event::Touch {
        device_id: TouchDeviceId(device),
        id: TouchId(id),
        phase,
        pos: position,
        force: None,
    }
}

fn raw_input(events: Vec<Event>) -> RawInput {
    RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
        events,
        ..Default::default()
    }
}

fn frame(context: &Context, controller: &mut CameraController, events: Vec<Event>) {
    let mut output = context.run_ui(raw_input(events), |ui| {
        controller.input.process_eframe_input(ui.ctx());
    });
    output.textures_delta.clear();
}

fn start_joystick(context: &Context, controller: &mut CameraController) {
    frame(
        context,
        controller,
        vec![
            touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
            touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
        ],
    );
}

#[test]
fn joystick_matches_keyboard_movement_in_free_and_subject_modes() {
    for subject in [None, Some((Camera::default().id(), Mat4::IDENTITY))] {
        for direction in [
            vec2(0.0, -1.0),
            vec2(0.0, 1.0),
            vec2(-1.0, 0.0),
            vec2(1.0, 0.0),
            vec2(1.0, -1.0),
        ] {
            let mut touch_controller = CameraController::default();
            let context = Context::default();
            frame(
                &context,
                &mut touch_controller,
                vec![
                    touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
                    touch(0, 1, TouchPhase::Move, pos2(88.0, 512.0) + direction * 60.0),
                ],
            );
            let mut keyboard_controller = CameraController::new(6.0, 0.0025);
            keyboard_controller.forward = direction.y < 0.0;
            keyboard_controller.backward = direction.y > 0.0;
            keyboard_controller.left = direction.x < 0.0;
            keyboard_controller.right = direction.x > 0.0;
            if subject.is_some() && direction.x != 0.0 && direction.y != 0.0 {
                keyboard_controller.speed *= std::f32::consts::FRAC_1_SQRT_2;
                keyboard_controller.orbit_speed *= std::f32::consts::FRAC_1_SQRT_2;
            }
            let mut touch_camera = Camera::default();
            let mut keyboard_camera = touch_camera.clone();
            touch_controller.update_camera(&mut touch_camera, subject, 0.1);
            keyboard_controller.update_camera(&mut keyboard_camera, subject, 0.1);

            assert!(
                touch_camera
                    .pose()
                    .abs_diff_eq(keyboard_camera.pose(), 1e-5)
            );
        }
    }
}

#[test]
fn joystick_has_a_dead_zone_and_proportional_speed() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    let initial = camera.position();
    frame(
        &context,
        &mut controller,
        vec![touch(0, 1, TouchPhase::Start, pos2(89.0, 511.0))],
    );
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.position().abs_diff_eq(initial, 1e-6));

    frame(
        &context,
        &mut controller,
        vec![touch(0, 1, TouchPhase::Move, pos2(88.0, 492.0))],
    );
    controller.update_camera(&mut camera, None, 0.1);
    let partial_distance = camera.position().distance(initial);
    assert!(partial_distance > 0.0 && partial_distance < 0.6);

    let before = camera.position();
    frame(
        &context,
        &mut controller,
        vec![touch(0, 1, TouchPhase::Move, pos2(88.0, 100.0))],
    );
    controller.update_camera(&mut camera, None, 0.1);
    assert!((camera.position().distance(before) - 0.6).abs() < 1e-5);
}

#[test]
fn two_fingers_can_move_and_look_in_either_start_order() {
    for look_first in [false, true] {
        for subject in [None, Some((Camera::default().id(), Mat4::IDENTITY))] {
            let context = Context::default();
            let mut controller = CameraController::default();
            let mut starts = vec![
                touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
                touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0)),
            ];
            if look_first {
                starts.reverse();
            }
            frame(&context, &mut controller, starts);
            frame(
                &context,
                &mut controller,
                vec![
                    touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
                    touch(0, 2, TouchPhase::Move, pos2(420.0, 235.0)),
                ],
            );
            let mut expected_controller = CameraController::new(6.0, 0.0025);
            expected_controller.forward = true;
            expected_controller.process_mouse_motion((20.0, -15.0));
            let mut camera = Camera::default();
            let mut expected = camera.clone();
            controller.update_camera(&mut camera, subject, 0.1);
            expected_controller.update_camera(&mut expected, subject, 0.1);
            assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
        }
    }
}

#[test]
fn synthetic_pointer_motion_does_not_rotate_the_joystick_or_double_touch_look() {
    for on_joystick in [false, true] {
        let context = Context::default();
        let mut controller = CameraController::default();
        let start = if on_joystick {
            pos2(88.0, 512.0)
        } else {
            pos2(400.0, 250.0)
        };
        frame(
            &context,
            &mut controller,
            vec![
                Event::PointerMoved(start),
                Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                touch(0, 1, TouchPhase::Start, start),
            ],
        );
        let end = start + vec2(20.0, -15.0);
        frame(
            &context,
            &mut controller,
            vec![Event::PointerMoved(end), touch(0, 1, TouchPhase::Move, end)],
        );
        let mut camera = Camera::default();
        let mut expected = camera.clone();
        let mut expected_controller = CameraController::default();
        if !on_joystick {
            expected_controller.process_mouse_motion((20.0, -15.0));
        }
        controller.update_camera(&mut camera, None, 0.1);
        expected_controller.update_camera(&mut expected, None, 0.1);
        assert!(camera.forward().abs_diff_eq(expected.forward(), 1e-5));
    }
}

#[test]
fn releasing_or_cancelling_one_finger_keeps_the_other_control_working() {
    for phase in [TouchPhase::End, TouchPhase::Cancel] {
        let context = Context::default();
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        start_joystick(&context, &mut controller);
        frame(
            &context,
            &mut controller,
            vec![touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0))],
        );
        controller.update_camera(&mut camera, None, 0.1);
        let before = camera.position();
        frame(
            &context,
            &mut controller,
            vec![touch(0, 2, phase, pos2(400.0, 250.0))],
        );
        controller.update_camera(&mut camera, None, 0.1);
        assert!((camera.position().distance(before) - 0.6).abs() < 1e-5);

        frame(
            &context,
            &mut controller,
            vec![touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0))],
        );
        frame(
            &context,
            &mut controller,
            vec![
                touch(0, 1, phase, pos2(88.0, 452.0)),
                touch(0, 2, TouchPhase::Move, pos2(420.0, 250.0)),
            ],
        );
        let before = camera.position();
        let forward = camera.forward();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.position().abs_diff_eq(before, 1e-5));
        assert!(!camera.forward().abs_diff_eq(forward, 1e-5));
    }
}

#[test]
fn extra_fingers_and_other_devices_cannot_take_over_a_gesture() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    start_joystick(&context, &mut controller);
    frame(
        &context,
        &mut controller,
        vec![
            touch(1, 1, TouchPhase::Start, pos2(88.0, 512.0)),
            touch(1, 1, TouchPhase::Move, pos2(88.0, 572.0)),
            touch(1, 1, TouchPhase::End, pos2(88.0, 572.0)),
        ],
    );
    let initial = camera.position();
    controller.update_camera(&mut camera, None, 0.1);
    assert!((camera.position().z - initial.z + 0.6).abs() < 1e-5);
    assert!(
        camera
            .forward()
            .abs_diff_eq(Camera::default().forward(), 1e-5)
    );
}

#[test]
fn touch_input_is_independent_of_keyboard_and_mouse_and_can_be_disabled() {
    let context = Context::default();
    let mut controller = CameraController::default()
        .with_keyboard_enabled(false)
        .with_mouse_enabled(false);
    let mut camera = Camera::default();
    start_joystick(&context, &mut controller);
    frame(
        &context,
        &mut controller,
        vec![
            touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0)),
            touch(0, 2, TouchPhase::Move, pos2(420.0, 250.0)),
        ],
    );
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.position().distance(Camera::default().position()) > 0.5);
    assert!(camera.forward().x > 0.0);

    controller.set_touch_enabled(false);
    let before = camera.pose();
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
    start_joystick(&context, &mut controller);
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
    assert!(!controller.is_enabled());

    controller.set_enabled(true);
    start_joystick(&context, &mut controller);
    controller.set_enabled(false);
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
}

#[test]
fn touch_on_egui_widget_does_not_start_camera_gesture() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    let initial = camera.pose();
    for events in [
        vec![],
        vec![touch(0, 1, TouchPhase::Start, pos2(100.0, 50.0))],
        vec![touch(0, 1, TouchPhase::Move, pos2(120.0, 70.0))],
    ] {
        let mut output = context.run_ui(raw_input(events), |ui| {
            ui.allocate_exact_size(vec2(200.0, 150.0), egui::Sense::drag());
            controller.input.process_eframe_input(ui.ctx());
        });
        output.textures_delta.clear();
    }
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(initial, 1e-5));
}

#[test]
fn focus_loss_and_resize_cancel_touch_gestures() {
    for lose_focus in [false, true] {
        let context = Context::default();
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        start_joystick(&context, &mut controller);
        frame(
            &context,
            &mut controller,
            vec![
                touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0)),
                touch(0, 2, TouchPhase::Move, pos2(420.0, 250.0)),
            ],
        );
        let mut input = raw_input(vec![]);
        if lose_focus {
            input.focused = false;
        } else {
            input.screen_rect = Some(Rect::from_min_size(Pos2::ZERO, vec2(600.0, 800.0)));
        }
        let mut output =
            context.run_ui(input, |ui| controller.input.process_eframe_input(ui.ctx()));
        output.textures_delta.clear();
        let initial = camera.pose();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.pose().abs_diff_eq(initial, 1e-5));
        frame(
            &context,
            &mut controller,
            vec![
                touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
                touch(0, 2, TouchPhase::Move, pos2(450.0, 250.0)),
            ],
        );
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.pose().abs_diff_eq(initial, 1e-5));
    }
}

#[test]
fn touch_drag_uses_physical_pixels_at_high_dpi() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut input = raw_input(vec![
        touch(0, 1, TouchPhase::Start, pos2(400.0, 250.0)),
        touch(0, 1, TouchPhase::Move, pos2(410.0, 245.0)),
    ]);
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .native_pixels_per_point = Some(2.0);
    let mut output = context.run_ui(input, |ui| controller.input.process_eframe_input(ui.ctx()));
    output.textures_delta.clear();
    let mut camera = Camera::default();
    let mut expected = camera.clone();
    let mut expected_controller = CameraController::default();
    expected_controller.process_mouse_motion((20.0, -10.0));
    controller.update_camera(&mut camera, None, 0.1);
    expected_controller.update_camera(&mut expected, None, 0.1);
    assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
    let before = camera.pose();
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
}

#[test]
fn keyboard_and_joystick_share_speed_limits_and_sprint() {
    for sprint in [false, true] {
        let context = Context::default();
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        let mut events = vec![
            touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
            touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
            Event::Key {
                key: egui::Key::W,
                physical_key: Some(egui::Key::W),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            },
        ];
        if sprint {
            events.push(Event::Key {
                key: egui::Key::ShiftLeft,
                physical_key: Some(egui::Key::ShiftLeft),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            });
        }
        frame(&context, &mut controller, events);
        let initial = camera.position();
        controller.update_camera(&mut camera, None, 0.1);
        let expected_distance = if sprint { 1.5 } else { 0.6 };
        assert!((camera.position().distance(initial) - expected_distance).abs() < 1e-5);

        frame(
            &context,
            &mut controller,
            vec![touch(0, 1, TouchPhase::Move, pos2(88.0, 572.0))],
        );
        let before = camera.position();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.position().abs_diff_eq(before, 1e-5));
    }
}

#[test]
fn a_second_finger_on_an_egui_area_does_not_rotate_the_camera() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    for events in [
        vec![],
        vec![
            touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
            touch(0, 2, TouchPhase::Start, pos2(400.0, 250.0)),
        ],
        vec![
            touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
            touch(0, 2, TouchPhase::Move, pos2(420.0, 250.0)),
        ],
    ] {
        let mut output = context.run_ui(raw_input(events), |ui| {
            egui::Area::new(egui::Id::new("controls"))
                .fixed_pos(pos2(300.0, 200.0))
                .show(ui.ctx(), |ui| {
                    ui.allocate_exact_size(vec2(200.0, 150.0), egui::Sense::drag());
                });
            controller.input.process_eframe_input(ui.ctx());
        });
        output.textures_delta.clear();
    }
    controller.update_camera(&mut camera, None, 0.1);
    assert!(
        camera
            .forward()
            .abs_diff_eq(Camera::default().forward(), 1e-5)
    );
    assert!((camera.position().z - Camera::default().position().z + 0.6).abs() < 1e-5);
}

#[test]
fn holding_an_egui_widget_allows_other_fingers_to_move_and_look() {
    for subject in [None, Some((Camera::default().id(), Mat4::IDENTITY))] {
        let context = Context::default();
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        let initial = camera.pose();
        for events in [
            vec![],
            vec![
                Event::PointerMoved(pos2(100.0, 50.0)),
                Event::PointerButton {
                    pos: pos2(100.0, 50.0),
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
                touch(0, 1, TouchPhase::Start, pos2(100.0, 50.0)),
            ],
            vec![
                Event::PointerMoved(pos2(120.0, 70.0)),
                touch(0, 1, TouchPhase::Move, pos2(120.0, 70.0)),
            ],
        ] {
            let mut output = context.run_ui(raw_input(events), |ui| {
                ui.allocate_exact_size(vec2(200.0, 150.0), egui::Sense::drag());
                controller.input.process_eframe_input(ui.ctx());
            });
            output.textures_delta.clear();
        }
        assert!(context.egui_wants_pointer_input());
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.pose().abs_diff_eq(initial, 1e-5));

        for events in [
            vec![
                touch(0, 2, TouchPhase::Start, pos2(88.0, 512.0)),
                touch(0, 3, TouchPhase::Start, pos2(400.0, 250.0)),
            ],
            vec![
                touch(0, 2, TouchPhase::Move, pos2(88.0, 452.0)),
                touch(0, 3, TouchPhase::Move, pos2(420.0, 235.0)),
            ],
        ] {
            let mut output = context.run_ui(raw_input(events), |ui| {
                let (_, response) = ui.allocate_exact_size(vec2(200.0, 150.0), egui::Sense::drag());
                assert!(response.dragged());
                assert!(ui.ctx().egui_wants_pointer_input());
                controller.input.process_eframe_input(ui.ctx());
            });
            output.textures_delta.clear();
        }
        let mut expected_controller = CameraController::new(6.0, 0.0025);
        expected_controller.forward = true;
        expected_controller.process_mouse_motion((20.0, -15.0));
        let mut expected = camera.clone();
        controller.update_camera(&mut camera, subject, 0.1);
        expected_controller.update_camera(&mut expected, subject, 0.1);
        assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
    }
}

#[test]
fn a_second_finger_on_a_background_widget_does_not_rotate_the_camera() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    for events in [
        vec![],
        vec![
            touch(0, 1, TouchPhase::Start, pos2(88.0, 512.0)),
            touch(0, 2, TouchPhase::Start, pos2(100.0, 50.0)),
        ],
        vec![
            touch(0, 1, TouchPhase::Move, pos2(88.0, 452.0)),
            touch(0, 2, TouchPhase::Move, pos2(120.0, 70.0)),
        ],
    ] {
        let mut output = context.run_ui(raw_input(events), |ui| {
            ui.allocate_exact_size(vec2(200.0, 150.0), egui::Sense::drag());
            controller.input.process_eframe_input(ui.ctx());
        });
        output.textures_delta.clear();
    }
    controller.update_camera(&mut camera, None, 0.1);
    assert!(
        camera
            .forward()
            .abs_diff_eq(Camera::default().forward(), 1e-5)
    );
    assert!((camera.position().z - Camera::default().position().z + 0.6).abs() < 1e-5);
}

#[test]
fn mouse_drag_and_keyboard_movement_work_after_touch_ends() {
    let context = Context::default();
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    frame(
        &context,
        &mut controller,
        vec![
            touch(0, 1, TouchPhase::Start, pos2(400.0, 250.0)),
            touch(0, 1, TouchPhase::End, pos2(400.0, 250.0)),
        ],
    );
    frame(
        &context,
        &mut controller,
        vec![
            Event::PointerMoved(pos2(400.0, 250.0)),
            Event::PointerButton {
                pos: pos2(400.0, 250.0),
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    controller.update_camera(&mut camera, None, 0.1);
    let mut expected = camera.clone();
    frame(
        &context,
        &mut controller,
        vec![
            Event::PointerMoved(pos2(420.0, 235.0)),
            Event::Key {
                key: egui::Key::W,
                physical_key: Some(egui::Key::W),
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            },
        ],
    );
    let mut expected_controller = CameraController::new(6.0, 0.0025);
    expected_controller.forward = true;
    expected_controller.process_mouse_motion((20.0, -15.0));
    controller.update_camera(&mut camera, None, 0.1);
    expected_controller.update_camera(&mut expected, None, 0.1);
    assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
}
