# trnetwork

A tick-only prototype of Terrarium's client/server boundary. Scenes and physics
still run locally; only the server tick crosses the connection.

`Client::default()` owns an integrated `Server` and works on native and browser
targets without sockets or background threads. Call `Client::step(elapsed)` each
frame and read `Client::tick()`. The server advances at `TICK_DURATION` (about
60 Hz), independently of frame rate.

Enable `websocket` to use `Client::connect("ws://127.0.0.1:9001")` on native or
web. Connection establishment is asynchronous; `Client::status()` distinguishes
connecting, connected, and disconnected, and the tick is absent until received.
Remote clients only poll messages; their frame durations never advance the
remote server. HTTPS pages require a `wss://` endpoint.

Run a standalone native server, without the renderer:

```sh
cargo run -p trnetwork --features server --example server -- 127.0.0.1:9001
```

The native-only `server` feature provides `Server::run` and `Server::serve`.
New connections receive the current tick immediately, followed by updates.
The wire message is the four bytes `TRT\x01` followed by a little-endian `u64`.

The root `network` feature enables WebSocket clients through `terrarium`.
`Engine` creates an integrated client by default; use
`engine.set_client(Client::connect(url)?)` to replace it with a remote connection.

```sh
cargo run --example network
cargo run --example network --features network
CARGO_UNSTABLE_BUILD_STD=std,panic_abort trunk serve --example network --features network
```

The example displays the tick and can switch between integrated and remote
connections. Browser builds include the integrated server and WebSocket client;
the native listener is excluded even with all features enabled.

```sh
CARGO_UNSTABLE_BUILD_STD=std,panic_abort cargo check -p trnetwork --target wasm32-unknown-unknown
CARGO_UNSTABLE_BUILD_STD=std,panic_abort cargo check -p trnetwork --all-features --target wasm32-unknown-unknown
```

Run `cargo test -p trnetwork --all-features` for native connection tests. With
the standalone server running on port 9001, run the browser tests with the same
standard-library configuration as the workspace's WASM tasks:

```sh
CARGO_UNSTABLE_BUILD_STD=std,panic_abort wasm-pack test --headless --chrome crates/trnetwork --all-features
```

CI runs both and checks the web feature combinations.
