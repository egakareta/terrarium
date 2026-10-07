//! Local player lifecycle, rigid character movement, and subject-camera configuration.

use terrarium::{glam::*, *};

struct PlayerApp;

impl App for PlayerApp {
    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("player_controls").show(ui, |ui| {
            ui.heading("Local player");
            let character = engine.local_player().and_then(|player| player.character);
            if let Some(player) = engine.local_player_mut() {
                ui.label(format!("Player ID: {}", player.player_id));
                ui.add(
                    egui::Slider::new(&mut player.respawn_time, 0.0..=10.0).text("Respawn seconds"),
                );
                ui.checkbox(&mut player.auto_spawn, "Automatic spawning");
            }
            if let Some(id) = character {
                if ui.button("Destroy character").clicked()
                    && let Some(character) = engine.instance_mut(id)
                {
                    character.destroy();
                }
                if let Some(character) = engine.get_mut::<Humanoid>(id) {
                    if ui.button("Move +X").clicked() {
                        character.pivot_to(Mat4::from_translation(Vec3::X) * character.pose());
                    }
                    if ui.button("Turn 45 degrees").clicked() {
                        character.pivot_to(
                            character.pose() * Mat4::from_rotation_y(45.0_f32.to_radians()),
                        );
                    }
                }
            } else {
                ui.label("Character absent; waiting for respawn.");
                if ui.button("Spawn now").clicked() {
                    let player = engine.local_player().unwrap().id();
                    engine.spawn_character(player);
                }
            }
            ui.separator();
            let mut free = engine.current_camera.subject.is_none();
            if ui.checkbox(&mut free, "Free camera (WASD)").changed() {
                engine.current_camera.subject = if free { None } else { character };
            }
            ui.add(
                egui::Slider::new(&mut engine.current_camera.follow.distance, 2.0..=80.0)
                    .text("Camera distance"),
            );
            ui.label("Drag to orbit; scroll to zoom.");
        });
    }
}

fn main() {
    Terrarium::preset(WorkspacePreset::Baseplate)
        .with_local_player(Some(LocalPlayerConfig {
            respawn_time: 2.0,
            ..Default::default()
        }))
        .run(|_| Ok::<_, AppCreationError>(PlayerApp))
        .unwrap();
}
