use std::f32::consts::TAU;

use terrarium::{
    euphorium::{DistanceAttenuation, Occlusion, SpatialSound},
    glam::Vec3,
    *,
};

const SAMPLE_RATE: u32 = 48_000;
const AUDIO_SECONDS: u32 = 2;

struct SoundApp {
    emitter: InstanceId,
    sound: InstanceId,
    angle: f32,
    radius: f32,
    orbit_speed: f32,
    moving: bool,
    volume: f32,
    occluded: bool,
    output_error: Option<String>,
}

impl SoundApp {
    fn unlock_output(&mut self, engine: &Engine) {
        // Opening browser audio requires a recent user gesture, not startup.
        self.output_error = engine
            .soundscape()
            .ensure_output()
            .err()
            .map(|error| error.to_string());
    }
}

impl App for SoundApp {
    fn logic(&mut self, engine: &mut Engine, context: &egui::Context, _frame: &mut eframe::Frame) {
        let delta = context.input(|input| input.stable_dt);
        let delta = if delta.is_finite() {
            delta.clamp(0.0, 0.1)
        } else {
            0.0
        };
        if self.moving {
            self.angle = (self.angle + delta * self.orbit_speed).rem_euclid(TAU);
        }
        // Move before Engine::update so audio receives this frame's transform
        // and derives the emitter velocity used for Doppler.
        engine
            .get_mut::<Part>(self.emitter)
            .unwrap()
            .with_position(Vec3::new(
                self.radius * self.angle.cos(),
                1.0,
                self.radius * self.angle.sin(),
            ));
        engine.update(context);
    }

    fn ui(&mut self, engine: &mut Engine, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sound_controls")
            .resizable(false)
            .default_size(280.0)
            .show(ui, |ui| {
                ui.heading("Spatial sound");
                ui.label("Click Play to enable audio. Headphones recommended.");
                ui.label("WASD + left-click drag moves the camera/listener.");
                ui.separator();

                ui.horizontal(|ui| {
                    if ui.button("Play / resume").clicked() {
                        self.unlock_output(engine);
                        engine.get_mut::<Sound>(self.sound).unwrap().play();
                    }
                    if ui.button("Pause").clicked() {
                        engine.get_mut::<Sound>(self.sound).unwrap().pause();
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("Replay").clicked() {
                        self.unlock_output(engine);
                        engine.get_mut::<Sound>(self.sound).unwrap().replay();
                    }
                    if ui.button("Stop").clicked() {
                        engine.get_mut::<Sound>(self.sound).unwrap().stop();
                    }
                });
                if ui.add(egui::Slider::new(&mut self.volume, 0.0..=1.0).text("Volume")).changed() {
                    engine.get_mut::<Sound>(self.sound).unwrap().with_volume(self.volume);
                }
                ui.separator();
                ui.checkbox(&mut self.moving, "Orbit the emitter");
                ui.add(egui::Slider::new(&mut self.radius, 1.0..=12.0).text("Radius (m)"));
                ui.add(egui::Slider::new(&mut self.orbit_speed, 0.0..=3.0).text("Speed (rad/s)"));
                if ui.checkbox(&mut self.occluded, "Simulate wall occlusion").changed() {
                    let sound = engine.get_mut::<Sound>(self.sound).unwrap();
                    let settings = sound.spatial_settings();
                    sound.with_spatial_settings(SpatialSound {
                        occlusion: if self.occluded {
                            Occlusion { gain: 0.25, low_pass_hz: Some(1_200.0) }
                        } else {
                            Occlusion::default()
                        },
                        ..settings
                    });
                }
                ui.label("Occlusion is supplied explicitly; no geometry raycasts are used.");
                ui.separator();

                let sound = engine.get::<Sound>(self.sound).unwrap();
                match sound.playback_state() {
                    Ok(state) => { ui.label(format!("Playback: {state:?}")); }
                    Err(error) => { ui.colored_label(egui::Color32::LIGHT_RED, error.to_string()); }
                }
                if let Ok(position) = sound.time_position() {
                    ui.label(format!("Loop position: {:.2} s", position.as_secs_f32()));
                }
                let position = engine.get::<Part>(self.emitter).unwrap().position();
                let distance = position.distance(engine.current_camera.position());
                ui.label(format!("Emitter: ({:.1}, {:.1}, {:.1}) m", position.x, position.y, position.z));
                ui.label(format!("Distance: {distance:.1} m"));
                ui.label("HRTF, distance attenuation, and Doppler are enabled.");
                if let Some(error) = &self.output_error {
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                    ui.label("Try Play again after enabling browser audio or connecting an output device.");
                }
            });
    }
}

fn initialize(engine: &mut Engine) -> Result<SoundApp, AppCreationError> {
    engine.current_camera = Camera::new(Vec3::new(0.0, 1.5, 9.0), Vec3::Y, 1.0);
    engine.add_child(
        Part::new()
            .with_name("Ground")
            .with_size(Vec3::new(32.0, 0.2, 32.0))
            .with_position(Vec3::new(0.0, -0.1, 0.0))
            .with_color(Color3::new(0.16, 0.2, 0.25)),
    );
    let emitter = engine.add_child_ref(
        Part::new()
            .with_name("Emitter")
            .with_shape(PartShape::Ball)
            .with_size(Vec3::splat(0.7))
            .with_position(Vec3::new(4.0, 1.0, 0.0))
            .with_color(Color3::new(1.0, 0.65, 0.12)),
    );
    let emitter_id = emitter.id();
    let sound = emitter.add_child(
        Sound::new()
            .with_name("Spatial loop")
            .with_source(demo_audio())
            .with_looping(true)
            .with_volume(0.5)
            .with_spatial_settings(SpatialSound {
                attenuation: DistanceAttenuation::Inverse {
                    reference_distance_m: 3.0,
                    max_distance_m: 30.0,
                    rolloff: 1.0,
                },
                doppler: true,
                ..SpatialSound::default()
            }),
    );
    // Leave playback idle until Play is clicked, including in a browser.
    Ok(SoundApp {
        emitter: emitter_id,
        sound,
        angle: 0.0,
        radius: 4.0,
        orbit_speed: 0.8,
        moving: true,
        volume: 0.5,
        occluded: false,
        output_error: None,
    })
}

fn demo_audio() -> Vec<u8> {
    let frames = SAMPLE_RATE * AUDIO_SECONDS;
    let data_size = frames * 2;
    let mut bytes = Vec::with_capacity(44 + data_size as usize);
    // A standard little-endian, mono, 16-bit PCM WAV header.
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16_u32.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data_size.to_le_bytes());

    let mut noise = 1_u32;
    for frame in 0..frames {
        let time = frame as f32 / SAMPLE_RATE as f32;
        let envelope = (0.5 - 0.5 * (TAU * 2.0 * time).cos()).powi(2);
        let tone = 0.6 * (TAU * 220.0 * time).sin()
            + 0.3 * (TAU * 440.0 * time).sin()
            + 0.1 * (TAU * 880.0 * time).sin();
        // Broadband content makes spatial filtering easier to hear than a
        // pure tone. The fixed seed keeps the generated clip deterministic.
        noise = noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let noise_sample = (noise >> 16) as f32 / 32_767.5 - 1.0;
        let sample = (0.75 * tone + 0.25 * noise_sample) * envelope * 0.3;
        bytes.extend_from_slice(&((sample * f32::from(i16::MAX)).round() as i16).to_le_bytes());
    }
    bytes
}

fn main() {
    Terrarium::new().run(initialize).unwrap();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use terrarium::euphorium::{self, rodio::Source};

    use super::*;

    #[test]
    fn generated_clip_decodes_as_non_silent_mono_audio() {
        let decoder = euphorium::decoder::from_shared_bytes(demo_audio().into()).unwrap();
        assert_eq!(decoder.channels().get(), 1);
        assert_eq!(decoder.sample_rate().get(), SAMPLE_RATE);
        assert_eq!(
            decoder.total_duration(),
            Some(Duration::from_secs(u64::from(AUDIO_SECONDS)))
        );
        let peak = decoder.fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > 0.05 && peak <= 0.31, "unexpected audio peak: {peak}");
    }
}
