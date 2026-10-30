use std::{collections::HashSet, sync::Arc};

#[cfg(feature = "meshpart")]
mod gltf;
#[cfg(feature = "meshpart")]
pub(crate) use gltf::*;

use crate::{glam::*, *};

/// Invalid keyframe data or an unresolved animation target.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnimationError {
    /// A clip or track must contain at least one entry.
    #[error("animations need at least one track and tracks need at least one keyframe")]
    Empty,
    /// Target paths must be nonempty and may not contain empty names.
    #[error("animation target paths must contain nonempty instance names")]
    InvalidTarget,
    /// Two tracks target the same instance path.
    #[error("an animation may only contain one track per target path")]
    DuplicateTarget,
    /// Times must be finite, nonnegative, and strictly increasing within a track.
    #[error("keyframe times must be finite, nonnegative, and strictly increasing")]
    InvalidTime,
    /// Playback speed must be finite and nonnegative.
    #[error("animation speed must be finite and nonnegative")]
    InvalidSpeed,
    /// Transforms must contain a finite translation, rotation, and positive scale without shear.
    #[error("keyframes require finite transforms with positive scale and no shear")]
    InvalidTransform,
    /// A target path could not be resolved to a built-in spatial descendant.
    #[error("animation target was not found or is not spatial: {0:?}")]
    TargetNotFound(Vec<String>),
    /// More than one sibling has the name used by a target path.
    #[error("animation target path is ambiguous: {0:?}")]
    AmbiguousTarget(Vec<String>),
}

/// A timestamped pose relative to the target's nearest spatial ancestor.
///
/// Translation and scale interpolate linearly, and rotation uses shortest-path
/// quaternion interpolation. The easing curve applies to the segment beginning
/// at this keyframe.
#[derive(Clone, Copy, Debug)]
pub struct Keyframe {
    time: f32,
    transform: Mat4,
    easing: Easing,
}

impl Keyframe {
    /// Creates a local transform keyframe; [`AnimationTrack::try_new`] validates it.
    pub fn new(time_seconds: f32, transform: Mat4) -> Self {
        Self {
            time: time_seconds,
            transform,
            easing: Easing::Linear,
        }
    }

    /// Sets the easing curve for the segment leading to the next keyframe.
    pub fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Returns the keyframe time in seconds.
    pub fn time(&self) -> f32 {
        self.time
    }

    /// Returns the local transform at this keyframe.
    pub fn transform(&self) -> Mat4 {
        self.transform
    }
}

/// A local transform timeline targeting a path of instance names within a [`crate::Rig`].
///
/// For example, `["Torso", "Arm"]` targets the `Arm` child of `Torso`.
/// Clips can be reused across rigs with the same hierarchy and names.
#[derive(Clone, Debug)]
pub struct AnimationTrack {
    target: Vec<String>,
    keyframes: Vec<Keyframe>,
    #[cfg(feature = "meshpart")]
    channels: Option<Arc<GltfTransformTrack>>,
}

impl AnimationTrack {
    /// Creates a track, panicking if its path or keyframes are invalid.
    pub fn new(
        target: impl IntoIterator<Item = impl Into<String>>,
        keyframes: impl IntoIterator<Item = Keyframe>,
    ) -> Self {
        Self::try_new(target, keyframes).expect("invalid animation track")
    }

    /// Creates a track, validating its path, timestamps, and transforms.
    pub fn try_new(
        target: impl IntoIterator<Item = impl Into<String>>,
        keyframes: impl IntoIterator<Item = Keyframe>,
    ) -> Result<Self, AnimationError> {
        let target: Vec<_> = target.into_iter().map(Into::into).collect();
        if target.is_empty() || target.iter().any(String::is_empty) {
            return Err(AnimationError::InvalidTarget);
        }
        let keyframes: Vec<_> = keyframes.into_iter().collect();
        if keyframes.is_empty() {
            return Err(AnimationError::Empty);
        }
        for (index, keyframe) in keyframes.iter().enumerate() {
            if !keyframe.time.is_finite()
                || keyframe.time < 0.0
                || (index > 0 && keyframe.time <= keyframes[index - 1].time)
            {
                return Err(AnimationError::InvalidTime);
            }
            if !valid_transform(keyframe.transform) {
                return Err(AnimationError::InvalidTransform);
            }
        }
        Ok(Self {
            target,
            keyframes,
            #[cfg(feature = "meshpart")]
            channels: None,
        })
    }

    /// Returns the target's path relative to the rig.
    pub fn target(&self) -> &[String] {
        &self.target
    }

    /// Returns the ordered keyframes.
    ///
    /// Imported glTF tracks expose poses at channel timestamps. [`Self::sample`]
    /// retains the channels' individual interpolation modes between them.
    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    /// Returns the time of the last keyframe.
    pub fn duration(&self) -> f32 {
        self.keyframes
            .last()
            .expect("validated nonempty track")
            .time
    }

    /// Samples the timeline, holding the first or last pose outside its time range.
    ///
    /// NaN samples use time zero.
    pub fn sample(&self, time_seconds: f32) -> Mat4 {
        #[cfg(feature = "meshpart")]
        if let Some(channels) = &self.channels {
            return channels.sample(time_seconds);
        }
        let time = if time_seconds.is_nan() {
            0.0
        } else {
            time_seconds
        };
        let next = self.keyframes.partition_point(|key| key.time <= time);
        if next == 0 {
            return self.keyframes[0].transform;
        }
        if next == self.keyframes.len() {
            return self.keyframes[next - 1].transform;
        }
        let from = &self.keyframes[next - 1];
        let to = &self.keyframes[next];
        let amount = from
            .easing
            .sample((time - from.time) / (to.time - from.time));
        interpolate_transform(from.transform, to.transform, amount)
    }
}

pub(crate) fn valid_transform(transform: Mat4) -> bool {
    if !transform.is_finite() || transform.determinant() <= 0.0 {
        return false;
    }
    let (scale, rotation, translation) = transform.to_scale_rotation_translation();
    scale.is_finite()
        && scale.cmpgt(Vec3::ZERO).all()
        && rotation.is_finite()
        && rotation.is_normalized()
        && Mat4::from_scale_rotation_translation(scale, rotation, translation)
            .abs_diff_eq(transform, 0.0001)
}

pub(crate) fn interpolate_transform(from: Mat4, to: Mat4, amount: f32) -> Mat4 {
    if amount <= 0.0 {
        return from;
    }
    if amount >= 1.0 {
        return to;
    }
    let (from_scale, from_rotation, from_translation) = from.to_scale_rotation_translation();
    let (to_scale, to_rotation, to_translation) = to.to_scale_rotation_translation();
    Mat4::from_scale_rotation_translation(
        from_scale.lerp(to_scale, amount),
        from_rotation.slerp(to_rotation, amount).normalize(),
        from_translation.lerp(to_translation, amount),
    )
}

instance!({
    /// A reusable set of local transform tracks played on a [`crate::Rig`].
    #[derive(Clone, Debug)]
    pub struct AnimationClip {
        /// All tracks in this clip.
        #[property(borrow, into)]
        tracks: Arc<[AnimationTrack]>,
        /// The length of the longest track in seconds.
        #[property]
        duration: f32,
    }
});

impl AnimationClip {
    /// Creates a clip, panicking if it is empty or repeats a target path.
    pub fn new(tracks: impl IntoIterator<Item = AnimationTrack>) -> Self {
        Self::try_new(tracks).expect("invalid animation clip")
    }

    /// Creates a clip, rejecting empty clips and duplicate target paths.
    pub fn try_new(
        tracks: impl IntoIterator<Item = AnimationTrack>,
    ) -> Result<Self, AnimationError> {
        let tracks: Vec<_> = tracks.into_iter().collect();
        if tracks.is_empty() {
            return Err(AnimationError::Empty);
        }
        let mut targets = HashSet::new();
        for track in &tracks {
            if !targets.insert(track.target()) {
                return Err(AnimationError::DuplicateTarget);
            }
        }
        let duration = tracks
            .iter()
            .map(AnimationTrack::duration)
            .fold(0.0, f32::max);
        Ok(Self {
            tracks: tracks.into(),
            duration,
        })
    }

    pub(crate) fn same_clip(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.tracks, &other.tracks)
    }
}

instance!({
    /// Playback state for a keyframed [`AnimationClip`].
    ///
    /// Attach it using [`crate::Rig::play_animation`]. [`crate::Workspace::update`]
    /// advances rig animations automatically.
    #[derive(Clone, Debug)]
    pub struct AnimationPlayer {
        /// The clip being played.
        #[property(getter, borrow)]
        clip: AnimationClip,
        /// The playback position in seconds.
        #[property]
        time: f64,
        /// The playback multiplier, rejecting negative or nonfinite speeds.
        #[property(setter = apply_valid_speed)]
        speed: f32,
        /// Whether playback wraps at the clip's end.
        #[property]
        looping: bool,
        /// Whether playback is paused at its current time.
        #[property]
        paused: bool,
        /// Whether a one-shot clip has reached its final pose.
        #[property(getter)]
        finished: bool,
    }
});

impl AnimationPlayer {
    /// Creates a player at time zero, playing once at normal speed.
    pub fn new(clip: AnimationClip) -> Self {
        Self {
            clip,
            time: 0.0,
            speed: 1.0,
            looping: false,
            paused: false,
            finished: false,
        }
    }

    fn apply_valid_speed(&mut self, speed: f32) {
        if !speed.is_finite() || speed < 0.0 {
            self.speed = 1.0;
            return;
        }
        self.speed = speed;
    }

    /// Restarts playback at time zero.
    pub fn restart(&mut self) {
        self.time = 0.0;
        self.finished = false;
        self.paused = false;
    }

    /// Seeks to a time clamped to the clip's range, retaining the pause state.
    ///
    /// The pose is applied on the rig's next animation update.
    pub fn seek(&mut self, time_seconds: f32) -> Result<(), AnimationError> {
        if !time_seconds.is_finite() || time_seconds < 0.0 {
            return Err(AnimationError::InvalidTime);
        }
        self.time = f64::from(time_seconds.min(self.clip.duration));
        self.finished = false;
        Ok(())
    }

    /// Advances playback, ignoring negative and nonfinite deltas.
    pub fn update(&mut self, delta_seconds: f32) {
        if self.paused || self.finished || !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return;
        }
        let duration = f64::from(self.clip.duration);
        self.time += f64::from(delta_seconds) * f64::from(self.speed);
        if self.time >= duration {
            if self.looping && duration > 0.0 {
                self.time %= duration;
            } else {
                self.time = duration;
                self.finished = true;
            }
        }
    }
}
