//! Tokenizer for Evident source. Matches compiler/lexer.ev's lexical
//! grammar: ASCII identifiers `[A-Za-z_][A-Za-z0-9_]*` (underscore is an
//! identifier char — `_x` is ONE token, the carry dual), `"..."` string
//! literals with `\` escapes, `--` line comments, numeric literals, and
//! the unicode + multi-char ASCII operators as single tokens.
//!
//! Columns are counted in Unicode scalar values (chars), 1-based, so a
//! `∈` counts as one column even though it is 3 UTF-8 bytes. Byte offsets
//! are also tracked so a token-accurate rewrite can splice the original
//! source without re-encoding.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// ASCII identifier or keyword. Keyword classification is left to the
    /// parser layer (matches the .ev lexer's "recognized post-collection").
    Ident(String),
    Int(String),
    Float(String),
    Str(String),
    /// `--` comment text (without the leading `--`, includes nothing past EOL).
    Comment(String),
    /// Any operator / punctuation token, stored as its source text
    /// (e.g. "∈", "↦", "++", "<=", "(", ".", "=").
    Op(String),
    Newline,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,    // 1-based
    pub col: usize,     // 1-based, in chars
    pub byte_start: usize,
    pub byte_end: usize,
}

impl Token {
    /// The identifier text if this token is an identifier, else None.
    pub fn ident(&self) -> Option<&str> {
        match &self.tok {
            Tok::Ident(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn op_is(&self, s: &str) -> bool {
        matches!(&self.tok, Tok::Op(o) if o == s)
    }
}

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_ascii_alphabetic()
}
fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_ascii_alphanumeric()
}

/// Single-char operator chars recognized by SingleCharTok in lexer.ev,
/// plus the bracket/brace family and `‖`/`⟦`/`⟧`/`∉` extras the prose
/// references. Multi-char ASCII ops are handled separately.
const OP_CHARS: &str = "(),+*/-=<>.|:?#∈⇒⟨⟩↦≤≥≠∧∨¬∀∃[]{}∉‖⟦⟧";

pub fn lex(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    // byte offset of each char index, plus a final sentinel == src.len().
    let mut byte_of: Vec<usize> = Vec::with_capacity(chars.len() + 1);
    {
        let mut b = 0;
        for c in &chars {
            byte_of.push(b);
            b += c.len_utf8();
        }
        byte_of.push(src.len());
    }

    let mut out = Vec::new();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;
    let n = chars.len();

    let push_simple = |out: &mut Vec<Token>, tok: Tok, line, col, bs, be| {
        out.push(Token { tok, line, col, byte_start: bs, byte_end: be });
    };

    while i < n {
        let c = chars[i];
        let start_col = col;
        let bs = byte_of[i];

        if c == '\n' {
            push_simple(&mut out, Tok::Newline, line, start_col, bs, byte_of[i + 1]);
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if c == ' ' || c == '\t' || c == '\r' {
            i += 1;
            col += 1;
            continue;
        }

        // Line comment: `--` ... EOL.
        if c == '-' && i + 1 < n && chars[i + 1] == '-' {
            let cstart = i + 2;
            let mut j = cstart;
            while j < n && chars[j] != '\n' {
                j += 1;
            }
            let text: String = chars[cstart..j].iter().collect();
            push_simple(&mut out, Tok::Comment(text), line, start_col, bs, byte_of[j]);
            col += j - i;
            i = j;
            continue;
        }

        // String literal with `\` escapes. We keep the *contents* but the
        // byte span covers the surrounding quotes so a rename never lands
        // inside a string (we never treat string contents as identifiers).
        if c == '"' {
            let mut j = i + 1;
            let mut content = String::new();
            while j < n {
                let cj = chars[j];
                if cj == '\\' && j + 1 < n {
                    content.push(chars[j + 1]);
                    j += 2;
                    continue;
                }
                if cj == '"' {
                    j += 1;
                    break;
                }
                content.push(cj);
                j += 1;
            }
            push_simple(&mut out, Tok::Str(content), line, start_col, bs, byte_of[j]);
            col += j - i;
            i = j;
            continue;
        }

        // Identifier / underscore-leading carry dual.
        if is_ident_start(c) {
            let mut j = i + 1;
            while j < n && is_ident_continue(chars[j]) {
                j += 1;
            }
            let text: String = chars[i..j].iter().collect();
            push_simple(&mut out, Tok::Ident(text), line, start_col, bs, byte_of[j]);
            col += j - i;
            i = j;
            continue;
        }

        // Numeric literal (Int or Float). A leading digit only; `0 - 1`
        // for negatives is handled as Int + Op('-') + Int by the grammar.
        if c.is_ascii_digit() {
            let mut j = i + 1;
            while j < n && chars[j].is_ascii_digit() {
                j += 1;
            }
            let mut is_float = false;
            // `.` followed by a digit is a decimal point; `..` is DotDot.
            if j + 1 < n && chars[j] == '.' && chars[j + 1].is_ascii_digit() {
                is_float = true;
                j += 1;
                while j < n && chars[j].is_ascii_digit() {
                    j += 1;
                }
            }
            let text: String = chars[i..j].iter().collect();
            let tok = if is_float { Tok::Float(text) } else { Tok::Int(text) };
            push_simple(&mut out, tok, line, start_col, bs, byte_of[j]);
            col += j - i;
            i = j;
            continue;
        }

        // Multi-char ASCII operators (single token): ++ => <= >= != ..
        if i + 1 < n {
            let two: String = [c, chars[i + 1]].iter().collect();
            let is_two = matches!(two.as_str(), "++" | "=>" | "<=" | ">=" | "!=" | "..");
            if is_two {
                push_simple(&mut out, Tok::Op(two), line, start_col, bs, byte_of[i + 2]);
                col += 2;
                i += 2;
                continue;
            }
        }

        // Single-char operator / punctuation.
        if OP_CHARS.contains(c) {
            push_simple(&mut out, Tok::Op(c.to_string()), line, start_col, bs, byte_of[i + 1]);
            col += 1;
            i += 1;
            continue;
        }

        // Unknown char — emit as an Op so spans stay continuous; never an
        // identifier, so rename can't touch it.
        push_simple(&mut out, Tok::Op(c.to_string()), line, start_col, bs, byte_of[i + 1]);
        col += 1;
        i += 1;
    }

    out
}

pub const KEYWORDS: &[&str] = &[
    "claim", "type", "schema", "fsm", "enum", "import", "match", "subclaim",
    "external", "matches", "in", "true", "false", "mapsto",
];

pub fn is_keyword(s: &str) -> bool {
    KEYWORDS.contains(&s)
}
