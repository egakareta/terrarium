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
    fn logic(&mut self, engine: &mut Engine, input: &FrameInput, _frame: &mut Frame) {
        let delta = input.delta_secs();
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
        engine.update(input);
    }

    fn state(&self, engine: &Engine) -> serde_json::Value {
        let sound = engine.get::<Sound>(self.sound).unwrap();
        let position = engine.get::<Part>(self.emitter).unwrap().position();
        serde_json::json!({
            "moving": self.moving, "volume": self.volume, "radius": self.radius, "speed": self.orbit_speed, "occluded": self.occluded,
            "playback": sound.playback_state().map(|state| format!("{state:?}")).unwrap_or_else(|e| e.to_string()),
            "position": sound.time_position().ok().map(|v| v.as_secs_f32()), "emitter": position.to_array(),
            "distance": position.distance(engine.current_camera.position()), "error": self.output_error,
        })
    }
    fn command(
        &mut self,
        engine: &mut Engine,
        command: &str,
        payload: serde_json::Value,
    ) -> Result<serde_json::Value, AppCreationError> {
        match command {
            "play" => {
                self.unlock_output(engine);
                engine.get_mut::<Sound>(self.sound).unwrap().play();
            }
            "replay" => {
                self.unlock_output(engine);
                engine.get_mut::<Sound>(self.sound).unwrap().replay();
            }
            "pause" => {
                engine.get_mut::<Sound>(self.sound).unwrap().pause();
            }
            "stop" => {
                engine.get_mut::<Sound>(self.sound).unwrap().stop();
            }
            "volume" | "radius" | "speed" => {
                let bounds = match command {
                    "radius" => 1.0..=12.0,
                    "speed" => 0.0..=3.0,
                    _ => 0.0..=1.0,
                };
                let value = payload
                    .as_f64()
                    .filter(|v| bounds.contains(v))
                    .ok_or("invalid sound setting")? as f32;
                match command {
                    "radius" => self.radius = value,
                    "speed" => self.orbit_speed = value,
                    _ => {
                        self.volume = value;
                        engine
                            .get_mut::<Sound>(self.sound)
                            .unwrap()
                            .with_volume(value);
                    }
                }
            }
            "moving" => self.moving = payload.as_bool().ok_or("moving must be a boolean")?,
            "occluded" => {
                self.occluded = payload.as_bool().ok_or("occluded must be a boolean")?;
                let sound = engine.get_mut::<Sound>(self.sound).unwrap();
                sound.with_spatial_settings(SpatialSound {
                    occlusion: if self.occluded {
                        Occlusion {
                            gain: 0.25,
                            low_pass_hz: Some(1_200.0),
                        }
                    } else {
                        Occlusion::default()
                    },
                    ..sound.spatial_settings()
                });
            }
            _ => return Err("unknown command".into()),
        }
        Ok(serde_json::Value::Null)
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
    Terrarium::new().with_ui("sound").run(initialize).unwrap();
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
