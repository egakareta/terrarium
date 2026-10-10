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

Signals can be consumed by application-owned state without callback captures.
Store a receiver in your app and drain it in `App::after_update`:

```rust
use terrarium::{Instance, Part, Workspace};

let mut workspace = Workspace::new();
let mut removed = workspace.on_child_removed().receive();
let id = workspace.add_child(
    Part::new().with_name("Enemy").with_attribute("score", serde_json::json!(10)),
);
workspace.remove_child(id);
workspace.update(0.0);
for snapshot in removed.drain() {
    // The metadata is still available after the object has been destroyed.
    println!("Removed {}: {:?}", snapshot.name, snapshot.attributes);
}
```

`connect` and `once` return guards that disconnect on drop. Store the guard, or
explicitly call `.detach()` when the signal owner's lifetime should own the
callback. Receivers disconnect automatically when dropped. See the physics
example for an app that counts contact transitions using ordinary fields.

Property and attribute signals are invalidations: repeated changes to the same
property or attribute coalesce until dispatch, and handlers read the latest
value. Built-in transform and part setters are observed; direct edits to public
material-slot storage and camera, light, and sound settings are not observed.
Lifecycle events preserve owned name, class, parent, attributes, and child IDs.

The callback queue and each receiver hold at most 4,096 entries. Normal updates
drain the existing callback backlog; nested callback cascades have a bounded
dispatch budget. Cancelled calls do not consume that budget. A full callback
queue rejects an entire user-fired emission with `EventQueueFull`; dispatch
before retrying. Built-in emissions rejected at capacity are counted by
`Signal::missed_emissions` and logged at dispatch. Receivers retain the newest
values when unread data exceeds their capacity, and expose a cumulative
`missed_emissions` counter for both kinds of overflow.
