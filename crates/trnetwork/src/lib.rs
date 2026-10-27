#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

use std::time::Duration;

#[cfg(all(feature = "server", not(target_family = "wasm")))]
mod server;

/// The fixed interval between server ticks, approximately 60 Hz.
pub const TICK_DURATION: Duration = Duration::from_nanos(1_000_000_000 / 60);

/// An authoritative server with a fixed update interval.
#[derive(Debug, Default)]
pub struct Server {
    tick: u64,
    remainder: Duration,
}

impl Server {
    /// Creates a server at tick zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current authoritative tick.
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// Advances by elapsed time, retaining partial ticks for the next step.
    ///
    /// The tick saturates at [`u64::MAX`].
    pub fn step(&mut self, elapsed: Duration) -> u64 {
        let total = self.remainder.saturating_add(elapsed).as_nanos();
        let ticks = total / TICK_DURATION.as_nanos();
        self.remainder = Duration::from_nanos((total % TICK_DURATION.as_nanos()) as u64);
        self.tick = self
            .tick
            .saturating_add(u64::try_from(ticks).unwrap_or(u64::MAX));
        self.tick
    }
}

/// The state of a client's connection to its server.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClientStatus {
    /// A remote connection is being established.
    Connecting,
    /// The integrated or remote server is connected.
    Connected,
    /// The remote connection closed or failed.
    Disconnected,
}

/// A failure to connect to or communicate with a server.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The WebSocket transport failed.
    #[error("server connection failed: {0}")]
    Transport(String),
    /// The server sent an unsupported tick message.
    #[error("invalid server tick message")]
    Protocol,
    /// The server closed the connection.
    #[error("server disconnected")]
    Disconnected,
}

enum Connection {
    Integrated(Server),
    #[cfg(feature = "websocket")]
    Remote {
        sender: ewebsock::WsSender,
        receiver: ewebsock::WsReceiver,
        status: ClientStatus,
    },
}

/// A client connected to an integrated or remote server.
///
/// The default connection owns an integrated server. With `websocket` enabled,
/// `Client::connect` creates a remote connection instead.
pub struct Client {
    connection: Connection,
    tick: Option<u64>,
}

impl Default for Client {
    fn default() -> Self {
        Self {
            connection: Connection::Integrated(Server::new()),
            tick: Some(0),
        }
    }
}

impl Client {
    /// Starts a WebSocket connection without blocking for the handshake.
    ///
    /// Poll with [`Self::step`]. The tick remains absent until the server sends
    /// its first message. Both `ws://` and `wss://` URLs are supported.
    #[cfg(feature = "websocket")]
    pub fn connect(url: impl Into<String>) -> Result<Self, ClientError> {
        let (sender, receiver) = ewebsock::connect(
            url,
            ewebsock::Options {
                max_incoming_frame_size: 1024,
                ..Default::default()
            },
        )
        .map_err(ClientError::Transport)?;
        Ok(Self {
            connection: Connection::Remote {
                sender,
                receiver,
                status: ClientStatus::Connecting,
            },
            tick: None,
        })
    }

    /// Returns the last received tick, or `None` before a remote server sends one.
    ///
    /// The last tick remains available after disconnection.
    pub fn tick(&self) -> Option<u64> {
        self.tick
    }

    /// Returns the connection's current state.
    pub fn status(&self) -> ClientStatus {
        match &self.connection {
            Connection::Integrated(_) => ClientStatus::Connected,
            #[cfg(feature = "websocket")]
            Connection::Remote { status, .. } => *status,
        }
    }

    /// Returns whether this client owns an integrated server.
    pub fn is_integrated(&self) -> bool {
        matches!(self.connection, Connection::Integrated(_))
    }

    /// Advances an integrated server or polls a remote server without blocking.
    ///
    /// Remote connections ignore `elapsed`. Failures mark the connection as
    /// disconnected and are returned once; subsequent steps do nothing.
    pub fn step(&mut self, elapsed: Duration) -> Result<(), ClientError> {
        match &mut self.connection {
            Connection::Integrated(server) => self.tick = Some(server.step(elapsed)),
            #[cfg(feature = "websocket")]
            Connection::Remote {
                sender,
                receiver,
                status,
            } => {
                if *status == ClientStatus::Disconnected {
                    return Ok(());
                }
                while let Some(event) = receiver.try_recv() {
                    let error = match event {
                        ewebsock::WsEvent::Opened => {
                            *status = ClientStatus::Connected;
                            continue;
                        }
                        ewebsock::WsEvent::Message(ewebsock::WsMessage::Binary(bytes)) => {
                            match decode_tick(&bytes) {
                                Ok(tick) => {
                                    self.tick = Some(tick);
                                    continue;
                                }
                                Err(error) => error,
                            }
                        }
                        ewebsock::WsEvent::Message(
                            ewebsock::WsMessage::Ping(_) | ewebsock::WsMessage::Pong(_),
                        ) => continue,
                        ewebsock::WsEvent::Message(_) => ClientError::Protocol,
                        ewebsock::WsEvent::Error(error) => ClientError::Transport(error),
                        ewebsock::WsEvent::Closed => ClientError::Disconnected,
                    };
                    *status = ClientStatus::Disconnected;
                    sender.close();
                    return Err(error);
                }
            }
        }
        Ok(())
    }
}

#[cfg(all(feature = "server", not(target_family = "wasm")))]
fn encode_tick(tick: u64) -> Vec<u8> {
    let mut message = b"TRT\x01".to_vec();
    message.extend_from_slice(&tick.to_le_bytes());
    message
}

#[cfg(feature = "websocket")]
fn decode_tick(message: &[u8]) -> Result<u64, ClientError> {
    if message.len() != 12 || &message[..4] != b"TRT\x01" {
        return Err(ClientError::Protocol);
    }
    let bytes = message[4..].try_into().map_err(|_| ClientError::Protocol)?;
    Ok(u64::from_le_bytes(bytes))
}
