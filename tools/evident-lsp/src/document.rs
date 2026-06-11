//! In-memory document store with incremental text synchronisation.
//!
//! Holds the authoritative text of every open buffer, applying LSP
//! `TextDocumentContentChangeEvent`s. Each change carries an optional `range`
//! (UTF-16 LSP coords) and the replacement text; a change with no range is a
//! full-document replacement. We splice by byte offset, using a `LineIndex`
//! built from the *current* text, so multi-byte unicode operators never
//! corrupt the offset math (the #1 LSP bug).

use evident_tools::positions::LineIndex;
use std::collections::HashMap;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, Url};

#[derive(Default)]
pub struct DocStore {
    docs: HashMap<Url, String>,
}

impl DocStore {
    pub fn new() -> Self {
        DocStore::default()
    }

    pub fn open(&mut self, uri: Url, text: String) {
        self.docs.insert(uri, text);
    }

    pub fn close(&mut self, uri: &Url) {
        self.docs.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<&String> {
        self.docs.get(uri)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Url, &String)> {
        self.docs.iter()
    }

    /// Apply a batch of content changes to `uri`. Each change is applied in
    /// order against the running text. Returns the new text (also stored), or
    /// `None` if the document is not open.
    pub fn apply_changes(
        &mut self,
        uri: &Url,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) -> Option<&String> {
        let text = self.docs.get_mut(uri)?;
        for ch in changes {
            match ch.range {
                None => {
                    // full replacement
                    *text = ch.text;
                }
                Some(range) => {
                    let li = LineIndex::new(text);
                    let start = li.lsp_to_byte(
                        range.start.line as usize,
                        range.start.character as usize,
                    );
                    let end = li.lsp_to_byte(
                        range.end.line as usize,
                        range.end.character as usize,
                    );
                    let (lo, hi) = if start <= end { (start, end) } else { (end, start) };
                    let mut new = String::with_capacity(text.len() + ch.text.len());
                    new.push_str(&text[..lo]);
                    new.push_str(&ch.text);
                    new.push_str(&text[hi..]);
                    *text = new;
                }
            }
        }
        Some(&*text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn url() -> Url {
        Url::parse("file:///t.ev").unwrap()
    }

    fn change(range: Option<Range>, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range,
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn incremental_insert_after_unicode() {
        let mut s = DocStore::new();
        s.open(url(), "x ∈ Int\n".to_string());
        // insert " = 5" before the newline: line 0, char col after "Int" = 7
        let r = Range::new(Position::new(0, 7), Position::new(0, 7));
        let out = s.apply_changes(&url(), vec![change(Some(r), " = 5")]).unwrap();
        assert_eq!(out, "x ∈ Int = 5\n");
    }

    #[test]
    fn incremental_replace_range() {
        let mut s = DocStore::new();
        s.open(url(), "abc ∈ Int\n".to_string());
        // replace "abc" (cols 0..3) with "xy"
        let r = Range::new(Position::new(0, 0), Position::new(0, 3));
        let out = s.apply_changes(&url(), vec![change(Some(r), "xy")]).unwrap();
        assert_eq!(out, "xy ∈ Int\n");
    }

    #[test]
    fn full_replacement() {
        let mut s = DocStore::new();
        s.open(url(), "old".to_string());
        let out = s.apply_changes(&url(), vec![change(None, "new text")]).unwrap();
        assert_eq!(out, "new text");
    }
}
