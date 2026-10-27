#[cfg(not(target_family = "wasm"))]
fn main() -> std::io::Result<()> {
    let address = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9001".to_owned());
    println!("Server listening on ws://{address}");
    trnetwork::Server::new().run(address)
}

#[cfg(target_family = "wasm")]
fn main() {}
