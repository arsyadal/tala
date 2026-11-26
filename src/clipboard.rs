#[derive(Default)]
pub struct Clipboard(Option<arboard::Clipboard>);

impl Clipboard {
    fn native(&mut self) -> Result<&mut arboard::Clipboard, String> {
        if self.0.is_none() {
            self.0 = Some(arboard::Clipboard::new().map_err(|error| format!("Clipboard: {error}"))?);
        }
        Ok(self.0.as_mut().expect("clipboard initialized"))
    }

    pub fn copy(&mut self, text: String) -> Result<(), String> {
        self.native()?.set_text(text).map_err(|error| format!("Copy: {error}"))
    }

    pub fn paste(&mut self) -> Result<String, String> {
        self.native()?.get_text().map_err(|error| format!("Paste: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::Clipboard;

    #[test]
    #[ignore = "requires a dedicated desktop clipboard; run explicitly in CI"]
    fn desktop_round_trip() {
        let mut clipboard = Clipboard::default();
        let previous = clipboard.paste().ok();
        let text = "Tala clipboard · テスト";
        clipboard.copy(text.into()).unwrap();
        let pasted = clipboard.paste();
        if let Some(previous) = previous {
            clipboard.copy(previous).unwrap();
        }
        assert_eq!(pasted.unwrap(), text);
    }
}
