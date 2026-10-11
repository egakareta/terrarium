use std::{collections::VecDeque, time::Duration};

use euphorium::{
    EmitterState, PlaybackState, SoundSource, Soundscape, SoundscapeError, SpatialSound,
};
use glam::Vec3;

use crate::{Instance, InstanceData, InstanceId, scene::audio::velocity};

#[derive(Clone, Debug, PartialEq)]
struct SoundSettings {
    source: SoundSource,
    volume: f32,
    playback_speed: f32,
    looping: bool,
    spatial_enabled: bool,
    emitter_offset: Vec3,
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
pub struct Sound {
    instance: InstanceData,
    settings: SoundSettings,
    commands: VecDeque<PlaybackCommand>,
    playback: Option<SoundPlayback>,
}

impl Sound {
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
    /// when this instance is destroyed. Prefer [`HasSound`] for authored
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

crate::impl_instance!(Sound, class_name = "Sound", data = instance,);

/// Authored sound properties, available on owned sounds and mutable sound references.
pub trait HasSound {
    /// Returns the underlying sound.
    fn sound(&self) -> &Sound;

    /// Returns mutable access to the underlying sound.
    fn sound_mut(&mut self) -> &mut Sound;

    /// Returns the encoded audio source descriptor.
    fn source(&self) -> &SoundSource {
        &self.sound().settings.source
    }

    /// Returns the local volume before distance, occlusion, and master multipliers.
    fn volume(&self) -> f32 {
        self.sound().settings.volume
    }

    /// Returns the authored playback speed, before Doppler pitch shifting.
    fn playback_speed(&self) -> f32 {
        self.sound().settings.playback_speed
    }

    /// Returns whether the source repeats when it reaches the end.
    fn looping(&self) -> bool {
        self.sound().settings.looping
    }

    /// Returns whether a part ancestor enables positional playback.
    fn spatial_enabled(&self) -> bool {
        self.sound().settings.spatial_enabled
    }

    /// Returns the emitter offset in its nearest part ancestor's pivot coordinates.
    fn emitter_offset(&self) -> Vec3 {
        self.sound().settings.emitter_offset
    }

    /// Returns attenuation, occlusion, Doppler, and input-channel settings.
    ///
    /// The authored emitter field is ignored; scene transforms supply it instead.
    fn spatial_settings(&self) -> SpatialSound {
        self.sound().settings.spatial
    }

    /// Replaces the source, preserving playback state and position when possible.
    fn with_source(mut self, source: impl Into<SoundSource>) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.source = source.into();
        self
    }

    /// Sets the local volume. Invalid values are reported by the next audio update.
    fn with_volume(mut self, volume: f32) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.volume = volume;
        self
    }

    /// Sets playback speed. Invalid values are reported by the next audio update.
    fn with_playback_speed(mut self, speed: f32) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.playback_speed = speed;
        self
    }

    /// Enables or disables repeating the source.
    fn with_looping(mut self, looping: bool) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.looping = looping;
        self
    }

    /// Enables or disables positional playback beneath a part.
    ///
    /// Like Euphorium's spatial renderer configuration, this takes effect on
    /// the next playback start or replay, not on an already playing voice.
    fn with_spatial_enabled(mut self, enabled: bool) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.spatial_enabled = enabled;
        self
    }

    /// Sets the emitter offset in part-local pivot coordinates, without part scale.
    /// Active positional sounds follow changes on the next audio update.
    fn with_emitter_offset(mut self, offset: Vec3) -> Self
    where
        Self: Sized,
    {
        self.sound_mut().settings.emitter_offset = offset;
        self
    }

    /// Sets attenuation, occlusion, Doppler, and input-channel handling.
    ///
    /// Occlusion changes apply on the next audio update. Other configuration
    /// changes take effect when playback next starts or is replayed. Emitter
    /// position and velocity are always supplied by the scene. Invalid spatial
    /// settings are reported by the next audio update for positional sounds.
    fn with_spatial_settings(mut self, mut settings: SpatialSound) -> Self
    where
        Self: Sized,
    {
        settings.emitter = EmitterState::default();
        self.sound_mut().settings.spatial = settings;
        self
    }
}

impl HasSound for Sound {
    fn sound(&self) -> &Sound {
        self
    }

    fn sound_mut(&mut self) -> &mut Sound {
        self
    }
}

impl<T: HasSound + ?Sized> HasSound for &mut T {
    fn sound(&self) -> &Sound {
        (**self).sound()
    }

    fn sound_mut(&mut self) -> &mut Sound {
        (**self).sound_mut()
    }
}
