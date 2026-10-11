#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

use std::future::Future;

/// Schedules a future to run to completion without blocking the caller.
///
/// On native targets, the future runs on a dedicated thread and must be `Send`.
/// On wasm32, it is scheduled locally, does not need to be `Send`, and must not
/// block the browser's event loop.
#[cfg(not(target_arch = "wasm32"))]
pub fn spawn_background<F>(future: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    let _ = std::thread::spawn(move || pollster::block_on(future));
}

/// Schedules a future to run to completion without blocking the caller.
///
/// On native targets, the future runs on a dedicated thread and must be `Send`.
/// On wasm32, it is scheduled locally, does not need to be `Send`, and must not
/// block the browser's event loop.
#[cfg(target_arch = "wasm32")]
pub fn spawn_background<F>(future: F)
where
    F: Future<Output = ()> + 'static,
{
    wasm_bindgen_futures::spawn_local(future);
}
