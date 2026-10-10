#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// # Introduction
///
/// Terrarium is a simple, extensible 3D engine for Rust. It stands on two
/// foundations:
///
/// - [wgpu](https://github.com/gfx-rs/wgpu) for 3D graphics.
/// - [eframe](https://github.com/emilk/egui) for 2D graphics.
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

// re-exports
pub use eframe::{self, egui, egui_wgpu, wgpu};
#[cfg(feature = "sound")]
pub use euphorium;
pub use glam;
#[cfg(feature = "physics")]
pub use rapier3d;
pub use winit;

// actual stuff
#[cfg(feature = "bevy")]
pub mod bevy;
mod clipboard;
mod engine;
mod instance;
mod interpolation;
#[cfg(feature = "persistence")]
mod persistence;
mod primitive;
mod renderer;
mod scene;

pub use clipboard::*;
pub use engine::*;
pub use instance::*;
pub use interpolation::*;
#[cfg(feature = "persistence")]
pub use persistence::*;
pub use primitive::*;
pub use renderer::*;
pub use scene::*;
