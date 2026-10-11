# trpersistence

Local persistent key-value storage for Rust applications, backed by RenFS.

```rust,no_run
use trpersistence::LocalDataStore;

static PLAYER_DATA: LocalDataStore = LocalDataStore::new("my-game", "players");

fn save(coins: u32) -> Result<(), trpersistence::DataStoreError> {
    PLAYER_DATA.set("user:42", &coins)
}
```
