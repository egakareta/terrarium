<div align="center">

# Terrarium 🌍

</div>

`terrarium` is a simple, extensible 3D engine built around two core crates:

- [wgpu](https://github.com/gfx-rs/wgpu) for 3D graphics
- [eframe](https://github.com/emilk/egui) for 2D graphics

Here's an example of how Terrarium can render a 1x1x1 cube.

```rust,no_run
use terrarium::*;

fn main() {
    Terrarium::new()
        .run(|engine| {
            engine.add_child(Part::new());
            Ok::<(), AppCreationError>(())
        })
        .unwrap();
}
```

Please check out [examples](https://github.com/egakareta/terrarium/tree/master/examples) to
learn more.

`Terrarium::new()` starts with an empty workspace. To opt into a starting floor,
use a workspace preset:

```rust,no_run
use terrarium::*;

Terrarium::preset(WorkspacePreset::Baseplate)
    .run(|engine| {
        engine.add_child(Part::new().with_position(glam::Vec3::new(0.0, 0.5, 0.0)));
        Ok::<(), AppCreationError>(())
    })
    .unwrap();
```

`WorkspacePreset::Empty` is the default and keeps the original empty-scene behavior.
`Baseplate` adds an anchored, collidable 512 by 512 plate whose top is at world Y = 0,
before the initializer runs. For a standalone scene, use `Workspace::preset(...)`;
`Workspace::new()` and `Workspace::default()` remain empty. A workspace supplied
directly to Bevy takes precedence over the launcher's preset.
