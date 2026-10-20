use std::{collections::VecDeque, time::Duration};

use euphorium::{
    EmitterState, PlaybackState, SoundSource, Soundscape, SoundscapeError, SpatialSound,
};

use crate::{Instance, InstanceData, InstanceId, glam::Vec3, scene::audio::velocity};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
struct SoundSettings {
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    source: SoundSource,
    volume: f32,
    playback_speed: f32,
    looping: bool,
    spatial_enabled: bool,
    emitter_offset: Vec3,
    #[cfg_attr(feature = "devtools", inspect(readonly))]
    spatial: SpatialSound,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            source: SoundSource::empty(),
            volume: 1.0,
            playback_speed: 1.0,
            looping: false,
            spatial_enabled: true,
            emitter_offset: Vec3::ZERO,
            spatial: SpatialSound {
                doppler: true,
                ..SpatialSound::default()
            },
        }
    }
}

#[derive(Debug)]
enum PlaybackCommand {
    Play,
    Replay,
    Pause,
    Resume,
    Stop,
    Seek(Duration),
}

#[derive(Debug)]
struct SoundPlayback {
    sound: euphorium::Sound,
    applied: Option<SoundSettings>,
    parent: Option<InstanceId>,
    previous_position: Option<Vec3>,
}

impl Drop for SoundPlayback {
    fn drop(&mut self) {
        // The workspace may already have been dropped, invalidating this handle.
        let _ = self.sound.clone().remove();
    }
}

crate::instance! {
    #[instance(default = false, inspect = false)]
    /// A scene-tree sound backed by Euphorium.
    ///
    /// Sounds beneath a [`crate::Part`], [`crate::BasePart`], or `MeshPart` use their
    /// nearest part ancestor as a point emitter. Other sounds play globally without
    /// spatial processing. One world unit is one meter for attenuation and Doppler.
    ///
    /// Properties and playback requests are applied by [`crate::Workspace::update_audio`]
    /// (also called by [`crate::Workspace::update`]). Cloning copies the source and
    /// settings, but not playback or queued requests. Destroying a sound removes its
    /// backend voice immediately.
    #[derive(Debug)]
    #[cfg_attr(feature = "devtools", derive(crate::devtools::Inspect))]
    pub struct Sound {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        instance: InstanceData,
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        settings: SoundSettings,
        #[cfg_attr(feature = "devtools", inspect(skip))]
        commands: VecDeque<PlaybackCommand>,
        #[cfg_attr(feature = "devtools", inspect(skip))]
        playback: Option<SoundPlayback>,
    }
    impl Sound {
        /// Returns the underlying sound.
        pub fn sound(&self) -> &Sound {
            crate::Project::<Sound>::project(self)
        }

        /// Returns mutable access to the underlying sound.
        pub fn sound_mut(&mut self) -> &mut Sound {
            crate::Project::<Sound>::project_mut(self)
        }

        /// Encoded audio source descriptor. Replacing it preserves playback
        /// state and position when possible.
        pub fn source(&self) -> &SoundSource {
            &self.sound().settings.source
        }
        /// Sets `source`.
        pub fn set_source(&mut self, source: SoundSource) {
            self.sound_mut().settings.source = source;
        }
        /// Returns this value with `source` set.
        pub fn with_source(mut self, source: impl Into<SoundSource>) -> Self
        where
            Self: Sized,
        {
            self.set_source(source.into());
            self
        }

        /// Local volume before distance, occlusion, and master multipliers.
        /// Invalid values are reported by the next audio update.
        #[property]
        pub fn volume(&self) -> f32 {
            self.sound().settings.volume
        }
        /// Sets `volume`.
        pub fn set_volume(&mut self, volume: f32) {
            self.sound_mut().settings.volume = volume;
        }
        /// Returns this value with `volume` set.
        pub fn with_volume(mut self, volume: f32) -> Self
        where
            Self: Sized,
        {
            self.set_volume(volume);
            self
        }

        /// Authored playback speed before Doppler pitch shifting. Invalid
        /// values are reported by the next audio update.
        #[property]
        pub fn playback_speed(&self) -> f32 {
            self.sound().settings.playback_speed
        }
        /// Sets `playback_speed`.
        pub fn set_playback_speed(&mut self, speed: f32) {
            self.sound_mut().settings.playback_speed = speed;
        }
        /// Returns this value with `playback_speed` set.
        pub fn with_playback_speed(mut self, speed: f32) -> Self
        where
            Self: Sized,
        {
            self.set_playback_speed(speed);
            self
        }

        /// Whether the source repeats when it reaches the end.
        #[property]
        pub fn looping(&self) -> bool {
            self.sound().settings.looping
        }
        /// Sets `looping`.
        pub fn set_looping(&mut self, looping: bool) {
            self.sound_mut().settings.looping = looping;
        }
        /// Returns this value with `looping` set.
        pub fn with_looping(mut self, looping: bool) -> Self
        where
            Self: Sized,
        {
            self.set_looping(looping);
            self
        }

        /// Whether a part ancestor enables positional playback. Changes take
        /// effect on the next playback start or replay.
        #[property]
        pub fn spatial_enabled(&self) -> bool {
            self.sound().settings.spatial_enabled
        }
        /// Sets `spatial_enabled`.
        pub fn set_spatial_enabled(&mut self, enabled: bool) {
            self.sound_mut().settings.spatial_enabled = enabled;
        }
        /// Returns this value with `spatial_enabled` set.
        pub fn with_spatial_enabled(mut self, enabled: bool) -> Self
        where
            Self: Sized,
        {
            self.set_spatial_enabled(enabled);
            self
        }

        /// Emitter offset in the nearest part ancestor's pivot coordinates,
        /// without part scale. Active sounds follow changes on the next audio update.
        #[property]
        pub fn emitter_offset(&self) -> Vec3 {
            self.sound().settings.emitter_offset
        }
        /// Sets `emitter_offset`.
        pub fn set_emitter_offset(&mut self, offset: Vec3) {
            self.sound_mut().settings.emitter_offset = offset;
        }
        /// Returns this value with `emitter_offset` set.
        pub fn with_emitter_offset(mut self, offset: Vec3) -> Self
        where
            Self: Sized,
        {
            self.set_emitter_offset(offset);
            self
        }

        /// Attenuation, occlusion, Doppler, and input-channel settings. The
        /// authored emitter is ignored because scene transforms supply it.
        /// Occlusion changes apply on the next audio update; other settings
        /// take effect on playback start or replay. Invalid values are reported
        /// by the next audio update for positional sounds.
        pub fn spatial_settings(&self) -> SpatialSound {
            self.sound().settings.spatial
        }
        /// Sets `spatial_settings`.
        pub fn set_spatial_settings(&mut self, settings: SpatialSound) {
            let mut settings = settings;
            settings.emitter = EmitterState::default();
            self.sound_mut().settings.spatial = settings;
        }
        /// Returns this value with `spatial_settings` set.
        pub fn with_spatial_settings(mut self, settings: SpatialSound) -> Self
        where
            Self: Sized,
        {
            self.set_spatial_settings(settings);
            self
        }

        /// Creates an idle sound with no source, unit volume and speed, and spatial
        /// rendering enabled when parented beneath a part.
        pub fn new() -> Self {
            Self {
                instance: InstanceData::new("Sound"),
                settings: SoundSettings::default(),
                commands: VecDeque::new(),
                playback: None,
            }
        }

        /// Queues playback, resuming a paused sound or restarting an ended sound.
        #[inherent]
        pub fn play(&mut self) {
            self.commands.push_back(PlaybackCommand::Play);
        }

        /// Queues playback from the beginning, even if already playing.
        #[inherent]
        pub fn replay(&mut self) {
            self.commands.push_back(PlaybackCommand::Replay);
        }

        /// Queues a pause while retaining the current playback position.
        #[inherent]
        pub fn pause(&mut self) {
            self.commands.push_back(PlaybackCommand::Pause);
        }

        /// Queues resumption of paused playback without starting an idle sound.
        #[inherent]
        pub fn resume(&mut self) {
            self.commands.push_back(PlaybackCommand::Resume);
        }

        /// Queues stopping playback and resetting the position to zero.
        #[inherent]
        pub fn stop(&mut self) {
            self.commands.push_back(PlaybackCommand::Stop);
        }

        /// Queues a seek without changing the intended pause state.
        #[inherent]
        pub fn seek(&mut self, position: Duration) {
            self.commands.push_back(PlaybackCommand::Seek(position));
        }

        /// Returns the backend handle after this sound's first audio update.
        ///
        /// This provides playback events, loading, effects, and advanced Euphorium
        /// controls. A cloned handle refers to the same voice and is invalidated
        /// when this instance is destroyed. Prefer [`HasSound`] for authored
        /// properties so subsequent scene updates and scene clones retain them.
        #[inherent]
        pub fn playback(&self) -> Option<&euphorium::Sound> {
            self.playback.as_ref().map(|playback| &playback.sound)
        }

        /// Returns the live playback state, or [`PlaybackState::Idle`] before the
        /// first audio update. Pending playback requests are not reflected yet.
        #[inherent]
        pub fn playback_state(&self) -> Result<PlaybackState, SoundscapeError> {
            self.playback()
                .map_or(Ok(PlaybackState::Idle), euphorium::Sound::playback_state)
        }

        /// Returns the live playback position, or zero before the first audio update.
        #[inherent]
        pub fn time_position(&self) -> Result<Duration, SoundscapeError> {
            self.playback()
                .map_or(Ok(Duration::ZERO), euphorium::Sound::position)
        }

        /// Returns the source duration once known to the backend.
        #[inherent]
        pub fn duration(&self) -> Result<Option<Duration>, SoundscapeError> {
            self.playback().map_or(Ok(None), euphorium::Sound::duration)
        }

        pub(crate) fn sync_audio(
            &mut self,
            soundscape: &Soundscape,
            emitter: Option<(InstanceId, Vec3)>,
            delta_seconds: f32,
        ) -> Result<(), SoundscapeError> {
            if self.playback.is_none() {
                self.playback = Some(SoundPlayback {
                    // Instance display names need not be unique or valid mixer paths.
                    sound: soundscape.create_sound(format!("{:?}", self.id()), SoundSource::empty())?,
                    applied: None,
                    parent: None,
                    previous_position: None,
                });
            }
            let playback = self.playback.as_mut().expect("just initialized playback");
            let settings = &self.settings;
            let emitter = emitter.filter(|_| settings.spatial_enabled);
            let parent = emitter.map(|(parent, _)| parent);
            let spatial = emitter.map(|(_, position)| SpatialSound {
                emitter: EmitterState {
                    position: position.to_array(),
                    velocity: velocity(
                        position,
                        playback
                            .previous_position
                            .filter(|_| playback.parent == parent),
                        delta_seconds,
                    )
                    .to_array(),
                },
                ..settings.spatial
            });
            let previous = playback.applied.as_ref();

            if playback.parent != parent || previous.is_none_or(|old| old.spatial != settings.spatial) {
                playback.sound.set_spatial(spatial)?;
            }
            if let Some(spatial) = spatial {
                playback.sound.set_emitter(spatial.emitter)?;
                playback.sound.set_occlusion(spatial.occlusion)?;
            }
            if previous.is_none_or(|old| old.volume != settings.volume) {
                playback.sound.set_volume(settings.volume)?;
            }
            if previous.is_none_or(|old| old.playback_speed != settings.playback_speed) {
                playback.sound.set_speed(settings.playback_speed)?;
            }
            if previous.is_none_or(|old| old.looping != settings.looping) {
                playback.sound.set_looping(settings.looping)?;
            }
            if previous.is_none_or(|old| old.source != settings.source) {
                playback.sound.set_source(settings.source.clone())?;
            }
            playback.parent = parent;
            playback.previous_position = emitter.map(|(_, position)| position);
            if previous != Some(settings) {
                playback.applied = Some(settings.clone());
            }

            while let Some(command) = self.commands.pop_front() {
                match command {
                    PlaybackCommand::Play => playback.sound.play(),
                    PlaybackCommand::Replay => playback.sound.replay(),
                    PlaybackCommand::Pause => playback.sound.pause(),
                    PlaybackCommand::Resume => playback.sound.resume(),
                    PlaybackCommand::Stop => playback.sound.stop(),
                    PlaybackCommand::Seek(position) => playback.sound.try_seek(position),
                }?;
            }
            Ok(())
        }
    }
}

impl Default for Sound {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for Sound {
    fn clone(&self) -> Self {
        Self {
            instance: self.instance.clone(),
            settings: self.settings.clone(),
            commands: VecDeque::new(),
            playback: None,
        }
    }
}

/// Compatibility name for the generated Sound API.
pub use SoundExt as HasSound;
