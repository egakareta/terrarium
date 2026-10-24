use std::{collections::VecDeque, time::Duration};

use euphorium::*;

use crate::{glam::Vec3, scene::audio::velocity, *};

instance! {
    #[instance(inspect = false)]
    /// Authored audio properties shared by sound instances.
    #[doc(hidden)]
    #[derive(Clone, Debug, PartialEq)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct SoundSettings {
        /// Encoded audio source. Replacing it preserves playback state when possible.
        #[property(default = SoundSource::empty(), borrow, into)]
        #[cfg_attr(feature = "devtools", inspect(readonly))]
        source: SoundSource,
        /// Local volume before distance, occlusion, and master multipliers.
        #[property(default = 1.0)]
        volume: f32,
        /// Authored speed before Doppler shifting. Invalid values fail the next update.
        #[property(default = 1.0)]
        playback_speed: f32,
        /// Whether the source repeats when it reaches the end.
        #[property]
        looping: bool,
        /// Whether a part ancestor enables positional playback on the next start.
        #[property(default = true)]
        spatial_enabled: bool,
        /// Offset in the nearest part ancestor's pivot coordinates, without its scale.
        #[property]
        emitter_offset: Vec3,
        /// Attenuation, occlusion, Doppler, and channel settings; scene transforms supply the emitter.
        #[property(default = SpatialSound { doppler: true, ..SpatialSound::default() },
                    sanitize = |mut settings: SpatialSound| { settings.emitter = EmitterState::default(); settings })]
        #[cfg_attr(feature = "devtools", inspect(readonly))]
        spatial_settings: SpatialSound,
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

instance! {
    #[instance(inspect = false)]
    /// A scene-tree sound backed by Euphorium.
    ///
    /// Sounds beneath a [`crate::Part`], [`crate::BasePart`], or `MeshPart` use their
    /// nearest part ancestor as a point emitter. Other sounds play globally without
    /// spatial processing. One world unit is one meter for attenuation and Doppler.
    ///
    /// Properties and playback requests are applied by [`crate::Workspace::step_audio`]
    /// (also called by [`crate::Workspace::update`]). Cloning copies the source and
    /// settings, but not playback or queued requests. Destroying a sound removes its
    /// backend voice immediately.
    #[derive(Debug)]
    #[cfg_attr(feature = "devtools", derive(devtools::Inspect))]
    pub struct Sound {
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[data]
        instance: InstanceData,
        #[cfg_attr(feature = "devtools", inspect(flatten))]
        #[compose]
        settings: SoundSettings,
        #[cfg_attr(feature = "devtools", inspect(skip))]
        commands: VecDeque<PlaybackCommand>,
        #[cfg_attr(feature = "devtools", inspect(skip))]
        playback: Option<SoundPlayback>,
    }
    impl Sound {
        /// Creates an idle sound with no source, unit volume and speed, and spatial
        /// rendering enabled when parented beneath a part.
        pub fn new() -> Self {
            Self::default()
        }

        /// Queues playback, resuming a paused sound or restarting an ended sound.
        pub fn play(&mut self) {
            self.commands.push_back(PlaybackCommand::Play);
        }

        /// Queues playback from the beginning, even if already playing.
        pub fn replay(&mut self) {
            self.commands.push_back(PlaybackCommand::Replay);
        }

        /// Queues a pause while retaining the current playback position.
        pub fn pause(&mut self) {
            self.commands.push_back(PlaybackCommand::Pause);
        }

        /// Queues resumption of paused playback without starting an idle sound.
        pub fn resume(&mut self) {
            self.commands.push_back(PlaybackCommand::Resume);
        }

        /// Queues stopping playback and resetting the position to zero.
        pub fn stop(&mut self) {
            self.commands.push_back(PlaybackCommand::Stop);
        }

        /// Queues a seek without changing the intended pause state.
        pub fn seek(&mut self, position: Duration) {
            self.commands.push_back(PlaybackCommand::Seek(position));
        }

        /// Returns the backend handle after this sound's first audio update.
        ///
        /// This provides playback events, loading, effects, and advanced Euphorium
        /// controls. A cloned handle refers to the same voice and is invalidated
        /// when this instance is destroyed. Prefer [`SoundExt`] for authored
        /// properties so subsequent scene updates and scene clones retain them.
        pub fn playback(&self) -> Option<&euphorium::Sound> {
            self.playback.as_ref().map(|playback| &playback.sound)
        }

        /// Returns the live playback state, or [`PlaybackState::Idle`] before the
        /// first audio update. Pending playback requests are not reflected yet.
        pub fn playback_state(&self) -> Result<PlaybackState, SoundscapeError> {
            self.playback()
                .map_or(Ok(PlaybackState::Idle), euphorium::Sound::playback_state)
        }

        /// Returns the live playback position, or zero before the first audio update.
        pub fn time_position(&self) -> Result<Duration, SoundscapeError> {
            self.playback()
                .map_or(Ok(Duration::ZERO), euphorium::Sound::position)
        }

        /// Returns the source duration once known to the backend.
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
                ..settings.spatial_settings
            });
            let previous = playback.applied.as_ref();

            if playback.parent != parent
                || previous.is_none_or(|old| old.spatial_settings != settings.spatial_settings)
            {
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
