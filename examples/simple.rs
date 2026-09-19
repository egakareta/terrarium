//! The bare minimum to get a cube rendering in Terrarium.
//!
//! If you cloned the repo, you can run this in:
//! - your native platform. `cargo run --example simple`
//! - a browser at http://localhost:8080. `trunk serve --example simple`
//!
//! By default, Terrarium gives you a camera, a skybox and shadows.
//! Use WASD and left click drag to move the camera.
//!
//! When you are done, check out `examples/app.rs` for more advanced usage!

use terrarium::*;

fn main() {
    Terrarium::new()
        .run(|engine| {
            // Create a cube
            let mut part = Part::new();
            part.size = glam::Vec3::new(2.0, 2.0, 2.0);

            // Add it into the scene
            engine.add_child(part);

            // Finish initializing :)
            Ok::<(), AppCreationError>(())
        })
        .unwrap();
}
