use std::{io, net::ToSocketAddrs};

use futures_util::{SinkExt, StreamExt};
use tokio::{net::TcpListener, sync::watch, task::JoinSet, time};
use tokio_tungstenite::{accept_async, tungstenite::Message};

use crate::{Server, TICK_DURATION, encode_tick};

impl Server {
    /// Runs a standalone WebSocket server until its listener fails.
    ///
    /// This blocks the current thread and starts no graphics or audio services.
    /// Available on native targets with the `server` feature.
    pub fn run(self, address: impl ToSocketAddrs) -> io::Result<()> {
        let listener = std::net::TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async { self.serve(TcpListener::from_std(listener)?).await })
    }

    /// Publishes ticks to WebSocket clients using the current Tokio runtime.
    ///
    /// Each new client receives the current tick before subsequent updates.
    /// Dropping this future disconnects its clients. Available on native targets
    /// with the `server` feature.
    pub async fn serve(mut self, listener: TcpListener) -> io::Result<()> {
        let (ticks, _) = watch::channel(self.tick());
        let mut clients = JoinSet::new();
        let mut interval = time::interval(TICK_DURATION);
        interval.tick().await;

        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;
                    stream.set_nodelay(true)?;
                    let ticks = ticks.subscribe();
                    clients.spawn(serve_client(stream, ticks));
                }
                _ = interval.tick() => {
                    ticks.send_replace(self.step(TICK_DURATION));
                }
                Some(_) = clients.join_next(), if !clients.is_empty() => {}
            }
        }
    }
}

async fn serve_client(
    stream: tokio::net::TcpStream,
    mut ticks: watch::Receiver<u64>,
) -> Result<(), tokio_tungstenite::tungstenite::Error> {
    let mut socket = accept_async(stream).await?;
    let tick = *ticks.borrow_and_update();
    socket.send(Message::Binary(encode_tick(tick))).await?;

    loop {
        tokio::select! {
            changed = ticks.changed() => {
                if changed.is_err() {
                    return Ok(());
                }
                let tick = *ticks.borrow_and_update();
                socket.send(Message::Binary(encode_tick(tick))).await?;
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Ping(_))) => socket.flush().await?,
                    Some(Ok(Message::Pong(_))) => {}
                    _ => {
                        let _ = socket.close(None).await;
                        return Ok(());
                    }
                }
            }
        }
    }
}
