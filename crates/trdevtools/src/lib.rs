#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

extern crate self as trdevtools;

mod font_market;
mod window;

pub use trscene::devtools::*;
pub use window::{Devtools, DevtoolsHost, SelectionProperty};
