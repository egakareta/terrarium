#![cfg(all(feature = "websocket", target_family = "wasm", not(target_os = "wasi")))]

use std::time::Duration;

use trnetwork::{Client, ClientStatus};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

async fn wait_for_tick(client: &mut Client, minimum: u64) -> u64 {
    for _ in 0..500 {
        client.step(Duration::ZERO).unwrap();
        if let Some(tick) = client.tick().filter(|&tick| tick >= minimum) {
            return tick;
        }
        let timeout = js_sys::Promise::new(&mut |resolve, _| {
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, 10)
                .unwrap();
        });
        wasm_bindgen_futures::JsFuture::from(timeout).await.unwrap();
    }
    panic!("server tick was not received");
}

#[wasm_bindgen_test]
async fn browser_receives_ticks_from_standalone_server() {
    let mut client = Client::connect("ws://127.0.0.1:9001").unwrap();
    let first_tick = wait_for_tick(&mut client, 0).await;
    assert_eq!(client.status(), ClientStatus::Connected);
    assert!(!client.is_integrated());
    let next_tick = wait_for_tick(&mut client, first_tick + 2).await;
    assert!(next_tick > first_tick);
}
