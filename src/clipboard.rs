/// A failure to initialize or access the clipboard.
#[derive(Debug, thiserror::Error)]
#[error("clipboard error: {0}")]
pub struct ClipboardError(String);

#[cfg(not(any(
    all(
        any(unix, windows),
        not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
    ),
    target_arch = "wasm32"
)))]
static FALLBACK_TEXT: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

/// Cross-platform access to clipboard text.
///
/// Desktop targets use the system clipboard. Keep this handle alive while its
/// contents are needed: on Linux, dropping the last handle can clear the clipboard.
/// Web targets use the browser Clipboard API, which requires a secure context
/// and may require permission and a user gesture. Other targets, including
/// Android and iOS, use an in-process text buffer shared by all handles.
///
/// ```no_run
/// use terrarium::{Clipboard, ClipboardError};
///
/// # async fn example() -> Result<(), ClipboardError> {
/// let mut clipboard = Clipboard::new()?;
/// clipboard.write_text("Hello!").await?;
/// let text = clipboard.read_text().await?;
/// # Ok(())
/// # }
/// ```
pub struct Clipboard {
    #[cfg(all(
        any(unix, windows),
        not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
    ))]
    inner: arboard::Clipboard,
}

impl Clipboard {
    /// Opens the system clipboard, or creates a web or in-process handle.
    ///
    /// Returns an error if the desktop clipboard cannot be initialized.
    pub fn new() -> Result<Self, ClipboardError> {
        Ok(Self {
            #[cfg(all(
                any(unix, windows),
                not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
            ))]
            inner: arboard::Clipboard::new().map_err(|e| ClipboardError(e.to_string()))?,
        })
    }

    /// Reads UTF-8 text, returning an error if access fails or text is unavailable.
    ///
    /// Desktop operations complete synchronously when polled; web operations
    /// await the browser's clipboard promise.
    pub async fn read_text(&mut self) -> Result<String, ClipboardError> {
        #[cfg(all(
            any(unix, windows),
            not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
        ))]
        {
            self.inner
                .get_text()
                .map_err(|e| ClipboardError(e.to_string()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            let text = wasm_bindgen_futures::JsFuture::from(web_clipboard()?.read_text())
                .await
                .map_err(|e| ClipboardError(format!("{e:?}")))?;
            text.as_string()
                .ok_or_else(|| ClipboardError("clipboard did not return text".into()))
        }
        #[cfg(not(any(
            all(
                any(unix, windows),
                not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
            ),
            target_arch = "wasm32"
        )))]
        {
            Ok(FALLBACK_TEXT
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .clone())
        }
    }

    /// Replaces the clipboard text, returning an error if access fails.
    ///
    /// On web, success means the browser's write promise has resolved.
    pub async fn write_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        #[cfg(all(
            any(unix, windows),
            not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
        ))]
        {
            self.inner
                .set_text(text)
                .map_err(|e| ClipboardError(e.to_string()))
        }
        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::JsFuture::from(web_clipboard()?.write_text(text))
                .await
                .map_err(|e| ClipboardError(format!("{e:?}")))?;
            Ok(())
        }
        #[cfg(not(any(
            all(
                any(unix, windows),
                not(any(target_os = "android", target_os = "ios", target_os = "emscripten"))
            ),
            target_arch = "wasm32"
        )))]
        {
            *FALLBACK_TEXT
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = text.to_owned();
            Ok(())
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn web_clipboard() -> Result<web_sys::Clipboard, ClipboardError> {
    use wasm_bindgen::JsValue;

    let clipboard = web_sys::window()
        .ok_or_else(|| ClipboardError("no browser window is available".into()))?
        .navigator()
        .clipboard();
    let value = JsValue::from(clipboard.clone());
    if value.is_undefined() || value.is_null() {
        return Err(ClipboardError("browser clipboard is unavailable".into()));
    }
    Ok(clipboard)
}
