#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(doctest)]
#[doc = include_str!("../docs/src/quick_start.md")]
pub struct Doctests;

/// Workspace inspection and editing, available with the `devtools` feature.
#[cfg(feature = "devtools")]
pub use trdevtools as devtools;
#[cfg(all(feature = "dynamic_linking", not(target_family = "wasm")))]
#[doc(inline)]
pub use trdylib::*;
#[cfg(any(not(feature = "dynamic_linking"), target_family = "wasm"))]
#[doc(inline)]
pub use trinternal::*;
