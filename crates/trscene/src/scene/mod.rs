//! Optional workspace physics and audio integrations.

#[cfg(feature = "sound")]
pub(crate) mod audio;
#[cfg(feature = "physics")]
mod physics;

#[cfg(feature = "sound")]
pub use audio::AudioError;
#[cfg(feature = "physics")]
pub use physics::*;
