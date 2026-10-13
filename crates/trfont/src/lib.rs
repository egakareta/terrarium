#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

/// Name of the bundled proportional font.
pub const PROPORTIONAL_FONT_NAME: &str = "Outfit";
/// Name of the bundled monospace font.
pub const MONOSPACE_FONT_NAME: &str = "SUSEMono";
/// Bundled proportional font bytes for host font registration.
#[cfg(feature = "default-fonts")]
pub const PROPORTIONAL_FONT: &[u8] = include_bytes!("bin/Outfit-VariableFont_wght.ttf");
/// Bundled monospace font bytes for host font registration.
#[cfg(feature = "default-fonts")]
pub const MONOSPACE_FONT: &[u8] = include_bytes!("bin/SUSEMono-VariableFont_wght.ttf");
