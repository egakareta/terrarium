use crate::{glam::*, *};

#[derive(Clone, Debug)]
pub(crate) struct GltfChannel {
    pub times: Vec<f32>,
    pub values: Vec<Vec4>,
    pub interpolation: gltf::animation::Interpolation,
    pub rotation: bool,
}

impl GltfChannel {
    pub fn sample(&self, time: f32) -> Vec4 {
        use gltf::animation::Interpolation;

        let cubic = self.interpolation == Interpolation::CubicSpline;
        let value = |index: usize| self.values[if cubic { index * 3 + 1 } else { index }];
        let next = self.times.partition_point(|key| *key <= time);
        if next == 0 {
            return value(0);
        }
        if next == self.times.len() {
            return value(next - 1);
        }
        let previous = next - 1;
        let duration = self.times[next] - self.times[previous];
        let t = (time - self.times[previous]) / duration;
        let from = value(previous);
        let to = value(next);
        match self.interpolation {
            Interpolation::Step => from,
            Interpolation::Linear if self.rotation => Vec4::from_array(
                Quat::from_array(from.to_array())
                    .slerp(Quat::from_array(to.to_array()), t)
                    .normalize()
                    .to_array(),
            ),
            Interpolation::Linear => from.lerp(to, t),
            Interpolation::CubicSpline => {
                let t2 = t * t;
                let t3 = t2 * t;
                let result = from * (2.0 * t3 - 3.0 * t2 + 1.0)
                    + self.values[previous * 3 + 2] * duration * (t3 - 2.0 * t2 + t)
                    + to * (-2.0 * t3 + 3.0 * t2)
                    + self.values[next * 3] * duration * (t3 - t2);
                if self.rotation {
                    result.normalize()
                } else {
                    result
                }
            }
        }
    }

    pub fn validate(&self) -> Result<(), AnimationError> {
        let stride = if self.interpolation == gltf::animation::Interpolation::CubicSpline {
            3
        } else {
            1
        };
        if self.times.is_empty() || self.values.len() != self.times.len() * stride {
            return Err(AnimationError::Empty);
        }
        if self
            .times
            .iter()
            .any(|time| !time.is_finite() || *time < 0.0)
            || self.times.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(AnimationError::InvalidTime);
        }
        if self.values.iter().any(|value| !value.is_finite()) {
            return Err(AnimationError::InvalidTransform);
        }
        if self.rotation
            && (0..self.times.len()).any(|index| {
                let value = self.values[index * stride + usize::from(stride == 3)];
                (value.length_squared() - 1.0).abs() > 0.001
            })
        {
            return Err(AnimationError::InvalidTransform);
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(crate) struct GltfTransformTrack {
    pub rest: Mat4,
    pub translation: Option<GltfChannel>,
    pub rotation: Option<GltfChannel>,
    pub scale: Option<GltfChannel>,
}

impl GltfTransformTrack {
    pub fn sample(&self, time: f32) -> Mat4 {
        let time = if time.is_nan() { 0.0 } else { time };
        let (scale, rotation, translation) = self.rest.to_scale_rotation_translation();
        Mat4::from_scale_rotation_translation(
            self.scale
                .as_ref()
                .map_or(scale, |channel| channel.sample(time).truncate()),
            self.rotation.as_ref().map_or(rotation, |channel| {
                Quat::from_array(channel.sample(time).to_array()).normalize()
            }),
            self.translation
                .as_ref()
                .map_or(translation, |channel| channel.sample(time).truncate()),
        )
    }
}

impl AnimationTrack {
    pub(crate) fn from_gltf(
        target: Vec<String>,
        channels: GltfTransformTrack,
    ) -> Result<Self, AnimationError> {
        let mut times = Vec::new();
        for channel in [&channels.translation, &channels.rotation, &channels.scale]
            .into_iter()
            .flatten()
        {
            channel.validate()?;
            times.extend_from_slice(&channel.times);
        }
        times.sort_by(f32::total_cmp);
        times.dedup();
        let mut track = Self::try_new(
            target,
            times
                .into_iter()
                .map(|time| Keyframe::new(time, channels.sample(time))),
        )?;
        track.channels = Some(std::sync::Arc::new(channels));
        Ok(track)
    }
}
