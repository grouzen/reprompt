pub mod app;
pub mod ollama;
pub mod prompt;
pub mod view;

use arboard::Clipboard;
use std::error::Error;

/// Copies the given text to the system clipboard
pub fn copy_to_clipboard(text: &str) -> Result<(), Box<dyn Error>> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}

#[macro_export]
macro_rules! assign_if_some {
    ($target:ident, $expr:expr) => {
        if let val @ Some(_) = $expr {
            $target = val;
        }
    };
}
