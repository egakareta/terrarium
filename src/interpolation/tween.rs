use crate::{
    BasePart, Color3, InstanceId, Part, Workspace,
    glam::{Vec2, Vec3, Vec4},
};

/// Interpolates a value between two keyframes.
pub trait Tweenable: Clone {
    /// Returns the value at `amount`, where `0.0` is `from` and `1.0` is `to`.
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self;
}

impl Tweenable for f32 {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        from + (to - from) * amount
    }
}

impl Tweenable for Vec2 {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        from.lerp(*to, amount)
    }
}

impl Tweenable for Vec3 {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        from.lerp(*to, amount)
    }
}

impl Tweenable for Vec4 {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        from.lerp(*to, amount)
    }
}

impl Tweenable for Color3 {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        Self::new(
            f32::tween_lerp(&from.r, &to.r, amount),
            f32::tween_lerp(&from.g, &to.g, amount),
            f32::tween_lerp(&from.b, &to.b, amount),
        )
    }
}

impl<const N: usize> Tweenable for [f32; N] {
    fn tween_lerp(from: &Self, to: &Self, amount: f32) -> Self {
        std::array::from_fn(|index| f32::tween_lerp(&from[index], &to[index], amount))
    }
}

/// Common easing curves for tweened values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Easing {
    /// Constant-rate interpolation.
    #[default]
    Linear,
    /// Starts slowly and accelerates.
    EaseIn,
    /// Starts quickly and decelerates.
    EaseOut,
    /// Accelerates through the first half and decelerates through the second.
    EaseInOut,
}

impl Easing {
    /// Evaluates the curve for a normalized progress value.
    pub fn sample(self, progress: f32) -> f32 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::EaseIn => progress * progress,
            Self::EaseOut => 1.0 - (1.0 - progress) * (1.0 - progress),
            Self::EaseInOut => {
                if progress < 0.5 {
                    2.0 * progress * progress
                } else {
                    1.0 - (-2.0 * progress + 2.0).powi(2) / 2.0
                }
            }
        }
    }
}

/// Controls how many times a tween runs after its first pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat {
    /// Runs once and finishes at the final keyframe.
    #[default]
    Once,
    /// Runs `count` additional times after the first pass.
    Count(u32),
    /// Repeats until it is cancelled or removed.
    Forever,
}

/// Errors returned when constructing a keyframe tween.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TweenError {
    /// A tween path needs at least a start and an end keyframe.
    #[error("a tween path needs at least two keyframes")]
    NotEnoughKeyframes,
    /// A tween duration must be finite and non-negative.
    #[error("a tween duration must be finite and non-negative")]
    InvalidDuration,
}

/// A time-based interpolation between one or more keyframes.
#[derive(Clone, Debug)]
pub struct Tween<T: Tweenable> {
    keyframes: Vec<T>,
    duration: f32,
    elapsed: f32,
    delay_remaining: f32,
    easing: Easing,
    repeat: Repeat,
    completed_repeats: u32,
    yoyo: bool,
    reversed: bool,
    finished: bool,
}

impl<T: Tweenable> Tween<T> {
    /// Creates a tween from `from` to `to` over `duration_seconds`.
    ///
    /// A zero-duration tween completes on its first update. Negative, infinite, and
    /// NaN durations are rejected by [`Self::try_new`].
    pub fn new(from: T, to: T, duration_seconds: f32) -> Self {
        Self::try_new(from, to, duration_seconds).expect("invalid tween duration")
    }

    /// Creates a tween from `from` to `to`, returning an error for an invalid duration.
    pub fn try_new(from: T, to: T, duration_seconds: f32) -> Result<Self, TweenError> {
        Self::try_path([from, to], duration_seconds)
    }

    /// Creates a tween that interpolates through each supplied keyframe in order.
    ///
    /// The total duration is divided evenly between adjacent keyframes. Use
    /// [`Self::try_path`] when invalid input should be handled instead of panicking.
    pub fn path(keyframes: impl IntoIterator<Item = T>, duration_seconds: f32) -> Self {
        Self::try_path(keyframes, duration_seconds).expect("invalid tween path")
    }

    /// Creates a keyframe tween, validating the path and duration.
    pub fn try_path(
        keyframes: impl IntoIterator<Item = T>,
        duration_seconds: f32,
    ) -> Result<Self, TweenError> {
        let keyframes: Vec<_> = keyframes.into_iter().collect();
        if keyframes.len() < 2 {
            return Err(TweenError::NotEnoughKeyframes);
        }
        if !duration_seconds.is_finite() || duration_seconds < 0.0 {
            return Err(TweenError::InvalidDuration);
        }
        Ok(Self {
            keyframes,
            duration: duration_seconds,
            elapsed: 0.0,
            delay_remaining: 0.0,
            easing: Easing::Linear,
            repeat: Repeat::Once,
            completed_repeats: 0,
            yoyo: false,
            reversed: false,
            finished: false,
        })
    }

    /// Sets the easing curve used across the keyframe path.
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Sets a delay in seconds before the first pass starts.
    pub fn delay(mut self, delay_seconds: f32) -> Self {
        self.delay_remaining = delay_seconds.max(0.0);
        self
    }

    /// Sets the repeat policy.
    pub fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }

    /// Repeats the tween forever.
    pub fn repeat_forever(self) -> Self {
        self.repeat(Repeat::Forever)
    }

    /// Repeats the tween `count` additional times.
    pub fn repeat_count(self, count: u32) -> Self {
        self.repeat(Repeat::Count(count))
    }

    /// Reverses direction on every repeat, producing ping-pong motion.
    pub fn yoyo(mut self) -> Self {
        self.yoyo = true;
        self
    }

    /// Alias for [`Self::yoyo`].
    pub fn ping_pong(self) -> Self {
        self.yoyo()
    }

    /// Returns the current interpolated value without advancing time.
    pub fn value(&self) -> T {
        self.sample(self.progress())
    }

    /// Advances the tween and returns its new value.
    pub fn update(&mut self, delta_seconds: f32) -> T {
        if self.finished || !delta_seconds.is_finite() || delta_seconds <= 0.0 {
            return self.value();
        }

        if self.duration == 0.0 {
            self.finished = true;
            self.elapsed = self.duration;
            return self.value();
        }

        let mut remaining = delta_seconds;
        if self.delay_remaining > 0.0 {
            let consumed = remaining.min(self.delay_remaining);
            self.delay_remaining -= consumed;
            remaining -= consumed;
        }

        while remaining > 0.0 && !self.finished {
            let until_boundary = self.duration - self.elapsed;
            if remaining < until_boundary {
                self.elapsed += remaining;
                break;
            }

            remaining -= until_boundary;
            self.elapsed = self.duration;
            self.advance_cycle();
            if self.finished {
                break;
            }
            self.skip_full_cycles(&mut remaining);
        }

        self.value()
    }

    /// Returns whether the tween has completed all of its passes.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Returns the current raw progress within the active pass.
    pub fn progress(&self) -> f32 {
        if self.duration == 0.0 {
            1.0
        } else {
            (self.elapsed / self.duration).clamp(0.0, 1.0)
        }
    }

    fn advance_cycle(&mut self) {
        let should_repeat = match self.repeat {
            Repeat::Once => false,
            Repeat::Count(count) => self.completed_repeats < count,
            Repeat::Forever => true,
        };
        if !should_repeat {
            self.finished = true;
            return;
        }

        self.completed_repeats = self.completed_repeats.saturating_add(1);
        self.elapsed = 0.0;
        if self.yoyo {
            self.reversed = !self.reversed;
        }
    }

    fn skip_full_cycles(&mut self, remaining: &mut f32) {
        if *remaining < self.duration {
            return;
        }

        let full_cycles = (*remaining / self.duration).floor() as u64;
        if full_cycles == 0 {
            return;
        }

        match self.repeat {
            Repeat::Forever => {
                self.completed_repeats = self
                    .completed_repeats
                    .saturating_add(full_cycles.min(u64::from(u32::MAX)) as u32);
                if self.yoyo && full_cycles % 2 == 1 {
                    self.reversed = !self.reversed;
                }
                self.elapsed = 0.0;
                *remaining %= self.duration;
            }
            Repeat::Count(count) => {
                let cycles_until_finish =
                    u64::from(count.saturating_sub(self.completed_repeats)) + 1;
                if full_cycles >= cycles_until_finish {
                    if self.yoyo && (cycles_until_finish - 1) % 2 == 1 {
                        self.reversed = !self.reversed;
                    }
                    self.elapsed = self.duration;
                    self.finished = true;
                } else {
                    self.completed_repeats =
                        self.completed_repeats.saturating_add(full_cycles as u32);
                    if self.yoyo && full_cycles % 2 == 1 {
                        self.reversed = !self.reversed;
                    }
                    self.elapsed = 0.0;
                    *remaining %= self.duration;
                }
            }
            Repeat::Once => unreachable!("a one-shot tween finishes in advance_cycle"),
        }
    }

    fn sample(&self, progress: f32) -> T {
        let progress = if self.reversed {
            1.0 - progress
        } else {
            progress
        };
        let progress = self.easing.sample(progress);
        let segment_count = self.keyframes.len() - 1;
        let scaled = progress * segment_count as f32;
        let segment = (scaled.floor() as usize).min(segment_count - 1);
        let amount = (scaled - segment as f32).clamp(0.0, 1.0);
        T::tween_lerp(
            &self.keyframes[segment],
            &self.keyframes[segment + 1],
            amount,
        )
    }
}

/// Stable identifier for a registered scene tween.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TweenId(usize);

#[derive(Clone, Debug)]
enum TweenTrack {
    Position {
        id: TweenId,
        target: InstanceId,
        tween: Tween<Vec3>,
    },
    Orientation {
        id: TweenId,
        target: InstanceId,
        tween: Tween<Vec3>,
    },
    Size {
        id: TweenId,
        target: InstanceId,
        tween: Tween<Vec3>,
    },
    Color {
        id: TweenId,
        target: InstanceId,
        tween: Tween<Color3>,
    },
}

impl TweenTrack {
    fn id(&self) -> TweenId {
        match self {
            Self::Position { id, .. }
            | Self::Orientation { id, .. }
            | Self::Size { id, .. }
            | Self::Color { id, .. } => *id,
        }
    }

    fn update(&mut self, delta_seconds: f32, workspace: &mut Workspace) {
        match self {
            Self::Position { target, tween, .. } => {
                let value = tween.update(delta_seconds);
                with_base_part(workspace, *target, |part| part.set_position(value));
            }
            Self::Orientation { target, tween, .. } => {
                let value = tween.update(delta_seconds);
                with_base_part(workspace, *target, |part| part.set_orientation(value));
            }
            Self::Size { target, tween, .. } => {
                let value = tween.update(delta_seconds);
                with_base_part(workspace, *target, |part| part.size = value);
            }
            Self::Color { target, tween, .. } => {
                let value = tween.update(delta_seconds);
                with_base_part(workspace, *target, |part| part.color = value);
            }
        }
    }

    fn is_finished(&self) -> bool {
        match self {
            Self::Position { tween, .. } => tween.is_finished(),
            Self::Orientation { tween, .. } => tween.is_finished(),
            Self::Size { tween, .. } => tween.is_finished(),
            Self::Color { tween, .. } => tween.is_finished(),
        }
    }
}

fn with_base_part(
    workspace: &mut Workspace,
    target: InstanceId,
    apply: impl FnOnce(&mut BasePart),
) {
    if let Some(part) = workspace.get_mut::<Part>(target) {
        apply(part);
    } else if let Some(part) = workspace.get_mut::<BasePart>(target) {
        apply(part);
    }
}

/// Owns scene-property tweens and advances them against a [`Workspace`].
#[derive(Clone, Debug, Default)]
pub struct TweenManager {
    next_id: usize,
    tracks: Vec<TweenTrack>,
}

impl TweenManager {
    /// Creates an empty tween manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a position tween for a [`Part`] or [`BasePart`].
    pub fn add_position(&mut self, target: InstanceId, tween: Tween<Vec3>) -> TweenId {
        self.add_track(|id| TweenTrack::Position { id, target, tween })
    }

    /// Adds an XYZ Euler orientation tween in degrees.
    pub fn add_orientation(&mut self, target: InstanceId, tween: Tween<Vec3>) -> TweenId {
        self.add_track(|id| TweenTrack::Orientation { id, target, tween })
    }

    /// Adds a size tween for a [`Part`] or [`BasePart`].
    pub fn add_size(&mut self, target: InstanceId, tween: Tween<Vec3>) -> TweenId {
        self.add_track(|id| TweenTrack::Size { id, target, tween })
    }

    /// Adds an RGB color tween for a [`Part`] or [`BasePart`].
    pub fn add_color(&mut self, target: InstanceId, tween: Tween<Color3>) -> TweenId {
        self.add_track(|id| TweenTrack::Color { id, target, tween })
    }

    /// Advances every active tween and applies its values to `workspace`.
    pub fn update(&mut self, delta_seconds: f32, workspace: &mut Workspace) {
        let tracks = std::mem::take(&mut self.tracks);
        let mut active = Vec::with_capacity(tracks.len());
        for mut track in tracks {
            track.update(delta_seconds, workspace);
            if !track.is_finished() {
                active.push(track);
            }
        }
        self.tracks = active;
    }

    /// Cancels a tween, returning `true` when it was active.
    pub fn cancel(&mut self, id: TweenId) -> bool {
        let old_len = self.tracks.len();
        self.tracks.retain(|track| track.id() != id);
        self.tracks.len() != old_len
    }

    /// Cancels all active tweens.
    pub fn clear(&mut self) {
        self.tracks.clear();
    }

    /// Returns whether a tween with `id` is active.
    pub fn is_active(&self, id: TweenId) -> bool {
        self.tracks.iter().any(|track| track.id() == id)
    }

    /// Returns the number of active tweens.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// Returns whether there are no active tweens.
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    fn add_track(&mut self, create: impl FnOnce(TweenId) -> TweenTrack) -> TweenId {
        let id = TweenId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1);
        self.tracks.push(create(id));
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Instance, Part};

    #[test]
    fn tween_interpolates_and_finishes() {
        let mut tween = Tween::new(0.0, 10.0, 2.0);
        assert_eq!(tween.update(0.5), 2.5);
        assert!(!tween.is_finished());
        assert_eq!(tween.update(1.5), 10.0);
        assert!(tween.is_finished());
    }

    #[test]
    fn easing_is_applied_and_paths_interpolate() {
        let mut tween = Tween::new(0.0, 10.0, 1.0).easing(Easing::EaseIn);
        assert_eq!(tween.update(0.5), 2.5);

        let mut path = Tween::path([0.0, 10.0, 20.0], 2.0);
        assert_eq!(path.update(1.0), 10.0);
    }

    #[test]
    fn delay_repeat_and_yoyo_preserve_time_remainder() {
        let mut tween = Tween::new(0.0, 1.0, 1.0).delay(0.5).repeat_count(1).yoyo();
        assert_eq!(tween.update(0.25), 0.0);
        assert_eq!(tween.update(0.5), 0.25);
        assert_eq!(tween.update(0.75), 1.0);
        assert_eq!(tween.update(0.5), 0.5);
        assert_eq!(tween.update(0.5), 0.0);
        assert!(tween.is_finished());
    }

    #[test]
    fn manager_applies_and_removes_completed_tracks() {
        let mut workspace = Workspace::new();
        let id = workspace.add_child(Part::new("animated"));
        let tween_id = workspace
            .tweens_mut()
            .add_position(id, Tween::new(Vec3::ZERO, Vec3::X, 1.0));

        workspace.update_tweens(0.5);
        assert_eq!(
            workspace.get::<Part>(id).unwrap().position(),
            Vec3::new(0.5, 0.0, 0.0)
        );
        assert!(workspace.tweens().is_active(tween_id));

        workspace.update_tweens(0.5);
        assert_eq!(workspace.get::<Part>(id).unwrap().position(), Vec3::X);
        assert!(workspace.tweens().is_empty());
    }

    #[test]
    fn invalid_paths_are_reported() {
        assert!(matches!(
            Tween::try_path([1.0], 1.0),
            Err(TweenError::NotEnoughKeyframes)
        ));
        assert!(matches!(
            Tween::try_new(0.0, 1.0, -1.0),
            Err(TweenError::InvalidDuration)
        ));
    }
}
