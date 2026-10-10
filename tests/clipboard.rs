#![cfg(all(
    any(unix, windows),
    not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
))]

use terrarium::Clipboard;

#[test]
fn clipboard_text_can_be_read_and_replaced() {
    let mut clipboard = Clipboard::new().unwrap();
    let mut reader = Clipboard::new().unwrap();
    pollster::block_on(async {
        for text in ["first", "Hello, 世界! 🌍\nsecond line", ""] {
            clipboard.write_text(text).await.unwrap();
            assert_eq!(reader.read_text().await.unwrap(), text);
        }
    });
}
