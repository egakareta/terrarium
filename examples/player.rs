use terrarium::{glam::*, *};

const SPAWN: Vec3 = Vec3::new(0.0, 4.0, 22.0);

struct PlayerApp {
    rig: InstanceId,
}

impl PlayerApp {
    fn respawn(&self, engine: &mut Engine) {
        if let Some(rig) = engine.get_mut::<Rig>(self.rig) {
            rig.with_position(SPAWN);
        }
    }
}

impl App for PlayerApp {
    fn after_update(
        &mut self,
        engine: &mut Engine,
        _context: &egui::Context,
        _frame: &mut terrarium::eframe::Frame,
    ) {
        if engine
            .get::<Rig>(self.rig)
            .is_some_and(|rig| rig.position().y < -10.0)
        {
            self.respawn(engine);
        }
    }

    fn ui(
        &mut self,
        engine: &mut Engine,
        ui: &mut egui::Ui,
        _frame: &mut terrarium::eframe::Frame,
    ) {
        egui::Panel::left("controls").show(ui, |ui| {
            ui.heading("Courtyard");
            ui.label("WASD: walk · Shift: sprint");
            ui.label("Space: jump");
            ui.label("Drag: orbit camera · Touch: joystick and drag");
            ui.separator();
            ui.label("Explore the buildings and take the ramp to the lookout.");
            if let Some(player) = engine
                .controller()
                .as_instance()
                .downcast_ref::<PlayerController>()
            {
                ui.label(format!("Animation: {:?}", player.animation_state()));
            }
            if ui.button("Respawn").clicked() {
                self.respawn(engine);
            }
        });
    }
}

fn map_part(name: &str, position: Vec3, size: Vec3, color: Color3) -> Part {
    Part::new()
        .with_name(name)
        .with_position(position)
        .with_size(size)
        .with_color(color)
}

fn add_map(workspace: &mut Workspace) {
    let grass = Color3::new(0.30, 0.48, 0.29);
    let stone = Color3::new(0.52, 0.55, 0.57);
    let path = Color3::new(0.76, 0.69, 0.52);
    let wood = Color3::new(0.38, 0.25, 0.16);
    workspace.add_child(map_part(
        "Ground",
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(64.0, 1.0, 64.0),
        grass,
    ));
    for (position, size) in [
        (Vec3::new(0.0, 1.25, -32.0), Vec3::new(65.0, 2.5, 1.0)),
        (Vec3::new(0.0, 1.25, 32.0), Vec3::new(65.0, 2.5, 1.0)),
        (Vec3::new(-32.0, 1.25, 0.0), Vec3::new(1.0, 2.5, 64.0)),
        (Vec3::new(32.0, 1.25, 0.0), Vec3::new(1.0, 2.5, 64.0)),
    ] {
        workspace.add_child(map_part("Boundary", position, size, stone));
    }
    for size in [Vec3::new(5.0, 0.02, 62.0), Vec3::new(62.0, 0.02, 5.0)] {
        workspace.add_child(map_part("Path", Vec3::ZERO, size, path).with_can_collide(false));
    }
    workspace.add_child(
        map_part("Plaza", Vec3::ZERO, Vec3::new(12.0, 0.03, 12.0), path).with_can_collide(false),
    );

    for (x, color) in [
        (-15.0, Color3::new(0.75, 0.48, 0.31)),
        (15.0, Color3::new(0.38, 0.56, 0.65)),
    ] {
        let center = Vec3::new(x, 0.0, -13.0);
        for (offset, size) in [
            (Vec3::new(-5.0, 2.0, 0.0), Vec3::new(0.5, 4.0, 10.0)),
            (Vec3::new(5.0, 2.0, 0.0), Vec3::new(0.5, 4.0, 10.0)),
            (Vec3::new(0.0, 2.0, -5.0), Vec3::new(10.0, 4.0, 0.5)),
            (Vec3::new(-3.5, 2.0, 5.0), Vec3::new(3.0, 4.0, 0.5)),
            (Vec3::new(3.5, 2.0, 5.0), Vec3::new(3.0, 4.0, 0.5)),
        ] {
            workspace.add_child(map_part("BuildingWall", center + offset, size, color));
        }
        workspace.add_child(map_part(
            "Roof",
            center + Vec3::Y * 4.25,
            Vec3::new(11.0, 0.5, 11.0),
            wood,
        ));
    }

    workspace.add_child(map_part(
        "Lookout",
        Vec3::new(15.0, 2.75, 3.0),
        Vec3::new(10.0, 0.5, 8.0),
        wood,
    ));
    let rise = 3.0_f32;
    let run = 12.0_f32;
    // Overlap both landings so the upright player collider clears their vertical faces.
    workspace.add_child(
        map_part(
            "Ramp",
            Vec3::new(15.0, 1.4, 13.0),
            Vec3::new(5.0, 0.3, rise.hypot(run) + 2.0),
            stone,
        )
        .with_orientation(Vec3::new((rise / run).atan().to_degrees(), 0.0, 0.0)),
    );
    for (position, size) in [
        (Vec3::new(10.2, 3.6, 3.0), Vec3::new(0.4, 1.2, 8.0)),
        (Vec3::new(19.8, 3.6, 3.0), Vec3::new(0.4, 1.2, 8.0)),
        (Vec3::new(15.0, 3.6, -0.8), Vec3::new(10.0, 1.2, 0.4)),
    ] {
        workspace.add_child(map_part("LookoutRail", position, size, wood));
    }

    for position in [
        Vec3::new(-6.0, 0.75, 8.0),
        Vec3::new(6.0, 0.75, -5.0),
        Vec3::new(-5.0, 0.75, -23.0),
    ] {
        workspace.add_child(map_part("Crate", position, Vec3::splat(1.5), wood));
    }
    for (x, z) in [
        (-24.0, -24.0),
        (24.0, -24.0),
        (-24.0, 0.0),
        (25.0, 12.0),
        (-20.0, 20.0),
        (-12.0, 15.0),
        (23.0, 25.0),
    ] {
        workspace.add_child(
            map_part(
                "TreeTrunk",
                Vec3::new(x, 1.5, z),
                Vec3::new(0.8, 3.0, 0.8),
                wood,
            )
            .with_shape(PartShape::Cylinder),
        );
        workspace.add_child(
            map_part(
                "TreeCanopy",
                Vec3::new(x, 4.0, z),
                Vec3::splat(4.0),
                Color3::new(0.18, 0.37, 0.23),
            )
            .with_shape(PartShape::Ball)
            .with_can_collide(false),
        );
    }
}

fn create_scene(workspace: &mut Workspace) -> PlayerController {
    add_map(workspace);
    workspace.lighting.with_clock_time(15.0);
    let player = PlayerController::spawn(workspace);
    let rig = player.rig.unwrap();
    workspace.get_mut::<Rig>(rig).unwrap().with_position(SPAWN);
    *workspace.camera_mut() = Camera::new(
        SPAWN + Vec3::new(0.0, 4.0, 7.0),
        SPAWN + Vec3::Y,
        16.0 / 9.0,
    )
    .with_subject(Some(rig))
    .with_zfar(150.0);
    player
}

fn initialize(engine: &mut Engine) -> Result<PlayerApp, AppCreationError> {
    let player = create_scene(engine.workspace_mut());
    let rig = player.rig.unwrap();
    let _ = engine
        .workspace_mut()
        .camera_mut()
        .with_subject_offset(Vec3::Y)
        .with_subject_distance(7.6);
    engine.set_controller(Box::new(player));
    Ok(PlayerApp { rig })
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[test]
fn player_example_loads_headlessly() -> Result<(), AppCreationError> {
    let engine = Terrarium::new()
        .with_size([64, 64])
        .with_headless_frames(Some(Some(2)))
        .run(initialize)?
        .expect("native headless runs return the engine");
    let player = engine
        .get_all::<Rig>()
        .find(|rig| rig.name() == "Player")
        .unwrap();
    assert!(player.animation_clips().contains_key("Idle_Loop"));
    assert_eq!(engine.camera().subject(), Some(player.id()));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key_event(key: egui::Key, pressed: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn input_frame(
        context: &egui::Context,
        _workspace: &mut Workspace,
        controller: &mut dyn ControllerView,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            events,
            ..Default::default()
        };
        let mut output = context.run_ui(input, |ui| {
            controller.process_input(ui.ctx());
        });
        output.textures_delta.clear();
    }

    fn walk_forward(workspace: &mut Workspace, controller: &mut dyn ControllerView) {
        input_frame(
            &egui::Context::default(),
            workspace,
            controller,
            vec![key_event(egui::Key::W, true)],
        );
    }

    fn advance(workspace: &mut Workspace, controller: &mut dyn ControllerView, frames: usize) {
        for _ in 0..frames {
            step(workspace, controller, 1.0 / 60.0);
        }
    }

    fn step(workspace: &mut Workspace, controller: &mut dyn ControllerView, delta_seconds: f32) {
        workspace.step_tweens(delta_seconds);
        workspace.step_animations(delta_seconds);
        controller.step(workspace, delta_seconds);
        #[cfg(feature = "physics")]
        workspace.step_physics(delta_seconds);
        controller.post_physics(workspace, delta_seconds.clamp(0.0, 0.1));
        #[cfg(feature = "sound")]
        let _ = workspace.step_audio(delta_seconds);
    }

    fn process_device_event(
        _workspace: &mut Workspace,
        controller: &mut dyn ControllerView,
        event: &terrarium::winit::event::DeviceEvent,
    ) {
        controller.process_device_event(event);
    }

    fn create_test_scene(workspace: &mut Workspace) -> (InstanceId, PlayerController) {
        let mut controller = create_scene(workspace);
        let rig = controller.rig.unwrap();
        workspace.camera_mut().set_subject_offset(Vec3::Y);
        workspace.camera_mut().set_subject_distance(7.6);
        controller.post_physics(workspace, 0.0);
        (rig, controller)
    }

    #[test]
    fn walking_and_turning_preserve_gravity_and_the_player_lands_upright() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        walk_forward(&mut workspace, &mut controller);
        for _ in 0..30 {
            process_device_event(
                &mut workspace,
                &mut controller,
                &terrarium::winit::event::DeviceEvent::MouseMotion { delta: (2.0, 0.0) },
            );
            step(&mut workspace, &mut controller, 1.0 / 60.0);
        }
        let falling = workspace.get::<Rig>(rig).unwrap();
        assert!(falling.position().y < SPAWN.y - 0.8);
        assert!(falling.position().y > 0.5);
        assert!(falling.position().z < SPAWN.z - controller.speed * 30.0 / 60.0 * 0.8);
        assert!(falling.forward().x > 0.1);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Fall);

        advance(&mut workspace, &mut controller, 150);
        let landed = workspace.get::<Rig>(rig).unwrap();
        assert!(landed.position().y.abs() < 0.05, "{:?}", landed.position());
        assert!(landed.position().z < SPAWN.z - controller.speed * 180.0 / 60.0 * 0.8);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Walk);
        assert!(
            landed
                .pose()
                .transform_vector3(Vec3::Y)
                .abs_diff_eq(Vec3::Y, 1e-5)
        );
    }

    #[test]
    fn player_animations_transition_between_idle_walk_sprint_jump_and_fall() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Idle);
        let mesh_id = workspace.get_all::<MeshPart>().next().unwrap().id();
        let idle = workspace.get::<MeshPart>(mesh_id).unwrap().mesh().clone();
        walk_forward(&mut workspace, &mut controller);
        advance(&mut workspace, &mut controller, 20);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Walk);
        let walk = workspace.get::<MeshPart>(mesh_id).unwrap().mesh();
        assert!(
            idle.vertices
                .iter()
                .zip(&walk.vertices)
                .any(|(idle, walk)| {
                    !Vec3::from_array(idle.position)
                        .abs_diff_eq(Vec3::from_array(walk.position), 0.001)
                })
        );
        let before = workspace
            .get::<Rig>(rig)
            .unwrap()
            .animation()
            .unwrap()
            .time();
        step(&mut workspace, &mut controller, 1.0 / 60.0);
        assert!(
            workspace
                .get::<Rig>(rig)
                .unwrap()
                .animation()
                .unwrap()
                .time()
                > before
        );

        controller.process_key(winit::keyboard::KeyCode::ShiftLeft, true);
        advance(&mut workspace, &mut controller, 15);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Sprint);
        controller.process_key(winit::keyboard::KeyCode::KeyW, false);
        controller.process_key(winit::keyboard::KeyCode::ShiftLeft, false);
        advance(&mut workspace, &mut controller, 15);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Idle);

        controller.process_key(winit::keyboard::KeyCode::Space, true);
        advance(&mut workspace, &mut controller, 3);
        controller.process_key(winit::keyboard::KeyCode::Space, false);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Jump);
        for _ in 0..120 {
            if controller.animation_state() == PlayerAnimationState::Fall {
                break;
            }
            advance(&mut workspace, &mut controller, 1);
        }
        assert_eq!(controller.animation_state(), PlayerAnimationState::Fall);
        advance(&mut workspace, &mut controller, 80);
        assert_eq!(controller.animation_state(), PlayerAnimationState::Idle);
        assert!(workspace.physics().is_grounded(rig));
    }

    #[test]
    fn map_walls_stop_the_walking_player() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        walk_forward(&mut workspace, &mut controller);
        let frames = ((SPAWN.z + 32.0) / controller.speed * 60.0 * 1.5).ceil() as usize;
        advance(&mut workspace, &mut controller, frames);
        let position = workspace.get::<Rig>(rig).unwrap().position();
        assert!(position.z > -31.2 && position.z < -30.8, "{position:?}");
        assert!(position.y.abs() < 0.05, "{position:?}");
    }

    #[test]
    fn the_lookout_is_reachable_by_walking_up_the_ramp() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        workspace
            .get_mut::<Rig>(rig)
            .unwrap()
            .with_position(Vec3::new(15.0, 0.05, 22.0));
        walk_forward(&mut workspace, &mut controller);
        let frames = (18.0 / controller.speed * 60.0 * 1.5).ceil() as usize;
        advance(&mut workspace, &mut controller, frames);
        let position = workspace.get::<Rig>(rig).unwrap().position();
        assert!(position.y > 2.9 && position.y < 3.1, "{position:?}");
        assert!(position.z < 6.5, "{position:?}");
    }

    #[test]
    fn strafing_turns_toward_travel_and_releasing_movement_stops_mid_turn() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        let initial = workspace.get::<Rig>(rig).unwrap().position();
        let context = egui::Context::default();
        input_frame(
            &context,
            &mut workspace,
            &mut controller,
            vec![key_event(egui::Key::D, true)],
        );
        advance(&mut workspace, &mut controller, 2);
        let player = workspace.get::<Rig>(rig).unwrap();
        assert!(player.position().x > initial.x + controller.speed * 2.0 / 60.0 * 0.8);
        let partial_facing = player.forward();
        assert!(
            partial_facing.x > 0.0 && partial_facing.x < 0.9,
            "{partial_facing:?}"
        );
        assert!(partial_facing.z < -0.1, "{partial_facing:?}");

        input_frame(
            &context,
            &mut workspace,
            &mut controller,
            vec![key_event(egui::Key::D, false)],
        );
        let stopped_at = workspace.get::<Rig>(rig).unwrap().position();
        advance(&mut workspace, &mut controller, 60);
        let stopped = workspace.get::<Rig>(rig).unwrap();
        assert!(stopped.forward().abs_diff_eq(partial_facing, 1e-5));
        assert!((stopped.position() - stopped_at).with_y(0.0).length() < 1e-4);

        input_frame(
            &context,
            &mut workspace,
            &mut controller,
            vec![key_event(egui::Key::D, true)],
        );
        advance(&mut workspace, &mut controller, 60);
        assert!(
            workspace
                .get::<Rig>(rig)
                .unwrap()
                .forward()
                .abs_diff_eq(Vec3::X, 1e-3)
        );
    }

    #[test]
    fn mouse_look_orbits_without_turning_or_moving_an_idle_player() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        let position = workspace.get::<Rig>(rig).unwrap().position();
        let facing = workspace.get::<Rig>(rig).unwrap().forward();
        let focus = position + Vec3::Y;
        let offset = workspace.camera().position() - focus;
        process_device_event(
            &mut workspace,
            &mut controller,
            &terrarium::winit::event::DeviceEvent::MouseMotion {
                delta: (80.0, -20.0),
            },
        );
        step(&mut workspace, &mut controller, 1.0 / 60.0);

        let player = workspace.get::<Rig>(rig).unwrap();
        assert!(player.forward().abs_diff_eq(facing, 1e-5));
        assert!((player.position() - position).with_y(0.0).length() < 1e-4);
        let focus = player.position() + Vec3::Y;
        let orbit = workspace.camera().position() - focus;
        assert!(!orbit.abs_diff_eq(offset, 1e-3));
        assert!((orbit.length() - offset.length()).abs() < 1e-4);
        assert!(workspace.camera().forward().dot(-orbit.normalize()) > 0.9999);
    }

    #[test]
    fn movement_follows_the_camera_even_while_the_rig_is_still_turning() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        let initial = workspace.get::<Rig>(rig).unwrap().position();
        walk_forward(&mut workspace, &mut controller);
        let sensitivity = controller.camera_controller.sensitivity;
        process_device_event(
            &mut workspace,
            &mut controller,
            &terrarium::winit::event::DeviceEvent::MouseMotion {
                delta: ((std::f32::consts::FRAC_PI_2 / sensitivity) as f64, 0.0),
            },
        );
        step(&mut workspace, &mut controller, 1.0 / 60.0);

        let player = workspace.get::<Rig>(rig).unwrap();
        let displacement = player.position() - initial;
        assert!(
            displacement.x > controller.speed / 60.0 * 0.8,
            "{displacement:?}"
        );
        assert!(displacement.z.abs() < 1e-4, "{displacement:?}");
        let facing = player.forward();
        assert!(facing.x > 0.0 && facing.x < 0.9, "{facing:?}");
        assert!(facing.z < -0.1, "{facing:?}");
        assert!(workspace.camera().forward().x > 0.9);
    }

    #[test]
    fn touch_look_orbits_the_camera_once_without_turning_the_idle_player() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        let facing = workspace.get::<Rig>(rig).unwrap().forward();
        let focus = workspace.get::<Rig>(rig).unwrap().position() + Vec3::Y;
        let offset = workspace.camera().position() - focus;
        let context = egui::Context::default();
        for (phase, x) in [
            (egui::TouchPhase::Start, 400.0),
            (egui::TouchPhase::Move, 460.0),
        ] {
            input_frame(
                &context,
                &mut workspace,
                &mut controller,
                vec![egui::Event::Touch {
                    device_id: egui::TouchDeviceId(0),
                    id: egui::TouchId(1),
                    phase,
                    pos: egui::pos2(x, 250.0),
                    force: None,
                }],
            );
        }
        step(&mut workspace, &mut controller, 1.0 / 60.0);
        let player = workspace.get::<Rig>(rig).unwrap();
        let orbit = workspace.camera().position() - (player.position() + Vec3::Y);
        let angle = -60.0 * controller.camera_controller.sensitivity;
        let expected = terrarium::glam::Quat::from_rotation_y(angle) * offset;
        assert!(orbit.abs_diff_eq(expected, 1e-4), "{orbit:?}");
        assert!(player.forward().abs_diff_eq(facing, 1e-5));
    }

    #[test]
    fn turning_takes_the_short_path_across_the_yaw_wrap() {
        let mut workspace = Workspace::new();
        let (rig, mut controller) = create_test_scene(&mut workspace);
        advance(&mut workspace, &mut controller, 120);
        workspace
            .get_mut::<Rig>(rig)
            .unwrap()
            .with_orientation(Vec3::new(0.0, 170.0, 0.0));
        step(&mut workspace, &mut controller, 0.0);
        let initial_facing = workspace.get::<Rig>(rig).unwrap().forward();
        walk_forward(&mut workspace, &mut controller);
        let sensitivity = controller.camera_controller.sensitivity;
        process_device_event(
            &mut workspace,
            &mut controller,
            &terrarium::winit::event::DeviceEvent::MouseMotion {
                delta: ((170.0_f32.to_radians() / sensitivity) as f64, 0.0),
            },
        );
        step(&mut workspace, &mut controller, 1.0 / 60.0);
        let facing = workspace.get::<Rig>(rig).unwrap().forward();
        assert!(facing.z > 0.98, "{facing:?}");
        assert!(facing.x > initial_facing.x && facing.x < 0.0, "{facing:?}");
    }
}
