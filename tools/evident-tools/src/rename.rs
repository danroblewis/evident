//! Token-accurate tree-wide rename.
//!
//! Renames every IDENTIFIER token whose base name == `old` (this catches BOTH
//! `old` AND the carry dual `_old`, since the lexer makes `_old` one token and
//! we split the leading underscore). It never touches:
//!   * a substring (`ctx_h` is untouched by renaming `x_h` — token boundaries),
//!   * string-literal contents or comments (those aren't Ident tokens),
//!   * a longer identifier that merely contains `old` (token equality, not
//!     substring).
//!
//! The dual is preserved: `old` → `new`, `_old` → `_new`.
//!
//! Collision detection: before applying, we scan for any existing identifier
//! token equal to `new` (base name). If found, we refuse unless --force,
//! because names-match composition would silently MERGE the two symbols.

use crate::lexer::{lex, Tok};
use std::path::{Path, PathBuf};

pub struct Edit {
    pub byte_start: usize,
    pub byte_end: usize,
    pub old_text: String,
    pub new_text: String,
    pub line: usize,
    pub col: usize,
}

pub struct FileEdits {
    pub path: PathBuf,
    pub edits: Vec<Edit>,
    pub new_content: String,
}

/// Validate that `new` is a syntactically legal identifier base name.
pub fn valid_ident(s: &str) -> bool {
    let mut ch = s.chars();
    match ch.next() {
        Some(c) if c == '_' || c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    ch.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Count existing occurrences of base name `name` across files (Ident tokens
/// only). Used for collision detection (target) and dry-run summary (source).
pub fn count_base(files: &[(PathBuf, String)], name: &str) -> usize {
    let mut n = 0;
    for (_p, src) in files {
        for t in lex(src) {
            if let Tok::Ident(w) = &t.tok {
                let base = w.strip_prefix('_').filter(|r| !r.is_empty()).unwrap_or(w);
                if base == name {
                    n += 1;
                }
            }
        }
    }
    n
}

/// Compute the per-file edits to rename base `old` → base `new`. The dual is
/// rewritten by replacing only the post-underscore portion, preserving the
/// leading `_`.
pub fn compute(files: &[(PathBuf, String)], old: &str, new: &str) -> Vec<FileEdits> {
    let mut result = Vec::new();
    for (path, src) in files {
        let mut edits: Vec<Edit> = Vec::new();
        for t in lex(src) {
            if let Tok::Ident(w) = &t.tok {
                let (lead, base) = if let Some(rest) = w.strip_prefix('_') {
                    if rest.is_empty() {
                        ("", w.as_str()) // bare `_` wildcard
                    } else {
                        ("_", rest)
                    }
                } else {
                    ("", w.as_str())
                };
                if base == old {
                    let new_text = format!("{lead}{new}");
                    edits.push(Edit {
                        byte_start: t.byte_start,
                        byte_end: t.byte_end,
                        old_text: w.clone(),
                        new_text,
                        line: t.line,
                        col: t.col,
                    });
                }
            }
        }
        if !edits.is_empty() {
            let new_content = apply_edits(src, &edits);
            result.push(FileEdits {
                path: path.clone(),
                edits,
                new_content,
            });
        }
    }
    result
}

fn apply_edits(src: &str, edits: &[Edit]) -> String {
    // edits are produced in source order by lex(); splice in order.
    let mut out = String::with_capacity(src.len());
    let mut cursor = 0usize;
    for e in edits {
        out.push_str(&src[cursor..e.byte_start]);
        out.push_str(&e.new_text);
        cursor = e.byte_end;
    }
    out.push_str(&src[cursor..]);
    out
}

pub fn write_back(fe: &FileEdits) -> std::io::Result<()> {
    std::fs::write(&fe.path, &fe.new_content)
}

/// Render a minimal unified-ish diff (line-level) for a file's edits.
pub fn diff_preview(path: &Path, fe: &FileEdits) -> String {
    let mut s = String::new();
    s.push_str(&format!("--- {}\n", path.display()));
    // Group edits by line for a readable preview.
    let mut by_line: std::collections::BTreeMap<usize, Vec<&Edit>> =
        std::collections::BTreeMap::new();
    for e in &fe.edits {
        by_line.entry(e.line).or_default().push(e);
    }
    for (ln, es) in by_line {
        let cols: Vec<String> = es
            .iter()
            .map(|e| format!("col {} {}→{}", e.col, e.old_text, e.new_text))
            .collect();
        s.push_str(&format!("  line {}: {}\n", ln, cols.join(", ")));
    }
    s
}
