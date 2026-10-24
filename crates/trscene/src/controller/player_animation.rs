use std::collections::BTreeMap;

use crate::AnimationPlayer;

/// Locomotion states selected by [`crate::PlayerController`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PlayerAnimationState {
    /// No horizontal movement is requested while grounded.
    #[default]
    Idle,
    /// Horizontal movement at normal speed.
    Walk,
    /// Horizontal movement with sprint magnitudes applied.
    Sprint,
    /// An airborne dynamic rig with upward velocity.
    Jump,
    /// An airborne dynamic rig without upward velocity.
    Fall,
}

/// Optional animation players bound to player locomotion states.
///
/// Unbound states leave the current animation playing. Airborne states require
/// physics; idle, walk, and sprint also work with anchored rigs.
#[derive(Clone, Debug)]
pub struct PlayerAnimations {
    states: BTreeMap<PlayerAnimationState, AnimationPlayer>,
    /// Crossfade duration in seconds. Invalid values switch immediately.
    pub transition_duration: f32,
}

impl Default for PlayerAnimations {
    fn default() -> Self {
        Self {
            states: BTreeMap::new(),
            transition_duration: 0.18,
        }
    }
}

impl PlayerAnimations {
    /// Binds a player to a state, retaining its speed, looping, and starting time.
    pub fn with_state(mut self, state: PlayerAnimationState, player: AnimationPlayer) -> Self {
        self.states.insert(state, player);
        self
    }

    /// Sets the crossfade duration in seconds.
    pub fn with_transition_duration(mut self, seconds: f32) -> Self {
        self.transition_duration = seconds;
        self
    }

    /// Returns the player bound to a state.
    pub fn get(&self, state: PlayerAnimationState) -> Option<&AnimationPlayer> {
        self.states.get(&state)
    }
}
