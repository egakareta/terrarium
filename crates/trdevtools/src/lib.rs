#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

extern crate self as trdevtools;

mod window;

pub use trscene::devtools::*;
pub use window::{Devtools, SelectionProperty};
