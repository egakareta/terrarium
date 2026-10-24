#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

#[cfg(feature = "devtools")]
mod devtools;
#[cfg(feature = "engine")]
mod engine;
#[cfg(feature = "scene")]
mod scene;

/// Defines a composed instance and generates its APIs.
///
/// See the [instance composition guide](https://egakareta.github.io/terrarium/docs/instance.html)
/// for the field roles, property options, generated methods, overrides, and examples.
///
/// `#[property(deref)]` borrows a property's [`std::ops::Deref::Target`] without cloning it.
#[cfg(feature = "scene")]
#[proc_macro]
pub fn instance(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    scene::instance(input)
}

/// Continues expansion after a composed type supplies its public schema.
#[cfg(feature = "scene")]
#[doc(hidden)]
#[proc_macro]
pub fn __instance_expand(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    scene::__instance_expand(input)
}

/// Derives field inspection for named structs and enums with unit variants.
#[cfg(feature = "devtools")]
#[proc_macro_derive(Inspect, attributes(inspect))]
pub fn inspect(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    devtools::inspect(input)
}

/// Makes `workspace` shorthand for per-use access to an engine's mutable workspace.
///
/// The function must have one `&mut Engine` parameter. Use `#[with_workspace(name)]`
/// to select a parameter explicitly, including when its type is an alias. Methods
/// can also select `self` explicitly.
///
/// ```no_run
/// use trengine::{*, trscene::*};
///
/// #[with_workspace]
/// fn initialize(engine: &mut Engine) {
///     workspace.add_child(Part::new());
///     engine.set_clear_color([0.0, 0.0, 0.0, 1.0]);
///     workspace.lighting.with_clock_time(18.0);
/// }
/// ```
///
/// Local `workspace` bindings shadow the shorthand. Nested items are not rewritten.
/// Standard assertion, formatting, and `vec!` macro expressions are supported;
/// custom macro syntax and implicit format-string captures are not rewritten.
/// Normal Rust borrowing rules still apply to references retained across uses.
///
/// ```compile_fail
/// use trmacros::with_workspace;
///
/// #[with_workspace]
/// fn missing_engine() {}
/// ```
///
/// ```compile_fail
/// use trmacros::with_workspace;
/// struct Engine;
///
/// #[with_workspace]
/// fn ambiguous(first: &mut Engine, second: &mut Engine) {}
/// ```
#[cfg(feature = "engine")]
#[proc_macro_attribute]
pub fn with_workspace(
    attribute: proc_macro::TokenStream,
    item: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    engine::with_workspace(attribute, item)
}
