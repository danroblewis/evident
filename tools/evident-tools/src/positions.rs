//! UTF-16 / char / byte position conversion for LSP.
//!
//! LSP positions are `(line, character)` with **0-based** line and a
//! **0-based UTF-16 code-unit** offset within the line (the default
//! `PositionEncodingKind`). Evident source is dense with multi-byte unicode
//! operators (`∈ ∀ ⇒ ↦ ¬ ∧ ∨ ⟨ ⟩`), every one of which is 3 UTF-8 bytes but
//! exactly one UTF-16 code unit — so naive byte or char arithmetic is the #1
//! LSP bug. This module is the single source of truth for all conversions.
//!
//! A `LineIndex` is built once per document text and answers conversions in
//! O(line length). It tracks, for each line, the byte offset of the line
//! start so byte↔(line,utf16) round-trips exactly. Line breaks recognised:
//! `\n` and `\r\n` (the `\r` is not counted as a column).

/// Precomputed line-start byte offsets for a document.
pub struct LineIndex {
    /// `line_starts[i]` = byte offset where line `i` begins (0-based line).
    /// Has `lines + 1` entries; the last is `text.len()` for EOF handling.
    line_starts: Vec<usize>,
    text: String,
}

impl LineIndex {
    pub fn new(text: &str) -> LineIndex {
        let mut line_starts = vec![0usize];
        let bytes = text.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'\n' {
                line_starts.push(i + 1);
            }
            i += 1;
        }
        line_starts.push(text.len());
        LineIndex {
            line_starts,
            text: text.to_string(),
        }
    }

    pub fn line_count(&self) -> usize {
        // last entry is the EOF sentinel
        self.line_starts.len().saturating_sub(1)
    }

    /// Byte range `[start, end)` of line `line0` (0-based), EXCLUDING the
    /// trailing `\n`. A trailing `\r` (CRLF) is also excluded.
    fn line_byte_range(&self, line0: usize) -> (usize, usize) {
        if line0 + 1 >= self.line_starts.len() {
            let s = *self.line_starts.last().unwrap();
            return (s, s);
        }
        let start = self.line_starts[line0];
        // end is the next line start minus the newline; the next start always
        // points just past a `\n`, so subtract 1 (and a `\r` if present).
        let mut end = self.line_starts[line0 + 1];
        if end > start {
            // strip the `\n`
            if self.text.as_bytes().get(end - 1) == Some(&b'\n') {
                end -= 1;
            }
            if end > start && self.text.as_bytes().get(end - 1) == Some(&b'\r') {
                end -= 1;
            }
        }
        (start, end)
    }

    /// LSP `(line0, utf16col)` → absolute byte offset in `text`.
    /// Out-of-range columns clamp to end-of-line; out-of-range lines clamp to
    /// end-of-text (matches what tolerant LSP servers do).
    pub fn lsp_to_byte(&self, line0: usize, utf16col: usize) -> usize {
        if line0 + 1 >= self.line_starts.len() {
            return self.text.len();
        }
        let (lstart, lend) = self.line_byte_range(line0);
        let line = &self.text[lstart..lend];
        let mut u16seen = 0usize;
        for (boff, c) in line.char_indices() {
            if u16seen >= utf16col {
                return lstart + boff;
            }
            u16seen += c.len_utf16();
        }
        lend
    }

    /// Absolute byte offset → LSP `(line0, utf16col)`.
    pub fn byte_to_lsp(&self, byte: usize) -> (usize, usize) {
        // binary search for the line whose start is <= byte < next start
        let byte = byte.min(self.text.len());
        // line0 = greatest i with line_starts[i] <= byte (excluding EOF sentinel)
        let mut lo = 0usize;
        let mut hi = self.line_starts.len().saturating_sub(1); // sentinel index
        while lo + 1 < hi {
            let mid = (lo + hi) / 2;
            if self.line_starts[mid] <= byte {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let line0 = lo;
        let lstart = self.line_starts[line0];
        let line_end = self
            .line_starts
            .get(line0 + 1)
            .copied()
            .unwrap_or(self.text.len());
        let slice_end = byte.min(line_end);
        let mut u16 = 0usize;
        if slice_end > lstart {
            for c in self.text[lstart..slice_end].chars() {
                if c == '\n' {
                    break;
                }
                u16 += c.len_utf16();
            }
        }
        (line0, u16)
    }

    /// Our lexer's 1-based `(line, char-col)` → LSP `(line0, utf16col)`.
    pub fn char_to_lsp(&self, line1: usize, col1: usize) -> (usize, usize) {
        let line0 = line1.saturating_sub(1);
        let (lstart, lend) = self.line_byte_range(line0);
        let line = &self.text[lstart..lend];
        let mut u16 = 0usize;
        for (i, c) in line.chars().enumerate() {
            if i + 1 >= col1 {
                break;
            }
            u16 += c.len_utf16();
        }
        (line0, u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_roundtrip() {
        // `x ∈ Int` — the ∈ is 3 bytes / 1 utf16 unit.
        let text = "x ∈ Int\ny ∈ Bool\n";
        let li = LineIndex::new(text);
        // byte offset of `Int`: "x ∈ " = 1 + 1 + 3 + 1 = 6 bytes
        let b_int = text.find("Int").unwrap();
        let (l, c) = li.byte_to_lsp(b_int);
        assert_eq!(l, 0);
        // utf16 col of Int: x(1) space(1) ∈(1) space(1) = 4
        assert_eq!(c, 4);
        // round-trip
        assert_eq!(li.lsp_to_byte(l, c), b_int);
    }

    #[test]
    fn second_line() {
        let text = "x ∈ Int\ny ∈ Bool\n";
        let li = LineIndex::new(text);
        let b_bool = text.find("Bool").unwrap();
        let (l, c) = li.byte_to_lsp(b_bool);
        assert_eq!(l, 1);
        assert_eq!(c, 4);
        assert_eq!(li.lsp_to_byte(l, c), b_bool);
    }

    #[test]
    fn crlf() {
        let text = "a\r\nbb\r\n";
        let li = LineIndex::new(text);
        let b_bb = text.find("bb").unwrap();
        let (l, c) = li.byte_to_lsp(b_bb);
        assert_eq!((l, c), (1, 0));
        assert_eq!(li.lsp_to_byte(1, 0), b_bb);
    }
}
