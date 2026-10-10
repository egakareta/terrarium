# Quick start

This guide assumes you are starting from scratch.

## 1. Install Rust

Terrarium is built in Rust. You can install Rust with any of those options:

- [rustup.](https://rustup.rs/)
- [mise (`mise use -g rust`).](https://mise.jdx.dev/installing-mise.html)
- [Docker.](https://hub.docker.com/_/rust)

Verify that Rust is correctly installed:

```sh
rustc --version
cargo --version
```

## 2. Install Terrarium

Create a Rust project:

```sh
cargo new awesome_game
cd awesome_game
```

Terrarium is directly accessible from [crates.io](https://crates.io/crates/terrarium). Add
it as a dependency:

```sh
cargo add terrarium
```

On Debian/Ubuntu, install native build dependencies if they are not already present:

```sh
sudo apt update
sudo apt install build-essential pkg-config libasound2-dev libjack-jackd2-dev libudev-dev
```

## 3. Create your first scene

Replace `src/main.rs` with:

```rust
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

Run the app:

```sh
cargo run
```
