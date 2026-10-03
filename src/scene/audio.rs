use std::{fmt, rc::Rc};

use euphorium::{
    HrtfProfile, ListenerState, Output, Soundscape, SoundscapeError, SpatialAudioSettings,
    SpatialRenderer,
};
use glam::Vec3;
use thiserror::Error;

#[cfg(feature = "meshpart")]
use crate::MeshPart;
use crate::{
    BasePart, Camera, HasPVInstance, HasSound, Instance, InstanceId, Part, Sound, Workspace,
};

/// A failure discovered while updating the workspace's audio scene.
#[derive(Debug, Error)]
pub enum AudioError {
    /// The active camera could not be represented as a valid audio listener.
    #[error("audio listener: {0}")]
    Listener(#[source] SoundscapeError),
    /// A scene sound failed to configure, load, or execute a playback request.
    #[error("sound {instance:?}: {source}")]
    Sound {
        /// The instance whose audio operation failed.
        instance: InstanceId,
        /// The underlying Euphorium failure.
        #[source]
        source: SoundscapeError,
    },
    /// A sound created directly through [`Workspace::soundscape`] failed asynchronously.
    #[error("unmanaged sound {sound:?}: {source}")]
    UnmanagedSound {
        /// The backend identity of the sound without a Terrarium instance owner.
        sound: euphorium::SoundId,
        /// The underlying Euphorium failure.
        #[source]
        source: SoundscapeError,
    },
}

pub(crate) struct AudioRuntime {
    pub(crate) soundscape: Rc<Soundscape>,
    previous_listener_position: Option<Vec3>,
    instances: Vec<InstanceId>,
    generation: Option<u64>,
}

impl Default for AudioRuntime {
    fn default() -> Self {
        let soundscape = Soundscape::new_with_output(Output::new_deferred(None));
        soundscape
            .set_spatial_audio(SpatialAudioSettings {
                renderer: SpatialRenderer::Hrtf(HrtfProfile::builtin()),
                ..SpatialAudioSettings::default()
            })
            .expect("built-in spatial audio settings are valid");
        Self {
            soundscape: Rc::new(soundscape),
            previous_listener_position: None,
            instances: Vec::new(),
            generation: None,
        }
    }
}

impl fmt::Debug for AudioRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioRuntime")
            .field("listener", &self.soundscape.listener())
            .finish_non_exhaustive()
    }
}

impl AudioRuntime {
    pub(crate) fn clone_configuration(&self) -> Self {
        let soundscape =
            Soundscape::new_with_output(Output::new_deferred(self.soundscape.preferred_backend()));
        soundscape
            .set_spatial_audio(self.soundscape.spatial_audio())
            .expect("existing spatial audio settings are valid");
        soundscape
            .set_volume(self.soundscape.volume())
            .expect("existing master volume is valid");
        soundscape.set_muted(self.soundscape.is_muted());
        soundscape
            .set_effects(self.soundscape.effects())
            .expect("existing master effects are valid");
        Self {
            soundscape: Rc::new(soundscape),
            previous_listener_position: None,
            instances: Vec::new(),
            generation: None,
        }
    }

    pub(crate) fn update_listener(
        &mut self,
        camera: &Camera,
        delta_seconds: f32,
    ) -> Result<(), SoundscapeError> {
        let position = camera.position();
        self.soundscape.set_listener(ListenerState {
            position: position.to_array(),
            velocity: velocity(position, self.previous_listener_position, delta_seconds).to_array(),
            forward: camera.forward().to_array(),
            up: camera.pivot().transform_vector3(Vec3::Y).to_array(),
        })?;
        self.previous_listener_position = Some(position);
        Ok(())
    }
}

pub(crate) fn velocity(position: Vec3, previous: Option<Vec3>, delta_seconds: f32) -> Vec3 {
    if let Some(previous) = previous
        && delta_seconds.is_finite()
        && delta_seconds > 0.0
    {
        let velocity = (position - previous) / delta_seconds;
        if velocity.is_finite() {
            return velocity;
        }
    }
    Vec3::ZERO
}

fn emitter(workspace: &Workspace, sound: &Sound) -> Option<(InstanceId, Vec3)> {
    let mut parent = sound.parent();
    while let Some(id) = parent {
        let instance = workspace.instance(id)?;
        let pivot = instance
            .downcast_ref::<Part>()
            .map(HasPVInstance::pivot)
            .or_else(|| {
                instance
                    .downcast_ref::<BasePart>()
                    .map(HasPVInstance::pivot)
            });
        #[cfg(feature = "meshpart")]
        let pivot = pivot.or_else(|| {
            instance
                .downcast_ref::<MeshPart>()
                .map(HasPVInstance::pivot)
        });
        if let Some(pivot) = pivot {
            return Some((id, pivot.transform_point3(sound.emitter_offset())));
        }
        parent = instance.parent();
    }
    None
}

impl Workspace {
    /// Returns the workspace-owned Euphorium scene for master volume, spatial
    /// renderer configuration, playback events, and output-device controls.
    ///
    /// Defaults to the built-in HRTF renderer. The listener is owned by the
    /// active camera and refreshed every audio update. Output initialization is
    /// deferred until playback needs it; on the web, call `ensure_output()`
    /// from a browser user gesture when explicit audio unlocking is needed.
    /// Manually created sounds and mixer groups are not copied by scene cloning.
    pub fn soundscape(&self) -> &Soundscape {
        &self.audio.soundscape
    }

    /// Synchronizes sound instances and camera/part motion, applies pending
    /// playback requests, and advances asynchronous audio loading.
    ///
    /// Returns both synchronous and asynchronous sound errors. New motion
    /// samples use zero velocity; non-positive or non-finite deltas also produce
    /// zero velocity. Call after moving the camera and scene objects.
    /// [`Workspace::update`] calls this automatically and logs returned errors.
    pub fn update_audio(&mut self, delta_seconds: f32) -> Vec<AudioError> {
        let mut errors = Vec::new();
        if let Err(error) = self
            .audio
            .update_listener(&self.current_camera, delta_seconds)
        {
            errors.push(AudioError::Listener(error));
        }
        let generation = self.lookup.generation();
        if self.audio.generation != Some(generation) {
            self.audio.instances = self.get_all::<Sound>().map(Instance::id).collect();
            self.audio.generation = Some(generation);
        }
        let emitters: Vec<_> = self
            .audio
            .instances
            .iter()
            .filter_map(|&id| Some((id, emitter(self, self.get::<Sound>(id)?))))
            .collect();
        // Clone only the root owner, never a scene or a voice. This allows typed
        // mutable scene access without moving or recreating the audio runtime.
        let soundscape = Rc::clone(&self.audio.soundscape);
        for (id, emitter) in emitters {
            if let Some(sound) = self.get_mut::<Sound>(id)
                && let Err(source) = sound.sync_audio(&soundscape, emitter, delta_seconds)
            {
                errors.push(AudioError::Sound {
                    instance: id,
                    source,
                });
            }
        }
        for failure in soundscape.update() {
            if let Some(sound) = self.get_all::<Sound>().find(|sound| {
                sound
                    .playback()
                    .is_some_and(|playback| playback.id() == failure.sound)
            }) {
                errors.push(AudioError::Sound {
                    instance: sound.id(),
                    source: failure.error,
                });
            } else {
                errors.push(AudioError::UnmanagedSound {
                    sound: failure.sound,
                    source: failure.error,
                });
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use std::{f32::consts::FRAC_PI_2, time::Duration};

    use approx::assert_abs_diff_eq;
    use euphorium::{DistanceAttenuation, Occlusion, PlaybackState, SoundSource, SpatialSound};

    use super::*;
    #[cfg(feature = "physics")]
    use crate::HasBasePart;

    fn assert_vector(actual: [f32; 3], expected: Vec3) {
        for (actual, expected) in actual.into_iter().zip(expected.to_array()) {
            assert_abs_diff_eq!(actual, expected, epsilon = 1e-5);
        }
    }

    fn silent_wav() -> SoundSource {
        let sample_rate = 8_000_u32;
        let data_size = sample_rate * 4 * 2;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_size).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16_u32.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&1_u16.to_le_bytes());
        bytes.extend_from_slice(&sample_rate.to_le_bytes());
        bytes.extend_from_slice(&(sample_rate * 2).to_le_bytes());
        bytes.extend_from_slice(&2_u16.to_le_bytes());
        bytes.extend_from_slice(&16_u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_size.to_le_bytes());
        bytes.resize(bytes.len() + data_size as usize, 0);
        SoundSource::from(bytes)
    }

    #[test]
    fn camera_motion_and_orientation_drive_the_listener() {
        let mut workspace = Workspace::new();
        (&mut workspace.current_camera).with_position(Vec3::ZERO);
        assert!(workspace.update_audio(0.5).is_empty());
        assert_vector(workspace.soundscape().listener().velocity, Vec3::ZERO);

        (&mut workspace.current_camera)
            .with_position(Vec3::new(2.0, 1.0, 0.0))
            .with_orientation(Vec3::new(0.0, 90.0, 0.0));
        assert!(workspace.update_audio(0.5).is_empty());
        let listener = workspace.soundscape().listener();
        assert_vector(listener.position, Vec3::new(2.0, 1.0, 0.0));
        assert_vector(listener.velocity, Vec3::new(4.0, 2.0, 0.0));
        assert_vector(listener.forward, Vec3::NEG_X);
        assert_vector(listener.up, Vec3::Y);
    }

    #[test]
    fn sound_follows_the_nearest_part_pivot_and_publishes_velocity() {
        let mut workspace = Workspace::new();
        let outer = workspace.add_child_ref(Part::new().with_position(Vec3::splat(100.0)));
        let inner = outer.add_child_ref(BasePart::new().with_pivot(
            glam::Mat4::from_rotation_translation(
                glam::Quat::from_rotation_y(FRAC_PI_2),
                Vec3::new(3.0, 2.0, 1.0),
            ),
        ));
        let parent_id = inner.id();
        let sound_id = inner.add_child(
            Sound::new()
                .with_emitter_offset(Vec3::X)
                .with_spatial_settings(SpatialSound {
                    attenuation: DistanceAttenuation::None,
                    doppler: true,
                    ..SpatialSound::default()
                }),
        );
        assert!(workspace.update_audio(0.25).is_empty());
        let playback = workspace
            .get::<Sound>(sound_id)
            .unwrap()
            .playback()
            .unwrap()
            .clone();
        let spatial = playback.spatial().unwrap().unwrap();
        assert_vector(spatial.emitter.position, Vec3::new(3.0, 2.0, 0.0));
        assert_vector(spatial.emitter.velocity, Vec3::ZERO);
        assert_eq!(spatial.attenuation, DistanceAttenuation::None);
        assert!(spatial.doppler);

        workspace
            .get_mut::<BasePart>(parent_id)
            .unwrap()
            .with_position(Vec3::new(4.0, 2.0, 1.0));
        assert!(workspace.update_audio(0.25).is_empty());
        let spatial = playback.spatial().unwrap().unwrap();
        assert_vector(spatial.emitter.position, Vec3::new(4.0, 2.0, 0.0));
        assert_vector(spatial.emitter.velocity, Vec3::new(4.0, 0.0, 0.0));

        let occlusion = Occlusion {
            gain: 0.3,
            low_pass_hz: Some(900.0),
        };
        let sound = workspace.get_mut::<Sound>(sound_id).unwrap();
        let settings = sound.spatial_settings();
        sound.with_spatial_settings(SpatialSound {
            occlusion,
            ..settings
        });
        assert!(workspace.update_audio(0.25).is_empty());
        let occlusion = playback.spatial().unwrap().unwrap().occlusion;
        assert_abs_diff_eq!(occlusion.gain, 0.3, epsilon = 1e-6);
        assert_abs_diff_eq!(occlusion.low_pass_hz.unwrap(), 900.0, epsilon = 1e-6);
    }

    #[test]
    fn global_and_opted_out_sounds_remain_non_spatial() {
        let mut workspace = Workspace::new();
        let global_id = workspace.add_child(Sound::new().with_name("music/ambient"));
        let part = workspace.add_child_ref(Part::new());
        let disabled_id = part.add_child(
            Sound::new()
                .with_name("music/ambient")
                .with_spatial_enabled(false),
        );
        assert!(workspace.update_audio(0.0).is_empty());
        for id in [global_id, disabled_id] {
            assert!(
                workspace
                    .get::<Sound>(id)
                    .unwrap()
                    .playback()
                    .unwrap()
                    .spatial()
                    .unwrap()
                    .is_none()
            );
        }
        workspace
            .get_mut::<Sound>(disabled_id)
            .unwrap()
            .with_spatial_enabled(true);
        assert!(workspace.update_audio(0.0).is_empty());
        assert!(
            workspace
                .get::<Sound>(disabled_id)
                .unwrap()
                .playback()
                .unwrap()
                .spatial()
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn invalid_frame_deltas_do_not_publish_invalid_doppler_velocities() {
        let mut workspace = Workspace::new();
        let part = workspace.add_child_ref(Part::new());
        let parent_id = part.id();
        let sound_id = part.add_child(Sound::new());
        assert!(workspace.update_audio(0.25).is_empty());
        for delta in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            (&mut workspace.current_camera).with_position(Vec3::splat(10.0));
            workspace
                .get_mut::<Part>(parent_id)
                .unwrap()
                .with_position(Vec3::splat(20.0));
            assert!(workspace.update_audio(delta).is_empty());
            assert_vector(workspace.soundscape().listener().velocity, Vec3::ZERO);
            let spatial = workspace
                .get::<Sound>(sound_id)
                .unwrap()
                .playback()
                .unwrap()
                .spatial()
                .unwrap()
                .unwrap();
            assert_vector(spatial.emitter.velocity, Vec3::ZERO);
        }
    }

    #[test]
    fn seek_and_stop_requests_apply_in_order_without_starting_output() {
        let mut workspace = Workspace::new();
        let mut sound = Sound::new();
        sound.seek(Duration::from_secs(3));
        sound.stop();
        sound.seek(Duration::from_millis(250));
        let id = workspace.add_child(sound);
        assert!(workspace.update_audio(0.1).is_empty());
        let sound = workspace.get::<Sound>(id).unwrap();
        assert_eq!(sound.time_position().unwrap(), Duration::from_millis(250));
        assert_eq!(sound.playback_state().unwrap(), PlaybackState::Idle);
        assert!(!workspace.soundscape().has_output());

        workspace.get_mut::<Sound>(id).unwrap().stop();
        assert!(workspace.update_audio(0.1).is_empty());
        assert_eq!(
            workspace.get::<Sound>(id).unwrap().time_position().unwrap(),
            Duration::ZERO
        );
    }

    #[test]
    fn playback_failure_identifies_the_sound_and_is_not_retried_automatically() {
        let mut workspace = Workspace::new();
        let mut sound = Sound::new();
        sound.play();
        let id = workspace.add_child(sound);
        let errors = workspace.update_audio(0.1);
        assert!(
            matches!(errors.as_slice(), [AudioError::Sound { instance, source: SoundscapeError::NoAudioSource }] if *instance == id)
        );
        assert!(workspace.update_audio(0.1).is_empty());
    }

    #[test]
    fn queued_playback_can_pause_resume_replay_and_stop() {
        let mut workspace = Workspace::new();
        workspace.soundscape().set_muted(true);
        let mut sound = Sound::new().with_source(silent_wav()).with_looping(true);
        sound.seek(Duration::from_secs(2));
        sound.play();
        sound.pause();
        let id = workspace.add_child_ref(Part::new()).add_child(sound);
        assert!(workspace.update_audio(0.1).is_empty());
        let sound = workspace.get::<Sound>(id).unwrap();
        assert_eq!(sound.playback_state().unwrap(), PlaybackState::Paused);
        assert_eq!(sound.time_position().unwrap(), Duration::from_secs(2));
        assert_eq!(sound.duration().unwrap(), Some(Duration::from_secs(4)));

        workspace.get_mut::<Sound>(id).unwrap().resume();
        assert!(workspace.update_audio(0.1).is_empty());
        assert_eq!(
            workspace
                .get::<Sound>(id)
                .unwrap()
                .playback_state()
                .unwrap(),
            PlaybackState::Playing
        );

        let sound = workspace.get_mut::<Sound>(id).unwrap();
        sound.replay();
        sound.pause();
        assert!(workspace.update_audio(0.1).is_empty());
        let sound = workspace.get::<Sound>(id).unwrap();
        assert_eq!(sound.playback_state().unwrap(), PlaybackState::Paused);
        // A live native callback may consume a few samples before the pause.
        assert!(sound.time_position().unwrap() < Duration::from_secs(1));

        workspace.get_mut::<Sound>(id).unwrap().stop();
        assert!(workspace.update_audio(0.1).is_empty());
        let sound = workspace.get::<Sound>(id).unwrap();
        assert_eq!(sound.playback_state().unwrap(), PlaybackState::Idle);
        assert_eq!(sound.time_position().unwrap(), Duration::ZERO);
    }

    #[test]
    fn cloned_scenes_keep_audio_settings_but_have_independent_idle_voices() {
        let mut workspace = Workspace::new();
        workspace.soundscape().set_volume(0.75).unwrap();
        workspace.soundscape().set_muted(true);
        let id = workspace.add_child(
            Sound::new()
                .with_source(silent_wav())
                .with_volume(0.5)
                .with_playback_speed(1.25)
                .with_looping(true),
        );
        assert!(workspace.update_audio(0.1).is_empty());
        workspace
            .get_mut::<Sound>(id)
            .unwrap()
            .seek(Duration::from_secs(2));
        assert!(workspace.update_audio(0.1).is_empty());
        // This request belongs only to the original scene, not to its clone.
        workspace.get_mut::<Sound>(id).unwrap().play();

        let mut cloned = workspace.clone();
        assert!(cloned.update_audio(0.1).is_empty());
        let clone_id = cloned.get_all::<Sound>().next().unwrap().id();
        let clone = cloned.get::<Sound>(clone_id).unwrap();
        assert_eq!(clone.playback_state().unwrap(), PlaybackState::Idle);
        assert_eq!(clone.time_position().unwrap(), Duration::ZERO);
        assert_eq!(clone.source(), workspace.get::<Sound>(id).unwrap().source());
        assert!(clone.playback().unwrap().is_looping().unwrap());
        assert_abs_diff_eq!(
            clone.playback().unwrap().speed().unwrap(),
            1.25,
            epsilon = 1e-6
        );
        assert_abs_diff_eq!(cloned.soundscape().volume(), 0.75, epsilon = 1e-6);
        assert!(cloned.soundscape().is_muted());

        cloned.get_mut::<Sound>(clone_id).unwrap().with_volume(0.2);
        assert!(cloned.update_audio(0.1).is_empty());
        assert_abs_diff_eq!(
            cloned
                .get::<Sound>(clone_id)
                .unwrap()
                .playback()
                .unwrap()
                .local_volume()
                .unwrap(),
            0.2,
            epsilon = 1e-6
        );
        let original = workspace.get::<Sound>(id).unwrap();
        assert_abs_diff_eq!(
            original.playback().unwrap().local_volume().unwrap(),
            0.5,
            epsilon = 1e-6
        );
        assert_eq!(original.time_position().unwrap(), Duration::from_secs(2));
    }

    #[test]
    fn destroying_a_parent_immediately_invalidates_its_sound_handles() {
        let mut workspace = Workspace::new();
        let parent = workspace.add_child_ref(Part::new());
        let parent_id = parent.id();
        let id = parent.add_child(Sound::new());
        assert!(workspace.update_audio(0.1).is_empty());
        let playback = workspace
            .get::<Sound>(id)
            .unwrap()
            .playback()
            .unwrap()
            .clone();
        assert!(workspace.get_mut::<Part>(parent_id).unwrap().destroy());
        assert!(matches!(
            playback.playback_state(),
            Err(SoundscapeError::InvalidSoundHandle)
        ));
    }

    #[test]
    #[cfg(feature = "physics")]
    fn positional_audio_uses_the_post_physics_part_position() {
        let mut workspace = Workspace::new();
        let parent = workspace.add_child_ref(
            Part::new()
                .with_position(Vec3::new(0.0, 5.0, 0.0))
                .with_anchored(false),
        );
        let parent_id = parent.id();
        let id = parent.add_child(Sound::new());
        workspace.update(0.1);
        let position = workspace.get::<Part>(parent_id).unwrap().position();
        assert!(position.y < 5.0);
        let spatial = workspace
            .get::<Sound>(id)
            .unwrap()
            .playback()
            .unwrap()
            .spatial()
            .unwrap()
            .unwrap();
        assert_vector(spatial.emitter.position, position);
    }
}
