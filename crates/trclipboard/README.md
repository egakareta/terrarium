# trclipboard

Cross-platform clipboard text access for Rust applications.

```rust,no_run
use trclipboard::{Clipboard, ClipboardError};

async fn copy_greeting() -> Result<(), ClipboardError> {
    let mut clipboard = Clipboard::new()?;
    clipboard.write_text("Hello!").await
}
```

Web clipboard access requires a secure context and may require permission and a
user gesture.
