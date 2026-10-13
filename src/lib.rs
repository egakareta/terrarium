#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// # Introduction
///
/// Terrarium is a simple, extensible 3D engine for Rust. It stands on two
/// foundations:
///
/// - [wgpu](https://github.com/gfx-rs/wgpu) for 3D graphics.
/// - [Tauri](https://tauri.app) and [React](https://react.dev) for windows and UI.
///
/// ## Motivation
///
/// Building a 3D application in Rust usually starts with an integration project.
///
/// Before the first triangle appears you have chosen a windowing layer, an event
/// loop, a renderer, a way to draw interface on top of it, and an asset pipeline,
/// and then wired them together.
///
/// Each choice is reasonable on its own, but the wiring is repeated by every app
/// and rarely resembles the interesting part of the work.
///
/// ### Core Features
///
/// Terrarium hands you what you need so you can focus on the game, not the machine:
///
/// - **Batteries included.** Terrarium is your audio, lighting, physics, windowing,
///   shading, scene graph and persistence.
///
/// - **Native and web, one code branch.** All Terrarium systems are carefully
///   designed to work anywhere.
///
/// - **Deterministic enough to test.** Run the full update and render path
///   headlessly. Don't let the complexity of your application get in the way of
///   testing it.
///
/// - **Interoperable by default.** The API is built in layers and meant to give you
///   the flexibility to integrate with other ecosystems.
///
/// ## Where to go next
///
/// - [Quick start](cookbook::quick_start) walks through creating your first project.
/// - [Examples.](https://github.com/egakareta/terrarium/tree/master/examples)
pub mod cookbook {
    #[doc = include_str!("../docs/quick_start.md")]
    pub mod quick_start {}
}

/// Workspace inspection and editing, available with the `devtools` feature.
#[cfg(feature = "devtools")]
pub use trdevtools as devtools;
#[cfg(all(feature = "dynamic_linking", not(target_family = "wasm")))]
#[doc(inline)]
pub use trdylib::*;
#[cfg(any(not(feature = "dynamic_linking"), target_family = "wasm"))]
#[doc(inline)]
pub use trinternal::*;
