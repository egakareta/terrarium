use crate::{
    InstanceData, InstanceId,
    glam::{Mat4, Vec3},
};

/// Startup settings for an automatically created local [`Player`].
#[derive(Clone, Debug)]
pub struct LocalPlayerConfig {
    /// Application-level player ID; defaults to `"local"`.
    pub player_id: String,
    /// Seconds a character may be absent before it is recreated; defaults to three.
    pub respawn_time: f32,
    /// Initial and respawn world-space pose of the character's torso pivot.
    pub spawn_pose: Mat4,
    /// Whether missing characters are automatically recreated.
    pub auto_spawn: bool,
}

impl Default for LocalPlayerConfig {
    fn default() -> Self {
        Self {
            player_id: "local".to_owned(),
            respawn_time: 3.0,
            spawn_pose: Mat4::from_translation(Vec3::new(0.0, 3.0, 0.0)),
            auto_spawn: true,
        }
    }
}

/// A persistent player identity with a replaceable [`Humanoid`](crate::Humanoid) character.
///
/// Players and their characters are separate scene instances. The workspace checks
/// character existence every update and respawns after `respawn_time` seconds of
/// simulation time. Networking and character input are not implemented yet.
#[derive(Clone, Debug)]
pub struct Player {
    instance: InstanceData,
    /// Application-level ID, copied into newly spawned humanoids.
    pub player_id: String,
    /// Whether this player represents a local user; defaults to `true`.
    pub local_player: bool,
    /// The current character's instance ID, cleared when its absence is detected.
    pub character: Option<InstanceId>,
    /// Delay in seconds before automatic respawn. Negative or non-finite values mean zero.
    pub respawn_time: f32,
    /// World-space rigid pose used for spawning; scale is ignored.
    pub spawn_pose: Mat4,
    /// Whether the workspace automatically creates missing characters.
    pub auto_spawn: bool,
    pub(crate) missing_seconds: f32,
    pub(crate) previous_character: Option<InstanceId>,
}

impl Player {
    /// Creates a local player with the supplied application-level ID and default spawn settings.
    ///
    /// When added to a workspace, its first character is created after the respawn delay.
    /// Call [`Workspace::spawn_character`](crate::Workspace::spawn_character) for an immediate spawn.
    pub fn new(player_id: impl Into<String>) -> Self {
        Self::from_config(LocalPlayerConfig {
            player_id: player_id.into(),
            ..Default::default()
        })
    }

    /// Creates a local player with custom spawn and respawn settings.
    pub fn from_config(config: LocalPlayerConfig) -> Self {
        Self {
            instance: InstanceData::new("Player"),
            player_id: config.player_id,
            local_player: true,
            character: None,
            respawn_time: config.respawn_time,
            spawn_pose: config.spawn_pose,
            auto_spawn: config.auto_spawn,
            missing_seconds: 0.0,
            previous_character: None,
        }
    }
}

crate::impl_instance!(Player, class_name = "Player", data = instance,);
