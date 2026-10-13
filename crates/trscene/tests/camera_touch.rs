use trscene::{
    Camera, CameraController, FrameInput, HasPVInstance, Instance, TouchEvent, TouchPhase,
    glam::{Mat4, Vec2},
};

fn touch(device: u64, id: u64, phase: TouchPhase, position: [f32; 2]) -> TouchEvent {
    TouchEvent {
        device,
        id,
        phase,
        position,
        captured: false,
    }
}
fn input(touches: Vec<TouchEvent>) -> FrameInput {
    FrameInput {
        size: [800.0, 600.0],
        touches,
        ..Default::default()
    }
}
fn start_joystick(controller: &mut CameraController) {
    controller.process_input(&input(vec![
        touch(0, 1, TouchPhase::Start, [88.0, 512.0]),
        touch(0, 1, TouchPhase::Move, [88.0, 452.0]),
    ]));
}

#[test]
fn joystick_matches_keyboard_movement_in_free_and_subject_modes() {
    for subject in [None, Some((Camera::default().id(), Mat4::IDENTITY))] {
        for direction in [
            Vec2::NEG_Y,
            Vec2::Y,
            Vec2::NEG_X,
            Vec2::X,
            Vec2::new(1.0, -1.0),
        ] {
            let mut touch_controller = CameraController::default();
            touch_controller.process_input(&input(vec![
                touch(0, 1, TouchPhase::Start, [88.0, 512.0]),
                touch(
                    0,
                    1,
                    TouchPhase::Move,
                    (Vec2::new(88.0, 512.0) + direction * 60.0).to_array(),
                ),
            ]));
            let mut keyboard = CameraController::default();
            keyboard.forward = direction.y < 0.0;
            keyboard.backward = direction.y > 0.0;
            keyboard.left = direction.x < 0.0;
            keyboard.right = direction.x > 0.0;
            if subject.is_some() && direction.x != 0.0 && direction.y != 0.0 {
                keyboard.speed *= std::f32::consts::FRAC_1_SQRT_2;
                keyboard.orbit_speed *= std::f32::consts::FRAC_1_SQRT_2;
            }
            let mut camera = Camera::default();
            let mut expected = camera.clone();
            touch_controller.update_camera(&mut camera, subject, 0.1);
            keyboard.update_camera(&mut expected, subject, 0.1);
            assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
        }
    }
}

#[test]
fn joystick_has_a_dead_zone_and_proportional_speed() {
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    let initial = camera.position();
    controller.process_input(&input(vec![touch(0, 1, TouchPhase::Start, [89.0, 511.0])]));
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.position().abs_diff_eq(initial, 1e-6));
    controller.process_input(&input(vec![touch(0, 1, TouchPhase::Move, [88.0, 492.0])]));
    controller.update_camera(&mut camera, None, 0.1);
    let distance = camera.position().distance(initial);
    assert!(distance > 0.0 && distance < 0.6);
    let before = camera.position();
    controller.process_input(&input(vec![touch(0, 1, TouchPhase::Move, [88.0, 100.0])]));
    controller.update_camera(&mut camera, None, 0.1);
    assert!((camera.position().distance(before) - 0.6).abs() < 1e-5);
}

#[test]
fn two_fingers_can_move_and_look_in_either_start_order() {
    for look_first in [false, true] {
        for subject in [None, Some((Camera::default().id(), Mat4::IDENTITY))] {
            let mut controller = CameraController::default();
            let mut starts = vec![
                touch(0, 1, TouchPhase::Start, [88.0, 512.0]),
                touch(0, 2, TouchPhase::Start, [400.0, 250.0]),
            ];
            if look_first {
                starts.reverse();
            }
            controller.process_input(&input(starts));
            controller.process_input(&input(vec![
                touch(0, 1, TouchPhase::Move, [88.0, 452.0]),
                touch(0, 2, TouchPhase::Move, [420.0, 235.0]),
            ]));
            let mut expected_controller = CameraController::default();
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
        let mut controller = CameraController::default();
        let start = if on_joystick {
            [88.0, 512.0]
        } else {
            [400.0, 250.0]
        };
        controller.process_input(&input(vec![touch(0, 1, TouchPhase::Start, start)]));
        let end = [start[0] + 20.0, start[1] - 15.0];
        let mut frame = input(vec![touch(0, 1, TouchPhase::Move, end)]);
        frame.buttons = vec![0];
        frame.mouse_delta = [20.0, -15.0];
        controller.process_input(&frame);
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
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        start_joystick(&mut controller);
        controller.process_input(&input(vec![touch(0, 2, TouchPhase::Start, [400.0, 250.0])]));
        controller.update_camera(&mut camera, None, 0.1);
        let before = camera.position();
        controller.process_input(&input(vec![touch(0, 2, phase, [400.0, 250.0])]));
        controller.update_camera(&mut camera, None, 0.1);
        assert!((camera.position().distance(before) - 0.6).abs() < 1e-5);
        controller.process_input(&input(vec![touch(0, 2, TouchPhase::Start, [400.0, 250.0])]));
        controller.process_input(&input(vec![
            touch(0, 1, phase, [88.0, 452.0]),
            touch(0, 2, TouchPhase::Move, [420.0, 250.0]),
        ]));
        let before = camera.position();
        let forward = camera.forward();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.position().abs_diff_eq(before, 1e-5));
        assert!(!camera.forward().abs_diff_eq(forward, 1e-5));
    }
}

#[test]
fn extra_fingers_and_other_devices_cannot_take_over_a_gesture() {
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    start_joystick(&mut controller);
    controller.process_input(&input(vec![
        touch(1, 1, TouchPhase::Start, [88.0, 512.0]),
        touch(1, 1, TouchPhase::Move, [88.0, 572.0]),
        touch(1, 1, TouchPhase::End, [88.0, 572.0]),
    ]));
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
    let mut controller = CameraController::default()
        .with_keyboard_enabled(false)
        .with_mouse_enabled(false);
    let mut camera = Camera::default();
    start_joystick(&mut controller);
    controller.process_input(&input(vec![
        touch(0, 2, TouchPhase::Start, [400.0, 250.0]),
        touch(0, 2, TouchPhase::Move, [420.0, 250.0]),
    ]));
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.position().distance(Camera::default().position()) > 0.5);
    assert!(camera.forward().x > 0.0);
    controller.set_touch_enabled(false);
    let before = camera.pose();
    start_joystick(&mut controller);
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
    assert!(!controller.is_enabled());
    controller.set_enabled(true);
    start_joystick(&mut controller);
    controller.set_enabled(false);
    controller.update_camera(&mut camera, None, 0.1);
    assert!(camera.pose().abs_diff_eq(before, 1e-5));
}

#[test]
fn captured_touch_does_not_block_other_fingers_from_moving_and_looking() {
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    let mut ui_touch = touch(0, 1, TouchPhase::Start, [100.0, 50.0]);
    ui_touch.captured = true;
    let mut frame = input(vec![
        ui_touch,
        touch(0, 2, TouchPhase::Start, [88.0, 512.0]),
        touch(0, 3, TouchPhase::Start, [400.0, 250.0]),
    ]);
    frame.pointer_captured = true;
    controller.process_input(&frame);
    frame.touches = vec![
        touch(0, 1, TouchPhase::Move, [120.0, 70.0]),
        touch(0, 2, TouchPhase::Move, [88.0, 452.0]),
        touch(0, 3, TouchPhase::Move, [420.0, 235.0]),
    ];
    controller.process_input(&frame);
    let mut expected = camera.clone();
    let mut expected_controller = CameraController::default();
    expected_controller.forward = true;
    expected_controller.process_mouse_motion((20.0, -15.0));
    controller.update_camera(&mut camera, None, 0.1);
    expected_controller.update_camera(&mut expected, None, 0.1);
    assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
}

#[test]
fn focus_loss_and_resize_cancel_touch_gestures() {
    for lose_focus in [false, true] {
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        start_joystick(&mut controller);
        controller.process_input(&input(vec![
            touch(0, 2, TouchPhase::Start, [400.0, 250.0]),
            touch(0, 2, TouchPhase::Move, [420.0, 250.0]),
        ]));
        let mut frame = input(vec![]);
        if lose_focus {
            frame.focused = false;
        } else {
            frame.size = [600.0, 800.0];
        }
        controller.process_input(&frame);
        let initial = camera.pose();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.pose().abs_diff_eq(initial, 1e-5));
        frame.touches = vec![
            touch(0, 1, TouchPhase::Move, [88.0, 452.0]),
            touch(0, 2, TouchPhase::Move, [450.0, 250.0]),
        ];
        controller.process_input(&frame);
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.pose().abs_diff_eq(initial, 1e-5));
    }
}

#[test]
fn touch_drag_uses_physical_pixels_at_high_dpi() {
    let mut controller = CameraController::default();
    let mut frame = input(vec![
        touch(0, 1, TouchPhase::Start, [400.0, 250.0]),
        touch(0, 1, TouchPhase::Move, [410.0, 245.0]),
    ]);
    frame.scale = 2.0;
    controller.process_input(&frame);
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
        let mut controller = CameraController::default();
        let mut camera = Camera::default();
        let mut frame = input(vec![
            touch(0, 1, TouchPhase::Start, [88.0, 512.0]),
            touch(0, 1, TouchPhase::Move, [88.0, 452.0]),
        ]);
        frame.keys = vec!["KeyW".into()];
        if sprint {
            frame.keys.push("ShiftLeft".into());
        }
        controller.process_input(&frame);
        let before = camera.position();
        controller.update_camera(&mut camera, None, 0.1);
        assert!((camera.position().distance(before) - if sprint { 1.5 } else { 0.6 }).abs() < 1e-5);
        frame.touches = vec![touch(0, 1, TouchPhase::Move, [88.0, 572.0])];
        controller.process_input(&frame);
        let before = camera.position();
        controller.update_camera(&mut camera, None, 0.1);
        assert!(camera.position().abs_diff_eq(before, 1e-5));
    }
}

#[test]
fn mouse_drag_and_keyboard_movement_work_after_touch_ends() {
    let mut controller = CameraController::default();
    let mut camera = Camera::default();
    controller.process_input(&input(vec![
        touch(0, 1, TouchPhase::Start, [400.0, 250.0]),
        touch(0, 1, TouchPhase::End, [400.0, 250.0]),
    ]));
    controller.update_camera(&mut camera, None, 0.1);
    let mut expected = camera.clone();
    let mut frame = input(vec![]);
    frame.keys = vec!["KeyW".into()];
    frame.buttons = vec![0];
    frame.mouse_delta = [20.0, -15.0];
    controller.process_input(&frame);
    let mut expected_controller = CameraController::default();
    expected_controller.forward = true;
    expected_controller.process_mouse_motion((20.0, -15.0));
    controller.update_camera(&mut camera, None, 0.1);
    expected_controller.update_camera(&mut expected, None, 0.1);
    assert!(camera.pose().abs_diff_eq(expected.pose(), 1e-5));
}
