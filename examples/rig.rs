use terrarium::{glam::*, *};

struct RigApp {
    rig: InstanceId,
}

impl App for RigApp {
    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("rig_controls").show(ui, |ui| {
            let rig = engine.get_mut::<Rig>(self.rig).unwrap();
            let mut x = rig.position().x;
            if ui
                .add(egui::Slider::new(&mut x, -3.0..=3.0).text("Rig position"))
                .changed()
            {
                rig.with_position(Vec3::new(x, 0.0, 0.0));
            }
            let player = rig.animation_mut().unwrap();
            ui.label(format!("Animation time: {:.2}s", player.time()));
            if ui
                .button(if player.is_paused() {
                    "Resume"
                } else {
                    "Pause"
                })
                .clicked()
            {
                if player.is_paused() {
                    player.resume();
                } else {
                    player.pause();
                }
            }
            if ui.button("Restart").clicked() {
                player.restart();
            }
        });
    }
}

fn arm_pose(side: f32, angle: f32) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::new(0.5, 1.5, 0.5),
        Quat::from_rotation_z(angle),
        Vec3::new(
            side * (0.8 + 0.75 * angle.sin().abs()),
            1.8 - 0.75 * angle.cos(),
            0.0,
        ),
    )
}

#[with_workspace]
fn initialize(engine: &mut Engine) -> Result<RigApp, AppCreationError> {
    let mut rig = Rig::new().with_name("Character");
    rig.add_child_local(
        Part::new()
            .with_name("Torso")
            .with_size(Vec3::new(1.0, 1.5, 0.6))
            .with_position(Vec3::Y)
            .with_color(Color3::new(0.2, 0.5, 0.9)),
    );
    rig.add_child_local(
        Part::new()
            .with_name("Head")
            .with_size(Vec3::splat(0.75))
            .with_position(Vec3::new(0.0, 2.2, 0.0))
            .with_color(Color3::new(0.95, 0.75, 0.5)),
    );
    let mut tracks = Vec::new();
    for (name, side) in [("LeftArm", -1.0), ("RightArm", 1.0)] {
        rig.add_child_local(
            Part::new()
                .with_name(name)
                .with_pivot(arm_pose(side, 0.0))
                .with_color(Color3::new(0.95, 0.75, 0.5)),
        );
        tracks.push(AnimationTrack::new(
            [name],
            [
                Keyframe::new(0.0, arm_pose(side, 0.0)).with_easing(Easing::EaseInOut),
                Keyframe::new(0.75, arm_pose(side, side * 1.4)).with_easing(Easing::EaseInOut),
                Keyframe::new(1.5, arm_pose(side, 0.0)),
            ],
        ));
    }
    rig.play_animation(AnimationPlayer::new(AnimationClip::new(tracks)).with_looping(true))?;
    let rig = workspace.add_child(rig);
    engine.current_camera = terrarium::Camera::new(Vec3::new(5.0, 3.0, 7.0), Vec3::Y, 16.0 / 9.0);
    Ok(RigApp { rig })
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[test]
fn rig_example_loads_headlessly() -> Result<(), AppCreationError> {
    let engine = Terrarium::new()
        .with_size([64, 64])
        .with_headless(Some(2))
        .run(initialize)?
        .expect("native headless runs return the engine");
    assert_eq!(engine.get_all::<Rig>().count(), 1);
    assert_eq!(engine.get_all::<Part>().count(), 4);
    Ok(())
}
