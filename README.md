<div align="center">

# Terrarium 🌍

</div>

`terrarium` is a simple, highly extensible 3D engine built around two core crates:

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
