#![cfg(not(target_arch = "wasm32"))]

use terrarium::Clipboard;

struct PreservedClipboard {
    clipboard: Clipboard,
    original: Option<String>,
}

impl PreservedClipboard {
    fn new() -> Self {
        let mut clipboard = Clipboard::new().unwrap();
        let original = pollster::block_on(clipboard.read_text()).ok();
        Self {
            clipboard,
            original,
        }
    }
}

impl Drop for PreservedClipboard {
    fn drop(&mut self) {
        if let Some(text) = &self.original {
            let _ = pollster::block_on(self.clipboard.write_text(text));
        }
    }
}

#[test]
fn clipboard_text_can_be_read_and_replaced() {
    let mut original = PreservedClipboard::new();
    let original_text = "text before the round-trip test";
    pollster::block_on(original.clipboard.write_text(original_text)).unwrap();
    let mut reader = Clipboard::new().unwrap();
    {
        let mut clipboard = PreservedClipboard::new();
        pollster::block_on(async {
            for text in ["first", "Hello, 世界! 🌍\nsecond line", ""] {
                clipboard.clipboard.write_text(text).await.unwrap();
                assert_eq!(reader.read_text().await.unwrap(), text);
                let mut fresh_reader = Clipboard::new().unwrap();
                assert_eq!(fresh_reader.read_text().await.unwrap(), text);
            }
        });
    }
    assert_eq!(
        pollster::block_on(reader.read_text()).unwrap(),
        original_text
    );

    let failure = std::panic::catch_unwind(|| {
        let mut clipboard = PreservedClipboard::new();
        pollster::block_on(clipboard.clipboard.write_text("text before a panic")).unwrap();
        panic!("exercise clipboard cleanup during unwinding");
    });
    assert!(failure.is_err());
    assert_eq!(
        pollster::block_on(reader.read_text()).unwrap(),
        original_text
    );
}
