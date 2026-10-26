#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

mod terrarium {
    #[cfg(all(
        feature = "dynamic_linking",
        not(feature = "no_dynamic_linking"),
        not(target_family = "wasm"),
    ))]
    #[doc(inline)]
    pub use trdylib::*;
    #[cfg(any(
        not(feature = "dynamic_linking"),
        feature = "no_dynamic_linking",
        target_family = "wasm"
    ))]
    #[doc(inline)]
    pub use trinternal::*;
}

pub use terrarium::*;

#[cfg(doctest)]
#[doc = include_str!("../docs/src/instance.md")]
#[doc = include_str!("../docs/src/quick_start.md")]
pub struct Doctests;
