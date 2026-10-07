<div align="center">

# Terrarium 🌍

</div>

`terrarium` is a simple, extensible 3D engine built around two core crates:

- [wgpu](https://github.com/gfx-rs/wgpu) for 3D graphics
- [eframe](https://github.com/emilk/egui) for 2D graphics

Terrarium starts with a baseplate, a local player named `local`, and a six-part block
character. The camera follows the character in third person: drag with the left mouse
button to orbit and scroll to zoom. Missing characters respawn after three seconds.

Here's an example of adding a cube next to the character.

```rust,no_run
use terrarium::*;

fn main() {
    Terrarium::new()
        .run(|engine| {
            engine.add_child(Part::new().with_position(glam::Vec3::new(4.0, 0.5, 0.0)));
            Ok::<(), AppCreationError>(())
        })
        .unwrap();
}
```

Please check out [examples](https://github.com/egakareta/terrarium/tree/master/examples) to
learn more.

Configure the starting world and local player before running:

```rust,no_run
use terrarium::*;

Terrarium::preset(WorkspacePreset::Baseplate)
    .with_local_player(Some(LocalPlayerConfig {
        player_id: "my-player".into(),
        respawn_time: 2.0,
        ..Default::default()
    }))
    .run(|engine| {
        engine.current_camera.follow.distance = 16.0;
        engine.current_camera.follow.target_offset = glam::Vec3::new(0.0, 1.5, 0.0);

        let character = engine.local_player().unwrap().character.unwrap();
        engine.get_mut::<Humanoid>(character).unwrap()
            .pivot_to(glam::Mat4::from_translation(glam::Vec3::new(5.0, 3.0, 0.0)));
        Ok::<(), AppCreationError>(())
    })
    .unwrap();
```

`Player` persists across respawns and holds `character: Option<InstanceId>`,
`respawn_time`, `spawn_pose`, and `auto_spawn`. Its `player_id` is an application ID
(a string), distinct from its scene `InstanceId`. A `Humanoid` carries the same
`player_id` and owns Head, Torso, LeftArm, RightArm, LeftLeg, and RightLeg parts.
The pivot is at the torso center, three units above the feet. Characters are anchored
assemblies for now; movement input, walking physics, networking, and animation are
left for later. `pivot_to` moves and rotates the assembly while preserving limb sizes.

`HasPVInstance::pivot_to` applies a rigid pose change to every spatial descendant,
including those beneath non-spatial containers. `pivot_to_self` changes only the root.
Existing `with_pose`, `with_pivot`, `with_position`, and `with_orientation` builders
continue to affect only the instance itself. Custom spatial nodes should expose their
PV storage through `Instance::as_pv_instance` and `as_pv_instance_mut`, and implement
`HasPVInstance::spatial_children_mut` to return their mutable children.

Set `engine.current_camera.subject = Some(instance_id)` to follow any spatial node.
Set it to `None` for the existing WASD free-flight controls. The camera follows a
replacement character after respawn only while its subject still points at the old
character, so explicit free-flight and custom subjects persist. A missing subject
holds the camera's last pose. Orbit distance, pitch/yaw, limits, local target offset,
and zoom sensitivity are configurable in `Camera::follow`; projection and input
bindings retain their existing configuration APIs. Subject following ignores keyboard
flight input. Camera collision avoidance is not implemented yet.

Use `Terrarium::preset(WorkspacePreset::Empty)` to supply your own world geometry.
Add `.with_local_player(None)` to disable automatic player creation as well.
`Workspace::new()` remains a completely empty scene for low-level uses;
`Workspace::preset(...)` adds starting geometry, and `set_local_player(...)` configures
the local player separately. A workspace supplied directly to Bevy takes precedence
over the launcher's presets and player configuration.

Run `cargo run --example player` to try movement by pivot, respawning, and camera settings.
