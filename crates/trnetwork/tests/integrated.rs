use std::time::Duration;

use trnetwork::{Client, ClientStatus, Server, TICK_DURATION};

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[cfg_attr(not(target_family = "wasm"), test)]
#[cfg_attr(
    all(target_family = "wasm", not(target_os = "wasi")),
    wasm_bindgen_test::wasm_bindgen_test
)]
fn integrated_client_receives_fixed_ticks_across_partial_frames() {
    let mut client = Client::default();
    assert!(client.is_integrated());
    assert_eq!(client.status(), ClientStatus::Connected);
    assert_eq!(client.tick(), Some(0));

    client
        .step(TICK_DURATION - Duration::from_nanos(1))
        .unwrap();
    assert_eq!(client.tick(), Some(0));
    client.step(Duration::from_nanos(1)).unwrap();
    assert_eq!(client.tick(), Some(1));
    client.step(TICK_DURATION * 10).unwrap();
    assert_eq!(client.tick(), Some(11));
    client.step(Duration::ZERO).unwrap();
    assert_eq!(client.tick(), Some(11));
}

#[cfg_attr(not(target_family = "wasm"), test)]
#[cfg_attr(
    all(target_family = "wasm", not(target_os = "wasi")),
    wasm_bindgen_test::wasm_bindgen_test
)]
fn frame_partitioning_preserves_server_time() {
    let mut whole = Server::new();
    let mut partitioned = Server::new();
    whole.step(Duration::from_secs(1));
    for _ in 0..100 {
        partitioned.step(Duration::from_millis(10));
    }
    assert_eq!(whole.tick(), 60);
    assert_eq!(partitioned.tick(), whole.tick());

    whole.step(TICK_DURATION);
    partitioned.step(TICK_DURATION);
    assert_eq!(partitioned.tick(), whole.tick());
}
