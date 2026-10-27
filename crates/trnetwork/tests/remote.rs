#![cfg(all(feature = "server", feature = "websocket", not(target_family = "wasm")))]

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, time};
use tokio_tungstenite::{accept_async, tungstenite::Message};
use trnetwork::{Client, ClientError, ClientStatus, Server, TICK_DURATION};

async fn wait_for_tick(client: &mut Client, minimum: u64) -> u64 {
    time::timeout(Duration::from_secs(5), async {
        loop {
            client.step(Duration::ZERO).unwrap();
            if let Some(tick) = client.tick().filter(|&tick| tick >= minimum) {
                return tick;
            }
            time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("server tick was not received")
}

#[tokio::test(flavor = "current_thread")]
async fn remote_clients_receive_current_and_advancing_ticks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("ws://{}", listener.local_addr().unwrap());
    let mut server = Server::new();
    server.step(TICK_DURATION * 120);
    let server_task = tokio::spawn(server.serve(listener));

    let mut first = Client::connect(address.clone()).unwrap();
    assert!(!first.is_integrated());
    assert_eq!(first.tick(), None);
    assert_eq!(first.status(), ClientStatus::Connecting);
    let first_tick = wait_for_tick(&mut first, 120).await;
    assert_eq!(first.status(), ClientStatus::Connected);

    let mut second = Client::connect(address).unwrap();
    let second_tick = wait_for_tick(&mut second, first_tick).await;
    wait_for_tick(&mut first, second_tick + 2).await;
    wait_for_tick(&mut second, second_tick + 2).await;

    let last_tick = first.tick();
    first.step(Duration::from_secs(3600)).unwrap();
    assert!(first.tick().unwrap() < last_tick.unwrap() + 60);

    server_task.abort();
    let _ = server_task.await;
    time::timeout(Duration::from_secs(5), async {
        loop {
            if first.step(Duration::ZERO).is_err() {
                break;
            }
            time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("client did not observe server shutdown");
    assert_eq!(first.status(), ClientStatus::Disconnected);
    assert!(first.tick().unwrap() >= last_tick.unwrap());
    first.step(TICK_DURATION).unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn unsupported_tick_messages_disconnect_the_client() {
    for message in [vec![], b"TRT\x02\0\0\0\0\0\0\0\0".to_vec()] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("ws://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_async(stream).await.unwrap();
            socket.send(Message::Binary(message)).await.unwrap();
            let _ = socket.next().await;
        });
        let mut client = Client::connect(address).unwrap();
        let error = time::timeout(Duration::from_secs(5), async {
            loop {
                if let Err(error) = client.step(Duration::ZERO) {
                    return error;
                }
                time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("invalid tick was not rejected");
        assert!(matches!(error, ClientError::Protocol));
        assert_eq!(client.status(), ClientStatus::Disconnected);
        assert_eq!(client.tick(), None);
        server.abort();
    }
}
